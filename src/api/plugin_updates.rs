//! Plugin updates, rollback and local revocation API (T86 to T88).
//!
//! Reading needs `plugins:read`; every change needs `plugins:admin`. The
//! service writes the audit entry and emits the event of every change.

use axum::{
    body::Bytes,
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::de::DeserializeOwned;

use super::plugins::{require, ApiResult};
use crate::{
    middleware::AuthUser,
    models::*,
    services::plugins::{updates::UpdateTrigger, PluginError, UpdateActor},
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/updates", get(overview))
        .route("/updates/apply", post(apply_batch))
        .route("/{id}/versions", get(list_versions))
        .route("/{id}/versions/{version}/revoke", post(revoke_version))
        .route("/{id}/versions/{version}/unrevoke", post(unrevoke_version))
        .route("/{id}/update", post(update_one))
        .route("/{id}/rollback", post(rollback))
        .route("/{id}/update-policy", put(set_policy))
}

/// An optional JSON body: empty means the defaults.
fn optional_body<T: DeserializeOwned + Default>(body: &Bytes) -> Result<T, PluginError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(body)
        .map_err(|e| PluginError::BadRequest(format!("Invalid request body: {}", e)))
}

fn actor(user: &AuthUser) -> UpdateActor {
    UpdateActor::user(user.id, user.organization_id, &user.username)
}

async fn overview(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<PluginUpdateInfo>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.update_overview().await?))
}

async fn apply_batch(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ApplyUpdatesRequest>,
) -> ApiResult<Json<ApplyUpdatesResponse>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let results = state
        .plugin_manager
        .apply_updates(&req.plugin_ids, &actor(&user))
        .await?;
    Ok(Json(ApplyUpdatesResponse { results }))
}

async fn list_versions(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<PluginVersionView>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.list_versions(&id).await?))
}

async fn update_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<Json<PluginUpdateResult>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let req: UpdatePluginRequest = optional_body(&body)?;
    Ok(Json(
        state
            .plugin_manager
            .apply_update(
                &id,
                req.version.as_deref(),
                &actor(&user),
                UpdateTrigger::Manual,
            )
            .await?,
    ))
}

async fn rollback(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<Json<RollbackResult>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let req: RollbackPluginRequest = optional_body(&body)?;
    Ok(Json(
        state
            .plugin_manager
            .rollback(&id, req.version.as_deref(), &actor(&user))
            .await?,
    ))
}

async fn set_policy(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdatePolicyRequest>,
) -> ApiResult<Json<PluginSummary>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    Ok(Json(
        state
            .plugin_manager
            .set_update_policy(&id, &req, &actor(&user))
            .await?,
    ))
}

async fn revoke_version(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, version)): Path<(String, String)>,
    Json(req): Json<RevokeVersionRequest>,
) -> ApiResult<Json<RevokeVersionResult>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    Ok(Json(
        state
            .plugin_manager
            .revoke_version(&id, &version, &req.reason, &actor(&user))
            .await?,
    ))
}

async fn unrevoke_version(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, version)): Path<(String, String)>,
) -> ApiResult<Json<PluginDetail>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    Ok(Json(
        state
            .plugin_manager
            .unrevoke_version(&id, &version, &actor(&user))
            .await?,
    ))
}
