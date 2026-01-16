//! Node classification service

use regex::Regex;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use crate::models::{
    ClassificationResult, ClassificationRule, GroupMatch, MatchType, NodeGroup, RuleEvaluation,
    RuleMatchType, RuleOperator,
};

/// Classification service for matching nodes to groups
pub struct ClassificationService {
    groups: Vec<NodeGroup>,
}

impl ClassificationService {
    /// Create a new classification service
    pub fn new(groups: Vec<NodeGroup>) -> Self {
        Self { groups }
    }

    /// Classify a node based on its facts (supports hierarchical groups and inheritance)
    ///
    /// This method classifies against groups from a single organization.
    /// For multi-organization classification with conflict detection, use `classify_across_organizations`.
    pub fn classify(&self, certname: &str, facts: &serde_json::Value) -> ClassificationResult {
        self.classify_internal(certname, facts, None)
    }

    /// Classify a node against groups from all organizations and detect conflicts
    ///
    /// This method:
    /// 1. Groups the input groups by organization
    /// 2. Classifies the node against each organization's groups
    /// 3. Detects if the node matches groups from multiple organizations (conflict)
    /// 4. Returns the classification from the matching organization, or an error if conflicted
    /// 5. If no groups match, uses the default organization
    pub fn classify_across_organizations(
        &self,
        certname: &str,
        facts: &serde_json::Value,
        default_org_id: Uuid,
    ) -> ClassificationResult {
        // Group all groups by organization
        let mut org_groups: HashMap<Uuid, Vec<&NodeGroup>> = HashMap::new();
        for group in &self.groups {
            org_groups
                .entry(group.organization_id)
                .or_default()
                .push(group);
        }

        // Classify against each organization and track which ones have matches
        let mut org_results: Vec<(Uuid, ClassificationResult)> = Vec::new();

        for (org_id, groups) in &org_groups {
            let org_service = ClassificationService {
                groups: groups.iter().map(|g| (*g).clone()).collect(),
            };
            let result = org_service.classify_internal(certname, facts, Some(*org_id));

            // Check if this organization had any non-inherited matches
            // (inherited matches don't count as the node being "in" that org)
            let has_direct_matches = result.groups.iter().any(|g| {
                g.match_type == MatchType::Rules || g.match_type == MatchType::Pinned
            });

            if has_direct_matches {
                org_results.push((*org_id, result));
            }
        }

        // Check for conflicts (node matches in multiple organizations)
        if org_results.len() > 1 {
            let org_names: Vec<String> = org_results
                .iter()
                .map(|(id, _)| id.to_string())
                .collect();

            // Return the first result but with a conflict error
            let mut result = org_results.remove(0).1;
            result.conflict_error = Some(format!(
                "Node '{}' matches groups from multiple organizations: {}. \
                 Please ensure nodes are assigned to only one organization.",
                certname,
                org_names.join(", ")
            ));
            return result;
        }

        // Return the single matching org's result, or empty result with default org
        if let Some((org_id, mut result)) = org_results.pop() {
            result.organization_id = Some(org_id);
            result
        } else {
            // No matches - return empty classification with default org
            ClassificationResult {
                certname: certname.to_string(),
                organization_id: Some(default_org_id),
                groups: vec![],
                classes: serde_json::json!({}),
                variables: serde_json::json!({}),
                environment: None,
                conflict_error: None,
            }
        }
    }

