//! Plugin facts administration API (M25, T89): the fact catalog of each
//! provider, the administrator's opt-in per fact and the per-node preview
//! shown before enabling export to agents.
//!
//! Reading the catalog needs `plugins:read`; changing a setting and the
//! preview (which shows values) need `plugins:admin`.

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;

use super::plugins::{require, visibility, ApiResult};
use crate::{
    middleware::AuthUser, models::*, services::plugins::facts::agent_names_of, utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/facts", get(catalogs))
        .route("/{id}/facts", get(catalog))
        .route("/{id}/facts/{name}/settings", put(update_settings))
        .route("/{id}/facts/{name}/preview", get(preview))
}

/// Names reported by agents (PuppetDB), to show conflicts; `None` when
/// PuppetDB is not configured or does not answer.
pub(super) async fn agent_fact_names(state: &AppState) -> Option<HashSet<String>> {
    let db = state.puppetdb.as_ref()?;
    match db.get_fact_names().await {
        Ok(names) => Some(agent_names_of(names.iter().map(String::as_str))),
        Err(e) => {
            tracing::warn!("could not read fact names for conflict checks: {}", e);
            None
        }
    }
}

/// Which `requires` of the declared plugin facts the user satisfies.
pub(super) async fn fact_visibility(
    state: &AppState,
    user: &AuthUser,
) -> Result<HashMap<(String, String), bool>, AppError> {
    let mut needed: Vec<Requires> = Vec::new();
    for (_, _, decl) in state.plugin_manager.declared_plugin_facts() {
        if let Some(r) = decl.requires {
            if !needed.contains(&r) {
                needed.push(r);
            }
        }
    }
    if needed.is_empty() {
        return Ok(HashMap::new());
    }
    visibility(state, user, needed).await
}

async fn catalogs(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<PluginFactCatalog>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let names = agent_fact_names(&state).await;
    Ok(Json(
        state.plugin_manager.fact_catalogs(names.as_ref()).await?,
    ))
}

async fn catalog(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginFactCatalog>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let names = agent_fact_names(&state).await;
    Ok(Json(
        state
            .plugin_manager
            .fact_catalog(&id, names.as_ref())
            .await?,
    ))
}

async fn update_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, name)): Path<(String, String)>,
    Json(req): Json<UpdateFactSettingsRequest>,
) -> ApiResult<Json<PluginFactInfo>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    Ok(Json(
        state
            .plugin_manager
            .update_fact_setting(&id, &name, &req, &user.username)
            .await?,
    ))
}

#[derive(Deserialize)]
struct PreviewQuery {
    limit: Option<u32>,
}

async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, name)): Path<(String, String)>,
    Query(q): Query<PreviewQuery>,
) -> ApiResult<Json<PluginFactPreview>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    Ok(Json(
        state
            .plugin_manager
            .fact_preview(&id, &name, q.limit.unwrap_or(50))
            .await?,
    ))
}
