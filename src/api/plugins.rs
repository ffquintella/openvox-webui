//! Plugin admin API and UI registry endpoints (T69 to T72).
//!
//! `plugins:admin` is `Resource::Plugins` + `Action::Admin`; `plugins:trust`
//! is `Resource::PluginTrust` + `Action::Admin`, kept separate so installing
//! plugins never implies the power to widen the set of trusted signers.

use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::{
    middleware::AuthUser,
    models::*,
    services::{
        plugins::{PluginError, TrustRequest, AUDIT_PLUGINS, AUDIT_TRUST},
        AuthService,
    },
    utils::{error::ErrorResponse, AppError},
    AppState,
};

const MAX_UPLOAD_BYTES: usize = 40 * 1024 * 1024;

/// Emits a platform event with the acting user as actor; never fails the request.
async fn emit(state: &AppState, user: &AuthUser, event_type: &str, payload: serde_json::Value) {
    state
        .plugin_runtime
        .events
        .emit(event_type, payload, EventActor::user(user.username.clone()))
        .await;
}

pub fn public_routes() -> Router<AppState> {
    super::plugin_events::public_routes()
}

pub fn routes() -> Router<AppState> {
    let upload = Router::new()
        .route("/", post(install))
        .route("/preflight", post(preflight))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES));
    Router::new()
        .route("/", get(list_plugins))
        .merge(upload)
        .merge(super::plugin_events::routes())
        .merge(super::plugin_governance::routes())
        .route("/ui/slots", get(ui_slots))
        .route(
            "/ui/contributions/{plugin_id}/{contribution_id}",
            get(ui_contribution),
        )
        .route("/trust/keys", get(list_keys).post(add_key))
        .route(
            "/trust/keys/{key_id}",
            get(get_key).patch(update_key).delete(delete_key),
        )
        .route("/trust/keys/{key_id}/revoke", post(revoke_key))
        .route("/{id}", get(get_plugin).delete(uninstall))
        .route("/{id}/enable", post(enable))
        .route("/{id}/disable", post(disable))
        .route("/{id}/verify", post(verify))
        .route(
            "/{id}/capabilities",
            get(get_capabilities).put(approve_capabilities),
        )
        .route("/{id}/config", get(get_config).put(put_config))
        .route("/{id}/storage", get(get_storage))
}

/// Error type of this module: service errors keep their machine-readable code.
pub(super) enum ApiError {
    Plugin(PluginError),
    App(AppError),
}

impl From<PluginError> for ApiError {
    fn from(e: PluginError) -> Self {
        ApiError::Plugin(e)
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError::App(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let e = match self {
            ApiError::App(e) => return e.into_response(),
            ApiError::Plugin(e) => e,
        };
        let (status, kind, code, details) = match &e {
            PluginError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request", None, None),
            PluginError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", None, None),
            PluginError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden", None, None),
            PluginError::Conflict { code, .. } => {
                (StatusCode::CONFLICT, "conflict", Some(*code), None)
            }
            PluginError::Unprocessable { code, details, .. } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                Some(*code),
                details.clone(),
            ),
            PluginError::Internal(inner) => {
                tracing::error!("plugin error: {:#}", inner);
                return AppError::internal("Plugin operation failed").into_response();
            }
        };
        let mut body = ErrorResponse::new(kind, e.to_string());
        if let Some(c) = code {
            body = body.with_code(c);
        }
        if let Some(d) = details {
            body = body.with_details(d);
        }
        (status, Json(body)).into_response()
    }
}

pub(super) type ApiResult<T> = Result<T, ApiError>;

async fn allowed(
    state: &AppState,
    user: &AuthUser,
    resource: Resource,
    action: Action,
) -> Result<bool, AppError> {
    state
        .rbac_db
        .check_permission(&user.id, resource, action, None, None)
        .await
        .map(|c| c.allowed)
        .map_err(|e| AppError::internal(format!("Permission check failed: {}", e)))
}

pub(super) async fn require(
    state: &AppState,
    user: &AuthUser,
    resource: Resource,
    action: Action,
) -> Result<(), AppError> {
    if allowed(state, user, resource, action).await? {
        Ok(())
    } else {
        Err(AppError::forbidden(format!(
            "Requires {}:{}",
            resource.as_str(),
            action.as_str()
        )))
    }
}

