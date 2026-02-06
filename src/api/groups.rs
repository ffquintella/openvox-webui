//! Node group API endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    db::repository::GroupRepository,
    middleware::AuthUser,
    models::{
        Action, AddPinnedNodeRequest, ClassificationRule, CreateGroupRequest, CreateRuleRequest,
        NodeGroup, Resource, UpdateGroupRequest,
    },
    utils::AppError,
    AppState,
};

/// Create routes for group endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_groups).post(create_group))
        .route(
            "/{id}",
            get(get_group).put(update_group).delete(delete_group),
        )
        .route("/{id}/nodes", get(get_group_nodes))
        .route("/{id}/rules", get(get_group_rules).post(add_rule))
        .route("/{id}/rules/{rule_id}", delete(delete_rule))
        .route("/{id}/pinned", post(add_pinned_node))
        .route("/{id}/pinned/{certname}", delete(remove_pinned_node))
}

#[derive(Debug, Deserialize, Default)]
struct OrgQuery {
    organization_id: Option<Uuid>,
}

fn resolve_org(auth_user: &AuthUser, requested: Option<Uuid>) -> Result<Uuid, AppError> {
    match requested {
        Some(org_id) if !auth_user.is_super_admin() => Err(AppError::forbidden(
            "organization_id can only be specified by super_admin",
        )),
        Some(org_id) => Ok(org_id),
        None => Ok(auth_user.organization_id),
    }
}

/// Check if user has permission to perform an action on a specific group
/// This supports group-scoped permissions where users can have edit access
/// to specific groups without having global group permissions.
async fn check_group_permission(
    state: &AppState,
    auth_user: &AuthUser,
    action: Action,
    group_id: Option<Uuid>,
) -> Result<(), AppError> {
    let check = state
        .rbac_db
        .check_permission(&auth_user.user_id(), Resource::Groups, action, group_id, None)
        .await
        .map_err(|e| AppError::internal(format!("Permission check failed: {}", e)))?;

    if check.allowed {
        Ok(())
    } else {
        Err(AppError::forbidden(
            &check
                .reason
                .unwrap_or_else(|| "No matching permission found".to_string()),
        ))
    }
}

/// List all node groups
async fn list_groups(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
) -> Result<Json<Vec<NodeGroup>>, AppError> {
    let org_id = resolve_org(&auth_user, query.organization_id)?;
    let repo = GroupRepository::new(&state.db);
    let groups = repo.get_all(org_id).await.map_err(|e| {
        tracing::error!("Failed to list groups: {}", e);
        AppError::internal("Failed to list groups")
    })?;
    Ok(Json(groups))
}

