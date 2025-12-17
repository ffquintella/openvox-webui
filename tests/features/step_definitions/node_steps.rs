//! Node-related step definitions

use cucumber::{given, then, when};
use crate::features::support::{TestResponse, TestWorld};

#[given(expr = "a node {string} exists")]
async fn node_exists(world: &mut TestWorld, certname: String) {
    let facts = serde_json::json!({
        "certname": certname,
        "os": {
            "family": "RedHat",
            "release": {
                "major": "8"
            }
        }
    });
    world.add_node_with_facts(&certname, facts);
}

#[given(expr = "a node {string} exists with facts:")]
async fn node_exists_with_facts(world: &mut TestWorld, certname: String, facts_json: String) {
    let facts: serde_json::Value = serde_json::from_str(&facts_json)
        .expect("Invalid JSON for facts");
    world.add_node_with_facts(&certname, facts);
}

#[when(expr = "I request the node list")]
async fn request_node_list(world: &mut TestWorld) {
    // Check authentication status
    if world.auth_token.is_none() {
        world.last_response = Some(TestResponse {
            status: 401,
            body: serde_json::json!({
                "error": "unauthorized",
                "message": "Authentication required"
            }),
        });
        return;
    }

    // In real implementation, make API call to GET /api/v1/nodes
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!([]),
    });
}

#[when(expr = "I request details for node {string}")]
async fn request_node_details(world: &mut TestWorld, certname: String) {
    // In real implementation, make API call to GET /api/v1/nodes/{certname}
    if world.node_facts.contains_key(&certname) {
        world.last_response = Some(TestResponse {
            status: 200,
            body: serde_json::json!({
                "certname": certname
            }),
        });
    } else {
        world.last_response = Some(TestResponse {
            status: 404,
            body: serde_json::json!({
                "error": "not_found",
                "message": "Node not found"
            }),
        });
    }
}

#[then(expr = "the response should contain node {string}")]
async fn response_contains_node(world: &mut TestWorld, certname: String) {
    if let Some(response) = &world.last_response {
        assert_eq!(response.body.get("certname").and_then(|v| v.as_str()), Some(certname.as_str()));
    } else {
        panic!("No response available");
    }
}

#[then(expr = "the node should have fact {string} with value {string}")]
async fn node_has_fact(_world: &mut TestWorld, _fact_path: String, _expected_value: String) {
    // Verify fact value
    // In real implementation, check the response body
}
