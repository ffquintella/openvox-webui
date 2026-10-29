//! Node group API endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use futures::StreamExt;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    db::repository::GroupRepository,
    middleware::AuthUser,
    models::{
        Action, AddPinnedNodeRequest, ClassificationRule, CreateGroupRequest,
        CreateGroupUpdateScheduleRequest, CreateRuleRequest, GroupBehavior, GroupKind,
        GroupUpdateSchedule, NodeGroup, Resource, RuleMatchType, UpdateGroupRequest,
        UpdateGroupUpdateScheduleRequest, UpdateJob, GROUP_PRIORITY_RANGE,
    },
    services::classification::{build_classification_facts_with_plugins, ClassificationService},
    services::classification_expr,
    services::plugins::facts::UsableFactsSnapshot,
    services::puppetdb::PuppetDbClient,
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
        .route("/{id}/preview", post(preview_group_change))
        .route("/{id}/members", get(get_group_members))
        .route("/{id}/membership/refresh", post(refresh_membership))
        .route(
            "/{id}/membership/held",
            get(get_held_membership).delete(reject_held_membership),
        )
        .route(
            "/{id}/membership/approve-held",
            post(approve_held_membership),
        )
        .route("/{id}/rules", get(get_group_rules).post(add_rule))
        .route("/{id}/rules/{rule_id}", delete(delete_rule))
        .route("/{id}/pinned", post(add_pinned_node))
        .route("/{id}/pinned/{certname}", delete(remove_pinned_node))
        .route(
            "/{id}/update-schedules",
            get(list_update_schedules).post(create_update_schedule),
        )
        .route(
            "/{id}/update-schedules/{schedule_id}",
            get(get_update_schedule)
                .put(update_update_schedule)
                .delete(delete_update_schedule),
        )
        .route(
            "/{id}/update-schedules/{schedule_id}/run",
            post(run_update_schedule),
        )
}

#[derive(Debug, Deserialize, Default)]
struct OrgQuery {
    organization_id: Option<Uuid>,
}

fn resolve_org(auth_user: &AuthUser, requested: Option<Uuid>) -> Result<Uuid, AppError> {
    match requested {
        Some(_) if !auth_user.is_super_admin() => Err(AppError::forbidden(
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
        .check_permission(
            &auth_user.user_id(),
            Resource::Groups,
            action,
            group_id,
            None,
        )
        .await
        .map_err(|e| AppError::internal(format!("Permission check failed: {}", e)))?;

    if check.allowed {
        Ok(())
    } else {
        Err(AppError::forbidden(check.reason.unwrap_or_else(|| {
            "No matching permission found".to_string()
        })))
    }
}

/// Validate the M26 fields of a create, update or preview request. An empty
/// expression object means "no expression" and is accepted.
async fn validate_advanced(
    state: &AppState,
    priority: Option<i32>,
    expression: Option<&serde_json::Value>,
    behavior: Option<&GroupBehavior>,
) -> Result<(), AppError> {
    if let Some(p) = priority {
        if !GROUP_PRIORITY_RANGE.contains(&p) {
            return Err(AppError::bad_request(format!(
                "priority must be between {} and {}",
                GROUP_PRIORITY_RANGE.start(),
                GROUP_PRIORITY_RANGE.end()
            )));
        }
    }
    if let Some(e) = expression {
        if !e.is_null() && !e.as_object().is_some_and(|o| o.is_empty()) {
            classification_expr::validate(e).map_err(AppError::bad_request)?;
        }
    }
    if let Some(b) = behavior {
        // Owners and generated groups are set by the host, never by a request
        if b.owner_plugin_id.is_some() || b.generated.is_some() || b.kind == GroupKind::Generated {
            return Err(AppError::bad_request(
                "generated groups are created by group templates only",
            ));
        }
        b.validate().map_err(AppError::bad_request)?;
    }
    let expression =
        expression.filter(|e| !e.is_null() && !e.as_object().is_some_and(|o| o.is_empty()));
    if expression.is_some() || behavior.is_some() {
        state
            .plugin_manager
            .validate_group_extensions(expression, &behavior.cloned().unwrap_or_default())
            .map_err(AppError::bad_request)?;
    }
    Ok(())
}

/// Generated groups belong to their template instance: read-only here.
fn ensure_not_generated(group: &NodeGroup) -> Result<(), AppError> {
    if group.behavior.generated.is_some() {
        return Err(AppError::conflict(
            "This group is generated by a group template; change the template instance instead",
        ));
    }
    Ok(())
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
    validate_advanced(
        &state,
        payload.priority,
        payload.expression.as_ref(),
        payload.behavior.as_ref(),
    )
    .await?;

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
    validate_advanced(
        &state,
        payload.priority,
        payload.expression.as_ref(),
        payload.behavior.as_ref(),
    )
    .await?;

    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = GroupRepository::new(&state.db);
    if let Some(existing) = repo.get_by_id(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to get group: {}", e);
        AppError::internal("Failed to get group")
    })? {
        ensure_not_generated(&existing)?;
    }
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
    if let Some(existing) = repo.get_by_id(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to get group: {}", e);
        AppError::internal("Failed to get group")
    })? {
        ensure_not_generated(&existing)?;
    }
    let deleted = repo.delete(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to delete group: {}", e);
        AppError::internal("Failed to delete group")
    })?;
    if deleted {
        state.plugin_manager.forget_group_membership(uuid).await;
    }

    Ok(Json(deleted))
}

