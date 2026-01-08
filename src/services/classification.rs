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

        // BFS from root groups; children are only considered if parent matched
        let mut queue: VecDeque<(&NodeGroup, bool)> = VecDeque::new();
        if let Some(roots) = children_map.get(&None) {
            for g in roots {
                queue.push_back((*g, false));
            }
        }

        while let Some((group, inherited)) = queue.pop_front() {
            tracing::debug!(
                "Classifying node '{}' against group '{}' (id={}, inherited={})",
                certname,
                group.name,
                group.id,
                inherited
            );

            let mut matched = false;
            let mut matched_rules = vec![];
            let match_type = if inherited {
                matched = true;
                MatchType::Inherited
            } else if group.pinned_nodes.contains(&certname.to_string()) {
                // Pinned nodes ALWAYS match, regardless of environment
                // This allows environment assignment without bootstrap problems
                matched = true;
                MatchType::Pinned
            } else {
                // Check environment for rule-based matching
                // If the group has a specific environment set (not None, "*", "All", or "Any"),
                // only nodes in that environment should match via rules
                let environment_matches = match &group.environment {
                    None => true, // No environment restriction
                    Some(env) if env == "*" || env.to_lowercase() == "all" || env.to_lowercase() == "any" => true,
                    Some(env) => {
                        // Group has specific environment requirement
                        match &node_environment {
                            Some(node_env) => node_env == env,
                            None => false, // Node has no environment, cannot match specific requirement
                        }
                    }
                };

                if !environment_matches {
                    tracing::debug!(
                        "Node '{}' environment {:?} does not match group '{}' requirement {:?}, skipping rule evaluation",
                        certname,
                        node_environment,
                        group.name,
                        group.environment
                    );
                    continue; // Skip this group and its children
                }

                // Evaluate rules only when not inherited and not pinned
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
                matched_groups.push(GroupMatch {
                    id: group.id,
                    name: group.name.clone(),
                    match_type,
                    matched_rules,
                });

                // Deep merge classes (Puppet Enterprise format with per-class parameters)
                merge_classes(&mut all_classes, &group.classes);
                merge_parameters(&mut all_variables, &group.variables);

                // Environment priority: Pinned/Rules > Inherited
                // Non-inherited groups (pinned or rule-matched) can override inherited environments
                // Note: Wildcards ("*", "All", "Any") are treated as "no environment" (None)
                let group_env = match &group.environment {
                    Some(env) if env == "*" || env.to_lowercase() == "all" || env.to_lowercase() == "any" => None,
                    other => other.clone(),
                };

                match match_type {
                    MatchType::Pinned | MatchType::Rules => {
                        // Non-inherited match: always set environment if group has one
                        if group_env.is_some() {
                            environment = group_env;
                        }
                    }
                    MatchType::Inherited => {
                        // Inherited match: only set if no environment is set yet
                        if environment.is_none() {
                            environment = group_env;
                        }
                    }
                }

                // Enqueue children as inherited matches
                if let Some(children) = children_map.get(&Some(group.id)) {
                    for child in children {
                        queue.push_back((*child, true));
                    }
                }
            }
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
        assert_eq!(result.groups[1].match_type, MatchType::Inherited);
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
            classes: serde_json::json!({"class_child": {}}),
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({});

        let result = service.classify("web1.example.com", &facts);

        assert_eq!(result.groups.len(), 2);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        assert_eq!(result.groups[1].match_type, MatchType::Inherited);
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
}
