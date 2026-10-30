//! SCIM administration (M31, T106) at `/api/v1/scim`: status, tokens and
//! group-to-role mappings. Everything needs `users:admin`; issuing, rotating
//! and revoking tokens and changing mappings also need step-up.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use super::auth_pipeline::{require_step_up, ApiErr};
use super::plugins::require;
use crate::{
    db::AuditRepository,
    middleware::AuthUser,
    models::{default_organization_uuid, Action, AuditLogQuery, Resource},
    services::{
        directory::mapping::MappingError,
        scim::{
            mapping::{self, ScimMappingInput},
            store, tokens, ScimActor, ScimContext, AUDIT_SCIM, ENDPOINT_PATH, PROVIDER_ID,
        },
    },
    utils::error::AppError,
    AppState,
};

type ApiResult<T> = Result<T, ApiErr>;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(status))
        .route("/tokens", get(list_tokens).post(create_token))
        .route("/tokens/{id}", delete(revoke_token))
        .route("/tokens/{id}/rotate", post(rotate_token))
        .route("/groups", get(list_groups))
        .route("/role-mappings", get(list_mappings).post(create_mapping))
        .route("/role-mappings/{id}", delete(delete_mapping))
}

async fn admin(state: &AppState, user: &AuthUser) -> ApiResult<()> {
    require(state, user, Resource::Users, Action::Admin)
        .await
        .map_err(ApiErr::from)
}

fn ctx<'a>(state: &'a AppState, actor: &'a ScimActor) -> ScimContext<'a> {
    ScimContext {
        pool: &state.db,
        rbac: Some(&state.rbac_db),
        config: &state.config.auth.scim,
        actor,
    }
}

fn token_view(t: &tokens::TokenInfo) -> Value {
    let mut v = json!(t);
    v["status"] = json!(t.status(Utc::now()));
    v
}

async fn status(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let cfg = &state.config.auth.scim;
    let all = tokens::list(&state.db).await?;
    let now = Utc::now();
    let active = all.iter().filter(|t| t.status(now) == "active").count();
    let last_activity = all.iter().filter_map(|t| t.last_used_at.clone()).max();
    let recent = AuditRepository::new(&state.db)
        .list(
            default_organization_uuid(),
            &AuditLogQuery {
                resource_type: Some(AUDIT_SCIM.to_string()),
                limit: Some(20),
                ..Default::default()
            },
        )
        .await
        .unwrap_or_default();
    Ok(Json(json!({
        "enabled": cfg.enabled,
        "endpoint": format!("{}{}", cfg.base_url.as_deref().unwrap_or(""), ENDPOINT_PATH),
        "base_url": cfg.base_url,
        "sso": {
            "provider": cfg.sso_provider,
            "link_attribute": cfg.sso_link_attribute.as_str(),
        },
        "limits": {
            "rate_limit_per_minute": cfg.rate_limit_per_minute,
            "max_auth_failures_per_minute": cfg.max_auth_failures_per_minute,
            "max_body_bytes": cfg.max_body_bytes,
            "max_page_size": cfg.max_page_size,
            "max_token_lifetime_days": cfg.max_token_lifetime_days,
        },
        "counts": store::counts(&state.db).await?,
        "tokens": {"active": active, "total": all.len()},
        "last_activity": last_activity,
        "recent": recent,
    })))
}

async fn list_tokens(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let list = tokens::list(&state.db).await?;
    Ok(Json(
        json!({ "tokens": list.iter().map(token_view).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize, Default)]
struct TokenBody {
    #[serde(default)]
    name: Option<String>,
    /// Days until the token expires; none for no expiry (when allowed).
    #[serde(default)]
    expires_in_days: Option<u32>,
}

fn expiry(state: &AppState, days: Option<u32>) -> ApiResult<Option<chrono::DateTime<Utc>>> {
    let max = state.config.auth.scim.max_token_lifetime_days;
    match (days, max) {
        (Some(0), _) => Err(AppError::bad_request("expires_in_days must be at least 1").into()),
        (Some(d), Some(m)) if d > m => Err(AppError::bad_request(format!(
            "Tokens may live at most {m} days (auth.scim.max_token_lifetime_days)"
        ))
        .into()),
        (Some(d), _) if d > 3650 => {
            Err(AppError::bad_request("expires_in_days must be at most 3650").into())
        }
        (None, Some(m)) => Err(AppError::bad_request(format!(
            "An expiry is required: at most {m} days (auth.scim.max_token_lifetime_days)"
        ))
        .into()),
        (Some(d), _) => Ok(Some(Utc::now() + Duration::days(i64::from(d)))),
        (None, None) => Ok(None),
    }
}

async fn create_token(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<TokenBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let name = tokens::validate_name(body.name.as_deref().unwrap_or(""))
        .map_err(|m| ApiErr::from(AppError::bad_request(m)))?;
    let expires_at = expiry(&state, body.expires_in_days)?;
    let created = tokens::create(&state.db, &name, expires_at, Some(user.id)).await?;
    let actor = ScimActor::admin(user.id);
    ctx(&state, &actor)
        .audit(
            None,
            "scim.token_created",
            &created.info.id,
            json!({"name": name, "expires_at": created.info.expires_at}),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": created.token,
            "info": token_view(&created.info),
            "notice": "Copy the token now: it is not shown again.",
        })),
    ))
}