/// Core group membership resolver: returns pinned nodes plus any nodes that
/// match the group's classification rules.
///
/// This does NOT verify the group exists (callers that need a 404 should check
/// separately). It is AppState-free so background services (e.g. the update
/// schedule scheduler) can reuse the exact same logic with just a DB pool and
/// an optional PuppetDB client.
pub(crate) async fn classify_group_members(
    repo: &GroupRepository<'_>,
    puppetdb: Option<&PuppetDbClient>,
    org_id: Uuid,
    group_id: Uuid,
) -> anyhow::Result<Vec<String>> {
    classify_group_members_with(repo, puppetdb, org_id, group_id, &Default::default()).await
}

/// Same as [`classify_group_members`], with the stored plugin facts enabled
/// for rules (M25) added to each node's facts.
pub(crate) async fn classify_group_members_with(
    repo: &GroupRepository<'_>,
    puppetdb: Option<&PuppetDbClient>,
    org_id: Uuid,
    group_id: Uuid,
    plugin_facts: &UsableFactsSnapshot,
) -> anyhow::Result<Vec<String>> {
    classify_group_members_full(repo, puppetdb, org_id, group_id, plugin_facts, None).await
}

/// Same as [`classify_group_members_with`]. With the plugin manager the plugin
/// functions, operators and merge strategies apply and dynamic groups use each
/// provider's stale policy (M27); without it dynamic groups use their last
/// materialized members as they are.
pub(crate) async fn classify_group_members_full(
    repo: &GroupRepository<'_>,
    puppetdb: Option<&PuppetDbClient>,
    org_id: Uuid,
    group_id: Uuid,
    plugin_facts: &UsableFactsSnapshot,
    manager: Option<&std::sync::Arc<crate::services::plugins::PluginManager>>,
) -> anyhow::Result<Vec<String>> {
    // Start with pinned nodes
    let mut matched_nodes: Vec<String> = repo.get_pinned_nodes(group_id).await?;

    // If PuppetDB is configured, also classify nodes by rules
    if let Some(puppetdb) = puppetdb {
        // Get all groups for classification
        let all_groups = repo.get_all(org_id).await?;

        // Create classification service
        let classification_service = match manager {
            Some(m) => m.classification_service(all_groups).await,
            None => {
                let members = crate::db::PluginClassificationRepository::new(repo.pool())
                    .all_members()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(id, rows)| {
                        (
                            id,
                            crate::models::DynamicMembers {
                                certnames: rows.into_iter().map(|(c, _)| c).collect(),
                                stale: false,
                            },
                        )
                    })
                    .collect();
                ClassificationService::new(all_groups).with_members(members)
            }
        };

        // Get all nodes from PuppetDB
        let nodes = puppetdb.get_nodes().await?;

        // Classify each node and check if it matches the target group
        for node in nodes {
            // Skip if already in matched_nodes (pinned)
            if matched_nodes.contains(&node.certname) {
                continue;
            }

            // Get facts for the node
            let facts = match puppetdb.get_node_facts(&node.certname).await {
                Ok(facts) => build_classification_facts_with_plugins(
                    facts,
                    &node.certname,
                    node.catalog_environment.as_deref(),
                    plugin_facts.for_node(&node.certname),
                    chrono::Utc::now(),
                ),
                Err(e) => {
                    tracing::warn!("Failed to get facts for {}: {}", node.certname, e);
                    continue;
                }
            };

            // Classify the node
            let classification = classification_service.classify(&node.certname, &facts);

            // Check if this node was classified into the target group
            let matches_group = classification.groups.iter().any(|g| g.id == group_id);
            tracing::debug!(
                "Node '{}' classification for group {}: matched={}, matched_groups={:?}",
                node.certname,
                group_id,
                matches_group,
                classification
                    .groups
                    .iter()
                    .map(|g| (&g.name, &g.match_type))
                    .collect::<Vec<_>>()
            );
            if matches_group {
                matched_nodes.push(node.certname);
            }
        }
    }

    // Remove duplicates (in case of edge cases)
    matched_nodes.sort();
    matched_nodes.dedup();

    Ok(matched_nodes)
}