/// Step-up: the acting user proves they are still present by their password.
/// Accounts without a usable local password (SSO) cannot change trust until
/// the authentication pipeline offers another factor.
async fn reauthenticate(state: &AppState, user: &AuthUser, password: &str) -> Result<(), AppError> {
    let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(user.id.to_string())
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::database(e.to_string()))?;
    let ok = hash
        .map(|h| AuthService::verify_password(password, &h).unwrap_or(false))
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "Re-authentication failed: the password is incorrect or this account has no local password",
        ))
    }
}

// ============================================================================
// Plugins
// ============================================================================

async fn list_plugins(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<PluginSummary>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.list().await?))
}

async fn get_plugin(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginDetail>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.get(&id).await?))
}

async fn preflight(
    State(state): State<AppState>,
    user: AuthUser,
    body: Bytes,
) -> ApiResult<Json<PreflightResult>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    Ok(Json(state.plugin_manager.preflight(&body).await?))
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

async fn install(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<(StatusCode, Json<PluginDetail>)> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;

    // Trusting a signer while installing needs the separate trust permission
    // and a fresh proof of presence.
    let trust = match header(&headers, "x-trust-key-confirm") {
        Some(prefix) => {
            require(&state, &user, Resource::PluginTrust, Action::Admin).await?;
            let password = header(&headers, "x-reauth-password").ok_or_else(|| {
                AppError::forbidden("Re-authentication is required to trust a key")
            })?;
            reauthenticate(&state, &user, &password).await?;
            Some(TrustRequest {
                confirm_prefix: prefix,
                label: header(&headers, "x-trust-key-label"),
            })
        }
        None => None,
    };

    let out = state
        .plugin_manager
        .install(&body, &user.username, trust)
        .await?;
    let id = out.detail.summary.id.clone();
    let version = out.detail.summary.active_version.clone();
    if let Some(key_id) = &out.trusted_key_id {
        emit(
            &state,
            &user,
            "plugin.trust.key_added",
            json!({"key_id": key_id, "during_install_of": id}),
        )
        .await;
        state
            .plugin_manager
            .audit(
                user.organization_id,
                Some(user.id),
                AUDIT_TRUST,
                "plugin.trust.key_added",
                key_id,
                json!({"actor": user.username, "during_install_of": id}),
            )
            .await;
    }
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_PLUGINS,
            if out.upgraded_from.is_some() {
                "plugin.upgraded"
            } else {
                "plugin.installed"
            },
            &id,
            json!({
                "actor": user.username,
                "version": version,
                "upgraded_from": out.upgraded_from,
                "signer_key_id": out.detail.summary.signer_key_id,
            }),
        )
        .await;
    emit(
        &state,
        &user,
        if out.upgraded_from.is_some() {
            "plugin.upgraded"
        } else {
            "plugin.installed"
        },
        json!({"plugin_id": id, "version": version, "upgraded_from": out.upgraded_from}),
    )
    .await;
    Ok((StatusCode::CREATED, Json(out.detail)))
}

async fn lifecycle(
    state: &AppState,
    user: &AuthUser,
    id: &str,
    action: &str,
    detail: Result<PluginDetail, PluginError>,
) -> ApiResult<Json<PluginDetail>> {
    let outcome = detail.as_ref().map(|_| "ok").unwrap_or("rejected");
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_PLUGINS,
            action,
            id,
            json!({"actor": user.username, "outcome": outcome, "error": detail.as_ref().err().map(|e| e.to_string())}),
        )
        .await;
    let detail = detail?;
    if matches!(action, "plugin.enabled" | "plugin.disabled") {
        emit(
            state,
            user,
            action,
            json!({"plugin_id": id, "version": detail.summary.active_version}),
        )
        .await;
    }
    Ok(Json(detail))
}

async fn enable(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginDetail>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let r = state.plugin_manager.enable(&id).await;
    lifecycle(&state, &user, &id, "plugin.enabled", r).await
}