/// Create a new node group
async fn create_group(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Json(payload): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<NodeGroup>), AppError> {
    // Check create permission. If a parent group is provided, use it to satisfy
    // group-scoped permissions; otherwise require a global create permission.
    let permission_scope = payload.parent_id;
    check_group_permission(&state, &auth_user, Action::Create, permission_scope).await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;
    let repo = GroupRepository::new(&state.db);
    let group = repo.create(org_id, &payload).await.map_err(|e| {
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<NodeGroup>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);
    let group = repo.get_by_id(org_id, uuid).await.map_err(|e| {
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateGroupRequest>,
) -> Result<Json<NodeGroup>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    // Check update permission for this specific group
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);
    let group = repo.update(org_id, uuid, &payload).await.map_err(|e| {
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    // Check delete permission for this specific group
    check_group_permission(&state, &auth_user, Action::Delete, Some(uuid)).await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);
    let deleted = repo.delete(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to delete group: {}", e);
        AppError::internal("Failed to delete group")
    })?;

    Ok(Json(deleted))
}

/// Get nodes in a specific group (pinned + classified by rules)
async fn get_group_nodes(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);

    // Get the group to verify it exists
    let group = repo.get_by_id(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to get group: {}", e);
        AppError::internal("Failed to get group")
    })?;

    if group.is_none() {
        return Err(AppError::not_found("Group not found"));
    }

    // Start with pinned nodes
    let mut matched_nodes: Vec<String> = repo.get_pinned_nodes(uuid).await.map_err(|e| {
        tracing::error!("Failed to get pinned nodes: {}", e);
        AppError::internal("Failed to get pinned nodes")
    })?;

    // If PuppetDB is configured, also classify nodes by rules
    if let Some(ref puppetdb) = state.puppetdb {
        // Get all groups for classification
        let all_groups = repo.get_all(org_id).await.map_err(|e| {
            tracing::error!("Failed to get groups: {}", e);
            AppError::internal("Failed to get groups")
        })?;

        // Create classification service
        let classification_service =
            crate::services::classification::ClassificationService::new(all_groups);

        // Get all nodes from PuppetDB
        let nodes = puppetdb.get_nodes().await.map_err(|e| {
            tracing::error!("Failed to get nodes from PuppetDB: {}", e);
            AppError::internal("Failed to get nodes from PuppetDB")
        })?;

        // Classify each node and check if it matches the target group
        for node in nodes {
            // Skip if already in matched_nodes (pinned)
            if matched_nodes.contains(&node.certname) {
                continue;
            }

            // Get facts for the node
            let facts = match puppetdb.get_node_facts(&node.certname).await {
                Ok(facts) => {
                    // Convert facts Vec to JSON object
                    let mut facts_obj = serde_json::Map::new();
                    for fact in facts {
                        facts_obj.insert(fact.name, fact.value);
                    }
                    // Add certname as a pseudo-fact so rules can match against it
                    facts_obj.insert(
                        "clientcert".to_string(),
                        serde_json::Value::String(node.certname.clone()),
                    );
                    serde_json::Value::Object(facts_obj)
                }
                Err(e) => {
                    tracing::warn!("Failed to get facts for {}: {}", node.certname, e);
                    continue;
                }
            };

            // Classify the node
            let classification = classification_service.classify(&node.certname, &facts);

            // Check if this node was classified into the target group
            let matches_group = classification.groups.iter().any(|g| g.id == uuid);
            tracing::debug!(
                "Node '{}' classification for group {}: matched={}, matched_groups={:?}",
                node.certname,
                uuid,
                matches_group,
                classification.groups.iter().map(|g| (&g.name, &g.match_type)).collect::<Vec<_>>()
            );
            if matches_group {
                matched_nodes.push(node.certname);
            }
        }
    }

    // Remove duplicates (in case of edge cases)
    matched_nodes.sort();
    matched_nodes.dedup();

    Ok(Json(matched_nodes))
}

/// Get classification rules for a group
async fn get_group_rules(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ClassificationRule>>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);
    let group = repo.get_by_id(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to check group: {}", e);
        AppError::internal("Failed to check group")
    })?;
    if group.is_none() {
        return Err(AppError::not_found("Group not found"));
    }

    let rules = repo.get_rules(uuid).await.map_err(|e| {
        tracing::error!("Failed to get group rules: {}", e);
        AppError::internal("Failed to get group rules")
    })?;

    Ok(Json(rules))
}

/// Add a classification rule to a group
async fn add_rule(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
    Json(payload): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<ClassificationRule>), AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    // Check update permission for this specific group (adding rules is an update operation)
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);

    // Verify group exists
    let group = repo.get_by_id(org_id, uuid).await.map_err(|e| {
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path((group_id, rule_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let group_uuid =
        Uuid::parse_str(&group_id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    let rule_uuid =
        Uuid::parse_str(&rule_id).map_err(|_| AppError::bad_request("Invalid rule ID"))?;

    // Check update permission for this specific group (deleting rules is an update operation)
    check_group_permission(&state, &auth_user, Action::Update, Some(group_uuid)).await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);
    let group = repo.get_by_id(org_id, group_uuid).await.map_err(|e| {
        tracing::error!("Failed to check group: {}", e);
        AppError::internal("Failed to check group")
    })?;
    if group.is_none() {
        return Err(AppError::not_found("Group not found"));
    }

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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
    Json(payload): Json<AddPinnedNodeRequest>,
) -> Result<StatusCode, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    // Check update permission for this specific group (adding pinned nodes is an update operation)
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);

    // Verify group exists
    let group = repo.get_by_id(org_id, uuid).await.map_err(|e| {
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path((group_id, certname)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let uuid = Uuid::parse_str(&group_id).map_err(|_| AppError::bad_request("Invalid group ID"))?;

    // Check update permission for this specific group (removing pinned nodes is an update operation)
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);
    let group = repo.get_by_id(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to check group: {}", e);
        AppError::internal("Failed to check group")
    })?;
    if group.is_none() {
        return Err(AppError::not_found("Group not found"));
    }

    let removed = repo
        .remove_pinned_node(uuid, &certname)
        .await
        .map_err(|e| {
            tracing::error!("Failed to remove pinned node: {}", e);
            AppError::internal("Failed to remove pinned node")
        })?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Pinned node not found"))
    }
}
