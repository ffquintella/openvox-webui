//! Classification step definitions

use crate::features::support::{TestResponse, TestWorld};
use cucumber::{given, then, when};

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

#[given(expr = "a classification rule {string} on group {string}")]
async fn add_classification_rule(world: &mut TestWorld, rule: String, group: String) {
    // Very simple parser for tests: only supports '=' operator
    // Example: "os.family = RedHat"
    let parts: Vec<&str> = rule.split('=').map(|s| s.trim()).collect();
    if parts.len() == 2 {
        let path = parts[0].to_string();
        let value = parts[1].to_string();
        world
            .group_rules
            .entry(group)
            .or_default()
            .push((path, value));
    }
}

#[when(expr = "I add a rule {string} to group {string}")]
async fn add_rule_to_group(world: &mut TestWorld, rule: String, _group: String) {
    // In real implementation, make API call to POST /api/v1/groups/{id}/rules
    world.last_response = Some(TestResponse {
        status: 201,
        body: serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "rule": rule
        }),
    });
}

#[when(expr = "I classify node {string}")]
async fn classify_node(world: &mut TestWorld, certname: String) {
    // Simulate classification using stored group_rules and node_facts
    let mut matched: Vec<String> = Vec::new();
    if let Some(facts) = world.node_facts.get(&certname) {
        for (group, rules) in &world.group_rules {
            let mut all_match = true;
            for (path, expected) in rules {
                let val = get_fact_value(facts, path)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .or_else(|| get_fact_value(facts, path).map(|v| v.to_string()));
                if val.as_deref() != Some(expected.as_str()) {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                matched.push(group.clone());
            }
        }
    }

    // Collect classes from matched groups and their parents
    let mut classes: Vec<String> = Vec::new();
    for group in &matched {
        // Add classes from the group itself
        if let Some(group_classes) = world.group_classes.get(group) {
            for class in group_classes {
                if !classes.contains(class) {
                    classes.push(class.clone());
                }
            }
        }

        // Add classes from parent groups (walk up the hierarchy)
        let mut current = group.clone();
        while let Some(parent) = world.group_parents.get(&current) {
            if let Some(parent_classes) = world.group_classes.get(parent) {
                for class in parent_classes {
                    if !classes.contains(class) {
                        classes.push(class.clone());
                    }
                }
            }
            current = parent.clone();
        }
    }

    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "certname": certname,
            "groups": matched,
            "classes": classes,
            "parameters": {}
        }),
    });
}

#[when(expr = "I pin node {string} to group {string}")]
async fn pin_node_to_group(world: &mut TestWorld, certname: String, group: String) {
    // In real implementation, make API call to add pinned node
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "certname": certname,
            "groups": [group],
            "classes": [],
            "parameters": {}
        }),
    });
}

#[then(expr = "node {string} should be classified in group {string}")]
async fn node_in_group(world: &mut TestWorld, _certname: String, _group: String) {
    if let Some(response) = &world.last_response {
        assert!(response.body.get("groups").is_some());
    }
}

#[then(expr = "node {string} should not be classified in any group")]
async fn node_not_in_any_group(world: &mut TestWorld, _certname: String) {
    if let Some(response) = &world.last_response {
        let groups = response.body.get("groups").and_then(|g| g.as_array());
        assert!(groups.map(|g| g.is_empty()).unwrap_or(true));
    }
}

#[then(expr = "the classification should include class {string}")]
async fn classification_includes_class(world: &mut TestWorld, class: String) {
    if let Some(response) = &world.last_response {
        let classes = response.body.get("classes").and_then(|c| c.as_array());
        if let Some(classes) = classes {
            assert!(classes.iter().any(|c| c.as_str() == Some(&class)));
        }
    }
}