/// Resolve the full set of certnames belonging to a group: pinned nodes plus
/// any nodes that match the group's classification rules (via PuppetDB).
///
/// This is the canonical group membership resolver for API request handlers. It
/// must be used anywhere a group needs to be expanded into its actual node
/// targets (UI listing, update jobs, scheduled runs) so that rule-matched nodes
/// are never dropped.
pub(crate) async fn resolve_group_member_certnames(
    state: &AppState,
    org_id: Uuid,
    group_id: Uuid,
) -> Result<Vec<String>, AppError> {
    let repo = GroupRepository::new(&state.db);

    // Get the group to verify it exists
    let group = repo.get_by_id(org_id, group_id).await.map_err(|e| {
        tracing::error!("Failed to get group: {}", e);
        AppError::internal("Failed to get group")
    })?;

    if group.is_none() {
        return Err(AppError::not_found("Group not found"));
    }

    let plugin_facts = state.plugin_manager.usable_facts_snapshot().await;
    classify_group_members_full(
        &repo,
        state.puppetdb.as_deref(),
        org_id,
        group_id,
        &plugin_facts,
        Some(&state.plugin_manager),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to resolve group members: {}", e);
        AppError::internal("Failed to resolve group members")
    })
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

    let matched_nodes = resolve_group_member_certnames(&state, org_id, uuid).await?;

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

// ── Update Schedule Endpoints ──────────────────────────────────────

async fn list_update_schedules(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<GroupUpdateSchedule>>, AppError> {
    check_group_permission(&state, &auth_user, Action::Read, None).await?;

    let repo = state.inventory_repository();
    let schedules = repo.list_group_update_schedules(&id).await.map_err(|e| {
        tracing::error!("Failed to list update schedules: {}", e);
        AppError::internal("Failed to list update schedules")
    })?;

    Ok(Json(schedules))
}

async fn create_update_schedule(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<CreateGroupUpdateScheduleRequest>,
) -> Result<(StatusCode, Json<GroupUpdateSchedule>), AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let repo = state.inventory_repository();
    let schedule = repo
        .create_group_update_schedule(&id, &payload, &auth_user.username)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create update schedule: {}", e);
            AppError::bad_request(format!("Failed to create update schedule: {}", e))
        })?;

    Ok((StatusCode::CREATED, Json(schedule)))
}

