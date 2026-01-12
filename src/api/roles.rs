//! Role management API endpoints
//!
//! Provides CRUD operations for roles and permission management.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    models::{
        Action, CreatePermissionRequest, CreateRoleRequest, Permission, PermissionConstraint,
        Resource, Role, Scope,
    },
    utils::error::ErrorResponse,
    AppState,
};

/// Create routes for role management
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_roles).post(create_role))
        .route("/{id}", get(get_role).put(update_role).delete(delete_role))
        .route(
            "/{id}/permissions",
            get(get_role_permissions)
                .post(add_permission_to_role)
                .put(update_role_permissions),
        )
        .route(
            "/{id}/permissions/{permission_id}",
            axum::routing::delete(remove_permission_from_role),
        )
        .route(
            "/{id}/group-permissions",
            get(get_group_permissions).post(add_group_permission),
        )
        .route(
            "/{id}/group-permissions/{group_id}",
            axum::routing::delete(remove_group_permission),
        )
}

/// List all roles
///
/// GET /api/v1/roles
async fn list_roles(
    State(state): State<AppState>,
) -> Result<Json<Vec<Role>>, (StatusCode, Json<ErrorResponse>)> {
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

    Ok(Json(roles))
}

/// Create a new role
///
/// POST /api/v1/roles
async fn create_role(
    State(state): State<AppState>,
    Json(payload): Json<CreateRoleRequest>,
) -> Result<(StatusCode, Json<Role>), (StatusCode, Json<ErrorResponse>)> {
    // Validate input
    if payload.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Role name is required".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    if payload.display_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Role display name is required".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    let role = state.rbac_db.create_role(payload).await.map_err(|e| {
        let message = e.to_string();
        if message.contains("already exists") {
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "conflict".to_string(),
                    message,
                    details: None,
                    code: None,
                }),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to create role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        }
    })?;

    Ok((StatusCode::CREATED, Json(role)))
}

/// Get a specific role
///
/// GET /api/v1/roles/:id
async fn get_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Role>, (StatusCode, Json<ErrorResponse>)> {
    let role = state
        .rbac_db
        .get_role(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    Ok(Json(role))
}

/// Update a role
///
/// PUT /api/v1/roles/:id
async fn update_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CreateRoleRequest>,
) -> Result<Json<Role>, (StatusCode, Json<ErrorResponse>)> {
    let role = state.rbac_db.update_role(&id, payload).await.map_err(|e| {
        let message = e.to_string();
        if message.contains("not found") {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        } else if message.contains("Cannot modify system") {
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "forbidden".to_string(),
                    message,
                    details: None,
                    code: None,
                }),
            )
        } else if message.contains("already exists") {
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "conflict".to_string(),
                    message,
                    details: None,
                    code: None,
                }),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to update role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        }
    })?;

    Ok(Json(role))
}

/// Delete a role
///
/// DELETE /api/v1/roles/:id
async fn delete_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.rbac_db.delete_role(&id).await.map_err(|e| {
        let message = e.to_string();
        if message.contains("not found") {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        } else if message.contains("Cannot delete system") {
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "forbidden".to_string(),
                    message,
                    details: None,
                    code: None,
                }),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to delete role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        }
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get permissions for a role
///
/// GET /api/v1/roles/:id/permissions
async fn get_role_permissions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Permission>>, (StatusCode, Json<ErrorResponse>)> {
    let role = state
        .rbac_db
        .get_role(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    Ok(Json(role.permissions))
}

/// Update permissions for a role (replace all)
///
/// PUT /api/v1/roles/:id/permissions
async fn update_role_permissions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<Vec<CreatePermissionRequest>>,
) -> Result<Json<Vec<Permission>>, (StatusCode, Json<ErrorResponse>)> {
    let permissions = state
        .rbac_db
        .set_role_permissions(&id, payload)
        .await
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "not_found".to_string(),
                        message: "Role not found".to_string(),
                        details: None,
                        code: None,
                    }),
                )
            } else if message.contains("system role") {
                (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: "forbidden".to_string(),
                        message,
                        details: None,
                        code: None,
                    }),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        message: format!("Failed to update permissions: {}", e),
                        details: None,
                        code: None,
                    }),
                )
            }
        })?;

    Ok(Json(permissions))
}

