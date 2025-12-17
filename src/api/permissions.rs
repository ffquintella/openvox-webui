//! Permissions API endpoints

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    models::{Action, CreatePermissionRequest, PermissionWithRole, Resource, Role},
    utils::error::ErrorResponse,
    AppState,
};

/// Create routes for permission endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_permissions))
        .route("/resources", get(list_resources))
        .route("/actions", get(list_actions))
        .route("/matrix", get(get_permission_matrix))
        .route("/bulk", post(bulk_update_permissions))
}

/// List all defined permissions
///
/// GET /api/v1/permissions
async fn list_permissions(
    State(state): State<AppState>,
) -> Result<Json<Vec<PermissionWithRole>>, (StatusCode, Json<ErrorResponse>)> {
    let permissions = state.rbac_db.get_all_permissions().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch permissions: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    Ok(Json(permissions))
}

/// List all available resources
async fn list_resources(State(_state): State<AppState>) -> Json<Vec<ResourceInfo>> {
    let resources: Vec<ResourceInfo> = Resource::all()
        .iter()
        .map(|r| ResourceInfo {
            name: r.as_str().to_string(),
            display_name: format_resource_name(r),
            description: get_resource_description(r),
            available_actions: get_resource_actions(r),
        })
        .collect();

    Json(resources)
}

/// List all available actions
async fn list_actions(State(_state): State<AppState>) -> Json<Vec<ActionInfo>> {
    let actions: Vec<ActionInfo> = Action::all()
        .iter()
        .map(|a| ActionInfo {
            name: a.as_str().to_string(),
            display_name: format_action_name(a),
            description: get_action_description(a),
        })
        .collect();

    Json(actions)
}

#[derive(serde::Serialize)]
pub struct ResourceInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub available_actions: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct ActionInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
}

fn format_resource_name(resource: &Resource) -> String {
    match resource {
        Resource::Nodes => "Nodes".to_string(),
        Resource::Groups => "Node Groups".to_string(),
        Resource::Reports => "Reports".to_string(),
        Resource::Facts => "Facts".to_string(),
        Resource::Users => "Users".to_string(),
        Resource::Roles => "Roles".to_string(),
        Resource::Settings => "Settings".to_string(),
        Resource::AuditLogs => "Audit Logs".to_string(),
        Resource::FacterTemplates => "Facter Templates".to_string(),
        Resource::ApiKeys => "API Keys".to_string(),
    }
}

fn get_resource_description(resource: &Resource) -> String {
    match resource {
        Resource::Nodes => "Infrastructure nodes from PuppetDB".to_string(),
        Resource::Groups => "Node classification groups".to_string(),
        Resource::Reports => "Puppet run reports".to_string(),
        Resource::Facts => "Node facts from Facter".to_string(),
        Resource::Users => "User accounts".to_string(),
        Resource::Roles => "RBAC roles".to_string(),
        Resource::Settings => "System configuration".to_string(),
        Resource::AuditLogs => "Activity audit logs".to_string(),
        Resource::FacterTemplates => "Templates for generating external facts".to_string(),
        Resource::ApiKeys => "API authentication keys".to_string(),
    }
}

