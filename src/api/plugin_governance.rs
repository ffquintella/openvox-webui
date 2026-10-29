//! Plugin governance API (T80 to T82): metrics, the administrator dashboard,
//! the circuit breaker reset and the Prometheus endpoint.
//!
//! Reading needs `plugins:read` (a Prometheus scraper uses an API key with
//! that permission); resetting a breaker needs `plugins:admin`. Everything
//! here is aggregate counts and timings, never payload content.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use super::plugins::{require, ApiResult};
use crate::{middleware::AuthUser, models::*, services::plugins::AUDIT_PLUGINS, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/governance", get(overview))
        .route("/metrics", get(prometheus))
        .route("/{id}/metrics", get(plugin_metrics))
        .route("/{id}/circuit/reset", post(reset_circuit))
}

async fn overview(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<GovernanceOverview>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.governance_overview().await?))
}

#[derive(Deserialize)]
struct MetricsQuery {
    hours: Option<u32>,
}

async fn plugin_metrics(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<MetricsQuery>,
) -> ApiResult<Json<PluginMetricsView>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(
        state
            .plugin_manager
            .plugin_metrics(&id, q.hours.unwrap_or(24))
            .await?,
    ))
}

async fn reset_circuit(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<CircuitView>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let view = state.plugin_manager.reset_circuit(&id).await?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_PLUGINS,
            "plugin.circuit.reset",
            &id,
            json!({"actor": user.username}),
        )
        .await;
    Ok(Json(view))
}

async fn prometheus(State(state): State<AppState>, user: AuthUser) -> ApiResult<Response> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let body = state.plugin_manager.prometheus().await?;
    let mut res = body.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    Ok(res)
}
