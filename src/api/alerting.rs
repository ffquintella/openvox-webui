//! Alerting API endpoints
//!
//! This module provides REST API endpoints for managing alerts, rules, channels, and silences.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    models::{
        Alert, AlertRule, AlertRuleType, AlertSeverity, AlertSilence, AlertStats, AlertStatus,
        CreateAlertRuleRequest, CreateChannelRequest, CreateSilenceRequest, NotificationChannel,
        TestChannelRequest, TestChannelResponse, UpdateAlertRuleRequest, UpdateChannelRequest,
    },
    services::AlertingService,
    AppState, AuthUser,
};

/// Create alerting routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Notification channels
        .route("/channels", get(list_channels).post(create_channel))
        .route(
            "/channels/{id}",
            get(get_channel).put(update_channel).delete(delete_channel),
        )
        .route("/channels/{id}/test", post(test_channel))
        // Alert rules
        .route("/rules", get(list_rules).post(create_rule))
        .route(
            "/rules/{id}",
            get(get_rule).put(update_rule).delete(delete_rule),
        )
        // Alerts
        .route("/alerts", get(list_alerts))
        .route("/alerts/stats", get(get_alert_stats))
        .route("/alerts/{id}", get(get_alert))
        .route("/alerts/{id}/acknowledge", post(acknowledge_alert))
        .route("/alerts/{id}/resolve", post(resolve_alert))
        .route("/alerts/{id}/silence", post(silence_alert))
        // Silences
        .route("/silences", get(list_silences).post(create_silence))
        .route("/silences/{id}", delete(delete_silence))
        // Trigger
        .route("/trigger", post(trigger_alert))
        // Evaluate rules
        .route("/evaluate", post(evaluate_rules))
}

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AlertsQuery {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub rule_id: Option<Uuid>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct RulesQuery {
    pub rule_type: Option<String>,
    pub enabled: Option<bool>,
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AlertingResponse<T> {
    pub data: T,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct TriggerAlertRequest {
    pub rule_id: Uuid,
    pub title: String,
    pub message: String,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub alerts_triggered: usize,
    pub alerts: Vec<Alert>,
}

// ============================================================================
// Notification Channel Handlers
// ============================================================================

/// List all notification channels
async fn list_channels(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<Vec<NotificationChannel>>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.get_channels().await {
        Ok(channels) => Ok(Json(AlertingResponse { data: channels })),
        Err(e) => {
            tracing::error!("Failed to list channels: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get a notification channel by ID
async fn get_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<NotificationChannel>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.get_channel(id).await {
        Ok(Some(channel)) => Ok(Json(AlertingResponse { data: channel })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create a new notification channel
async fn create_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<AlertingResponse<NotificationChannel>>), StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.create_channel(&req, Some(user.user_id())).await {
        Ok(channel) => Ok((
            StatusCode::CREATED,
            Json(AlertingResponse { data: channel }),
        )),
        Err(e) => {
            tracing::error!("Failed to create channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update a notification channel
async fn update_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<AlertingResponse<NotificationChannel>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.update_channel(id, &req).await {
        Ok(Some(channel)) => Ok(Json(AlertingResponse { data: channel })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete a notification channel
async fn delete_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.delete_channel(id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to delete channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Test a notification channel
async fn test_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
    Json(req): Json<TestChannelRequest>,
) -> Result<Json<AlertingResponse<TestChannelResponse>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.test_channel(id, &req).await {
        Ok(response) => Ok(Json(AlertingResponse { data: response })),
        Err(e) => {
            tracing::error!("Failed to test channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ============================================================================
// Alert Rule Handlers
// ============================================================================

/// List all alert rules
async fn list_rules(
    State(state): State<AppState>,
    Query(query): Query<RulesQuery>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<Vec<AlertRule>>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    let rules = if let Some(type_str) = query.rule_type {
        if let Some(rule_type) = AlertRuleType::from_str(&type_str) {
            service.get_rules_by_type(rule_type).await
        } else {
            service.get_rules().await
        }
    } else if query.enabled == Some(true) {
        service.get_enabled_rules().await
    } else {
        service.get_rules().await
    };

    match rules {
        Ok(rules) => Ok(Json(AlertingResponse { data: rules })),
        Err(e) => {
            tracing::error!("Failed to list rules: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get an alert rule by ID
async fn get_rule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<AlertRule>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.get_rule(id).await {
        Ok(Some(rule)) => Ok(Json(AlertingResponse { data: rule })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create a new alert rule
async fn create_rule(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateAlertRuleRequest>,
) -> Result<(StatusCode, Json<AlertingResponse<AlertRule>>), StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.create_rule(&req, Some(user.user_id())).await {
        Ok(rule) => Ok((StatusCode::CREATED, Json(AlertingResponse { data: rule }))),
        Err(e) => {
            tracing::error!("Failed to create rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update an alert rule
async fn update_rule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
    Json(req): Json<UpdateAlertRuleRequest>,
) -> Result<Json<AlertingResponse<AlertRule>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.update_rule(id, &req).await {
        Ok(Some(rule)) => Ok(Json(AlertingResponse { data: rule })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete an alert rule
async fn delete_rule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.delete_rule(id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to delete rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ============================================================================
// Alert Handlers
// ============================================================================

/// List alerts with optional filtering
async fn list_alerts(
    State(state): State<AppState>,
    Query(query): Query<AlertsQuery>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<Vec<Alert>>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    let status = query.status.and_then(|s| AlertStatus::from_str(&s));
    let severity = query.severity.and_then(|s| AlertSeverity::from_str(&s));

    match service
        .get_alerts(status, severity, query.rule_id, query.limit)
        .await
    {
        Ok(alerts) => Ok(Json(AlertingResponse { data: alerts })),
        Err(e) => {
            tracing::error!("Failed to list alerts: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get alert statistics
async fn get_alert_stats(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<AlertStats>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.get_alert_stats().await {
        Ok(stats) => Ok(Json(AlertingResponse { data: stats })),
        Err(e) => {
            tracing::error!("Failed to get alert stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get an alert by ID
async fn get_alert(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<Alert>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.get_alert(id).await {
        Ok(Some(alert)) => Ok(Json(AlertingResponse { data: alert })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get alert: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Acknowledge an alert
async fn acknowledge_alert(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    user: AuthUser,
) -> Result<Json<AlertingResponse<Alert>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.acknowledge_alert(id, user.user_id()).await {
        Ok(Some(alert)) => Ok(Json(AlertingResponse { data: alert })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to acknowledge alert: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Resolve an alert
async fn resolve_alert(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<Alert>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.resolve_alert(id).await {
        Ok(Some(alert)) => Ok(Json(AlertingResponse { data: alert })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to resolve alert: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Silence an alert
async fn silence_alert(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<Alert>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.silence_alert(id).await {
        Ok(Some(alert)) => Ok(Json(AlertingResponse { data: alert })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to silence alert: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ============================================================================
// Silence Handlers
// ============================================================================

/// List all silences
async fn list_silences(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<Vec<AlertSilence>>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.get_silences().await {
        Ok(silences) => Ok(Json(AlertingResponse { data: silences })),
        Err(e) => {
            tracing::error!("Failed to list silences: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create a silence
async fn create_silence(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateSilenceRequest>,
) -> Result<(StatusCode, Json<AlertingResponse<AlertSilence>>), StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.create_silence(&req, Some(user.user_id())).await {
        Ok(silence) => Ok((
            StatusCode::CREATED,
            Json(AlertingResponse { data: silence }),
        )),
        Err(e) => {
            tracing::error!("Failed to create silence: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete a silence
async fn delete_silence(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.delete_silence(id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to delete silence: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ============================================================================
// Trigger and Evaluate Handlers
// ============================================================================

/// Manually trigger an alert for a rule
async fn trigger_alert(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(req): Json<TriggerAlertRequest>,
) -> Result<(StatusCode, Json<AlertingResponse<Alert>>), StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service
        .trigger_manual_alert(req.rule_id, &req.title, &req.message, req.context)
        .await
    {
        Ok(alert) => Ok((StatusCode::CREATED, Json(AlertingResponse { data: alert }))),
        Err(e) => {
            tracing::error!("Failed to trigger alert: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Evaluate all enabled rules and trigger alerts as needed
async fn evaluate_rules(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<AlertingResponse<EvaluateResponse>>, StatusCode> {
    let service = AlertingService::new(state.db.clone(), state.puppetdb.clone(), Some(state.notification_service.clone()));

    match service.evaluate_rules().await {
        Ok(alerts) => Ok(Json(AlertingResponse {
            data: EvaluateResponse {
                alerts_triggered: alerts.len(),
                alerts,
            },
        })),
        Err(e) => {
            tracing::error!("Failed to evaluate rules: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