async fn get_update_schedule(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((id, schedule_id)): Path<(String, String)>,
) -> Result<Json<GroupUpdateSchedule>, AppError> {
    let _uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    check_group_permission(&state, &auth_user, Action::Read, None).await?;

    let repo = state.inventory_repository();
    let schedule = repo
        .get_group_update_schedule(&schedule_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch update schedule: {}", e);
            AppError::internal("Failed to fetch update schedule")
        })?
        .ok_or_else(|| AppError::not_found("Update schedule not found"))?;

    Ok(Json(schedule))
}

async fn update_update_schedule(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((id, schedule_id)): Path<(String, String)>,
    Json(payload): Json<UpdateGroupUpdateScheduleRequest>,
) -> Result<Json<GroupUpdateSchedule>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let repo = state.inventory_repository();
    let schedule = repo
        .update_group_update_schedule(&schedule_id, &payload)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update schedule: {}", e);
            AppError::bad_request(format!("Failed to update schedule: {}", e))
        })?
        .ok_or_else(|| AppError::not_found("Update schedule not found"))?;

    Ok(Json(schedule))
}

async fn delete_update_schedule(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((id, schedule_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let repo = state.inventory_repository();
    let deleted = repo
        .delete_group_update_schedule(&schedule_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete update schedule: {}", e);
            AppError::internal("Failed to delete update schedule")
        })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Update schedule not found"))
    }
}

