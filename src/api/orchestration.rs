//! OpenBolt orchestration API (M32, S27).
//!
//! Reading needs `orchestration:read`, running `orchestration:create`,
//! approving `orchestration:approve` and the allowlist and arbitrary commands
//! `orchestration:admin`. A role limited to one group may run against that
//! group only; every other use needs an unscoped grant.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::plugins::allowed;
use crate::{
    middleware::AuthUser,
    models::{Action, Resource},
    services::{
        bolt::{
            model::{BoltJob, BoltJobDetail, JobKind, JobStatus},
            service::{self, Actor, BoltCtx, BoltError, CreateJobInput},
        },
        plugins::interceptors::{InterceptActor, InterceptRefusal, BEFORE_BOLT_JOB_RUN},
    },
    utils::{AppError, AppResult},
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/bolt/status", get(get_status))
        .route("/bolt/tasks", get(list_tasks))
        .route("/bolt/plans", get(list_plans))
        .route("/bolt/discover", get(discover))
        .route("/bolt/allowlist", get(get_allowlist).put(put_allowlist))
        .route("/bolt/jobs", get(list_jobs).post(create_job))
        .route("/bolt/jobs/{id}", get(get_job))
        .route("/bolt/jobs/{id}/approve", post(approve_job))
        .route("/bolt/jobs/{id}/reject", post(reject_job))
        .route("/bolt/jobs/{id}/cancel", post(cancel_job))
        .route("/nodes/{certname}/jobs", get(node_jobs))
}

fn ctx(state: &AppState) -> BoltCtx {
    BoltCtx {
        cfg: state.config.orchestration.bolt.clone(),
        pool: state.db.clone(),
        puppetdb: state.puppetdb.clone(),
    }
}

fn actor(user: &AuthUser) -> Actor {
    Actor {
        id: user.id,
        organization_id: user.organization_id,
        name: user.username.clone(),
    }
}

impl From<BoltError> for AppError {
    fn from(e: BoltError) -> Self {
        match e {
            BoltError::Disabled => AppError::service_unavailable(e.to_string()),
            BoltError::BadRequest(m) => AppError::bad_request(m),
            BoltError::NotFound(m) => AppError::not_found(m),
            BoltError::Forbidden(m) => AppError::forbidden(m),
            BoltError::Conflict(m) => AppError::conflict(m),
            BoltError::Unavailable(m) => AppError::service_unavailable(m),
            BoltError::Internal(inner) => {
                tracing::error!("orchestration error: {:#}", inner);
                AppError::internal("Orchestration operation failed")
            }
        }
    }
}

/// An unscoped grant of `action` (or admin).
async fn require(state: &AppState, user: &AuthUser, action: Action) -> AppResult<()> {
    if allowed(state, user, Resource::Orchestration, action).await? {
        Ok(())
    } else {
        Err(AppError::forbidden(format!(
            "Requires orchestration:{}",
            action.as_str()
        )))
    }
}

/// A grant of `action` on one group, or an unscoped one.
async fn require_for_group(
    state: &AppState,
    user: &AuthUser,
    action: Action,
    group: Uuid,
) -> AppResult<()> {
    if allowed(state, user, Resource::Orchestration, action).await? {
        return Ok(());
    }
    let scoped = state
        .rbac_db
        .check_permission(&user.id, Resource::Orchestration, action, Some(group), None)
        .await
        .map(|c| c.allowed)
        .map_err(|e| AppError::internal(format!("Permission check failed: {e}")))?;
    if scoped {
        Ok(())
    } else {
        Err(AppError::forbidden(format!(
            "Requires orchestration:{} on this group",
            action.as_str()
        )))
    }
}

// ----------------------------------------------------------------------------
// Status, discovery, allowlist
// ----------------------------------------------------------------------------

async fn get_status(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    require(&state, &user, Action::Read).await?;
    Ok(Json(service::status(&ctx(&state)).await))
}

/// Allowed items with their metadata (what the run form needs).
async fn allowed_runnables(
    state: &AppState,
    user: &AuthUser,
    kind: JobKind,
) -> AppResult<Vec<Value>> {
    let c = ctx(state);
    if !c.cfg.enabled {
        return Err(BoltError::Disabled.into());
    }
    let entries = c
        .store()
        .allowlist(user.organization_id)
        .await
        .map_err(BoltError::Internal)?;
    let mut out = Vec::new();
    for e in entries.into_iter().filter(|e| e.kind == kind) {
        // A task that vanished from the module path is skipped, not an error.
        if let Ok(meta) = service::describe(&c.cfg, kind, &e.name).await {
            out.push(json!({
                "kind": kind,
                "name": e.name,
                "description": meta.description,
                "parameters": meta.parameters,
                "supports_noop": meta.supports_noop,
                "requires_approval": e.requires_approval,
            }));
        }
    }
    Ok(out)
}

