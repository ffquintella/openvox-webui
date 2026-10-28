//! Plugin events, jobs and webhooks API (T77 to T79).
//!
//! Reading needs `plugins:read`; changing anything needs `plugins:admin`. The
//! one public route is the signed inbound webhook, which has no session.

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::plugins::{require, ApiError, ApiResult};
use crate::{
    db::PluginEventsRepository,
    middleware::AuthUser,
    models::*,
    services::plugins::{
        jobs::JobError,
        manager::PluginError,
        webhooks::{
            endpoint_record, generate_secret, validate_endpoint_fields, validate_endpoint_url,
            WebhookError, HEADER_SIGNATURE, HEADER_TIMESTAMP,
        },
        AUDIT_PLUGINS,
    },
    utils::{error::ErrorResponse, AppError},
    AppState,
};

const WEBHOOK_BODY_CEILING: usize = 2 * 1024 * 1024;

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/webhooks/{hook}", post(inbound_webhook))
        .layer(DefaultBodyLimit::max(WEBHOOK_BODY_CEILING))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/events/stats", get(event_stats))
        .route("/events/deliveries", get(list_deliveries))
        .route("/events/deliveries/{id}/retry", post(retry_delivery))
        .route("/jobs", get(list_jobs))
        .route("/{id}/jobs/{job}/runs", get(list_runs))
        .route("/{id}/jobs/{job}/run", post(run_job))
        .route("/{id}/webhooks", get(list_webhooks))
        .route("/{id}/webhooks/{hook}/secret", post(rotate_webhook_secret))
        .route(
            "/webhooks/outbound",
            get(list_outbound).post(create_outbound),
        )
        .route(
            "/webhooks/outbound/{id}",
            axum::routing::patch(update_outbound).delete(delete_outbound),
        )
        .route(
            "/webhooks/outbound/{id}/secret",
            post(rotate_outbound_secret),
        )
}

fn repo(state: &AppState) -> PluginEventsRepository<'_> {
    PluginEventsRepository::new(state.plugin_manager.pool())
}

fn internal(e: anyhow::Error) -> ApiError {
    ApiError::Plugin(PluginError::Internal(e))
}

fn not_found(msg: &str) -> ApiError {
    ApiError::Plugin(PluginError::NotFound(msg.to_string()))
}

fn bad_request(msg: impl Into<String>) -> ApiError {
    ApiError::Plugin(PluginError::BadRequest(msg.into()))
}

async fn audit(state: &AppState, user: &AuthUser, action: &str, id: &str, details: Value) {
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_PLUGINS,
            action,
            id,
            details,
        )
        .await;
}

// ---- events ---------------------------------------------------------------------

async fn event_stats(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<EventStats>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(repo(&state).stats().await.map_err(internal)?))
}

#[derive(Deserialize)]
struct DeliveryQuery {
    status: Option<String>,
    limit: Option<i64>,
}

async fn list_deliveries(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<DeliveryQuery>,
) -> ApiResult<Json<Vec<EventDeliveryView>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    if let Some(s) = &q.status {
        if ![
            "pending",
            "in_flight",
            "delivered",
            "dead_letter",
            "dropped",
        ]
        .contains(&s.as_str())
        {
            return Err(bad_request("unknown delivery status"));
        }
    }
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    Ok(Json(
        repo(&state)
            .list_deliveries(q.status.as_deref(), limit)
            .await
            .map_err(internal)?,
    ))
}

async fn retry_delivery(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    if !repo(&state).retry_dead_letter(id).await.map_err(internal)? {
        return Err(not_found("no dead-lettered delivery with that id"));
    }
    audit(
        &state,
        &user,
        "plugin.event.retried",
        &id.to_string(),
        json!({"actor": user.username}),
    )
    .await;
    Ok(StatusCode::ACCEPTED)
}

// ---- jobs ---------------------------------------------------------------------

async fn list_jobs(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<PluginJobView>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(
        state.plugin_runtime.jobs.list().await.map_err(internal)?,
    ))
}

async fn list_runs(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, job)): Path<(String, String)>,
) -> ApiResult<Json<Vec<PluginJobRunView>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(
        state
            .plugin_runtime
            .jobs
            .runs(&id, &job, 50)
            .await
            .map_err(internal)?,
    ))
}

async fn run_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, job)): Path<(String, String)>,
) -> ApiResult<Json<PluginJobRunView>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let run = state
        .plugin_runtime
        .jobs
        .run_now(&id, &job)
        .await
        .map_err(|e| match e {
            JobError::NotFound => not_found("job not found or plugin not enabled"),
            JobError::AlreadyRunning => ApiError::Plugin(PluginError::Conflict {
                code: "job_running",
                message: e.to_string(),
            }),
            JobError::Internal(e) => internal(e),
        })?;
    audit(
        &state,
        &user,
        "plugin.job.run_manually",
        &id,
        json!({"actor": user.username, "job": job, "status": run.status}),
    )
    .await;
    Ok(Json(run))
}

// ---- inbound webhooks -------------------------------------------------------------

async fn list_webhooks(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<PluginWebhookView>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let list = state
        .plugin_runtime
        .webhooks
        .list(&id)
        .await
        .map_err(|e| match e {
            WebhookError::NotFound => not_found("plugin not found or not enabled"),
            other => internal(anyhow::anyhow!(other)),
        })?;
    Ok(Json(list))
}

