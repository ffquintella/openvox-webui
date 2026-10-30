//! Directory administration API (M30): directories (read-only view of the
//! configuration, never the bind password), connection test, user and group
//! lookup, provisioning, synchronization and group-to-role mappings.
//!
//! Everything needs `users:admin`; changes also need step-up.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::auth_pipeline::{require_step_up, ApiErr};
use super::plugins::require;
use crate::{
    middleware::AuthUser,
    models::{Action, Resource},
    services::directory::{
        mapping::{self, MappingError, MappingInput},
        service::{self, DirectoryContext, Resolved},
        Directory, AUDIT_DIRECTORY,
    },
    utils::error::AppError,
    AppState,
};

type ApiResult<T> = Result<T, ApiErr>;

const LOOKUP_LIMIT: usize = 50;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{id}", get(get_one))
        .route("/{id}/test", post(test))
        .route("/{id}/users", get(lookup_users))
        .route("/{id}/users/provision", post(provision))
        .route("/{id}/groups", get(lookup_groups))
        .route("/{id}/sync", post(sync_now))
        .route("/{id}/sync-runs", get(sync_runs))
        .route(
            "/{id}/role-mappings",
            get(list_mappings)
                .post(create_mapping)
                .put(replace_mappings),
        )
        .route("/{id}/role-mappings/{mapping_id}", delete(delete_mapping))
}

async fn admin(state: &AppState, user: &AuthUser) -> ApiResult<()> {
    require(state, user, Resource::Users, Action::Admin)
        .await
        .map_err(ApiErr::from)
}

fn find<'a>(state: &'a AppState, id: &str) -> ApiResult<&'a std::sync::Arc<Directory>> {
    state
        .directories
        .get(id)
        .ok_or_else(|| AppError::not_found(format!("Unknown directory '{id}'")).into())
}

fn unavailable(e: impl std::fmt::Display, code: &'static str) -> ApiErr {
    ApiErr::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "service_unavailable",
        e.to_string(),
    )
    .code(code)
}

fn ctx(state: &AppState) -> DirectoryContext<'_> {
    DirectoryContext::new(&state.db, Some(&state.rbac_db))
}

/// The configuration as administrators see it: effective values, never the
/// bind password.
fn view(d: &Directory) -> Value {
    let c = &d.config;
    json!({
        "id": c.id,
        "label": c.label(),
        "kind": c.kind.as_str(),
        "enabled": c.enabled,
        "servers": c.servers,
        "start_tls": c.start_tls,
        "ca_file": c.ca_file,
        "bind_dn": c.bind_dn,
        "base_dn": c.base_dn,
        "user_filter": c.user_filter(),
        "domains": c.domains,
        "attributes": {
            "username": c.username_attr(),
            "email": c.email_attr(),
            "display_name": c.display_name_attr(),
            "id": c.id_attrs(),
        },
        "groups": {
            "base_dn": c.group_base(),
            "filter": c.group_filter(),
            "nested": c.groups.nested,
        },
        "connect_timeout_secs": c.connect_timeout_secs,
        "timeout_secs": c.timeout_secs,
        "page_size": c.page_size,
        "modes": {
            "lookup": c.lookup,
            "jit": c.jit,
            "sync": {"enabled": c.sync.enabled, "interval_secs": c.sync_interval_secs()},
        },
        "deny_without_role": c.deny_without_role,
        "sync_running": service::is_running(d),
    })
}

async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let mut out = Vec::new();
    for d in state.directories.all() {
        let mut v = view(d);
        v["last_sync"] = json!(service::list_runs(&state.db, d.id(), 1)
            .await?
            .into_iter()
            .next());
        v["mappings"] = json!(mapping::list(&state.db, d.id()).await?.len());
        out.push(v);
    }
    Ok(Json(json!({ "directories": out })))
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let d = find(&state, &id)?;
    let mut v = view(d);
    v["last_sync"] = json!(service::list_runs(&state.db, d.id(), 1)
        .await?
        .into_iter()
        .next());
    Ok(Json(v))
}