async fn list_tasks(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    require(&state, &user, Action::Read).await?;
    Ok(Json(allowed_runnables(&state, &user, JobKind::Task).await?))
}

async fn list_plans(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    require(&state, &user, Action::Read).await?;
    Ok(Json(allowed_runnables(&state, &user, JobKind::Plan).await?))
}

/// Everything Bolt finds, with the allowlist state (administrators).
async fn discover(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    require(&state, &user, Action::Admin).await?;
    let c = ctx(&state);
    if !c.cfg.enabled {
        return Err(BoltError::Disabled.into());
    }
    let found = service::discover(&c.cfg).await?;
    let list = c
        .store()
        .allowlist(user.organization_id)
        .await
        .map_err(BoltError::Internal)?;
    Ok(Json(
        found
            .into_iter()
            .map(|r| {
                let entry = list.iter().find(|e| e.kind == r.kind && e.name == r.name);
                json!({
                    "kind": r.kind,
                    "name": r.name,
                    "description": r.description,
                    "allowed": entry.is_some(),
                    "requires_approval": entry.is_some_and(|e| e.requires_approval),
                })
            })
            .collect(),
    ))
}

async fn get_allowlist(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    require(&state, &user, Action::Read).await?;
    let entries = ctx(&state)
        .store()
        .allowlist(user.organization_id)
        .await
        .map_err(BoltError::Internal)?;
    Ok(Json(json!({ "entries": entries })))
}

#[derive(Debug, Deserialize)]
struct AllowlistBody {
    entries: Vec<AllowlistItem>,
}

#[derive(Debug, Deserialize)]
struct AllowlistItem {
    kind: JobKind,
    name: String,
    #[serde(default)]
    requires_approval: bool,
}

async fn put_allowlist(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<AllowlistBody>,
) -> AppResult<Json<Value>> {
    require(&state, &user, Action::Admin).await?;
    let c = ctx(&state);
    if !c.cfg.enabled {
        return Err(BoltError::Disabled.into());
    }
    let mut entries = Vec::new();
    for e in &body.entries {
        if e.kind == JobKind::Command {
            return Err(AppError::bad_request(
                "Only tasks and plans can be allowlisted",
            ));
        }
        if !service::valid_runnable_name(&e.name) {
            return Err(AppError::bad_request(format!(
                "'{}' is not a valid name",
                e.name
            )));
        }
        entries.push((e.kind, e.name.clone(), e.requires_approval));
    }
    c.store()
        .replace_allowlist(user.organization_id, &entries, &user.username)
        .await
        .map_err(BoltError::Internal)?;
    service::audit(
        &state.db,
        user.organization_id,
        Some(user.id),
        service::ACTION_ALLOWLIST,
        "allowlist",
        json!({
            "actor": user.username,
            "entries": entries.iter().map(|(k, n, a)| json!({"kind": k, "name": n, "requires_approval": a})).collect::<Vec<_>>(),
        }),
    )
    .await;
    Ok(Json(
        json!({ "entries": c.store().allowlist(user.organization_id).await.map_err(BoltError::Internal)? }),
    ))
}

// ----------------------------------------------------------------------------
// Jobs
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateJobBody {
    kind: JobKind,
    name: String,
    #[serde(default)]
    parameters: Value,
    #[serde(default)]
    nodes: Vec<String>,
    #[serde(default)]
    group_id: Option<Uuid>,
    #[serde(default)]
    noop: bool,
    #[serde(default)]
    maintenance_window_start: Option<DateTime<Utc>>,
    #[serde(default)]
    maintenance_window_end: Option<DateTime<Utc>>,
}