fn get_resource_actions(resource: &Resource) -> Vec<String> {
    match resource {
        Resource::Nodes => vec!["read", "classify"],
        Resource::Groups => vec!["read", "create", "update", "delete", "admin"],
        Resource::Reports => vec!["read", "export"],
        Resource::Facts => vec!["read", "generate", "export"],
        Resource::Users => vec!["read", "create", "update", "delete", "admin"],
        Resource::Roles => vec!["read", "create", "update", "delete", "admin"],
        Resource::Settings => vec!["read", "update"],
        Resource::AuditLogs => vec!["read"],
        Resource::FacterTemplates => vec!["read", "create", "update", "delete"],
        Resource::ApiKeys => vec!["read", "create", "delete"],
    }
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn format_action_name(action: &Action) -> String {
    match action {
        Action::Read => "Read".to_string(),
        Action::Create => "Create".to_string(),
        Action::Update => "Update".to_string(),
        Action::Delete => "Delete".to_string(),
        Action::Admin => "Full Admin".to_string(),
        Action::Export => "Export".to_string(),
        Action::Classify => "Classify".to_string(),
        Action::Generate => "Generate".to_string(),
    }
}

fn get_action_description(action: &Action) -> String {
    match action {
        Action::Read => "View and list resources".to_string(),
        Action::Create => "Create new resources".to_string(),
        Action::Update => "Modify existing resources".to_string(),
        Action::Delete => "Remove resources".to_string(),
        Action::Admin => "Full administrative access including all actions".to_string(),
        Action::Export => "Export resource data".to_string(),
        Action::Classify => "Classify nodes into groups".to_string(),
        Action::Generate => "Generate derived data (e.g., facts)".to_string(),
    }
}

// =============================================================================
// Permission Matrix
// =============================================================================

/// Permission matrix showing all roles and their permissions per resource/action
#[derive(Debug, Serialize)]
pub struct PermissionMatrix {
    /// List of all roles
    pub roles: Vec<RoleInfo>,
    /// List of all resources with their available actions
    pub resources: Vec<ResourceWithActions>,
    /// Matrix data: role_id -> resource -> action -> granted
    pub matrix: HashMap<String, HashMap<String, HashMap<String, bool>>>,
}

/// Role info for matrix
#[derive(Debug, Serialize)]
pub struct RoleInfo {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub is_system: bool,
}

/// Resource with its available actions
#[derive(Debug, Serialize)]
pub struct ResourceWithActions {
    pub name: String,
    pub display_name: String,
    pub actions: Vec<String>,
}

/// Get permission matrix
///
/// GET /api/v1/permissions/matrix
async fn get_permission_matrix(
    State(state): State<AppState>,
) -> Result<Json<PermissionMatrix>, (StatusCode, Json<ErrorResponse>)> {
    // Get all roles
    let roles = state.rbac_db.get_all_roles().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch roles: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    // Build role info list
    let role_infos: Vec<RoleInfo> = roles
        .iter()
        .map(|r| RoleInfo {
            id: r.id,
            name: r.name.clone(),
            display_name: r.display_name.clone(),
            is_system: r.is_system,
        })
        .collect();

    // Build resources with actions
    let resources: Vec<ResourceWithActions> = Resource::all()
        .iter()
        .map(|r| ResourceWithActions {
            name: r.as_str().to_string(),
            display_name: format_resource_name(r),
            actions: get_resource_actions(r),
        })
        .collect();

    // Build the matrix
    let mut matrix: HashMap<String, HashMap<String, HashMap<String, bool>>> = HashMap::new();

    for role in &roles {
        let role_id = role.id.to_string();
        let mut role_permissions: HashMap<String, HashMap<String, bool>> = HashMap::new();

        // Initialize all resources/actions to false
        for resource in Resource::all() {
            let resource_name = resource.as_str().to_string();
            let mut actions_map: HashMap<String, bool> = HashMap::new();
            for action in get_resource_actions(&resource) {
                actions_map.insert(action, false);
            }
            role_permissions.insert(resource_name, actions_map);
        }

        // Mark granted permissions as true
        for perm in &role.permissions {
            let resource_name = perm.resource.as_str().to_string();
            let action_name = perm.action.as_str().to_string();

            if let Some(resource_map) = role_permissions.get_mut(&resource_name) {
                // Admin action grants all actions on this resource
                if perm.action == Action::Admin {
                    for (_, granted) in resource_map.iter_mut() {
                        *granted = true;
                    }
                } else if let Some(granted) = resource_map.get_mut(&action_name) {
                    *granted = true;
                }
            }
        }

        matrix.insert(role_id, role_permissions);
    }

    Ok(Json(PermissionMatrix {
        roles: role_infos,
        resources,
        matrix,
    }))
}

// =============================================================================
// Bulk Permission Operations
// =============================================================================

/// Bulk permission update request
#[derive(Debug, Deserialize)]
pub struct BulkPermissionRequest {
    /// Operations to perform
    pub operations: Vec<BulkOperation>,
}

/// A single bulk operation
#[derive(Debug, Deserialize)]
pub struct BulkOperation {
    /// Operation type
    pub op: BulkOperationType,
    /// Role ID to modify
    pub role_id: Uuid,
    /// Permission to add/remove (for add/remove operations)
    pub permission: Option<CreatePermissionRequest>,
    /// Permissions to set (for replace operation)
    pub permissions: Option<Vec<CreatePermissionRequest>>,
}

/// Bulk operation type
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BulkOperationType {
    /// Add a permission to a role
    Add,
    /// Remove a permission from a role
    Remove,
    /// Replace all permissions for a role
    Replace,
}

/// Bulk operation result
#[derive(Debug, Serialize)]
pub struct BulkPermissionResult {
    /// Total operations requested
    pub total: usize,
    /// Successful operations
    pub succeeded: usize,
    /// Failed operations
    pub failed: usize,
    /// Details of each operation result
    pub results: Vec<BulkOperationResult>,
}

/// Result of a single bulk operation
#[derive(Debug, Serialize)]
pub struct BulkOperationResult {
    /// Index of the operation (0-based)
    pub index: usize,
    /// Whether the operation succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Updated role if successful
    pub role: Option<Role>,
}

/// Perform bulk permission operations
///
/// POST /api/v1/permissions/bulk
async fn bulk_update_permissions(
    State(state): State<AppState>,
    Json(payload): Json<BulkPermissionRequest>,
) -> Result<Json<BulkPermissionResult>, (StatusCode, Json<ErrorResponse>)> {
    let total = payload.operations.len();
    let mut succeeded = 0;
    let mut failed = 0;
    let mut results = Vec::new();

    for (index, op) in payload.operations.into_iter().enumerate() {
        let result = match op.op {
            BulkOperationType::Add => {
                if let Some(perm) = op.permission {
                    match state.rbac_db.add_permission_to_role(&op.role_id, perm).await {
                        Ok(_) => {
                            let role = state.rbac_db.get_role(&op.role_id).await.ok().flatten();
                            BulkOperationResult {
                                index,
                                success: true,
                                error: None,
                                role,
                            }
                        }
                        Err(e) => BulkOperationResult {
                            index,
                            success: false,
                            error: Some(e.to_string()),
                            role: None,
                        },
                    }
                } else {
                    BulkOperationResult {
                        index,
                        success: false,
                        error: Some("Missing permission for add operation".to_string()),
                        role: None,
                    }
                }
            }
            BulkOperationType::Remove => {
                // For remove, we need to find and remove matching permission
                if let Some(perm_req) = op.permission {
                    match remove_matching_permission(&state, &op.role_id, &perm_req).await {
                        Ok(role) => BulkOperationResult {
                            index,
                            success: true,
                            error: None,
                            role: Some(role),
                        },
                        Err(e) => BulkOperationResult {
                            index,
                            success: false,
                            error: Some(e),
                            role: None,
                        },
                    }
                } else {
                    BulkOperationResult {
                        index,
                        success: false,
                        error: Some("Missing permission for remove operation".to_string()),
                        role: None,
                    }
                }
            }
            BulkOperationType::Replace => {
                if let Some(perms) = op.permissions {
                    match state.rbac_db.set_role_permissions(&op.role_id, perms).await {
                        Ok(_) => {
                            let role = state.rbac_db.get_role(&op.role_id).await.ok().flatten();
                            BulkOperationResult {
                                index,
                                success: true,
                                error: None,
                                role,
                            }
                        }
                        Err(e) => BulkOperationResult {
                            index,
                            success: false,
                            error: Some(e.to_string()),
                            role: None,
                        },
                    }
                } else {
                    BulkOperationResult {
                        index,
                        success: false,
                        error: Some("Missing permissions for replace operation".to_string()),
                        role: None,
                    }
                }
            }
        };

        if result.success {
            succeeded += 1;
        } else {
            failed += 1;
        }
        results.push(result);
    }

    Ok(Json(BulkPermissionResult {
        total,
        succeeded,
        failed,
        results,
    }))
}

/// Helper to remove a matching permission from a role
async fn remove_matching_permission(
    state: &AppState,
    role_id: &Uuid,
    perm_req: &CreatePermissionRequest,
) -> Result<Role, String> {
    // Get current role
    let role = state
        .rbac_db
        .get_role(role_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Role not found".to_string())?;

    // Find matching permission
    let matching_perm = role.permissions.iter().find(|p| {
        p.resource == perm_req.resource
            && p.action == perm_req.action
            && (perm_req.scope.is_none() || Some(&p.scope) == perm_req.scope.as_ref())
    });

    if let Some(perm) = matching_perm {
        state
            .rbac_db
            .remove_permission(&perm.id)
            .await
            .map_err(|e| e.to_string())?;

        // Return updated role
        state
            .rbac_db
            .get_role(role_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Role not found after update".to_string())
    } else {
        Err("No matching permission found".to_string())
    }
}