    /// Internal classification method that handles the actual matching logic
    fn classify_internal(
        &self,
        certname: &str,
        facts: &serde_json::Value,
        organization_id: Option<Uuid>,
    ) -> ClassificationResult {
        let mut matched_groups: Vec<GroupMatch> = vec![];
        // Classes are now in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
        let mut all_classes = serde_json::json!({});
        let mut all_variables = serde_json::json!({});
        let mut environment: Option<String> = None;
        // Track which groups have already had their configs merged to avoid duplicates
        let mut merged_group_ids: Vec<Uuid> = vec![];

        // Extract node's environment from facts (catalog_environment)
        let node_environment = get_fact_value(facts, "catalog_environment")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        // Index groups by id and build parent -> children map
        let mut children_map: HashMap<Option<Uuid>, Vec<&NodeGroup>> = HashMap::new();
        let mut group_index: HashMap<Uuid, &NodeGroup> = HashMap::new();
        for group in &self.groups {
            children_map.entry(group.parent_id).or_default().push(group);
            group_index.insert(group.id, group);
        }

        // Build a helper function to get all ancestors (parent chain) from root to group
        let get_ancestor_chain = |group_id: Uuid| -> Vec<&NodeGroup> {
            let mut chain = vec![];
            let mut current_id = group_id;

            // Walk up the tree from leaf to root
            while let Some(group) = group_index.get(&current_id) {
                chain.push(*group);
                if let Some(parent_id) = group.parent_id {
                    current_id = parent_id;
                } else {
                    break;
                }
            }

            // Reverse to get root -> leaf order
            chain.reverse();
            chain
        };

        // BFS from root groups; children are only considered if parent matched
        let mut queue: VecDeque<(&NodeGroup, bool)> = VecDeque::new();
        if let Some(roots) = children_map.get(&None) {
            for g in roots {
                queue.push_back((*g, false));
            }
        }

        while let Some((group, parent_matched)) = queue.pop_front() {
            tracing::debug!(
                "Classifying node '{}' against group '{}' (id={}, parent_matched={})",
                certname,
                group.name,
                group.id,
                parent_matched
            );

            let mut matched = false;
            let mut matched_rules = vec![];

            // Check environment filtering first (applies to all non-pinned matches)
            // UNLESS this is an "environment group" - which ASSIGNS environments rather than filtering by them
            let environment_matches = if group.is_environment_group {
                // Environment groups skip environment filtering - they ASSIGN environments
                true
            } else {
                match &group.environment {
                    None => true, // No environment restriction
                    Some(env) if env == "*" || env.to_lowercase() == "all" || env.to_lowercase() == "any" => true,
                    Some(env) => {
                        // Group has specific environment requirement
                        match &node_environment {
                            Some(node_env) => node_env == env,
                            None => false, // Node has no environment, cannot match specific requirement
                        }
                    }
                }
            };

            let match_type = if group.pinned_nodes.contains(&certname.to_string()) {
                // Pinned nodes ALWAYS match, regardless of environment
                // This allows environment assignment without bootstrap problems
                matched = true;
                MatchType::Pinned
            } else if !environment_matches {
                // Environment doesn't match - skip this group and its children
                tracing::debug!(
                    "Node '{}' environment {:?} does not match group '{}' requirement {:?}, skipping",
                    certname,
                    node_environment,
                    group.name,
                    group.environment
                );
                continue;
            } else if group.rules.is_empty() {
                // Group has no rules - behavior depends on match_all_nodes setting
                tracing::debug!(
                    "Group '{}' has no rules, match_all_nodes={}, parent_matched={}",
                    group.name,
                    group.match_all_nodes,
                    parent_matched
                );
                if group.match_all_nodes {
                    // match_all_nodes=true: Match all nodes (within parent context if has parent)
                    // If has parent, only match if parent matched
                    // If no parent (root group), match all nodes that pass environment filter
                    if group.parent_id.is_none() || parent_matched {
                        matched = true;
                        tracing::debug!("Group '{}' matched (match_all_nodes=true)", group.name);
                    } else {
                        tracing::debug!(
                            "Group '{}' NOT matched (match_all_nodes=true but parent didn't match)",
                            group.name
                        );
                    }
                    MatchType::Rules // Using Rules type since it's an explicit match configuration
                } else {
                    // match_all_nodes=false (default): Groups with no rules match NO nodes
                    // This is true regardless of whether parent matched or not
                    tracing::debug!(
                        "Group '{}' NOT matched (no rules, match_all_nodes=false)",
                        group.name
                    );
                    MatchType::Rules
                }
            } else {
                // Evaluate rules
                let evaluations = self.evaluate_rules(&group.rules, facts);
                matched_rules = evaluations
                    .iter()
                    .filter(|e| e.matched)
                    .map(|e| e.rule_id)
                    .collect();

                let rules_match = match group.rule_match_type {
                    RuleMatchType::All => {
                        !group.rules.is_empty()
                            && evaluations.iter().all(|e| e.matched || e.error.is_some())
                            && evaluations.iter().any(|e| e.matched)
                    }
                    RuleMatchType::Any => evaluations.iter().any(|e| e.matched),
                };

                if rules_match {
                    matched = true;
                    MatchType::Rules
                } else {
                    MatchType::Rules
                }
            };

            if matched {
                // Get the full ancestor chain (root to current group)
                // This ensures we inherit configurations from ALL parents, not just immediate parent
                let ancestor_chain = get_ancestor_chain(group.id);

                // Merge classes and variables from all ancestors (root to leaf order)
                // Only merge groups that haven't been merged yet to avoid duplicates
                // This ensures parent configs are applied first, then child configs override
                for ancestor in &ancestor_chain {
                    if !merged_group_ids.contains(&ancestor.id) {
                        merge_classes(&mut all_classes, &ancestor.classes);
                        merge_parameters(&mut all_variables, &ancestor.variables);
                        merged_group_ids.push(ancestor.id);
                    }
                }

                // Handle environment with proper precedence:
                // Leaf group environment takes precedence over parent environments
                // We walk the chain from leaf to root, taking the first non-None environment
                for ancestor in ancestor_chain.iter().rev() {
                    let group_env = match &ancestor.environment {
                        Some(env) if env == "*" || env.to_lowercase() == "all" || env.to_lowercase() == "any" => None,
                        other => other.clone(),
                    };

                    if group_env.is_some() {
                        environment = group_env;
                        break; // Take the first (closest to leaf) environment
                    }
                }

                matched_groups.push(GroupMatch {
                    id: group.id,
                    name: group.name.clone(),
                    match_type,
                    matched_rules,
                });

                // Enqueue children for evaluation - they inherit classes/variables but must match their own rules
                if let Some(children) = children_map.get(&Some(group.id)) {
                    for child in children {
                        // parent_matched=true means this child can inherit classes/variables from ancestors
                        tracing::debug!(
                            "Enqueueing child group '{}' (parent '{}' matched)",
                            child.name,
                            group.name
                        );
                        queue.push_back((*child, true));
                    }
                }
            } else {
                // Group didn't match - children will NOT be evaluated
                if let Some(children) = children_map.get(&Some(group.id)) {
                    tracing::debug!(
                        "Group '{}' didn't match, NOT enqueueing {} children: {:?}",
                        group.name,
                        children.len(),
                        children.iter().map(|c| &c.name).collect::<Vec<_>>()
                    );
                }
            }
            // NOTE: If parent matched but this group didn't match, we do NOT enqueue children.
            // Children can only be considered if their direct parent matched.
            // This ensures proper hierarchical classification where each level acts as a gate.
        }

        ClassificationResult {
            certname: certname.to_string(),
            organization_id,
            groups: matched_groups,
            classes: all_classes,
            variables: all_variables,
            environment,
            conflict_error: None,
        }
    }

    /// Evaluate rules against facts
    fn evaluate_rules(
        &self,
        rules: &[ClassificationRule],
        facts: &serde_json::Value,
    ) -> Vec<RuleEvaluation> {
        rules
            .iter()
            .map(|rule| self.evaluate_rule(rule, facts))
            .collect()
    }

    /// Evaluate a single rule against facts
    fn evaluate_rule(
        &self,
        rule: &ClassificationRule,
        facts: &serde_json::Value,
    ) -> RuleEvaluation {
        let fact_value = get_fact_value(facts, &rule.fact_path);

        let matched = match &fact_value {
            Some(value) => {
                let result = match_value(value, &rule.operator, &rule.value);
                tracing::debug!(
                    "Rule evaluation: path='{}' operator='{:?}' rule_value={:?} fact_value={:?} matched={}",
                    rule.fact_path,
                    rule.operator,
                    rule.value,
                    value,
                    result
                );
                result
            }
            None => {
                tracing::debug!(
                    "Rule evaluation: path='{}' fact not found, matched=false",
                    rule.fact_path
                );
                false
            }
        };

        RuleEvaluation {
            rule_id: rule.id,
            matched,
            fact_value,
            error: None,
        }
    }
}

/// Get a fact value by path (e.g., "os.family" -> facts["os"]["family"])
fn get_fact_value(facts: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = facts;

    for part in parts {
        match current.get(part) {
            Some(v) => current = v,
            None => return None,
        }
    }

    Some(current.clone())
}