/// Manually trigger a schedule: resolve group members and create an UpdateJob
async fn run_update_schedule(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((id, schedule_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<UpdateJob>), AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    check_group_permission(&state, &auth_user, Action::Update, Some(uuid)).await?;

    let inv_repo = state.inventory_repository();
    let schedule = inv_repo
        .get_group_update_schedule(&schedule_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch schedule: {}", e)))?
        .ok_or_else(|| AppError::not_found("Update schedule not found"))?;

    // Resolve group members (pinned + rule-matched)
    let org_id = resolve_org(&auth_user, None)?;
    let certnames = resolve_group_member_certnames(&state, org_id, uuid).await?;

    if certnames.is_empty() {
        return Err(AppError::bad_request(
            "No nodes found in this group to update",
        ));
    }

    let job = inv_repo
        .create_update_job(
            schedule.operation_type,
            &schedule.package_names,
            Some(&id),
            &certnames,
            schedule.requires_approval,
            None, // immediate execution
            None,
            None,
            &auth_user.username,
            Some(&format!(
                "Manually triggered from schedule '{}'",
                schedule.name
            )),
        )
        .await
        .map_err(|e| AppError::internal(format!("Failed to create update job: {}", e)))?;

    crate::services::update_audit::record(
        &state.db,
        auth_user.organization_id,
        Some(auth_user.user_id()),
        crate::services::update_audit::ACTION_REQUESTED,
        &job.id,
        crate::services::update_audit::requested_details(
            &job,
            &auth_user.username,
            "schedule_manual_run",
        ),
    )
    .await;

    // Update schedule last_run_at
    let _ = inv_repo
        .update_schedule_after_run(
            &schedule_id,
            chrono::Utc::now(),
            schedule.next_run_at, // keep existing next_run_at
            &job.id,
            false,
        )
        .await;

    Ok((StatusCode::CREATED, Json(job)))
}

/// Most nodes a preview evaluates; the rest are reported as skipped.
const PREVIEW_MAX_NODES: usize = 2000;
/// Most certnames listed per bucket of a preview.
const PREVIEW_LIST_LIMIT: usize = 200;
const PREVIEW_CHANGED_LIMIT: usize = 100;
const PREVIEW_FACT_CONCURRENCY: usize = 8;

/// Proposed changes to a group; whatever is omitted keeps its current value.
#[derive(Debug, Deserialize, Default)]
struct PreviewRequest {
    expression: Option<serde_json::Value>,
    rules: Option<Vec<CreateRuleRequest>>,
    rule_match_type: Option<RuleMatchType>,
    match_all_nodes: Option<bool>,
    environment: Option<String>,
    priority: Option<i32>,
    behavior: Option<GroupBehavior>,
    parent_id: Option<Uuid>,
}

#[derive(Debug, serde::Serialize)]
struct ClassificationChange {
    certname: String,
    groups_added: Vec<String>,
    groups_removed: Vec<String>,
    classes_changed: bool,
    variables_changed: bool,
    environment_changed: bool,
}

#[derive(Debug, serde::Serialize)]
struct PreviewResult {
    group_id: Uuid,
    nodes_evaluated: usize,
    nodes_skipped: usize,
    members_before: usize,
    members_after: usize,
    entering_count: usize,
    leaving_count: usize,
    entering: Vec<String>,
    leaving: Vec<String>,
    truncated: bool,
    classification_changed_count: usize,
    classification_changed: Vec<ClassificationChange>,
}

/// Apply a preview request to a copy of the group.
fn apply_preview(group: &NodeGroup, req: &PreviewRequest) -> NodeGroup {
    let mut g = group.clone();
    if let Some(e) = &req.expression {
        g.expression =
            (!e.is_null() && !e.as_object().is_some_and(|o| o.is_empty())).then(|| e.clone());
    }
    if let Some(rules) = &req.rules {
        g.rules = rules
            .iter()
            .map(|r| ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: r.fact_path.clone(),
                operator: r.operator,
                value: r.value.clone(),
            })
            .collect();
    }
    if let Some(m) = req.rule_match_type {
        g.rule_match_type = m;
    }
    if let Some(m) = req.match_all_nodes {
        g.match_all_nodes = m;
    }
    if let Some(env) = &req.environment {
        g.environment = (!env.is_empty()).then(|| env.clone());
    }
    if let Some(p) = req.priority {
        g.priority = p;
    }
    if let Some(b) = &req.behavior {
        g.behavior = b.clone();
    }
    if let Some(p) = req.parent_id {
        g.parent_id = Some(p);
    }
    g
}

/// Dry-run a change to a group against every node: which nodes enter and
/// leave the group and whose classification changes.
///
/// POST /api/v1/groups/:id/preview
async fn preview_group_change(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
    Json(req): Json<PreviewRequest>,
) -> Result<Json<PreviewResult>, AppError> {
    let group_id = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    check_group_permission(&state, &auth_user, Action::Update, Some(group_id)).await?;
    validate_advanced(
        &state,
        req.priority,
        req.expression.as_ref(),
        req.behavior.as_ref(),
    )
    .await?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let puppetdb = state
        .puppetdb
        .as_deref()
        .ok_or_else(|| AppError::service_unavailable("PuppetDB is not configured"))?;

    let repo = GroupRepository::new(&state.db);
    let before_groups = repo.get_all(org_id).await.map_err(|e| {
        tracing::error!("Failed to load groups for preview: {}", e);
        AppError::internal("Failed to load groups")
    })?;
    let current = before_groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| AppError::not_found("Group not found"))?;
    let candidate = apply_preview(current, &req);
    if candidate.parent_id == Some(group_id) {
        return Err(AppError::bad_request("A group cannot be its own parent"));
    }
    let after_groups: Vec<NodeGroup> = before_groups
        .iter()
        .map(|g| {
            if g.id == group_id {
                candidate.clone()
            } else {
                g.clone()
            }
        })
        .collect();

    let now = chrono::Utc::now();
    let before = state
        .plugin_manager
        .classification_service(before_groups)
        .await
        .with_now(now);
    let after = state
        .plugin_manager
        .classification_service(after_groups)
        .await
        .with_now(now);

    let mut nodes = puppetdb.get_nodes().await.map_err(|e| {
        tracing::error!("Failed to list nodes for preview: {}", e);
        AppError::internal("Failed to list nodes")
    })?;
    nodes.sort_by(|a, b| a.certname.cmp(&b.certname));
    let nodes_skipped = nodes.len().saturating_sub(PREVIEW_MAX_NODES);
    nodes.truncate(PREVIEW_MAX_NODES);

    let plugin_facts = state.plugin_manager.usable_facts_snapshot().await;
    let fetched: Vec<_> = futures::stream::iter(nodes)
        .map(|node| async move {
            let facts = puppetdb.get_node_facts(&node.certname).await;
            (node, facts)
        })
        .buffer_unordered(PREVIEW_FACT_CONCURRENCY)
        .collect()
        .await;

    let mut result = PreviewResult {
        group_id,
        nodes_evaluated: 0,
        nodes_skipped,
        members_before: 0,
        members_after: 0,
        entering_count: 0,
        leaving_count: 0,
        entering: vec![],
        leaving: vec![],
        truncated: nodes_skipped > 0,
        classification_changed_count: 0,
        classification_changed: vec![],
    };
    let mut fetched = fetched;
    fetched.sort_by(|a, b| a.0.certname.cmp(&b.0.certname));
    for (node, facts) in fetched {
        let Ok(facts) = facts else {
            result.nodes_skipped += 1;
            continue;
        };
        let facts = build_classification_facts_with_plugins(
            facts,
            &node.certname,
            node.catalog_environment.as_deref(),
            plugin_facts.for_node(&node.certname),
            now,
        );
        result.nodes_evaluated += 1;
        let b = before.classify(&node.certname, &facts);
        let a = after.classify(&node.certname, &facts);
        let in_b = b.groups.iter().any(|g| g.id == group_id);
        let in_a = a.groups.iter().any(|g| g.id == group_id);
        result.members_before += in_b as usize;
        result.members_after += in_a as usize;
        if in_a && !in_b {
            result.entering_count += 1;
            if result.entering.len() < PREVIEW_LIST_LIMIT {
                result.entering.push(node.certname.clone());
            }
        }
        if in_b && !in_a {
            result.leaving_count += 1;
            if result.leaving.len() < PREVIEW_LIST_LIMIT {
                result.leaving.push(node.certname.clone());
            }
        }

        let ids =
            |r: &crate::models::ClassificationResult| -> std::collections::BTreeMap<Uuid, String> {
                r.groups.iter().map(|g| (g.id, g.name.clone())).collect()
            };
        let (ib, ia) = (ids(&b), ids(&a));
        let change = ClassificationChange {
            certname: node.certname.clone(),
            groups_added: ia
                .iter()
                .filter(|(k, _)| !ib.contains_key(k))
                .map(|(_, n)| n.clone())
                .collect(),
            groups_removed: ib
                .iter()
                .filter(|(k, _)| !ia.contains_key(k))
                .map(|(_, n)| n.clone())
                .collect(),
            classes_changed: a.classes != b.classes,
            variables_changed: a.variables != b.variables,
            environment_changed: a.environment != b.environment,
        };
        if !change.groups_added.is_empty()
            || !change.groups_removed.is_empty()
            || change.classes_changed
            || change.variables_changed
            || change.environment_changed
        {
            result.classification_changed_count += 1;
            if result.classification_changed.len() < PREVIEW_CHANGED_LIMIT {
                result.classification_changed.push(change);
            }
        }
    }
    result.truncated |= result.entering_count > result.entering.len()
        || result.leaving_count > result.leaving.len()
        || result.classification_changed_count > result.classification_changed.len();

    tracing::info!(
        "Group change preview for {}: {} entering, {} leaving, {} classifications changed",
        group_id,
        result.entering_count,
        result.leaving_count,
        result.classification_changed_count
    );
    Ok(Json(result))
}

