//! Trusted module pages API (M31, T105). Permissions and serving rules are
//! in docs/architecture/PLUGIN_PLATFORM.md section 17.13.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use super::plugins::{allowed, check, reauthenticate, require, visibility, ApiResult};
use crate::{
    middleware::AuthUser,
    models::*,
    services::plugins::{
        modules::{MODULE_TRUSTED, MODULE_UNTRUSTED},
        ui_assets::{self, UiSessionKind},
        AUDIT_TRUST,
    },
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ui/modules", post(module_session))
        .route("/{id}/module-trust", post(trust).delete(untrust))
}

/// `import()` cannot send an `Authorization` header, so the module is served
/// on a public route authorised by a short-lived path token.
pub fn public_routes() -> Router<AppState> {
    Router::new().route("/ui-modules/{token}/{*path}", get(module_asset))
}

/// Trusting widens what runs unsandboxed in the host page: like adding a
/// signing key, it needs `plugins:trust` and the acting user's password.
async fn trust(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<TrustModuleRequest>,
) -> ApiResult<Json<PluginDetail>> {
    require(&state, &user, Resource::PluginTrust, Action::Admin).await?;
    reauthenticate(&state, &user, &req.password).await?;
    let result = state
        .plugin_manager
        .trust_modules(&id, req.version.as_deref(), &user.username)
        .await;
    let details = match &result {
        Ok(t) => json!({"actor": user.username, "outcome": "ok", "version": t.version,
                        "package_sha256": t.package_sha256, "modules": t.modules}),
        Err(e) => json!({"actor": user.username, "outcome": "rejected",
                         "version": req.version, "error": e.to_string()}),
    };
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_TRUST,
            MODULE_TRUSTED,
            &id,
            details,
        )
        .await;
    let record = result?;
    state
        .plugin_runtime
        .events
        .emit(
            MODULE_TRUSTED,
            json!({"plugin_id": id, "version": record.version,
                   "modules": record.modules.iter().map(|m| &m.id).collect::<Vec<_>>()}),
            EventActor::user(user.username.clone()),
        )
        .await;
    Ok(Json(state.plugin_manager.get(&id).await?))
}

/// Withdrawing only narrows what runs, so `plugins:admin` is enough (as is
/// `plugins:trust`), without a password.
async fn untrust(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<PluginDetail>> {
    if !allowed(&state, &user, Resource::Plugins, Action::Admin).await? {
        require(&state, &user, Resource::PluginTrust, Action::Admin).await?;
    }
    let removed = state.plugin_manager.untrust_modules(&id).await?;
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_TRUST,
            MODULE_UNTRUSTED,
            &id,
            json!({"actor": user.username, "version": removed.version, "reason": "admin"}),
        )
        .await;
    state
        .plugin_runtime
        .events
        .emit(
            MODULE_UNTRUSTED,
            json!({"plugin_id": id, "version": removed.version, "reason": "admin"}),
            EventActor::user(user.username.clone()),
        )
        .await;
    Ok(Json(state.plugin_manager.get(&id).await?))
}

#[derive(serde::Deserialize)]
struct ModuleSessionRequest {
    plugin_id: String,
    contribution_id: String,
}

#[derive(serde::Serialize)]
struct ModuleSessionResponse {
    /// Where the module is imported from; relative to the host origin.
    url: String,
    /// SRI of the module; the browser refuses it on a mismatch.
    integrity: String,
    /// Exposed module of the Module Federation container to render.
    expose: String,
    version: String,
    expires_in_secs: u64,
}

/// Issues a token for one trusted module the caller may see. Answers 404
/// when trusted modules are off, the version is not trusted or the plugin
/// is not enabled, because then the registry holds no module.
async fn module_session(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ModuleSessionRequest>,
) -> ApiResult<Json<ModuleSessionResponse>> {
    let registry = state.plugin_manager.registry();
    let vis = visibility(&state, &user, registry.required_permissions()).await?;
    let allow = check(&vis);
    let asset = registry
        .find_module(&req.plugin_id, &req.contribution_id, &allow)
        .ok_or_else(|| AppError::not_found("Contribution not found"))?;
    let token = state.plugin_manager.ui_sessions().issue_module(
        &asset.plugin_id,
        &asset.version,
        &asset.contribution_id,
        &user.username,
    );
    Ok(Json(ModuleSessionResponse {
        url: format!("/api/v1/plugins/ui-modules/{}/{}", token, asset.path),
        integrity: asset.integrity,
        expose: asset.expose,
        version: asset.version,
        expires_in_secs: ui_assets::SESSION_TTL.as_secs(),
    }))
}

/// Serves the entry file of a trusted module. Anything unknown, revoked,
/// disabled, untrusted or requested from another site is a plain 404.
async fn module_asset(
    State(state): State<AppState>,
    Path((token, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let not_found = || StatusCode::NOT_FOUND.into_response();
    let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
    if !ui_assets::same_origin_fetch(site) {
        return not_found();
    }
    let Some(session) = state
        .plugin_manager
        .ui_sessions()
        .lookup(&token)
        .filter(|s| s.kind == UiSessionKind::Module)
    else {
        return not_found();
    };
    let Some(path) = ui_assets::clean_path(&path) else {
        return not_found();
    };
    let registry = state.plugin_manager.registry();
    let Some(asset) = registry
        .module_asset(
            &session.plugin_id,
            &session.version,
            &session.contribution_id,
        )
        .filter(|a| a.path == path)
    else {
        return not_found();
    };
    let mut out = HeaderMap::new();
    for (k, v) in ui_assets::module_headers(&asset.integrity) {
        if let Ok(v) = v.parse() {
            out.insert(k, v);
        }
    }
    (StatusCode::OK, out, asset.bytes.as_ref().clone()).into_response()
}