async fn test(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let d = find(&state, &id)?;
    let result = d.test().await;
    ctx(&state)
        .audit(
            Some(user.id),
            "directory.tested",
            AUDIT_DIRECTORY,
            d.id(),
            json!({"ok": result["ok"], "stage": result["stage"], "error": result["error"]}),
        )
        .await;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct LookupQuery {
    #[serde(default)]
    q: String,
}

fn lookup_allowed(d: &Directory) -> ApiResult<()> {
    if d.config.lookup && d.config.enabled {
        Ok(())
    } else {
        Err(ApiErr::new(
            StatusCode::CONFLICT,
            "conflict",
            "Lookup is disabled for this directory",
        )
        .code("lookup_disabled"))
    }
}

async fn lookup_users(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<LookupQuery>,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let d = find(&state, &id)?;
    lookup_allowed(d)?;
    let users = d
        .search_users(&q.q, LOOKUP_LIMIT)
        .await
        .map_err(|e| unavailable(&e, e.code()))?;
    Ok(Json(json!({ "users": users })))
}

async fn lookup_groups(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<LookupQuery>,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let d = find(&state, &id)?;
    lookup_allowed(d)?;
    let groups = d
        .search_groups(&q.q, LOOKUP_LIMIT)
        .await
        .map_err(|e| unavailable(&e, e.code()))?;
    Ok(Json(json!({ "groups": groups })))
}

#[derive(Deserialize)]
struct ProvisionBody {
    username: String,
}

async fn provision(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<ProvisionBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let d = find(&state, &id)?;
    lookup_allowed(d)?;
    match service::provision(ctx(&state), d, &body.username)
        .await
        .map_err(|e| unavailable(format!("{e:#}"), "directory_unavailable"))?
    {
        Resolved::User {
            user: u,
            created,
            roles,
            ..
        } => {
            ctx(&state)
                .audit(
                    Some(user.id),
                    "directory.user_provision_requested",
                    AUDIT_DIRECTORY,
                    &u.username,
                    json!({"directory": d.id(), "created": created, "roles": roles}),
                )
                .await;
            let status = if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            let public: crate::models::UserPublic = (*u).into();
            Ok((
                status,
                Json(json!({"user": public, "created": created, "roles": roles})),
            ))
        }
        Resolved::Denied("not_found") => {
            Err(AppError::not_found("No such user in the directory").into())
        }
        Resolved::Denied(reason) => Err(ApiErr::new(
            StatusCode::CONFLICT,
            "conflict",
            format!("The user cannot be provisioned: {reason}"),
        )
        .code(reason)),
    }
}

#[derive(Deserialize)]
struct SyncQuery {
    #[serde(default)]
    wait: bool,
}

async fn sync_now(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<SyncQuery>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let d = find(&state, &id)?.clone();
    if !d.config.enabled {
        return Err(AppError::bad_request("The directory is disabled").into());
    }
    if service::is_running(&d) {
        return Err(ApiErr::new(
            StatusCode::CONFLICT,
            "conflict",
            "A synchronization of this directory is already running",
        )
        .code("sync_running"));
    }
    if q.wait {
        let run = service::run_sync(ctx(&state), &d, "manual", Some(user.id))
            .await
            .map_err(|e| ApiErr::new(StatusCode::CONFLICT, "conflict", format!("{e:#}")))?;
        return Ok((StatusCode::OK, Json(json!({ "run": run }))));
    }
    let pool = state.db.clone();
    let rbac = state.rbac_db.clone();
    let actor = user.id;
    tokio::spawn(async move {
        let ctx = DirectoryContext::new(&pool, Some(&rbac));
        if let Err(e) = service::run_sync(ctx, &d, "manual", Some(actor)).await {
            tracing::warn!("Manual sync of '{}' not run: {:#}", d.id(), e);
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "started", "directory": id })),
    ))
}

#[derive(Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
}

async fn sync_runs(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<RunsQuery>,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let d = find(&state, &id)?;
    let runs = service::list_runs(&state.db, d.id(), q.limit.unwrap_or(20).clamp(1, 200)).await?;
    Ok(Json(json!({ "runs": runs })))
}

fn mapping_err(e: MappingError) -> ApiErr {
    match e {
        MappingError::Invalid(m) => AppError::bad_request(m).into(),
        MappingError::Db(e) => e.into(),
    }
}

async fn list_mappings(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    let d = find(&state, &id)?;
    Ok(Json(
        json!({ "mappings": mapping::list(&state.db, d.id()).await? }),
    ))
}

async fn create_mapping(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(input): Json<MappingInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let d = find(&state, &id)?;
    let actor = user.id.to_string();
    let m = mapping::create(&state.db, d.id(), &input, Some(&actor))
        .await
        .map_err(mapping_err)?;
    ctx(&state)
        .audit(
            Some(user.id),
            "directory.mapping_created",
            AUDIT_DIRECTORY,
            d.id(),
            json!({"group_ref": m.group_ref, "kind": m.group_ref_kind, "role": m.role_name}),
        )
        .await;
    Ok((StatusCode::CREATED, Json(json!({ "mapping": m }))))
}

#[derive(Deserialize)]
struct ReplaceBody {
    mappings: Vec<MappingInput>,
}

async fn replace_mappings(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<ReplaceBody>,
) -> ApiResult<Json<Value>> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let d = find(&state, &id)?;
    let actor = user.id.to_string();
    let saved = mapping::replace(&state.db, d.id(), &body.mappings, Some(&actor))
        .await
        .map_err(mapping_err)?;
    ctx(&state)
        .audit(
            Some(user.id),
            "directory.mappings_replaced",
            AUDIT_DIRECTORY,
            d.id(),
            json!({"count": saved.len()}),
        )
        .await;
    Ok(Json(json!({ "mappings": saved })))
}

async fn delete_mapping(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, mapping_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    admin(&state, &user).await?;
    require_step_up(&state, &user).await?;
    let d = find(&state, &id)?;
    if !mapping::delete(&state.db, d.id(), &mapping_id).await? {
        return Err(AppError::not_found("Mapping not found").into());
    }
    ctx(&state)
        .audit(
            Some(user.id),
            "directory.mapping_deleted",
            AUDIT_DIRECTORY,
            d.id(),
            json!({"mapping": mapping_id}),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}