/// Match a fact value against a rule value
fn match_value(
    fact_value: &serde_json::Value,
    operator: &RuleOperator,
    rule_value: &serde_json::Value,
) -> bool {
    match operator {
        RuleOperator::Equals => fact_value == rule_value,
        RuleOperator::NotEquals => fact_value != rule_value,
        RuleOperator::Regex => {
            if let (Some(fv), Some(rv)) = (fact_value.as_str(), rule_value.as_str()) {
                Regex::new(rv).map(|re| re.is_match(fv)).unwrap_or(false)
            } else {
                false
            }
        }
        RuleOperator::NotRegex => {
            if let (Some(fv), Some(rv)) = (fact_value.as_str(), rule_value.as_str()) {
                Regex::new(rv).map(|re| !re.is_match(fv)).unwrap_or(true)
            } else {
                true
            }
        }
        RuleOperator::GreaterThan => compare_values(fact_value, rule_value) > 0,
        RuleOperator::GreaterThanOrEqual => compare_values(fact_value, rule_value) >= 0,
        RuleOperator::LessThan => compare_values(fact_value, rule_value) < 0,
        RuleOperator::LessThanOrEqual => compare_values(fact_value, rule_value) <= 0,
        RuleOperator::In => {
            if let Some(arr) = rule_value.as_array() {
                arr.contains(fact_value)
            } else {
                false
            }
        }
        RuleOperator::NotIn => {
            if let Some(arr) = rule_value.as_array() {
                !arr.contains(fact_value)
            } else {
                true
            }
        }
    }
}

/// Compare two JSON values for ordering
fn compare_values(a: &serde_json::Value, b: &serde_json::Value) -> i32 {
    match (a, b) {
        (serde_json::Value::Number(an), serde_json::Value::Number(bn)) => {
            let af = an.as_f64().unwrap_or(0.0);
            let bf = bn.as_f64().unwrap_or(0.0);
            if af > bf {
                1
            } else if af < bf {
                -1
            } else {
                0
            }
        }
        (serde_json::Value::String(as_), serde_json::Value::String(bs)) => as_.cmp(bs) as i32,
        _ => 0,
    }
}

/// Merge parameters from a group into the accumulated parameters
/// Merge parameters (flat object merge, last value wins)
fn merge_parameters(target: &mut serde_json::Value, source: &serde_json::Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source_obj {
            target_obj.insert(key.clone(), value.clone());
        }
    }
}

/// Deep merge classes in Puppet Enterprise format
/// Classes are objects like: {"ntp": {"servers": ["ntp1.example.com"]}, "apache": {"port": 8080}}
/// When the same class appears in multiple groups, their parameters are deep merged
fn merge_classes(target: &mut serde_json::Value, source: &serde_json::Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (class_name, class_params) in source_obj {
            if let Some(existing_params) = target_obj.get_mut(class_name) {
                // Class already exists, deep merge its parameters
                deep_merge(existing_params, class_params);
            } else {
                // New class, insert it
                target_obj.insert(class_name.clone(), class_params.clone());
            }
        }
    }
}

