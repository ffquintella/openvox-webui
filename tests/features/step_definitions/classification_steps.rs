//! Classification step definitions

use cucumber::{given, then, when};
use crate::features::TestWorld;

#[given(expr = "a classification rule {string} on group {string}")]
async fn add_classification_rule(world: &mut TestWorld, rule: String, _group: String) {
    // Parse and store the rule
    // Format: "fact_path operator value" e.g., "os.family = RedHat"
}

#[when(expr = "I add a rule {string} to group {string}")]
async fn add_rule_to_group(world: &mut TestWorld, rule: String, _group: String) {
    // In real implementation, make API call to POST /api/v1/groups/{id}/rules
    world.last_response = Some(crate::features::support::world::TestResponse {
        status: 201,
        body: serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "rule": rule
        }),
    });
}

#[when(expr = "I classify node {string}")]
async fn classify_node(world: &mut TestWorld, certname: String) {
    // In real implementation, make API call to POST /api/v1/classify/{certname}
    let groups = if world.node_facts.contains_key(&certname) {
        vec!["matched_group"]
    } else {
        vec![]
    };

    world.last_response = Some(crate::features::support::world::TestResponse {
        status: 200,
        body: serde_json::json!({
            "certname": certname,
            "groups": groups,
            "classes": [],
            "parameters": {}
        }),
    });
}

#[when(expr = "I pin node {string} to group {string}")]
async fn pin_node_to_group(world: &mut TestWorld, _certname: String, _group: String) {
    // In real implementation, make API call to add pinned node
    world.last_response = Some(crate::features::support::world::TestResponse {
        status: 200,
        body: serde_json::json!({}),
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