async fn disable(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginDetail>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let r = state.plugin_manager.disable(&id).await;
    lifecycle(&state, &user, &id, "plugin.disabled", r).await
}

async fn verify(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginDetail>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let r = state.plugin_manager.verify(&id).await;
    lifecycle(&state, &user, &id, "plugin.verified", r).await
}

async fn uninstall(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<UninstallQuery>,
) -> ApiResult<StatusCode> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    state.plugin_manager.uninstall(&id, q.purge).await?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_PLUGINS,
            "plugin.uninstalled",
            &id,
            json!({"actor": user.username, "purge": q.purge}),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Trusted keys
// ============================================================================

async fn list_keys(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<TrustedKeyView>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.list_keys().await?))
}

async fn get_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key_id): Path<String>,
) -> ApiResult<Json<TrustedKeyView>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.get_key(&key_id).await?))
}

async fn add_key(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<AddTrustedKeyRequest>,
) -> ApiResult<(StatusCode, Json<TrustedKeyView>)> {
    require(&state, &user, Resource::PluginTrust, Action::Admin).await?;
    reauthenticate(&state, &user, &req.password).await?;
    let key = state.plugin_manager.add_key(&req, &user.username).await?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_TRUST,
            "plugin.trust.key_added",
            &key.key_id,
            json!({"actor": user.username, "label": key.label, "expires_at": key.expires_at,
                   "allowed_plugin_ids": key.allowed_plugin_ids}),
        )
        .await;
    emit(
        &state,
        &user,
        "plugin.trust.key_added",
        json!({"key_id": key.key_id, "label": key.label}),
    )
    .await;
    Ok((StatusCode::CREATED, Json(key)))
}

async fn update_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key_id): Path<String>,
    Json(req): Json<UpdateTrustedKeyRequest>,
) -> ApiResult<Json<TrustedKeyView>> {
    require(&state, &user, Resource::PluginTrust, Action::Admin).await?;
    let key = state.plugin_manager.update_key(&key_id, &req).await?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_TRUST,
            "plugin.trust.key_updated",
            &key_id,
            json!({"actor": user.username, "label": key.label, "expires_at": key.expires_at,
                   "allowed_plugin_ids": key.allowed_plugin_ids}),
        )
        .await;
    Ok(Json(key))
}

async fn revoke_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key_id): Path<String>,
    Json(req): Json<RevokeTrustedKeyRequest>,
) -> ApiResult<Json<TrustedKeyView>> {
    require(&state, &user, Resource::PluginTrust, Action::Admin).await?;
    reauthenticate(&state, &user, &req.password).await?;
    let (key, affected) = state
        .plugin_manager
        .revoke_key(&key_id, &req.reason)
        .await?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_TRUST,
            "plugin.trust.key_revoked",
            &key_id,
            json!({"actor": user.username, "reason": req.reason, "affected_plugins": affected}),
        )
        .await;
    emit(
        &state,
        &user,
        "plugin.trust.key_revoked",
        json!({"key_id": key_id, "affected_plugins": affected}),
    )
    .await;
    Ok(Json(key))
}

async fn delete_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key_id): Path<String>,
) -> ApiResult<StatusCode> {
    require(&state, &user, Resource::PluginTrust, Action::Admin).await?;
    state.plugin_manager.delete_key(&key_id).await?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_TRUST,
            "plugin.trust.key_removed",
            &key_id,
            json!({"actor": user.username}),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// UI registry
// ============================================================================

/// Evaluates every permission the registry is gated on, once per request.
/// A contribution without `requires` is visible to any signed-in user. One
/// that names a plugin-owned permission is visible to plugin administrators
/// only, until plugin permissions can be assigned to roles (M20).
async fn visibility(
    state: &AppState,
    user: &AuthUser,
    needed: Vec<Requires>,
) -> Result<HashMap<(String, String), bool>, AppError> {
    let plugin_admin = allowed(state, user, Resource::Plugins, Action::Admin).await?;
    let mut out = HashMap::new();
    for r in needed {
        let ok = match (
            Resource::from_name(&r.resource),
            Action::from_name(&r.action),
        ) {
            (Some(res), Some(act)) => allowed(state, user, res, act).await?,
            _ => plugin_admin,
        };
        out.insert((r.resource, r.action), ok);
    }
    Ok(out)
}

fn check<'a>(map: &'a HashMap<(String, String), bool>) -> impl Fn(&Option<Requires>) -> bool + 'a {
    move |r| match r {
        None => true,
        Some(r) => map
            .get(&(r.resource.clone(), r.action.clone()))
            .copied()
            .unwrap_or(false),
    }
}

