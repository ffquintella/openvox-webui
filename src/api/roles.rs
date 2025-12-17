//! Role management API endpoints
//!
//! Provides CRUD operations for roles and permission management.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::{
    models::{CreatePermissionRequest, CreateRoleRequest, Permission, Role},
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
            get(get_role_permissions).put(update_role_permissions),
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