async fn rotate_webhook_secret(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, hook)): Path<(String, String)>,
) -> ApiResult<Json<WebhookSecretResponse>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let secret = state
        .plugin_runtime
        .webhooks
        .rotate_secret(&id, &hook)
        .await
        .map_err(|e| match e {
            WebhookError::NotFound => not_found("webhook not found or plugin not enabled"),
            other => internal(anyhow::anyhow!(other)),
        })?;
    audit(
        &state,
        &user,
        "plugin.webhook.secret_rotated",
        &id,
        json!({"actor": user.username, "hook": hook}),
    )
    .await;
    Ok(Json(WebhookSecretResponse {
        plugin_id: id,
        hook_id: hook,
        secret,
    }))
}

fn webhook_failure(status: StatusCode, kind: &str, msg: &str) -> Response {
    (status, Json(ErrorResponse::new(kind, msg))).into_response()
}

async fn inbound_webhook(
    State(state): State<AppState>,
    Path((id, hook)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let hdr = |n: &str| headers.get(n).and_then(|v| v.to_str().ok());
    if body.len() > state.plugin_runtime.webhooks.max_body_bytes() {
        return webhook_failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Body too large",
        );
    }
    let result = state
        .plugin_runtime
        .webhooks
        .handle(
            &id,
            &hook,
            hdr(HEADER_TIMESTAMP),
            hdr(HEADER_SIGNATURE),
            hdr("content-type"),
            &body,
        )
        .await;
    match result {
        Ok(v) => Json(json!({"accepted": true, "result": v})).into_response(),
        Err(WebhookError::NotFound) => {
            webhook_failure(StatusCode::NOT_FOUND, "not_found", "Webhook not found")
        }
        Err(WebhookError::Unauthorized) => {
            tracing::warn!("rejected webhook call to {}/{}", id, hook);
            webhook_failure(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Invalid signature",
            )
        }
        Err(WebhookError::RateLimited) => webhook_failure(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many requests",
        ),
        Err(WebhookError::Unavailable(m)) => {
            webhook_failure(StatusCode::SERVICE_UNAVAILABLE, "unavailable", &m)
        }
        Err(WebhookError::Handler(m)) => {
            webhook_failure(StatusCode::BAD_GATEWAY, "handler_failed", &m)
        }
        Err(WebhookError::Timeout) => webhook_failure(
            StatusCode::GATEWAY_TIMEOUT,
            "handler_timeout",
            "Handler timed out",
        ),
        Err(WebhookError::Internal(e)) => {
            tracing::error!("webhook error: {:#}", e);
            AppError::internal("Webhook failed").into_response()
        }
    }
}

// ---- outbound webhooks ------------------------------------------------------------

async fn list_outbound(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<OutboundWebhookView>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let all = repo(&state).list_endpoints().await.map_err(internal)?;
    Ok(Json(all.into_iter().map(|e| e.view).collect()))
}

async fn create_outbound(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateOutboundWebhookRequest>,
) -> ApiResult<(StatusCode, Json<OutboundWebhookCreated>)> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    validate_endpoint_fields(&req.name, &req.event_types, req.filter.as_deref())
        .and_then(|_| validate_endpoint_url(&req.url))
        .map_err(bad_request)?;
    let rec = endpoint_record(
        req.name,
        req.url,
        req.event_types,
        req.filter,
        Some(user.username.clone()),
    );
    repo(&state).insert_endpoint(&rec).await.map_err(internal)?;
    audit(
        &state,
        &user,
        "plugin.webhook.outbound_created",
        &rec.view.id,
        json!({"actor": user.username, "url": rec.view.url, "event_types": rec.view.event_types}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(OutboundWebhookCreated {
            webhook: rec.view,
            secret: rec.secret,
        }),
    ))
}

async fn update_outbound(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateOutboundWebhookRequest>,
) -> ApiResult<Json<OutboundWebhookView>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let mut rec = repo(&state)
        .get_endpoint(&id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("webhook not found"))?;
    if let Some(n) = req.name {
        rec.view.name = n;
    }
    if let Some(u) = req.url {
        rec.view.url = u;
    }
    if let Some(t) = req.event_types {
        rec.view.event_types = t;
    }
    if let Some(f) = req.filter {
        rec.view.filter = f;
    }
    if let Some(e) = req.enabled {
        rec.view.enabled = e;
    }
    validate_endpoint_fields(
        &rec.view.name,
        &rec.view.event_types,
        rec.view.filter.as_deref(),
    )
    .and_then(|_| validate_endpoint_url(&rec.view.url))
    .map_err(bad_request)?;
    repo(&state).update_endpoint(&rec).await.map_err(internal)?;
    audit(
        &state,
        &user,
        "plugin.webhook.outbound_updated",
        &id,
        json!({"actor": user.username, "enabled": rec.view.enabled}),
    )
    .await;
    Ok(Json(rec.view))
}

async fn delete_outbound(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    if !repo(&state).delete_endpoint(&id).await.map_err(internal)? {
        return Err(not_found("webhook not found"));
    }
    audit(
        &state,
        &user,
        "plugin.webhook.outbound_deleted",
        &id,
        json!({"actor": user.username}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn rotate_outbound_secret(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let mut rec = repo(&state)
        .get_endpoint(&id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("webhook not found"))?;
    rec.secret = generate_secret();
    repo(&state).update_endpoint(&rec).await.map_err(internal)?;
    audit(
        &state,
        &user,
        "plugin.webhook.outbound_secret_rotated",
        &id,
        json!({"actor": user.username}),
    )
    .await;
    Ok(Json(json!({"id": id, "secret": rec.secret})))
}