async fn create_job(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<CreateJobBody>,
) -> AppResult<(StatusCode, Json<BoltJob>)> {
    let c = ctx(&state);
    if !c.cfg.enabled {
        return Err(BoltError::Disabled.into());
    }
    // Commands are an administrator's tool; the rest needs `create`, on the
    // group when the run targets one and nothing else.
    if body.kind == JobKind::Command {
        require(&state, &user, Action::Admin).await?;
    } else if let (Some(group), true) = (body.group_id, body.nodes.is_empty()) {
        require_for_group(&state, &user, Action::Create, group).await?;
    } else {
        require(&state, &user, Action::Create).await?;
    }

    let mut certnames = body.nodes.clone();
    if let Some(group) = body.group_id {
        certnames.extend(
            super::groups::resolve_group_member_certnames(&state, user.organization_id, group)
                .await?,
        );
    }

    // Policy plugins see the request (never a parameter value) before it is stored.
    if state.plugin_manager.has_interceptors(BEFORE_BOLT_JOB_RUN) {
        let names: Vec<String> = body
            .parameters
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let mut targets = certnames.clone();
        targets.sort();
        targets.dedup();
        let payload = json!({
            "organization_id": user.organization_id,
            "kind": body.kind,
            "name": body.name,
            "parameter_names": names,
            "target_count": targets.len(),
            "targets": targets,
            "noop": body.noop,
            "actor": {"kind": "user", "id": user.id},
        });
        let icpt = InterceptActor {
            organization_id: user.organization_id,
            user_id: Some(user.id),
        };
        if let Err(refusal) = state
            .plugin_manager
            .intercept(BEFORE_BOLT_JOB_RUN, &body.name, icpt, payload)
            .await
        {
            return Err(match refusal {
                InterceptRefusal::Vetoed { .. } => AppError::conflict(refusal.to_string()),
                _ => AppError::service_unavailable(refusal.to_string()),
            });
        }
    }

    let job = service::create_job(
        &c,
        &actor(&user),
        CreateJobInput {
            kind: body.kind,
            name: body.name,
            parameters: body.parameters,
            certnames,
            group_id: body.group_id,
            noop: body.noop,
            window: (body.maintenance_window_start, body.maintenance_window_end),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(job)))
}

#[derive(Debug, Deserialize)]
struct JobsQuery {
    status: Option<String>,
    certname: Option<String>,
    limit: Option<i64>,
}

async fn list_jobs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<JobsQuery>,
) -> AppResult<Json<Vec<BoltJob>>> {
    require(&state, &user, Action::Read).await?;
    let status = match q.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(
            JobStatus::parse(s)
                .ok_or_else(|| AppError::bad_request(format!("Unknown status '{s}'")))?,
        ),
    };
    let jobs = ctx(&state)
        .store()
        .list_jobs(
            user.organization_id,
            status,
            q.certname.as_deref(),
            q.limit.unwrap_or(100),
        )
        .await
        .map_err(BoltError::Internal)?;
    Ok(Json(jobs))
}

async fn node_jobs(
    State(state): State<AppState>,
    user: AuthUser,
    Path(certname): Path<String>,
    Query(q): Query<JobsQuery>,
) -> AppResult<Json<Vec<BoltJob>>> {
    require(&state, &user, Action::Read).await?;
    let jobs = ctx(&state)
        .store()
        .list_jobs(
            user.organization_id,
            None,
            Some(&certname),
            q.limit.unwrap_or(50),
        )
        .await
        .map_err(BoltError::Internal)?;
    Ok(Json(jobs))
}

async fn get_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<BoltJobDetail>> {
    require(&state, &user, Action::Read).await?;
    ctx(&state)
        .store()
        .job_detail(user.organization_id, &id)
        .await
        .map_err(BoltError::Internal)?
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("Job '{id}' not found")))
}

#[derive(Debug, Default, Deserialize)]
struct DecisionBody {
    #[serde(default)]
    notes: Option<String>,
}

async fn approve_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: Option<Json<DecisionBody>>,
) -> AppResult<Json<BoltJob>> {
    require(&state, &user, Action::Approve).await?;
    let notes = body.and_then(|b| b.0.notes);
    Ok(Json(
        service::decide(&ctx(&state), &actor(&user), &id, true, notes.as_deref()).await?,
    ))
}

async fn reject_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: Option<Json<DecisionBody>>,
) -> AppResult<Json<BoltJob>> {
    require(&state, &user, Action::Approve).await?;
    let notes = body.and_then(|b| b.0.notes);
    Ok(Json(
        service::decide(&ctx(&state), &actor(&user), &id, false, notes.as_deref()).await?,
    ))
}

/// The requester may cancel their own job; anyone with `approve` may cancel any.
async fn cancel_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<BoltJob>> {
    let c = ctx(&state);
    let job = c
        .store()
        .job(user.organization_id, &id)
        .await
        .map_err(BoltError::Internal)?
        .ok_or_else(|| AppError::not_found(format!("Job '{id}' not found")))?;
    if job.requested_by_id == user.id.to_string() {
        // Their own request: running it needed `create` (possibly on a group).
        let by_group = match job
            .target_group_id
            .as_deref()
            .and_then(|g| Uuid::parse_str(g).ok())
        {
            Some(g) => require_for_group(&state, &user, Action::Create, g).await,
            None => require(&state, &user, Action::Create).await,
        };
        if by_group.is_err() {
            require(&state, &user, Action::Approve).await?;
        }
    } else {
        require(&state, &user, Action::Approve).await?;
    }
    Ok(Json(service::cancel(&c, &actor(&user), &id).await?))
}
