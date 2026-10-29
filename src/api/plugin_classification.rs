//! Classification extensions administration API (M27): what plugins add to
//! classification, membership providers and held updates, and group template
//! instances with their approval.
//!
//! Reading needs `plugins:read`; creating, approving, running and deleting
//! template instances need `plugins:admin`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::plugins::{require, ApiResult};
use crate::{
    middleware::AuthUser,
    models::*,
    services::plugins::classification::{
        ClassificationError, ExtensionCatalog, PluginClassificationView, TemplateRun,
    },
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/classification", get(overview))
        .route("/{id}/classification", get(plugin_view))
        .route(
            "/{id}/classification/templates/{template}/instances",
            post(create_instance),
        )
        .route(
            "/classification/instances/{instance}",
            axum::routing::put(set_instance_enabled).delete(delete_instance),
        )
        .route(
            "/classification/instances/{instance}/approve",
            post(approve_instance),
        )
        .route(
            "/classification/instances/{instance}/run",
            post(run_instance),
        )
}

fn app_error(e: ClassificationError) -> AppError {
    match e {
        ClassificationError::NotFound(m) => AppError::not_found(m),
        ClassificationError::Invalid(m) => AppError::bad_request(m),
        ClassificationError::Conflict(m) => AppError::conflict(m),
        ClassificationError::Failed(m) => AppError::service_unavailable(m),
    }
}

#[derive(Serialize)]
struct Overview {
    catalog: ExtensionCatalog,
    plugins: Vec<PluginClassificationView>,
    /// Membership updates waiting for approval.
    held: Vec<HeldMembership>,
    /// Groups that need a plugin that is not enabled: group name -> reasons.
    unavailable: std::collections::BTreeMap<String, Vec<String>>,
}

async fn overview(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Overview>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let m = &state.plugin_manager;
    let mut plugins = Vec::new();
    for h in m.classification_plugins() {
        if let Some(v) = m.plugin_classification_view(&h.plugin_id).await {
            plugins.push(v);
        }
    }
    Ok(Json(Overview {
        catalog: m.extension_catalog(),
        plugins,
        held: m.all_held_memberships().await,
        unavailable: m.unavailable_group_extensions().await,
    }))
}

async fn plugin_view(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginClassificationView>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    state
        .plugin_manager
        .plugin_classification_view(&id)
        .await
        .map(Json)
        .ok_or_else(|| {
            AppError::not_found("The plugin is not enabled or has no classification extensions")
                .into()
        })
}

#[derive(Deserialize)]
struct CreateInstanceRequest {
    name: String,
    #[serde(default)]
    parent_id: Option<Uuid>,
    #[serde(default = "empty_object")]
    classes: Value,
    #[serde(default = "empty_object")]
    variables: Value,
}

fn empty_object() -> Value {
    json!({})
}

async fn create_instance(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, template)): Path<(String, String)>,
    Json(req): Json<CreateInstanceRequest>,
) -> ApiResult<(StatusCode, Json<GroupTemplateInstance>)> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let instance = state
        .plugin_manager
        .create_template_instance(
            &id,
            &template,
            &req.name,
            req.parent_id,
            req.classes,
            req.variables,
            &user.username,
        )
        .await
        .map_err(app_error)?;
    Ok((StatusCode::CREATED, Json(instance)))
}

async fn approve_instance(
    State(state): State<AppState>,
    user: AuthUser,
    Path(instance): Path<Uuid>,
) -> ApiResult<StatusCode> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    state
        .plugin_manager
        .approve_template_instance(instance, &user.username)
        .await
        .map_err(app_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct EnabledRequest {
    enabled: bool,
}

async fn set_instance_enabled(
    State(state): State<AppState>,
    user: AuthUser,
    Path(instance): Path<Uuid>,
    Json(req): Json<EnabledRequest>,
) -> ApiResult<StatusCode> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    state
        .plugin_manager
        .set_template_instance_enabled(instance, req.enabled)
        .await
        .map_err(app_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn run_instance(
    State(state): State<AppState>,
    user: AuthUser,
    Path(instance): Path<Uuid>,
) -> ApiResult<Json<TemplateRun>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    state
        .plugin_manager
        .reconcile_template(instance, state.puppetdb.as_deref())
        .await
        .map(Json)
        .map_err(|e| app_error(e).into())
}

#[derive(Serialize)]
struct Deleted {
    removed_groups: usize,
}

async fn delete_instance(
    State(state): State<AppState>,
    user: AuthUser,
    Path(instance): Path<Uuid>,
) -> ApiResult<Json<Deleted>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let removed_groups = state
        .plugin_manager
        .delete_template_instance(instance, &user.username)
        .await
        .map_err(app_error)?;
    Ok(Json(Deleted { removed_groups }))
}