async fn rotate_token(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let body: TokenBody = if body.is_empty() {
        TokenBody::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| ApiErr::from(AppError::bad_request("Invalid JSON body")))?
    };
    let current = tokens::get(&state.db, &id)
        .await?
        .filter(|t| t.revoked_at.is_none())
        .ok_or_else(|| ApiErr::from(AppError::not_found("Token not found or revoked")))?;
    // Without a new lifetime the current expiry is kept while it is in the
    // future; a capped configuration falls back to the cap.
    let kept = current
        .expires_at
        .as_deref()
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
        .map(|e| e.with_timezone(&Utc))
        .filter(|e| *e > Utc::now());
    let expires_at = match (body.expires_in_days, kept) {
        (Some(_), _) => expiry(&state, body.expires_in_days)?,
        (None, Some(e)) => Some(e),
        (None, None) if state.config.auth.scim.max_token_lifetime_days.is_some() => {
            expiry(&state, state.config.auth.scim.max_token_lifetime_days)?
        }
        (None, None) => None,
    };
    let rotated = tokens::rotate(&state.db, &current.id, expires_at)
        .await?
        .ok_or_else(|| ApiErr::from(AppError::not_found("Token not found or revoked")))?;
    let actor = ScimActor::admin(user.id);
    ctx(&state, &actor)
        .audit(
            None,
            "scim.token_rotated",
            &current.id,
            json!({"name": current.name, "expires_at": rotated.info.expires_at}),
        )
        .await;
    Ok(Json(json!({
        "token": rotated.token,
        "info": token_view(&rotated.info),
        "notice": "Copy the token now: it is not shown again. The previous secret no longer works.",
    })))
}

async fn revoke_token(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    if !tokens::revoke(&state.db, &id, Some(user.id)).await? {
        return Err(AppError::not_found("Token not found or already revoked").into());
    }
    let actor = ScimActor::admin(user.id);
    ctx(&state, &actor)
        .audit(None, "scim.token_revoked", &id, json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_groups(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let groups = store::group_summaries(&state.db).await?;
    let mappings = mapping::list(&state.db).await?;
    let out: Vec<Value> = groups
        .iter()
        .map(|g| {
            let r = crate::services::scim::resources::GroupRef {
                id: g.id.clone(),
                display_name: g.display_name.clone(),
                external_id: g.external_id.clone(),
            };
            let roles: Vec<&str> = mappings
                .iter()
                .filter(|m| mapping::group_matches(m, &r))
                .map(|m| m.role_name.as_str())
                .collect();
            let mut v = json!(g);
            v["roles"] = json!(roles);
            v
        })
        .collect();
    Ok(Json(json!({ "groups": out })))
}

async fn list_mappings(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    Ok(Json(json!({ "mappings": mapping::list(&state.db).await? })))
}

fn mapping_err(e: MappingError) -> ApiErr {
    match e {
        MappingError::Invalid(m) => AppError::bad_request(m).into(),
        MappingError::Db(e) => e.into(),
    }
}

async fn create_mapping(
    State(state): State<AppState>,
    user: AuthUser,
    Json(input): Json<ScimMappingInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let actor_id = user.id.to_string();
    let m = mapping::create(&state.db, &input, Some(&actor_id))
        .await
        .map_err(mapping_err)?;
    let actor = ScimActor::admin(user.id);
    let c = ctx(&state, &actor);
    c.audit(
        None,
        "scim.mapping_created",
        PROVIDER_ID,
        json!({"group_ref": m.group_ref, "kind": m.group_ref_kind, "role": m.role_name}),
    )
    .await;
    let reconciled = store::reconcile_all(c)
        .await
        .map_err(|e| ApiErr::from(anyhow::anyhow!(e.detail)))?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "mapping": m, "reconciled_users": reconciled })),
    ))
}

async fn delete_mapping(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    if !mapping::delete(&state.db, &id).await? {
        return Err(AppError::not_found("Mapping not found").into());
    }
    let actor = ScimActor::admin(user.id);
    let c = ctx(&state, &actor);
    c.audit(
        None,
        "scim.mapping_deleted",
        PROVIDER_ID,
        json!({"mapping": id}),
    )
    .await;
    store::reconcile_all(c)
        .await
        .map_err(|e| ApiErr::from(anyhow::anyhow!(e.detail)))?;
    Ok(StatusCode::NO_CONTENT)
}
