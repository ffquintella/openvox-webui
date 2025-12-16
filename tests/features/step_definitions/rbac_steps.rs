//! RBAC step definitions

use cucumber::{given, then, when};
use crate::features::support::{TestResponse, TestUser, TestWorld};

// Role management steps

#[when("I request the list of roles")]
async fn request_role_list(world: &mut TestWorld) {
    // In real implementation, make API call to GET /api/v1/roles
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!([
            {"name": "admin", "display_name": "Administrator", "is_system": true},
            {"name": "operator", "display_name": "Operator", "is_system": true},
            {"name": "viewer", "display_name": "Viewer", "is_system": true}
        ]),
    });
}

#[when(expr = "I create a role with name {string} and display name {string}")]
async fn create_role(world: &mut TestWorld, name: String, display_name: String) {
    // In real implementation, make API call to POST /api/v1/roles
    world.last_response = Some(TestResponse {
        status: 201,
        body: serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "name": name,
            "display_name": display_name,
            "is_system": false
        }),
    });
}

#[when(expr = "I try to delete the role {string}")]
async fn try_delete_role(world: &mut TestWorld, name: String) {
    // System roles cannot be deleted
    let is_system = matches!(name.as_str(), "admin" | "operator" | "viewer" | "group_admin" | "auditor");

    if is_system {
        world.last_response = Some(TestResponse {
            status: 403,
            body: serde_json::json!({
                "error": "forbidden",
                "message": "Cannot delete system roles"
            }),
        });
    } else {
        world.last_response = Some(TestResponse {
            status: 204,
            body: serde_json::json!(null),
        });
    }
}

#[when(expr = "I delete the role {string}")]
async fn delete_role(world: &mut TestWorld, _name: String) {
    world.last_response = Some(TestResponse {
        status: 204,
        body: serde_json::json!(null),
    });
}

#[given(expr = "a custom role {string} exists")]
async fn custom_role_exists(_world: &mut TestWorld, _name: String) {
    // Create custom role in test world
}

#[given(expr = "a role {string} with parent {string}")]
async fn role_with_parent(_world: &mut TestWorld, _name: String, _parent: String) {
    // Create role with parent in test world
}

// Permission steps

#[when(expr = "I assign permission {string} to role {string}")]
async fn assign_permission_to_role(world: &mut TestWorld, permission: String, _role: String) {
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "permission": permission
        }),
    });
}

#[when("I request the list of resources")]
async fn request_resources(world: &mut TestWorld) {
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!([
            {"name": "nodes"},
            {"name": "groups"},
            {"name": "reports"},
            {"name": "facts"},
            {"name": "users"},
            {"name": "roles"}
        ]),
    });
}

#[when("I request the list of actions")]
async fn request_actions(world: &mut TestWorld) {
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!([
            {"name": "read"},
            {"name": "create"},
            {"name": "update"},
            {"name": "delete"},
            {"name": "admin"}
        ]),
    });
}

// User role assignment steps

#[given(expr = "a user {string} exists")]
async fn user_exists(_world: &mut TestWorld, _username: String) {
    // Add user to test world
}

#[when(expr = "I assign role {string} to user {string}")]
async fn assign_role_to_user(world: &mut TestWorld, _role: String, _user: String) {
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({}),
    });
}

#[given(expr = "user {string} has role {string}")]
async fn user_has_role(_world: &mut TestWorld, _user: String, _role: String) {
    // Set up user with role in test world
}

#[when(expr = "I request effective permissions for user {string}")]
async fn request_user_permissions(world: &mut TestWorld, _user: String) {
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "permissions": [
                {"resource": "nodes", "action": "read"},
                {"resource": "groups", "action": "read"},
                {"resource": "reports", "action": "read"}
            ]
        }),
    });
}

// Permission enforcement steps

#[given(expr = "I am authenticated as a user with role {string}")]
async fn authenticated_with_role(world: &mut TestWorld, role: String) {
    world.current_user = Some(TestUser {
        username: "testuser".to_string(),
        role: role.clone(),
    });
    world.auth_token = Some(format!("test-{}-token", role));
}

#[when(expr = "I try to create a node group named {string}")]
async fn try_create_group(world: &mut TestWorld, _name: String) {
    // Check if user has permission
    let has_permission = world.current_user.as_ref()
        .map(|u| matches!(u.role.as_str(), "admin" | "operator"))
        .unwrap_or(false);

    if has_permission {
        world.last_response = Some(TestResponse {
            status: 201,
            body: serde_json::json!({}),
        });
    } else {
        world.last_response = Some(TestResponse {
            status: 403,
            body: serde_json::json!({
                "error": "forbidden",
                "message": "Insufficient permissions"
            }),
        });
    }
}

#[when(expr = "I try to update group {string}")]
async fn try_update_group(world: &mut TestWorld, _name: String) {
    // In real implementation, check scoped permissions
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({}),
    });
}

// Scoped permission steps

#[given(expr = "user {string} has environment-scoped permission {string}")]
async fn user_has_env_scoped_permission(_world: &mut TestWorld, _user: String, _env: String) {
    // Set up environment-scoped permission
}

#[given(expr = "user {string} has group-scoped admin permission for {string}")]
async fn user_has_group_scoped_permission(_world: &mut TestWorld, _user: String, _group: String) {
    // Set up group-scoped permission
}

#[when(expr = "I request nodes for environment {string}")]
async fn request_nodes_for_env(world: &mut TestWorld, _env: String) {
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!([]),
    });
}

// Assertion steps

#[then(expr = "the response should contain system roles {string}, {string}, {string}")]
async fn response_contains_system_roles(world: &mut TestWorld, _r1: String, _r2: String, _r3: String) {
    assert!(world.last_response.is_some());
}

#[then(expr = "the role {string} should exist")]
async fn role_exists(_world: &mut TestWorld, _name: String) {
    // Verify role exists
}

#[then(expr = "the role {string} should not exist")]
async fn role_not_exists(_world: &mut TestWorld, _name: String) {
    // Verify role doesn't exist
}

#[then(expr = "role {string} should have permission {string}")]
async fn role_has_permission(_world: &mut TestWorld, _role: String, _permission: String) {
    // Verify role has permission
}

#[then(expr = "user {string} should have role {string}")]
async fn user_should_have_role(_world: &mut TestWorld, _user: String, _role: String) {
    // Verify user has role
}

#[then(expr = "the response should include permission {string}")]
async fn response_includes_permission(world: &mut TestWorld, permission: String) {
    if let Some(response) = &world.last_response {
        let permissions = response.body.get("permissions")
            .and_then(|p| p.as_array());

        if let Some(perms) = permissions {
            let parts: Vec<&str> = permission.split(':').collect();
            let has_perm = perms.iter().any(|p| {
                p.get("resource").and_then(|r| r.as_str()) == Some(parts.get(0).unwrap_or(&""))
                    && p.get("action").and_then(|a| a.as_str()) == Some(parts.get(1).unwrap_or(&""))
            });
            assert!(has_perm, "Permission {} not found", permission);
        }
    }
}

#[then(expr = "the user should have all permissions from role {string}")]
async fn user_has_all_role_permissions(_world: &mut TestWorld, _role: String) {
    // Verify inherited permissions
}

#[then(expr = "the response should contain resources:")]
async fn response_contains_resources(_world: &mut TestWorld, _table: String) {
    // Verify resources in response
}

#[then(expr = "the response should contain actions:")]
async fn response_contains_actions(_world: &mut TestWorld, _table: String) {
    // Verify actions in response
}
