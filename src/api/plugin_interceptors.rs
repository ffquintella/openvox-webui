//! Interceptors administration API (M31, T104): the interceptable events with
//! their fail mode, timeout and maximum, and per event the interceptors of the
//! enabled plugins in run order with their grant and telemetry.
//!
//! Reading needs `plugins:read`. Only aggregate counters are shown, never a
//! payload.

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use super::plugins::{require, ApiResult};
use crate::{
    middleware::AuthUser,
    models::*,
    services::plugins::interceptors::{InterceptableEventView, InterceptorsOverview},
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/interceptors", get(overview))
        .route("/interceptors/{event}", get(event_view))
}

async fn overview(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<InterceptorsOverview>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.interceptors_overview().await))
}

async fn event_view(
    State(state): State<AppState>,
    user: AuthUser,
    Path(event): Path<String>,
) -> ApiResult<Json<InterceptableEventView>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let view = state
        .plugin_manager
        .interceptors_overview()
        .await
        .events
        .into_iter()
        .find(|v| v.event.event_type == event)
        .ok_or_else(|| AppError::not_found(format!("'{}' is not an interceptable event", event)))?;
    Ok(Json(view))
}
