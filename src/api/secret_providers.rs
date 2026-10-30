//! Secret provider API (M28): the providers and a test that resolves a
//! reference. The value is never returned.
//!
//! Both endpoints need `plugins:admin`, and every test is audited.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::plugins::{require, ApiResult};
use crate::{
    middleware::AuthUser,
    models::*,
    services::secrets::{SecretContext, SecretRef, SecretResolver, AUDIT_SECRETS},
    utils::error::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_providers))
        .route("/{id}/test", post(test_reference))
}

async fn list_providers(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let resolver = SecretResolver::builtin(&state.config.secrets);
    let items: Vec<Value> = resolver
        .providers()
        .into_iter()
        .map(|(id, kind, health)| json!({"id": id, "kind": kind, "health": health}))
        .collect();
    Ok(Json(json!({ "providers": items })))
}

#[derive(Deserialize)]
struct TestRequest {
    path: String,
    key: Option<String>,
}

async fn test_reference(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<TestRequest>,
) -> ApiResult<Json<Value>> {
    require(&state, &user, Resource::Plugins, Action::Admin).await?;
    let resolver = SecretResolver::builtin(&state.config.secrets);
    if !resolver.providers().iter().any(|(pid, _, _)| *pid == id) {
        return Err(AppError::not_found(format!("Unknown secret provider '{id}'")).into());
    }
    let reference = SecretRef {
        provider: id.clone(),
        path: req.path,
        key: req.key,
        version: None,
    };
    let (result, access) = resolver.resolve(
        &reference,
        &SecretContext::core(format!("api-test:{}", user.username)),
    );
    state
        .plugin_manager
        .audit(
            user.organization_id,
            Some(user.id),
            AUDIT_SECRETS,
            "secret.tested",
            &reference.display_path(),
            json!({"actor": user.username, "result": access.result}),
        )
        .await;
    Ok(Json(match result {
        Ok(_) => json!({"ok": true}),
        Err(e) => json!({"ok": false, "error": e.code(), "message": e.to_string()}),
    }))
}
