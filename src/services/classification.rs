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
    pub fn classify(&self, certname: &str, facts: &serde_json::Value) -> ClassificationResult {
        let mut matched_groups: Vec<GroupMatch> = vec![];
        let mut all_classes: Vec<String> = vec![];
        let mut all_parameters = serde_json::json!({});
        let mut all_variables = serde_json::json!({});
        let mut environment: Option<String> = None;

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
            let mut matched = false;
            let mut matched_rules = vec![];
            let match_type = if inherited {
                matched = true;
                MatchType::Inherited
            } else if group.pinned_nodes.contains(&certname.to_string()) {
                matched = true;
                MatchType::Pinned
            } else {
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

                all_classes.extend(group.classes.clone());
                merge_parameters(&mut all_parameters, &group.parameters);
                merge_parameters(&mut all_variables, &group.variables);

                if environment.is_none() {
                    environment = group.environment.clone();
                }

                // Enqueue children as inherited matches
                if let Some(children) = children_map.get(&Some(group.id)) {
                    for child in children {
                        queue.push_back((*child, true));
                    }
                }
            }
        }

        // Remove duplicate classes
        all_classes.sort();
        all_classes.dedup();

        ClassificationResult {
            certname: certname.to_string(),
            groups: matched_groups,
            classes: all_classes,
            parameters: all_parameters,
            variables: all_variables,
            environment,
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
            Some(value) => match_value(value, &rule.operator, &rule.value),
            None => false,
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
fn merge_parameters(target: &mut serde_json::Value, source: &serde_json::Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source_obj {
            target_obj.insert(key.clone(), value.clone());
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
            classes: vec!["profile::webserver".to_string()],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![group]);
        let facts = serde_json::json!({});

        let result = service.classify("web1.example.com", &facts);

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        assert!(result.classes.contains(&"profile::webserver".to_string()));
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
            classes: vec!["profile::base".to_string()],
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
            classes: vec!["class_parent".to_string()],
            parameters: serde_json::json!({"p": "root"}),
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            classes: vec!["class_child".to_string()],
            parameters: serde_json::json!({"p": "child", "child": true}),
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
        assert_eq!(
            result.classes,
            vec!["class_child".to_string(), "class_parent".to_string()]
        );
        assert_eq!(result.parameters["p"], serde_json::json!("child"));
        assert_eq!(result.parameters["child"], serde_json::json!(true));
    }

    #[test]
    fn test_classify_pinned_inherits_children() {
        let parent_id = Uuid::new_v4();
        let parent = NodeGroup {
            id: parent_id,
            name: "parent".to_string(),
            pinned_nodes: vec!["web1.example.com".to_string()],
            classes: vec!["class_parent".to_string()],
            ..Default::default()
        };

        let child = NodeGroup {
            id: Uuid::new_v4(),
            name: "child".to_string(),
            parent_id: Some(parent_id),
            classes: vec!["class_child".to_string()],
            ..Default::default()
        };

        let service = ClassificationService::new(vec![parent, child]);
        let facts = serde_json::json!({});

        let result = service.classify("web1.example.com", &facts);

        assert_eq!(result.groups.len(), 2);
        assert_eq!(result.groups[0].match_type, MatchType::Pinned);
        assert_eq!(result.groups[1].match_type, MatchType::Inherited);
        assert!(result.classes.contains(&"class_parent".to_string()));
        assert!(result.classes.contains(&"class_child".to_string()));
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
            classes: vec!["class_any".to_string()],
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
        assert!(result.classes.contains(&"class_any".to_string()));
    }
}
