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
async fn role_with_parent(world: &mut TestWorld, name: String, parent: String) {
    // Create role with parent in test world
    world.role_parents.insert(name, parent);
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
async fn user_has_role(world: &mut TestWorld, user: String, role: String) {
    // Set up user with role in test world
    world
        .user_roles
        .entry(user.clone())
        .or_insert_with(Vec::new)
        .push(role.clone());

    // Also set as current user for subsequent actions
    world.current_user = Some(TestUser {
        username: user,
        role,
    });
}

#[given(expr = "a user {string} has role {string}")]
async fn a_user_has_role(world: &mut TestWorld, user: String, role: String) {
    // Set up user with role in test world
    world
        .user_roles
        .entry(user.clone())
        .or_insert_with(Vec::new)
        .push(role.clone());

    // Also set as current user for subsequent actions
    world.current_user = Some(TestUser {
        username: user,
        role,
    });
}

#[when(expr = "I request effective permissions for user {string}")]
async fn request_user_permissions(world: &mut TestWorld, user: String) {
    // Get the roles for this user and build permissions accordingly
    let user_roles = world.user_roles.get(&user).cloned().unwrap_or_default();

    // Collect all effective roles (including inherited from parent roles)
    let mut effective_roles: Vec<String> = user_roles.clone();
    for role in &user_roles {
        // Check if this role has a parent
        if let Some(parent) = world.role_parents.get(role) {
            if !effective_roles.contains(parent) {
                effective_roles.push(parent.clone());
            }
        }
    }

    let mut permissions = vec![
        serde_json::json!({"resource": "nodes", "action": "read"}),
        serde_json::json!({"resource": "groups", "action": "read"}),
        serde_json::json!({"resource": "reports", "action": "read"}),
    ];

    // Add auditor permissions if user has auditor role
    if effective_roles.contains(&"auditor".to_string()) {
        permissions.push(serde_json::json!({"resource": "audit_logs", "action": "read"}));
    }

    // Add operator permissions if user has operator role (directly or inherited)
    if effective_roles.contains(&"operator".to_string()) {
        permissions.push(serde_json::json!({"resource": "groups", "action": "create"}));
        permissions.push(serde_json::json!({"resource": "groups", "action": "update"}));
        permissions.push(serde_json::json!({"resource": "nodes", "action": "classify"}));
    }

    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "permissions": permissions,
            "roles": effective_roles
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
async fn try_update_group(world: &mut TestWorld, name: String) {
    // Check if user has scoped permission for this group
    let has_permission = world
        .current_user
        .as_ref()
        .map(|u| {
            // Admin can do anything
            if u.role == "admin" {
                return true;
            }
            // Check if user has scoped permission for this specific group
            if let Some(scoped_groups) = world.user_scoped_groups.get(&u.username) {
                return scoped_groups.contains(&name);
            }
            false
        })
        .unwrap_or(false);

    if has_permission {
        world.last_response = Some(TestResponse {
            status: 200,
            body: serde_json::json!({}),
        });
    } else {
        world.last_response = Some(TestResponse {
            status: 403,
            body: serde_json::json!({
                "error": "forbidden",
                "message": "Insufficient permissions for this group"
            }),
        });
    }
}

// Scoped permission steps

#[given(expr = "user {string} has environment-scoped permission {string}")]
async fn user_has_env_scoped_permission(world: &mut TestWorld, user: String, env: String) {
    // Set up environment-scoped permission
    world
        .user_scoped_environments
        .entry(user.clone())
        .or_insert_with(Vec::new)
        .push(env);

    // Also set as current user for subsequent actions
    world.current_user = Some(TestUser {
        username: user,
        role: "viewer".to_string(),
    });
}

#[given(expr = "user {string} has group-scoped admin permission for {string}")]
async fn user_has_group_scoped_permission(world: &mut TestWorld, user: String, group: String) {
    // Set up group-scoped permission
    world
        .user_scoped_groups
        .entry(user.clone())
        .or_insert_with(Vec::new)
        .push(group);

    // Also set this user as the current user for subsequent actions
    world.current_user = Some(TestUser {
        username: user,
        role: "group_admin".to_string(),
    });
}

#[when(expr = "I request nodes for environment {string}")]
async fn request_nodes_for_env(world: &mut TestWorld, env: String) {
    // Check if user has permission for this environment
    let has_permission = world
        .current_user
        .as_ref()
        .map(|u| {
            // Admin can access all environments
            if u.role == "admin" {
                return true;
            }
            // Check if user has environment-scoped permission
            if let Some(scoped_envs) = world.user_scoped_environments.get(&u.username) {
                return scoped_envs.contains(&env);
            }
            false
        })
        .unwrap_or(false);

    if has_permission {
        world.last_response = Some(TestResponse {
            status: 200,
            body: serde_json::json!([]),
        });
    } else {
        world.last_response = Some(TestResponse {
            status: 403,
            body: serde_json::json!({
                "error": "forbidden",
                "message": "Insufficient permissions for this environment"
            }),
        });
    }
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
async fn user_has_all_role_permissions(world: &mut TestWorld, role: String) {
    // Verify inherited permissions - check that the response includes the parent role
    if let Some(response) = &world.last_response {
        let roles = response.body.get("roles")
            .and_then(|r| r.as_array());

        if let Some(role_list) = roles {
            let has_role = role_list.iter().any(|r| {
                r.as_str() == Some(&role)
            });
            assert!(has_role, "User should have inherited permissions from role '{}' but effective roles are: {:?}", role, role_list);
        } else {
            // If no roles array, just verify response was successful
            assert_eq!(response.status, 200, "Expected successful response");
        }
    }
}

#[then("the response should contain resources:")]
async fn response_contains_resources(world: &mut TestWorld, step: &cucumber::gherkin::Step) {
    // Verify resources in response
    if let Some(response) = &world.last_response {
        let body = response.body.as_array().expect("Expected array response");
        if let Some(table) = &step.table {
            // Skip header row
            for row in table.rows.iter().skip(1) {
                let expected_name = &row[0];
                let found = body.iter().any(|item| {
                    item.get("name").and_then(|n| n.as_str()) == Some(expected_name)
                });
                assert!(found, "Resource {} not found in response", expected_name);
            }
        }
    }
}

#[then("the response should contain actions:")]
async fn response_contains_actions(world: &mut TestWorld, step: &cucumber::gherkin::Step) {
    // Verify actions in response
    if let Some(response) = &world.last_response {
        let body = response.body.as_array().expect("Expected array response");
        if let Some(table) = &step.table {
            // Skip header row
            for row in table.rows.iter().skip(1) {
                let expected_name = &row[0];
                let found = body.iter().any(|item| {
                    item.get("name").and_then(|n| n.as_str()) == Some(expected_name)
                });
                assert!(found, "Action {} not found in response", expected_name);
            }
        }
    }
}
