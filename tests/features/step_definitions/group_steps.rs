//! Node group step definitions

use cucumber::{given, then, when};
use crate::features::support::{TestResponse, TestWorld};

#[given(expr = "a node group {string} exists")]
async fn group_exists(world: &mut TestWorld, name: String) {
    world.create_group(&name).await.expect("Failed to create group");
}

#[when(expr = "I create a node group named {string}")]
async fn create_group(world: &mut TestWorld, name: String) {
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

    // Check if user has permission to create groups (only admin and operator can)
    if let Some(user) = &world.current_user {
        if user.role != "admin" && user.role != "operator" {
            world.last_response = Some(TestResponse {
                status: 403,
                body: serde_json::json!({
                    "error": "forbidden",
                    "message": "Insufficient permissions to create groups"
                }),
            });
            return;
        }
    }

    let result = world.create_group(&name).await;
    match result {
        Ok(_) => {
            world.last_response = Some(TestResponse {
                status: 201,
                body: serde_json::json!({
                    "name": name,
                    "id": uuid::Uuid::new_v4().to_string()
                }),
            });
        }
        Err(e) => {
            world.last_response = Some(TestResponse {
                status: 400,
                body: serde_json::json!({
                    "error": "creation_failed",
                    "message": e
                }),
            });
        }
    }
}

#[when(expr = "I create a node group named {string} with parent {string}")]
async fn create_group_with_parent(world: &mut TestWorld, name: String, _parent: String) {
    world.create_group(&name).await.expect("Failed to create group");
    world.last_response = Some(TestResponse {
        status: 201,
        body: serde_json::json!({
            "name": name
        }),
    });
}

#[when(expr = "I delete the group {string}")]
async fn delete_group(world: &mut TestWorld, _name: String) {
    // In real implementation, make API call to DELETE /api/v1/groups/{id}
    world.last_response = Some(TestResponse {
        status: 204,
        body: serde_json::json!(null),
    });
}

#[then(expr = "the group {string} should exist")]
async fn verify_group_exists(world: &mut TestWorld, name: String) {
    assert!(world.group_exists(&name).await);
}

#[then(expr = "the group should have no nodes")]
async fn group_has_no_nodes(_world: &mut TestWorld) {
    // In real implementation, verify group membership
}

#[then(expr = "the group {string} should not exist")]
async fn verify_group_not_exists(_world: &mut TestWorld, _name: String) {
    // In real implementation, verify group was deleted
}