/// Add a single permission to a role
///
/// POST /api/v1/roles/:id/permissions
async fn add_permission_to_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CreatePermissionRequest>,
) -> Result<Json<Role>, (StatusCode, Json<ErrorResponse>)> {
    // Get the current role
    let role = state
        .rbac_db
        .get_role(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Don't allow modifying system roles' base permissions
    if role.is_system {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Cannot modify permissions of system roles".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Build the new permissions list by adding the new permission
    let mut new_permissions: Vec<CreatePermissionRequest> = role
        .permissions
        .iter()
        .map(|p| CreatePermissionRequest {
            resource: p.resource.clone(),
            action: p.action.clone(),
            scope: Some(p.scope.clone()),
            constraint: p.constraint.clone(),
        })
        .collect();

    // Check if permission already exists
    let already_exists = new_permissions.iter().any(|p| {
        p.resource == payload.resource
            && p.action == payload.action
            && p.scope == payload.scope
    });

    if already_exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "conflict".to_string(),
                message: "Permission already exists for this role".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    new_permissions.push(payload);

    // Update all permissions
    state
        .rbac_db
        .set_role_permissions(&id, new_permissions)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to add permission: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Fetch and return the updated role
    let updated_role = state
        .rbac_db
        .get_role(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch updated role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found after update".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    Ok(Json(updated_role))
}

/// Remove a permission from a role
///
/// DELETE /api/v1/roles/:id/permissions/:permission_id
async fn remove_permission_from_role(
    State(state): State<AppState>,
    Path((role_id, permission_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Get the current role
    let role = state
        .rbac_db
        .get_role(&role_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Don't allow modifying system roles' permissions
    if role.is_system {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Cannot modify permissions of system roles".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Filter out the permission to remove
    let new_permissions: Vec<CreatePermissionRequest> = role
        .permissions
        .iter()
        .filter(|p| p.id != permission_id)
        .map(|p| CreatePermissionRequest {
            resource: p.resource.clone(),
            action: p.action.clone(),
            scope: Some(p.scope.clone()),
            constraint: p.constraint.clone(),
        })
        .collect();

    // Check if we actually removed anything
    if new_permissions.len() == role.permissions.len() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "Permission not found in this role".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Update permissions
    state
        .rbac_db
        .set_role_permissions(&role_id, new_permissions)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to remove permission: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// Group-Scoped Permissions
// =============================================================================

/// Request to add group permission to a role
#[derive(Debug, Clone, Deserialize)]
pub struct AddGroupPermissionRequest {
    /// The group ID to grant permission for
    pub group_id: Uuid,
    /// The action to grant (update, delete, admin, etc.)
    pub action: Action,
}

/// Group permission info
#[derive(Debug, Clone, Serialize)]
pub struct GroupPermissionInfo {
    /// The permission ID
    pub permission_id: Uuid,
    /// The group ID this permission applies to
    pub group_id: Uuid,
    /// The action granted
    pub action: Action,
}

/// Get group-scoped permissions for a role
///
/// GET /api/v1/roles/:id/group-permissions
async fn get_group_permissions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<GroupPermissionInfo>>, (StatusCode, Json<ErrorResponse>)> {
    let role = state
        .rbac_db
        .get_role(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Filter permissions to only those with Group scope on the Groups resource
    let group_permissions: Vec<GroupPermissionInfo> = role
        .permissions
        .iter()
        .filter_map(|perm| {
            if perm.resource == Resource::Groups {
                match &perm.scope {
                    Scope::Group(group_id) => Some(GroupPermissionInfo {
                        permission_id: perm.id,
                        group_id: *group_id,
                        action: perm.action,
                    }),
                    Scope::Specific => {
                        // Check if there's a GroupIds constraint - handled separately below
                        None
                    }
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();

    // Also include permissions with Specific scope and GroupIds constraint
    let mut all_group_permissions = group_permissions;
    for perm in &role.permissions {
        if perm.resource == Resource::Groups && perm.scope == Scope::Specific {
            if let Some(PermissionConstraint::GroupIds(ids)) = &perm.constraint {
                for group_id in ids {
                    all_group_permissions.push(GroupPermissionInfo {
                        permission_id: perm.id,
                        group_id: *group_id,
                        action: perm.action,
                    });
                }
            }
        }
    }

    Ok(Json(all_group_permissions))
}

/// Add a group-scoped permission to a role
///
/// POST /api/v1/roles/:id/group-permissions
async fn add_group_permission(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<AddGroupPermissionRequest>,
) -> Result<(StatusCode, Json<Permission>), (StatusCode, Json<ErrorResponse>)> {
    // Verify the role exists
    let role = state
        .rbac_db
        .get_role(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Don't allow modifying system roles (except through direct permission API)
    if role.is_system {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Cannot modify permissions of system roles".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Create the permission request with Group scope
    let permission_request = CreatePermissionRequest {
        resource: Resource::Groups,
        action: payload.action,
        scope: Some(Scope::Group(payload.group_id)),
        constraint: None,
    };

    // Add the permission
    let permission = state
        .rbac_db
        .add_permission_to_role(&id, permission_request)
        .await
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("already exists") {
                (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: "conflict".to_string(),
                        message: "This group permission already exists for this role".to_string(),
                        details: None,
                        code: None,
                    }),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        message: format!("Failed to add permission: {}", e),
                        details: None,
                        code: None,
                    }),
                )
            }
        })?;

    Ok((StatusCode::CREATED, Json(permission)))
}

/// Remove a group-scoped permission from a role
///
/// DELETE /api/v1/roles/:id/group-permissions/:group_id
async fn remove_group_permission(
    State(state): State<AppState>,
    Path((role_id, group_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Verify the role exists
    let role = state
        .rbac_db
        .get_role(&role_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch role: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "Role not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Don't allow modifying system roles
    if role.is_system {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "forbidden".to_string(),
                message: "Cannot modify permissions of system roles".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Find permissions with Group scope matching the group_id
    let permissions_to_remove: Vec<Uuid> = role
        .permissions
        .iter()
        .filter_map(|perm| {
            if perm.resource == Resource::Groups {
                match &perm.scope {
                    Scope::Group(gid) if *gid == group_id => Some(perm.id),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();

    if permissions_to_remove.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "No group permission found for this group".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Remove all matching permissions
    for perm_id in permissions_to_remove {
        state.rbac_db.remove_permission(&perm_id).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to remove permission: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?;
    }

    Ok(StatusCode::NO_CONTENT)
}