/// One member of a group with where it comes from (M27).
#[derive(Debug, serde::Serialize)]
struct GroupMemberEntry {
    certname: String,
    /// `pinned`, `dynamic` or `rule`
    source: &'static str,
    computed_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn classification_error(
    e: crate::services::plugins::classification::ClassificationError,
) -> AppError {
    use crate::services::plugins::classification::ClassificationError as E;
    match e {
        E::NotFound(m) => AppError::not_found(m),
        E::Invalid(m) => AppError::bad_request(m),
        E::Conflict(m) => AppError::conflict(m),
        E::Failed(m) => AppError::service_unavailable(m),
    }
}

async fn visible_group(
    state: &AppState,
    auth_user: &AuthUser,
    query: &OrgQuery,
    id: &str,
) -> Result<(Uuid, NodeGroup), AppError> {
    let uuid = Uuid::parse_str(id).map_err(|_| AppError::bad_request("Invalid group ID"))?;
    let org_id = resolve_org(auth_user, query.organization_id)?;
    let group = GroupRepository::new(&state.db)
        .get_by_id(org_id, uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get group: {}", e);
            AppError::internal("Failed to get group")
        })?
        .ok_or_else(|| AppError::not_found("Group not found"))?;
    Ok((uuid, group))
}

/// Members of a group: pinned, dynamic (with the time the provider confirmed
/// them) and matched by rules.
///
/// GET /api/v1/groups/:id/members
async fn get_group_members(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<Vec<GroupMemberEntry>>, AppError> {
    let (uuid, group) = visible_group(&state, &auth_user, &query, &id).await?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;
    let known: std::collections::HashMap<
        String,
        (&'static str, Option<chrono::DateTime<chrono::Utc>>),
    > = state
        .plugin_manager
        .group_members(&group)
        .await
        .into_iter()
        .map(|(c, s, at)| (c, (s, at)))
        .collect();
    let mut names = resolve_group_member_certnames(&state, org_id, uuid).await?;
    names.extend(known.keys().cloned());
    names.sort();
    names.dedup();
    Ok(Json(
        names
            .into_iter()
            .map(|certname| {
                let (source, computed_at) = known.get(&certname).copied().unwrap_or(("rule", None));
                GroupMemberEntry {
                    certname,
                    source,
                    computed_at,
                }
            })
            .collect(),
    ))
}

/// Run the membership provider of a dynamic group now.
///
/// POST /api/v1/groups/:id/membership/refresh
async fn refresh_membership(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<crate::services::plugins::classification::MembershipOutcome>, AppError> {
    super::plugins::require(&state, &auth_user, Resource::Plugins, Action::Admin).await?;
    let (uuid, _) = visible_group(&state, &auth_user, &query, &id).await?;
    state
        .plugin_manager
        .refresh_membership(uuid)
        .await
        .map(Json)
        .map_err(classification_error)
}

/// The update held by the churn guard, if any.
///
/// GET /api/v1/groups/:id/membership/held
async fn get_held_membership(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<Option<crate::models::HeldMembership>>, AppError> {
    let (uuid, _) = visible_group(&state, &auth_user, &query, &id).await?;
    Ok(Json(state.plugin_manager.held_membership(uuid).await))
}

/// Apply the update held by the churn guard.
///
/// POST /api/v1/groups/:id/membership/approve-held
async fn approve_held_membership(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<crate::services::plugins::classification::MembershipOutcome>, AppError> {
    super::plugins::require(&state, &auth_user, Resource::Plugins, Action::Admin).await?;
    let (uuid, _) = visible_group(&state, &auth_user, &query, &id).await?;
    state
        .plugin_manager
        .approve_held_membership(uuid, &auth_user.username)
        .await
        .map(Json)
        .map_err(classification_error)
}

/// Discard the update held by the churn guard.
///
/// DELETE /api/v1/groups/:id/membership/held
async fn reject_held_membership(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    super::plugins::require(&state, &auth_user, Resource::Plugins, Action::Admin).await?;
    let (uuid, _) = visible_group(&state, &auth_user, &query, &id).await?;
    state
        .plugin_manager
        .reject_held_membership(uuid, &auth_user.username)
        .await
        .map_err(classification_error)?;
    Ok(StatusCode::NO_CONTENT)
}