async fn ui_slots(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<UiSlots>> {
    let registry = state.plugin_manager.registry();
    let vis = visibility(&state, &user, registry.required_permissions()).await?;
    let allow = check(&vis);
    let ui = registry.to_ui(&allow);
    Ok(Json(ui))
}

async fn ui_contribution(
    State(state): State<AppState>,
    user: AuthUser,
    Path((plugin_id, contribution_id)): Path<(String, String)>,
) -> ApiResult<Json<UiContributionDetail>> {
    let registry = state.plugin_manager.registry();
    let vis = visibility(&state, &user, registry.required_permissions()).await?;
    let allow = check(&vis);
    let found = registry.find(&plugin_id, &contribution_id, &allow);
    found
        .map(Json)
        .ok_or_else(|| AppError::not_found("Contribution not found").into())
}

#[derive(serde::Deserialize, Default)]
struct UninstallQuery {
    /// Also delete the plugin's kv entries and private database.
    #[serde(default)]
    purge: bool,
}

async fn get_capabilities(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<CapabilityApproval>>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    Ok(Json(state.plugin_manager.get(&id).await?.approvals))
}

async fn approve_capabilities(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ApproveCapabilitiesRequest>,
) -> ApiResult<Json<PluginDetail>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let r = state
        .plugin_manager
        .approve_capabilities(&id, &req, &user.username)
        .await;
    if r.is_ok() {
        emit(
            &state,
            &user,
            "plugin.capabilities_approved",
            json!({"plugin": id, "capabilities": req.capabilities}),
        )
        .await;
    }
    Ok(Json(r?))
}

const MAX_CONFIG_BYTES: usize = 64 * 1024;

async fn get_config(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    state.plugin_manager.get(&id).await?;
    let cfg = crate::db::PluginRepository::new(state.plugin_manager.pool())
        .get_config(&id)
        .await
        .map_err(PluginError::Internal)?;
    Ok(Json(serde_json::Value::Object(cfg)))
}

async fn put_config(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(values): Json<serde_json::Map<String, serde_json::Value>>,
) -> ApiResult<Json<serde_json::Value>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    state.plugin_manager.get(&id).await?;
    let size = serde_json::to_vec(&values)
        .map(|v| v.len())
        .unwrap_or(usize::MAX);
    if size > MAX_CONFIG_BYTES || values.keys().any(|k| k.is_empty() || k.len() > 128) {
        return Err(PluginError::BadRequest(
            "Settings are limited to 64 KB and keys of 1 to 128 characters".into(),
        )
        .into());
    }
    crate::db::PluginRepository::new(state.plugin_manager.pool())
        .replace_config(&id, &values, &user.username)
        .await
        .map_err(PluginError::Internal)?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_PLUGINS,
            "plugin.config_changed",
            &id,
            json!({"actor": user.username, "keys": values.keys().collect::<Vec<_>>()}),
        )
        .await;
    Ok(Json(serde_json::Value::Object(values)))
}

async fn get_storage(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    require(&state, &user, Resource::Plugins, Action::Read).await?;
    let detail = state.plugin_manager.get(&id).await?;
    let Some(storage) = state.plugin_manager.storage() else {
        return Ok(Json(json!({"available": false})));
    };
    let (keys, bytes) = storage
        .kv_usage(&id)
        .await
        .map_err(|e| PluginError::Internal(anyhow::anyhow!(e.to_string())))?;
    let schema = storage
        .schema_version(&id)
        .await
        .map_err(|e| PluginError::Internal(anyhow::anyhow!(e.to_string())))?;
    let _ = detail;
    Ok(Json(json!({
        "available": true,
        "kv_keys": keys,
        "kv_bytes": bytes,
        "schema_version": schema,
    })))
}