/// Deep merge two JSON values (recursively merges objects, overwrites other types)
fn deep_merge(target: &mut serde_json::Value, source: &serde_json::Value) {
    match (target, source) {
        (serde_json::Value::Object(target_obj), serde_json::Value::Object(source_obj)) => {
            for (key, value) in source_obj {
                if let Some(existing) = target_obj.get_mut(key) {
                    deep_merge(existing, value);
                } else {
                    target_obj.insert(key.clone(), value.clone());
                }
            }
        }
        (target, source) => {
            // For non-objects, source overwrites target
            *target = source.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_fact_value_simple() {
        let facts = serde_json::json!({
            "os": "linux"
        });

        let value = get_fact_value(&facts, "os");
        assert_eq!(value, Some(serde_json::json!("linux")));
    }

    #[test]
    fn test_get_fact_value_nested() {
        let facts = serde_json::json!({
            "os": {
                "family": "RedHat",
                "release": {
                    "major": "8"
                }
            }
        });

        let value = get_fact_value(&facts, "os.family");
        assert_eq!(value, Some(serde_json::json!("RedHat")));

        let value = get_fact_value(&facts, "os.release.major");
        assert_eq!(value, Some(serde_json::json!("8")));
    }

    #[test]
    fn test_get_fact_value_missing() {
        let facts = serde_json::json!({
            "os": "linux"
        });

        let value = get_fact_value(&facts, "missing");
        assert_eq!(value, None);
    }

    #[test]
    fn test_match_value_equals() {
        assert!(match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::Equals,
            &serde_json::json!("RedHat")
        ));

        assert!(!match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::Equals,
            &serde_json::json!("RedHat")
        ));
    }

    #[test]
    fn test_match_value_not_equals() {
        assert!(match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::NotEquals,
            &serde_json::json!("RedHat")
        ));

        assert!(!match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::NotEquals,
            &serde_json::json!("RedHat")
        ));
    }

    #[test]
    fn test_match_value_regex() {
        assert!(match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::Regex,
            &serde_json::json!("^Red.*")
        ));

        assert!(!match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::Regex,
            &serde_json::json!("^Red.*")
        ));
    }

    #[test]
    fn test_match_value_not_regex() {
        assert!(match_value(
            &serde_json::json!("Debian"),
            &RuleOperator::NotRegex,
            &serde_json::json!("^Red.*")
        ));

        assert!(!match_value(
            &serde_json::json!("RedHat"),
            &RuleOperator::NotRegex,
            &serde_json::json!("^Red.*")
        ));
    }

    #[test]
    fn test_match_value_in() {
        assert!(match_value(
            &serde_json::json!("production"),
            &RuleOperator::In,
            &serde_json::json!(["production", "staging"])
        ));

        assert!(!match_value(
            &serde_json::json!("development"),
            &RuleOperator::In,
            &serde_json::json!(["production", "staging"])
        ));
    }

    #[test]
    fn test_match_value_not_in() {
        assert!(match_value(
            &serde_json::json!("development"),
            &RuleOperator::NotIn,
            &serde_json::json!(["production", "staging"])
        ));

        assert!(!match_value(
            &serde_json::json!("production"),
            &RuleOperator::NotIn,
            &serde_json::json!(["production", "staging"])
        ));
    }

    #[test]
    fn test_match_value_ordering() {
        assert!(match_value(
            &serde_json::json!(10),
            &RuleOperator::GreaterThan,
            &serde_json::json!(5)
        ));
        assert!(match_value(
            &serde_json::json!(5),
            &RuleOperator::LessThan,
            &serde_json::json!(10)
        ));
        assert!(match_value(
            &serde_json::json!(5),
            &RuleOperator::LessThanOrEqual,
            &serde_json::json!(5)
        ));
        assert!(match_value(
            &serde_json::json!(10),
            &RuleOperator::GreaterThanOrEqual,
            &serde_json::json!(10)
        ));
    }

    #[test]
    fn test_classify_pinned_node() {
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "webservers".to_string(),
            pinned_nodes: vec!["web1.example.com".to_string()],
            classes: serde_json::json!({"profile::webserver": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);
        let facts = serde_json::json!({});

        let result = service.classify("web1.example.com", &facts);

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        assert!(result.classes.as_object().unwrap().contains_key("profile::webserver"));
    }

    #[test]
    fn test_classify_by_rules() {
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "redhat_servers".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"profile::base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);
        let facts = serde_json::json!({
            "os": {
                "family": "RedHat"
            }
        });

        let result = service.classify("node1.example.com", &facts);

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
    }

    #[test]
    fn test_classify_inherits_child_groups() {
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"class_parent": {"p": "root"}}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Child inherits from parent when this is enabled
            classes: serde_json::json!({"class_child": {"child": true}, "class_parent": {"p": "child"}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({
            "os": {"family": "RedHat"}
        });

        let result = service.classify("node1.example.com", &facts);

        assert_eq!(result.groups.len(), 2);
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
        assert_eq!(result.groups[1].match_type, MatchType::Rules); // Changed from Inherited since match_all_nodes uses Rules type
        // Classes should be merged with deep merge
        assert!(result.classes.as_object().unwrap().contains_key("class_parent"));
        assert!(result.classes.as_object().unwrap().contains_key("class_child"));
        // Deep merge: child's parameters override parent's for same class
        assert_eq!(result.classes["class_parent"]["p"], serde_json::json!("child"));
        assert_eq!(result.classes["class_child"]["child"], serde_json::json!(true));
    }

    #[test]
    fn test_classify_pinned_inherits_children() {
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            pinned_nodes: vec!["web1.example.com".to_string()],
            classes: serde_json::json!({"class_parent": {}}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Child inherits from parent when this is enabled
            classes: serde_json::json!({"class_child": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({});

        let result = service.classify("web1.example.com", &facts);

        assert_eq!(result.groups.len(), 2);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        assert_eq!(result.groups[1].match_type, MatchType::Rules); // Child has match_all_nodes=true, uses Rules type
        assert!(result.classes.as_object().unwrap().contains_key("class_parent"));
        assert!(result.classes.as_object().unwrap().contains_key("class_child"));
    }

    #[test]
    fn test_classify_rule_match_any() {
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "any_rule".to_string(),
            rule_match_type: RuleMatchType::Any,
            rules: vec![
                ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                },
                ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.release.major".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("9"),
                },
            ],
            classes: serde_json::json!({"class_any": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);
        let facts = serde_json::json!({
            "os": {
                "family": "RedHat",
                "release": {"major": "8"}
            }
        });

        let result = service.classify("node1.example.com", &facts);

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
        assert_eq!(result.groups[0].matched_rules.len(), 1);
        assert!(result.classes.as_object().unwrap().contains_key("class_any"));
    }

    #[test]
    fn test_classify_environment_filter_matches() {
        // Group with specific environment requirement
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "production_servers".to_string(),
            environment: Some("production".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"profile::prod": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node in production environment should match
        let facts = serde_json::json!({
            "os": {"family": "RedHat"},
            "catalog_environment": "production"
        });

        let result = service.classify("node1.example.com", &facts);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].name, "production_servers");
    }

    #[test]
    fn test_classify_environment_filter_no_match() {
        // Group with specific environment requirement
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "production_servers".to_string(),
            environment: Some("production".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"profile::prod": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node in different environment should NOT match
        let facts = serde_json::json!({
            "os": {"family": "RedHat"},
            "catalog_environment": "development"
        });

        let result = service.classify("node1.example.com", &facts);
        assert_eq!(result.groups.len(), 0);
    }

    #[test]
    fn test_classify_environment_wildcard_matches_all() {
        // Test that "*", "All", and "Any" match nodes from any environment
        let groups = vec![
            NodeGroup {
                id: Uuid::new_v4(),
                name: "wildcard_group".to_string(),
                environment: Some("*".to_string()),
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class1": {}}),
                ..Default::default()
            },
            NodeGroup {
                id: Uuid::new_v4(),
                name: "all_group".to_string(),
                environment: Some("All".to_string()),
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class2": {}}),
                ..Default::default()
            },
            NodeGroup {
                id: Uuid::new_v4(),
                name: "any_group".to_string(),
                environment: Some("any".to_string()),
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class3": {}}),
                ..Default::default()
            },
            NodeGroup {
                id: Uuid::new_v4(),
                name: "none_group".to_string(),
                environment: None,
                rules: vec![ClassificationRule {
                    id: Uuid::new_v4(),
                    fact_path: "os.family".to_string(),
                    operator: RuleOperator::Equals,
                    value: serde_json::json!("RedHat"),
                }],
                classes: serde_json::json!({"class4": {}}),
                ..Default::default()
            },
        ];

        let service = ClassificationService::new(groups);

        let facts = serde_json::json!({
            "os": {"family": "RedHat"},
            "catalog_environment": "production"
        });

        let result = service.classify("node1.example.com", &facts);

        // All 4 groups should match
        assert_eq!(result.groups.len(), 4);
        assert!(result.classes.as_object().unwrap().contains_key("class1"));
        assert!(result.classes.as_object().unwrap().contains_key("class2"));
        assert!(result.classes.as_object().unwrap().contains_key("class3"));
        assert!(result.classes.as_object().unwrap().contains_key("class4"));
    }

    #[test]
    fn test_classify_pinned_node_ignores_environment() {
        // Pinned nodes ALWAYS match regardless of environment
        // This allows environment assignment without bootstrap problems
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "production_servers".to_string(),
            environment: Some("production".to_string()),
            pinned_nodes: vec!["web1.example.com".to_string()],
            classes: serde_json::json!({"profile::prod": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Pinned node with different environment should STILL match
        let facts = serde_json::json!({
            "catalog_environment": "development"
        });

        let result = service.classify("web1.example.com", &facts);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        // The group's environment should be returned
        assert_eq!(result.environment, Some("production".to_string()));

        // Pinned node in matching environment should also match
        let facts = serde_json::json!({
            "catalog_environment": "production"
        });

        let result = service.classify("web1.example.com", &facts);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
    }

    #[test]
    fn test_deep_hierarchy_inherits_all_ancestors() {
        // Test that a deeply nested group (grandchild) inherits from all ancestors
        // Hierarchy: Root -> Child -> Grandchild
        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let grandchild_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "root".to_string(),
            environment: None, // No environment restriction for root
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({
                "base": {"version": "1.0"},
                "common": {"root_param": "root_value"}
            }),
            variables: serde_json::json!({"root_var": "from_root"}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "child".to_string(),
            parent_id: Some(root_id),
            environment: None, // No environment restriction (inherits from parent)
            match_all_nodes: true, // Required to inherit from parent
            classes: serde_json::json!({
                "middleware": {"version": "2.0"},
                "common": {"child_param": "child_value"} // Merges with root's common class
            }),
            variables: serde_json::json!({"child_var": "from_child"}),
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: grandchild_id,
            name: "grandchild".to_string(),
            parent_id: Some(child_id),
            environment: None, // No environment (should inherit from child)
            match_all_nodes: true, // Required to inherit from parent
            classes: serde_json::json!({
                "application": {"version": "3.0"},
                "common": {"grandchild_param": "grandchild_value"} // Further merges common class
            }),
            variables: serde_json::json!({"grandchild_var": "from_grandchild"}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);
        let facts = serde_json::json!({
            "os": {"family": "RedHat"}
        });

        let result = service.classify("node1.example.com", &facts);

        // Should match all three groups
        assert_eq!(result.groups.len(), 3);
        assert_eq!(result.groups[0].name, "root");
        assert_eq!(result.groups[0].match_type, MatchType::Rules);
        assert_eq!(result.groups[1].name, "child");
        assert_eq!(result.groups[1].match_type, MatchType::Rules); // match_all_nodes=true uses Rules type
        assert_eq!(result.groups[2].name, "grandchild");
        assert_eq!(result.groups[2].match_type, MatchType::Rules); // match_all_nodes=true uses Rules type

        // Environment should be None since no group in the hierarchy has an environment
        assert_eq!(result.environment, None);

        // Classes should be merged from all three groups
        let classes = result.classes.as_object().unwrap();
        assert!(classes.contains_key("base")); // From root
        assert!(classes.contains_key("middleware")); // From child
        assert!(classes.contains_key("application")); // From grandchild
        assert!(classes.contains_key("common")); // Merged from all three

        // Check that common class has parameters from all levels (deep merge)
        let common = &classes["common"];
        assert_eq!(common["root_param"], serde_json::json!("root_value"));
        assert_eq!(common["child_param"], serde_json::json!("child_value"));
        assert_eq!(common["grandchild_param"], serde_json::json!("grandchild_value"));

        // Variables should be merged from all three groups
        let vars = result.variables.as_object().unwrap();
        assert_eq!(vars["root_var"], serde_json::json!("from_root"));
        assert_eq!(vars["child_var"], serde_json::json!("from_child"));
        assert_eq!(vars["grandchild_var"], serde_json::json!("from_grandchild"));
    }

    #[test]
    fn test_child_without_parent_match_should_not_match() {
        // Test that a child group doesn't match if its parent doesn't match
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "os.family".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("RedHat"),
            }],
            classes: serde_json::json!({"parent_class": {}}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            pinned_nodes: vec!["node1.example.com".to_string()], // Even pinned!
            classes: serde_json::json!({"child_class": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({
            "os": {"family": "Debian"} // Doesn't match parent's rule
        });

        let result = service.classify("node1.example.com", &facts);

        // Neither parent nor child should match
        assert_eq!(result.groups.len(), 0);
        assert!(result.classes.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_pinned_group_overrides_rule_matched_environment() {
        // Test that a pinned group's environment overrides a rule-matched group's environment
        // This simulates: node matches "Linux Servers" (production) AND is pinned to "Puppet Servers" (pserver)
        // Expected: pinned group's environment should take precedence

        let linux_servers = NodeGroup {
            id: Uuid::new_v4(),
            name: "Linux Servers".to_string(),
            environment: Some("production".to_string()),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            classes: serde_json::json!({"base": {}}),
            ..Default::default()
        };

        let puppet_servers = NodeGroup {
            id: Uuid::new_v4(),
            name: "Puppet Servers".to_string(),
            environment: Some("pserver".to_string()),
            pinned_nodes: vec!["segdc1vpr0018.fgv.br".to_string()],
            classes: serde_json::json!({"puppetserver": {}}),
            ..Default::default()
        };

        // Test scenario 1: Linux Servers processed first, then Puppet Servers
        let service1 = ClassificationService::new(vec![linux_servers.clone(), puppet_servers.clone()]);
        let facts = serde_json::json!({
            "kernel": "Linux",
            "catalog_environment": "pserver"  // Node is in pserver environment
        });

        let result1 = service1.classify("segdc1vpr0018.fgv.br", &facts);

        // Node should match Puppet Servers (pinned) but NOT Linux Servers (wrong environment)
        assert_eq!(result1.groups.len(), 1);
        assert_eq!(result1.groups[0].name, "Puppet Servers");
        assert_eq!(result1.groups[0].match_type, MatchType::Pinned);
        assert_eq!(result1.environment, Some("pserver".to_string()));
        assert!(result1.classes.as_object().unwrap().contains_key("puppetserver"));

        // Test scenario 2: Puppet Servers processed first, then Linux Servers
        let service2 = ClassificationService::new(vec![puppet_servers.clone(), linux_servers.clone()]);
        let result2 = service2.classify("segdc1vpr0018.fgv.br", &facts);

        // Same result regardless of order
        assert_eq!(result2.groups.len(), 1);
        assert_eq!(result2.groups[0].name, "Puppet Servers");
        assert_eq!(result2.environment, Some("pserver".to_string()));
    }

    #[test]
    fn test_environment_group_child_inherits_only_matching_nodes() {
        // Test scenario: Parent is an environment group with rules
        // Child has no rules, should only match nodes that match the parent
        //
        // Parent: "Homolog" (environment group) with rule clientcert ~ .*vhm.*
        // Child: "R - Docker Apps" with no rules
        //
        // Node A (segdc1vhm0001): certname contains "vhm" -> should match both
        // Node B (apldc1vds0045): certname doesn't contain "vhm" -> should match neither

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Homolog".to_string(),
            environment: Some("homolog".to_string()),
            is_environment_group: true, // This is an environment group
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*vhm.*"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "R - Docker Apps".to_string(),
            parent_id: Some(parent_id),
            environment: None, // No environment - inherits from parent
            match_all_nodes: true, // Required to inherit from parent
            // No rules - should inherit from parent
            rules: vec![],
            classes: serde_json::json!({"docker": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: certname contains "vhm" - should match both parent and child
        let facts_a = serde_json::json!({
            "clientcert": "segdc1vhm0001.fgv.br"
        });
        let result_a = service.classify("segdc1vhm0001.fgv.br", &facts_a);
        assert_eq!(result_a.groups.len(), 2, "Node with 'vhm' should match both groups");
        assert_eq!(result_a.groups[0].name, "Homolog");
        assert_eq!(result_a.groups[1].name, "R - Docker Apps");
        assert_eq!(result_a.groups[1].match_type, MatchType::Rules); // match_all_nodes=true uses Rules type

        // Node B: certname doesn't contain "vhm" - should match neither
        let facts_b = serde_json::json!({
            "clientcert": "apldc1vds0045.fgv.br"
        });
        let result_b = service.classify("apldc1vds0045.fgv.br", &facts_b);
        assert_eq!(result_b.groups.len(), 0, "Node without 'vhm' should not match any group");
    }

    #[test]
    fn test_child_with_rules_must_also_match_parent() {
        // Test that even if a child group has its own rules that match,
        // the node must ALSO match the parent for the child to match

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Docker Hosts".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "docker_installed".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!(true),
            }],
            classes: serde_json::json!({"docker": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node with Docker but NOT Linux - should not match child
        let facts = serde_json::json!({
            "kernel": "Windows",
            "docker_installed": true
        });
        let result = service.classify("win-docker.example.com", &facts);
        assert_eq!(result.groups.len(), 0, "Windows node should not match even with Docker");

        // Node with Linux AND Docker - should match both
        let facts = serde_json::json!({
            "kernel": "Linux",
            "docker_installed": true
        });
        let result = service.classify("linux-docker.example.com", &facts);
        assert_eq!(result.groups.len(), 2, "Linux node with Docker should match both groups");
    }

    #[test]
    fn test_match_all_nodes_root_group() {
        // Test that a root group with match_all_nodes=true matches all nodes
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Nodes".to_string(),
            match_all_nodes: true, // Matches all nodes
            rules: vec![],         // No rules
            classes: serde_json::json!({"base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Any node should match
        let facts = serde_json::json!({
            "kernel": "Linux"
        });
        let result = service.classify("any-node.example.com", &facts);
        assert_eq!(result.groups.len(), 1, "Node should match group with match_all_nodes=true");
        assert_eq!(result.groups[0].name, "All Nodes");
    }

    #[test]
    fn test_match_all_nodes_false_root_group() {
        // Test that a root group with match_all_nodes=false and no rules matches NO nodes
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Empty Group".to_string(),
            match_all_nodes: false, // Default - should not match when no rules
            rules: vec![],          // No rules
            classes: serde_json::json!({"base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        let facts = serde_json::json!({
            "kernel": "Linux"
        });
        let result = service.classify("any-node.example.com", &facts);
        assert_eq!(result.groups.len(), 0, "Node should NOT match group with match_all_nodes=false and no rules");
    }

    #[test]
    fn test_match_all_nodes_child_respects_parent() {
        // Test that a child group with match_all_nodes=true still requires parent to match
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Linux Nodes".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Matches all nodes within parent context
            rules: vec![],         // No rules
            classes: serde_json::json!({"linux_base": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Linux node should match both parent and child
        let linux_facts = serde_json::json!({
            "kernel": "Linux"
        });
        let result = service.classify("linux-node.example.com", &linux_facts);
        assert_eq!(result.groups.len(), 2, "Linux node should match both groups");
        assert_eq!(result.groups[0].name, "Linux");
        assert_eq!(result.groups[1].name, "All Linux Nodes");

        // Windows node should match neither (parent doesn't match)
        let windows_facts = serde_json::json!({
            "kernel": "Windows"
        });
        let result = service.classify("windows-node.example.com", &windows_facts);
        assert_eq!(result.groups.len(), 0, "Windows node should not match - parent doesn't match");
    }

    #[test]
    fn test_match_all_nodes_with_environment_filter() {
        // Test that match_all_nodes still respects environment filtering
        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Production All".to_string(),
            environment: Some("production".to_string()),
            is_environment_group: false, // Regular group - filters by environment
            match_all_nodes: true,       // Would match all nodes IF environment passes
            rules: vec![],
            classes: serde_json::json!({"production": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node in production environment should match
        let prod_facts = serde_json::json!({
            "catalog_environment": "production"
        });
        let result = service.classify("prod-node.example.com", &prod_facts);
        assert_eq!(result.groups.len(), 1, "Production node should match");

        // Node in staging environment should NOT match
        let staging_facts = serde_json::json!({
            "catalog_environment": "staging"
        });
        let result = service.classify("staging-node.example.com", &staging_facts);
        assert_eq!(result.groups.len(), 0, "Staging node should not match - wrong environment");
    }

    #[test]
    fn test_child_with_rules_does_not_match_when_parent_has_no_rules_and_no_match_all() {
        // Test scenario from user bug report:
        // - Grandparent: "Homolog" (has rules, matches)
        // - Parent: "ADM - ESI (H)" (0 rules, match_all_nodes=false) -> should NOT match
        // - Child: "R - Docker Apps" (has rules that match) -> should NOT match because parent doesn't
        //
        // Even if a child group has rules that match the node,
        // it should NOT match if its parent doesn't match.

        let grandparent_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();

        let grandparent = NodeGroup {
            id: grandparent_id,
            name: "Homolog".to_string(),
            environment: Some("homolog".to_string()),
            is_environment_group: true,
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*vhm.*"),
            }],
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let parent = NodeGroup {
            id: parent_id,
            name: "ADM - ESI (H)".to_string(),
            parent_id: Some(grandparent_id),
            match_all_nodes: false, // No rules + match_all_nodes=false = matches NO nodes
            rules: vec![],          // No rules!
            classes: serde_json::json!({}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "R - Docker Apps".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "clientcert".to_string(),
                operator: RuleOperator::Regex,
                value: serde_json::json!(".*vhm.*"),
            }],
            classes: serde_json::json!({"docker": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![grandparent, parent, child]);

        // Node that matches grandparent and child rules, but parent has no rules
        let facts = serde_json::json!({
            "clientcert": "segdc1vhm0001.fgv.br"
        });
        let result = service.classify("segdc1vhm0001.fgv.br", &facts);

        // Should only match grandparent (Homolog), NOT parent or child
        // Because parent has no rules and match_all_nodes=false, it doesn't match
        // And since parent doesn't match, child shouldn't even be evaluated
        assert_eq!(
            result.groups.len(),
            1,
            "Should only match grandparent, not parent or child. Got: {:?}",
            result.groups.iter().map(|g| &g.name).collect::<Vec<_>>()
        );
        assert_eq!(result.groups[0].name, "Homolog");
    }

    // =========================================================================
    // Comprehensive Classification Scenario Tests
    // =========================================================================

    #[test]
    fn test_scenario_simple_root_group_with_rules() {
        // Scenario: Single root group with rules
        // - Group: "Linux Servers" with rule kernel=Linux
        // - Node A: kernel=Linux -> should match
        // - Node B: kernel=Windows -> should not match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Linux Servers".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node A: Linux
        let result = service.classify("linux.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(result.groups.len(), 1, "Linux node should match");
        assert_eq!(result.groups[0].name, "Linux Servers");

        // Node B: Windows
        let result = service.classify("windows.example.com", &serde_json::json!({"kernel": "Windows"}));
        assert_eq!(result.groups.len(), 0, "Windows node should not match");
    }

    #[test]
    fn test_scenario_root_group_no_rules_match_all_false() {
        // Scenario: Root group with no rules and match_all_nodes=false
        // - Group: "Empty Group" with no rules, match_all_nodes=false
        // - Any node -> should NOT match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Empty Group".to_string(),
            match_all_nodes: false,
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        let result = service.classify("any.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(result.groups.len(), 0, "No node should match group with no rules and match_all_nodes=false");
    }

    #[test]
    fn test_scenario_root_group_no_rules_match_all_true() {
        // Scenario: Root group with no rules and match_all_nodes=true
        // - Group: "All Nodes" with no rules, match_all_nodes=true
        // - Any node -> should match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Nodes".to_string(),
            match_all_nodes: true,
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        let result = service.classify("any.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(result.groups.len(), 1, "All nodes should match group with match_all_nodes=true");
        assert_eq!(result.groups[0].name, "All Nodes");
    }

    #[test]
    fn test_scenario_parent_with_rules_child_with_rules() {
        // Scenario: Parent has rules, child has rules
        // - Parent: "Linux" with rule kernel=Linux
        // - Child: "Web Servers" with rule role=webserver
        // - Node A: kernel=Linux, role=webserver -> matches both
        // - Node B: kernel=Linux, role=dbserver -> matches parent only
        // - Node C: kernel=Windows, role=webserver -> matches neither

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Web Servers".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: Linux + webserver -> both
        let result = service.classify("web.example.com", &serde_json::json!({"kernel": "Linux", "role": "webserver"}));
        assert_eq!(result.groups.len(), 2, "Node A should match both groups");

        // Node B: Linux + dbserver -> parent only
        let result = service.classify("db.example.com", &serde_json::json!({"kernel": "Linux", "role": "dbserver"}));
        assert_eq!(result.groups.len(), 1, "Node B should match parent only");
        assert_eq!(result.groups[0].name, "Linux");

        // Node C: Windows + webserver -> neither
        let result = service.classify("winweb.example.com", &serde_json::json!({"kernel": "Windows", "role": "webserver"}));
        assert_eq!(result.groups.len(), 0, "Node C should match neither");
    }

    #[test]
    fn test_scenario_parent_with_rules_child_no_rules_match_all_false() {
        // Scenario: Parent has rules, child has NO rules, match_all_nodes=false
        // - Parent: "Linux" with rule kernel=Linux
        // - Child: "Empty Child" with no rules, match_all_nodes=false
        // - Node A: kernel=Linux -> matches parent only (child has no rules)
        // - Node B: kernel=Windows -> matches neither

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Empty Child".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: false, // Child has no rules and match_all_nodes=false
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: Linux -> parent only
        let result = service.classify("linux.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(result.groups.len(), 1, "Node A should match parent only, not empty child");
        assert_eq!(result.groups[0].name, "Linux");

        // Node B: Windows -> neither
        let result = service.classify("windows.example.com", &serde_json::json!({"kernel": "Windows"}));
        assert_eq!(result.groups.len(), 0, "Node B should match neither");
    }

    #[test]
    fn test_scenario_parent_with_rules_child_no_rules_match_all_true() {
        // Scenario: Parent has rules, child has NO rules, match_all_nodes=true
        // - Parent: "Linux" with rule kernel=Linux
        // - Child: "All Linux" with no rules, match_all_nodes=true
        // - Node A: kernel=Linux -> matches both (child inherits from parent)
        // - Node B: kernel=Windows -> matches neither (parent doesn't match, so child isn't evaluated)

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "All Linux".to_string(),
            parent_id: Some(parent_id),
            match_all_nodes: true, // Child has no rules but match_all_nodes=true
            rules: vec![],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: Linux -> both
        let result = service.classify("linux.example.com", &serde_json::json!({"kernel": "Linux"}));
        assert_eq!(result.groups.len(), 2, "Node A should match both groups");
        assert_eq!(result.groups[0].name, "Linux");
        assert_eq!(result.groups[1].name, "All Linux");

        // Node B: Windows -> neither
        let result = service.classify("windows.example.com", &serde_json::json!({"kernel": "Windows"}));
        assert_eq!(result.groups.len(), 0, "Node B should match neither (parent didn't match)");
    }

    #[test]
    fn test_scenario_parent_no_rules_match_all_false_child_with_rules() {
        // Scenario: Parent has NO rules (match_all_nodes=false), child HAS rules
        // This is the bug scenario reported by the user!
        // - Parent: "Empty Parent" with no rules, match_all_nodes=false
        // - Child: "Specific Child" with rule role=webserver
        // - Node A: role=webserver -> should match NEITHER (parent doesn't match, child not evaluated)

        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "Empty Parent".to_string(),
            match_all_nodes: false,
            rules: vec![],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "Specific Child".to_string(),
            parent_id: Some(parent_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);

        // Node A: webserver -> NEITHER (parent doesn't match)
        let result = service.classify("web.example.com", &serde_json::json!({"role": "webserver"}));
        assert_eq!(
            result.groups.len(),
            0,
            "Node should not match child because parent has no rules and match_all_nodes=false"
        );
    }

    #[test]
    fn test_scenario_three_level_hierarchy_all_with_rules() {
        // Scenario: Three-level hierarchy, all groups have rules
        // - Root: "Linux" with rule kernel=Linux
        // - Child: "Web" with rule role=webserver
        // - Grandchild: "Nginx" with rule app=nginx
        // Test various combinations

        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "Web".to_string(),
            parent_id: Some(root_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: Uuid::new_v4(),
            name: "Nginx".to_string(),
            parent_id: Some(child_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "app".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("nginx"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);

        // Node: Linux + webserver + nginx -> all three
        let result = service.classify(
            "nginx.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "webserver", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 3, "Should match all three");

        // Node: Linux + webserver + apache -> root and child only
        let result = service.classify(
            "apache.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "webserver", "app": "apache"}),
        );
        assert_eq!(result.groups.len(), 2, "Should match root and child only");

        // Node: Linux + dbserver + nginx -> root only
        let result = service.classify(
            "db.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "dbserver", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 1, "Should match root only");
        assert_eq!(result.groups[0].name, "Linux");

        // Node: Windows + webserver + nginx -> none
        let result = service.classify(
            "winweb.example.com",
            &serde_json::json!({"kernel": "Windows", "role": "webserver", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 0, "Should match none");
    }

    #[test]
    fn test_scenario_three_level_middle_no_rules_match_all_false() {
        // Scenario: Three-level hierarchy, middle group has no rules with match_all_nodes=false
        // - Root: "Linux" with rule kernel=Linux
        // - Child: "Empty Middle" with NO rules, match_all_nodes=false
        // - Grandchild: "Specific" with rule app=nginx
        //
        // Since middle has no rules and match_all_nodes=false, it won't match,
        // so grandchild won't be evaluated!

        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "Empty Middle".to_string(),
            parent_id: Some(root_id),
            match_all_nodes: false, // No rules, match_all_nodes=false
            rules: vec![],
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: Uuid::new_v4(),
            name: "Specific".to_string(),
            parent_id: Some(child_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "app".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("nginx"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);

        // Node: Linux + nginx -> should only match root!
        // Middle doesn't match (no rules, match_all_nodes=false), so grandchild isn't evaluated
        let result = service.classify(
            "nginx.example.com",
            &serde_json::json!({"kernel": "Linux", "app": "nginx"}),
        );
        assert_eq!(
            result.groups.len(),
            1,
            "Should match root only, middle blocks grandchild. Got: {:?}",
            result.groups.iter().map(|g| &g.name).collect::<Vec<_>>()
        );
        assert_eq!(result.groups[0].name, "Linux");
    }

    #[test]
    fn test_scenario_three_level_middle_no_rules_match_all_true() {
        // Scenario: Three-level hierarchy, middle group has no rules with match_all_nodes=true
        // - Root: "Linux" with rule kernel=Linux
        // - Child: "All Linux" with NO rules, match_all_nodes=true
        // - Grandchild: "Specific" with rule app=nginx
        //
        // Middle has match_all_nodes=true, so it matches all Linux nodes
        // Grandchild is then evaluated and matches if app=nginx

        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child = NodeGroup {
            id: child_id,
            name: "All Linux".to_string(),
            parent_id: Some(root_id),
            match_all_nodes: true, // No rules, but match_all_nodes=true
            rules: vec![],
            ..Default::default()
        };

        let grandchild = NodeGroup {
            id: Uuid::new_v4(),
            name: "Nginx".to_string(),
            parent_id: Some(child_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "app".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("nginx"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child, grandchild]);

        // Node: Linux + nginx -> all three
        let result = service.classify(
            "nginx.example.com",
            &serde_json::json!({"kernel": "Linux", "app": "nginx"}),
        );
        assert_eq!(result.groups.len(), 3, "Should match all three");

        // Node: Linux + apache -> root and middle only
        let result = service.classify(
            "apache.example.com",
            &serde_json::json!({"kernel": "Linux", "app": "apache"}),
        );
        assert_eq!(result.groups.len(), 2, "Should match root and middle");
        assert_eq!(result.groups[0].name, "Linux");
        assert_eq!(result.groups[1].name, "All Linux");
    }

    #[test]
    fn test_scenario_sibling_groups_independent() {
        // Scenario: Sibling groups should be evaluated independently
        // - Root: "Linux" with rule kernel=Linux
        // - Child A: "Web" with rule role=webserver
        // - Child B: "DB" with rule role=dbserver
        //
        // A node can match root + one child, or root + both children if it matches both

        let root_id = Uuid::new_v4();

        let root = NodeGroup {
            id: root_id,
            name: "Linux".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "kernel".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("Linux"),
            }],
            ..Default::default()
        };

        let child_a = NodeGroup {
            id: Uuid::new_v4(),
            name: "Web".to_string(),
            parent_id: Some(root_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("webserver"),
            }],
            ..Default::default()
        };

        let child_b = NodeGroup {
            id: Uuid::new_v4(),
            name: "DB".to_string(),
            parent_id: Some(root_id),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("dbserver"),
            }],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![root, child_a, child_b]);

        // Node: Linux + webserver -> root + Web
        let result = service.classify(
            "web.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "webserver"}),
        );
        assert_eq!(result.groups.len(), 2);

        // Node: Linux + dbserver -> root + DB
        let result = service.classify(
            "db.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "dbserver"}),
        );
        assert_eq!(result.groups.len(), 2);

        // Node: Linux only -> root only
        let result = service.classify(
            "linux.example.com",
            &serde_json::json!({"kernel": "Linux", "role": "other"}),
        );
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].name, "Linux");
    }

    #[test]
    fn test_scenario_pinned_node_bypasses_rules() {
        // Scenario: Pinned nodes should match regardless of rules
        // - Group: "Special" with rule role=special, pinned_nodes=["pinned.example.com"]
        // - Node A (pinned): should match even without matching facts
        // - Node B (not pinned, matches rule): should match
        // - Node C (not pinned, doesn't match rule): should not match

        let group = NodeGroup {
            id: Uuid::new_v4(),
            name: "Special".to_string(),
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "role".to_string(),
                operator: RuleOperator::Equals,
                value: serde_json::json!("special"),
            }],
            pinned_nodes: vec!["pinned.example.com".to_string()],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);

        // Node A: pinned -> matches even with wrong facts
        let result = service.classify("pinned.example.com", &serde_json::json!({"role": "other"}));
        assert_eq!(result.groups.len(), 1, "Pinned node should match");
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);

        // Node B: not pinned but matches rule
        let result = service.classify("special.example.com", &serde_json::json!({"role": "special"}));
        assert_eq!(result.groups.len(), 1, "Rule-matching node should match");
        assert_eq!(result.groups[0].match_type, MatchType::Rules);

        // Node C: not pinned, doesn't match rule
        let result = service.classify("other.example.com", &serde_json::json!({"role": "other"}));
        assert_eq!(result.groups.len(), 0, "Non-matching node should not match");
    }
}
