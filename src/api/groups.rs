//! Node group API endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    db::repository::GroupRepository,
    models::{
        AddPinnedNodeRequest, ClassificationRule, CreateGroupRequest, CreateRuleRequest, NodeGroup,
        UpdateGroupRequest,
    },
    utils::AppError,
    AppState,
};

/// Create routes for group endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_groups).post(create_group))
        .route("/{id}", get(get_group).put(update_group).delete(delete_group))
        .route("/{id}/nodes", get(get_group_nodes))
        .route("/{id}/rules", get(get_group_rules).post(add_rule))
        .route("/{id}/rules/{rule_id}", delete(delete_rule))
        .route("/{id}/pinned", post(add_pinned_node))
        .route("/{id}/pinned/{certname}", delete(remove_pinned_node))
}

/// List all node groups
async fn list_groups(State(state): State<AppState>) -> Result<Json<Vec<NodeGroup>>, AppError> {
    let repo = GroupRepository::new(&state.db);
    let groups = repo.get_all().await.map_err(|e| {
        tracing::error!("Failed to list groups: {}", e);
        AppError::internal("Failed to list groups")
    })?;
    Ok(Json(groups))
}

/// Create a new node group
async fn create_group(
    State(state): State<AppState>,
    Json(payload): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<NodeGroup>), AppError> {
    let repo = GroupRepository::new(&state.db);
    let group = repo.create(&payload).await.map_err(|e| {
        tracing::error!("Failed to create group: {}", e);
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::conflict("A group with this name already exists")
        } else {
            AppError::internal("Failed to create group")
        }
    })?;
    Ok((StatusCode::CREATED, Json(group)))
}

/// Get a specific node group
async fn get_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NodeGroup>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);
    let group = repo.get_by_id(uuid).await.map_err(|e| {
        tracing::error!("Failed to get group: {}", e);
        AppError::internal("Failed to get group")
    })?;

    match group {
        Some(g) => Ok(Json(g)),
        None => Err(AppError::not_found("Group not found")),
    }
}

/// Update a node group
async fn update_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateGroupRequest>,
) -> Result<Json<NodeGroup>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);
    let group = repo.update(uuid, &payload).await.map_err(|e| {
        tracing::error!("Failed to update group: {}", e);
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::conflict("A group with this name already exists")
        } else {
            AppError::internal("Failed to update group")
        }
    })?;

    match group {
        Some(g) => Ok(Json(g)),
        None => Err(AppError::not_found("Group not found")),
    }
}

/// Delete a node group
async fn delete_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);
    let deleted = repo.delete(uuid).await.map_err(|e| {
        tracing::error!("Failed to delete group: {}", e);
        AppError::internal("Failed to delete group")
    })?;

    Ok(Json(deleted))
}

/// Get nodes in a specific group
async fn get_group_nodes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);
    let nodes = repo.get_group_nodes(uuid).await.map_err(|e| {
        tracing::error!("Failed to get group nodes: {}", e);
        AppError::internal("Failed to get group nodes")
    })?;

    Ok(Json(nodes))
}

/// Get classification rules for a group
async fn get_group_rules(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ClassificationRule>>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);
    let rules = repo.get_rules(uuid).await.map_err(|e| {
        tracing::error!("Failed to get group rules: {}", e);
        AppError::internal("Failed to get group rules")
    })?;

    Ok(Json(rules))
}

/// Add a classification rule to a group
async fn add_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<ClassificationRule>), AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);

    // Verify group exists
    let group = repo.get_by_id(uuid).await.map_err(|e| {
        tracing::error!("Failed to check group: {}", e);
        AppError::internal("Failed to check group")
    })?;

    if group.is_none() {
        return Err(AppError::not_found("Group not found"));
    }

    let rule = repo.add_rule(uuid, &payload).await.map_err(|e| {
        tracing::error!("Failed to add rule: {}", e);
        AppError::internal("Failed to add rule")
    })?;

    Ok((StatusCode::CREATED, Json(rule)))
}

/// Delete a classification rule from a group
async fn delete_rule(
    State(state): State<AppState>,
    Path((group_id, rule_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let group_uuid =
        Uuid::parse_str(&group_id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    let rule_uuid =
        Uuid::parse_str(&rule_id).map_err(|_| AppError::bad_request("Invalid rule ID"))?;

    let repo = GroupRepository::new(&state.db);
    let deleted = repo.delete_rule(group_uuid, rule_uuid).await.map_err(|e| {
        tracing::error!("Failed to delete rule: {}", e);
        AppError::internal("Failed to delete rule")
    })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Rule not found"))
    }
}

/// Add a pinned node to a group
async fn add_pinned_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<AddPinnedNodeRequest>,
) -> Result<StatusCode, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);

    // Verify group exists
    let group = repo.get_by_id(uuid).await.map_err(|e| {
        tracing::error!("Failed to check group: {}", e);
        AppError::internal("Failed to check group")
    })?;

    if group.is_none() {
        return Err(AppError::not_found("Group not found"));
    }

    repo.add_pinned_node(uuid, &payload.certname)
        .await
        .map_err(|e| {
            tracing::error!("Failed to add pinned node: {}", e);
            AppError::internal("Failed to add pinned node")
        })?;

    Ok(StatusCode::CREATED)
}

/// Remove a pinned node from a group
async fn remove_pinned_node(
    State(state): State<AppState>,
    Path((group_id, certname)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let uuid = Uuid::parse_str(&group_id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    let repo = GroupRepository::new(&state.db);
    let removed = repo.remove_pinned_node(uuid, &certname).await.map_err(|e| {
        tracing::error!("Failed to remove pinned node: {}", e);
        AppError::internal("Failed to remove pinned node")
    })?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Pinned node not found"))
    }
}
