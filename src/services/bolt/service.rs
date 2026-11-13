//! Job lifecycle of the OpenBolt integration (T113, T124, T125, T126).
//!
//! Create -> (approve) -> queue -> run -> finish, with the limits, the
//! maintenance window, cancellation, audit entries and events around it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::sync::{watch, Semaphore};
use uuid::Uuid;

use super::config::BoltConfig;
use super::inventory::{valid_host, BoltTarget};
use super::model::{
    sensitive_parameter_names, validate_parameters, BoltJob, JobKind, JobStatus, Runnable,
};
use super::runner::{self, RunRequest};
use super::store::{BoltStore, NewJob};
use crate::db::AuditRepository;
use crate::models::EventActor;
use crate::services::plugins::events::EventBus;
use crate::services::puppetdb::PuppetDbClient;

pub const AUDIT_RESOURCE: &str = "bolt_jobs";
pub const ACTION_REQUESTED: &str = "bolt_job.requested";
pub const ACTION_APPROVED: &str = "bolt_job.approved";
pub const ACTION_REJECTED: &str = "bolt_job.rejected";
pub const ACTION_CANCELLED: &str = "bolt_job.cancelled";
pub const ACTION_STARTED: &str = "bolt_job.started";
pub const ACTION_FINISHED: &str = "bolt_job.finished";
pub const ACTION_ALLOWLIST: &str = "bolt_allowlist.updated";

pub const EVENT_REQUESTED: &str = "bolt.job.requested";
pub const EVENT_APPROVED: &str = "bolt.job.approved";
pub const EVENT_COMPLETED: &str = "bolt.job.completed";

#[derive(Debug, thiserror::Error)]
pub enum BoltError {
    #[error("The OpenBolt integration is disabled")]
    Disabled,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0:#}")]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for BoltError {
    fn from(e: sqlx::Error) -> Self {
        BoltError::Internal(e.into())
    }
}

/// Who acts: a user, with the organization the job belongs to.
#[derive(Debug, Clone)]
pub struct Actor {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
}

/// What a request to run something carries.
#[derive(Debug, Clone)]
pub struct CreateJobInput {
    pub kind: JobKind,
    pub name: String,
    pub parameters: Value,
    pub certnames: Vec<String>,
    pub group_id: Option<Uuid>,
    pub noop: bool,
    pub window: (Option<DateTime<Utc>>, Option<DateTime<Utc>>),
}

/// Everything the service needs, cheap to clone into background tasks.
#[derive(Clone)]
pub struct BoltCtx {
    pub cfg: BoltConfig,
    pub pool: SqlitePool,
    pub puppetdb: Option<Arc<PuppetDbClient>>,
}

impl BoltCtx {
    pub fn store(&self) -> BoltStore {
        BoltStore::new(self.pool.clone())
    }

    fn ensure_enabled(&self) -> Result<(), BoltError> {
        if self.cfg.enabled {
            Ok(())
        } else {
            Err(BoltError::Disabled)
        }
    }
}

// ----------------------------------------------------------------------------
// Process-wide run pool: bounded concurrency and cancellation
// ----------------------------------------------------------------------------

struct Pool {
    permits: Semaphore,
    cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
}

fn pool(cfg: &BoltConfig) -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| Pool {
        permits: Semaphore::new(cfg.max_concurrent_runs.max(1)),
        cancels: Mutex::new(HashMap::new()),
    })
}

/// Signals a running job to stop. False when it is not running here.
pub fn signal_cancel(cfg: &BoltConfig, job_id: &str) -> bool {
    let guard = pool(cfg).cancels.lock().unwrap_or_else(|e| e.into_inner());
    match guard.get(job_id) {
        Some(tx) => tx.send(true).is_ok(),
        None => false,
    }
}

// ----------------------------------------------------------------------------
// Audit and events
// ----------------------------------------------------------------------------

pub async fn audit(
    pool: &SqlitePool,
    org: Uuid,
    user: Option<Uuid>,
    action: &str,
    id: &str,
    details: Value,
) {
    if let Err(e) = AuditRepository::new(pool)
        .insert(
            org,
            user,
            action,
            AUDIT_RESOURCE,
            Some(id),
            Some(&details),
            None,
        )
        .await
    {
        tracing::warn!("Failed to write bolt audit entry '{}': {}", action, e);
    }
}

/// Event payloads carry ids, status and counts, never parameter values or output.
fn event_payload(org: Uuid, job: &BoltJob) -> Value {
    json!({
        "job_id": job.id,
        "organization_id": org,
        "kind": job.kind.as_str(),
        "name": job.name,
        "status": job.status.as_str(),
        "target_count": job.target_count,
        "requested_by": job.requested_by,
        "noop": job.noop,
    })
}

async fn emit(pool: &SqlitePool, event: &str, org: Uuid, job: &BoltJob, actor: EventActor) {
    EventBus::new(pool.clone())
        .emit(event, event_payload(org, job), actor)
        .await;
}

// ----------------------------------------------------------------------------
// Discovery
// ----------------------------------------------------------------------------

const CACHE_TTL: Duration = Duration::from_secs(60);

type DescribeCache = Mutex<HashMap<(JobKind, String), (Instant, Runnable)>>;

fn describe_cache() -> &'static DescribeCache {
    static C: OnceLock<DescribeCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Names and descriptions of the tasks and plans in the module path.
pub async fn discover(cfg: &BoltConfig) -> Result<Vec<Runnable>, BoltError> {
    let mut out = Vec::new();
    for (kind, args, key) in [
        (JobKind::Task, ["task", "show"], "tasks"),
        (JobKind::Plan, ["plan", "show"], "plans"),
    ] {
        let v = runner::show(cfg, &args)
            .await
            .map_err(BoltError::Unavailable)?;
        for item in v[key].as_array().into_iter().flatten() {
            let name = item.get(0).and_then(Value::as_str);
            if let Some(name) = name {
                out.push(Runnable {
                    kind,
                    name: name.to_string(),
                    description: item.get(1).and_then(Value::as_str).map(str::to_string),
                    parameters: Value::Null,
                    supports_noop: false,
                });
            }
        }
    }
    out.sort_by(|a, b| (a.kind.as_str(), &a.name).cmp(&(b.kind.as_str(), &b.name)));
    Ok(out)
}

/// The metadata of one task or plan: parameters and noop support.
pub async fn describe(cfg: &BoltConfig, kind: JobKind, name: &str) -> Result<Runnable, BoltError> {
    if !valid_runnable_name(name) {
        return Err(BoltError::BadRequest(format!(
            "'{name}' is not a valid task or plan name"
        )));
    }
    if let Some((at, r)) = describe_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(kind, name.to_string()))
    {
        if at.elapsed() < CACHE_TTL {
            return Ok(r.clone());
        }
    }
    let sub = if kind == JobKind::Task {
        "task"
    } else {
        "plan"
    };
    let v = runner::show(cfg, &[sub, "show", name])
        .await
        .map_err(BoltError::Unavailable)?;
    let r = parse_runnable(kind, name, &v);
    describe_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert((kind, name.to_string()), (Instant::now(), r.clone()));
    Ok(r)
}

/// Task and plan names: `module` or `module::name`, lower case.
pub fn valid_runnable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name.split("::").all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}

pub fn parse_runnable(kind: JobKind, name: &str, v: &Value) -> Runnable {
    // Tasks keep their metadata under `metadata`; plans at the top level.
    let meta = v.get("metadata").filter(|m| m.is_object()).unwrap_or(v);
    let mut parameters = meta.get("parameters").cloned().unwrap_or_else(|| json!({}));
    if kind == JobKind::Plan {
        // The targets come from the run request, not from a parameter.
        if let Some(m) = parameters.as_object_mut() {
            m.remove("targets");
        }
    }
    Runnable {
        kind,
        name: name.to_string(),
        description: meta
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        parameters,
        supports_noop: meta
            .get("supports_noop")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

// ----------------------------------------------------------------------------
// Create, approve, cancel
// ----------------------------------------------------------------------------

/// Checks the request, stores the job and, when nothing blocks it, queues it.
pub async fn create_job(
    ctx: &BoltCtx,
    actor: &Actor,
    input: CreateJobInput,
) -> Result<BoltJob, BoltError> {
    ctx.ensure_enabled()?;
    let cfg = &ctx.cfg;
    let store = ctx.store();

    let mut certnames: Vec<String> = input
        .certnames
        .iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    certnames.sort();
    if certnames.is_empty() {
        return Err(BoltError::BadRequest(
            "At least one target node is required".into(),
        ));
    }
    if certnames.len() > cfg.max_targets_per_run {
        return Err(BoltError::BadRequest(format!(
            "A run may have at most {} targets ({} requested)",
            cfg.max_targets_per_run,
            certnames.len()
        )));
    }
    if let Some(bad) = certnames.iter().find(|c| !valid_host(c)) {
        return Err(BoltError::BadRequest(format!(
            "'{bad}' is not a valid node name"
        )));
    }
    let (start, end) = input.window;
    if let (Some(s), Some(e)) = (start, end) {
        if s >= e {
            return Err(BoltError::BadRequest(
                "The maintenance window must end after it starts".into(),
            ));
        }
    }
    if end.is_some_and(|e| e <= Utc::now()) {
        return Err(BoltError::BadRequest(
            "The maintenance window has already ended".into(),
        ));
    }

    let (parameters, sensitive, entry_approval, name) = match input.kind {
        JobKind::Command => {
            if !cfg.allow_arbitrary_commands {
                return Err(BoltError::Forbidden(
                    "Arbitrary commands are disabled (orchestration.bolt.allow_arbitrary_commands)"
                        .into(),
                ));
            }
            let cmd = input.name.trim().to_string();
            if cmd.is_empty() || cmd.len() > 4096 || cmd.contains('\0') {
                return Err(BoltError::BadRequest(
                    "The command is empty or too long".into(),
                ));
            }
            // A command is never trusted without a second pair of eyes.
            (json!({}), Vec::new(), true, cmd)
        }
        kind => {
            let entry = store
                .allowlist_entry(actor.organization_id, kind, &input.name)
                .await?
                .ok_or_else(|| {
                    BoltError::Forbidden(format!(
                        "The {} '{}' is not on the allowlist",
                        kind.as_str(),
                        input.name
                    ))
                })?;
            let meta = describe(cfg, kind, &input.name).await?;
            let params = if input.parameters.is_null() {
                json!({})
            } else {
                input.parameters.clone()
            };
            validate_parameters(&meta.parameters, &params).map_err(BoltError::BadRequest)?;
            if input.noop && kind == JobKind::Task && !meta.supports_noop {
                return Err(BoltError::BadRequest(format!(
                    "The task '{}' does not support noop",
                    input.name
                )));
            }
            (
                params,
                sensitive_parameter_names(&meta.parameters),
                entry.requires_approval,
                input.name.clone(),
            )
        }
    };

    // PuppetDB last: the cheap checks above need no network.
    check_nodes_exist(ctx, &certnames).await?;

    let requires_approval = entry_approval || certnames.len() > cfg.approval_target_threshold;
    let status = if requires_approval {
        JobStatus::PendingApproval
    } else {
        JobStatus::Queued
    };
    let job = store
        .create_job(NewJob {
            organization_id: actor.organization_id,
            kind: input.kind,
            name,
            parameters,
            sensitive_names: sensitive,
            targets: certnames.clone(),
            target_group_id: input.group_id.map(|g| g.to_string()),
            noop: input.noop,
            status,
            requires_approval,
            requested_by: actor.name.clone(),
            requested_by_id: actor.id,
            window: input.window,
        })
        .await?;

    audit(
        &ctx.pool,
        actor.organization_id,
        Some(actor.id),
        ACTION_REQUESTED,
        &job.id,
        json!({
            "actor": actor.name,
            "kind": job.kind.as_str(),
            "name": job.name,
            // Names only: values may be secrets.
            "parameter_names": job.parameters.as_object().map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
            "targets": certnames,
            "target_count": certnames.len(),
            "target_group_id": job.target_group_id,
            "noop": job.noop,
            "requires_approval": requires_approval,
            "status": job.status.as_str(),
        }),
    )
    .await;
    emit(
        &ctx.pool,
        EVENT_REQUESTED,
        actor.organization_id,
        &job,
        EventActor::user(actor.name.clone()),
    )
    .await;

    if status == JobStatus::Queued {
        spawn_job(ctx.clone(), job.id.clone());
    }
    Ok(job)
}

async fn check_nodes_exist(ctx: &BoltCtx, certnames: &[String]) -> Result<(), BoltError> {
    let Some(pdb) = &ctx.puppetdb else {
        return Err(BoltError::Unavailable(
            "PuppetDB is not configured, so the nodes cannot be checked".into(),
        ));
    };
    let active = pdb
        .get_active_certnames()
        .await
        .map_err(|e| BoltError::Unavailable(format!("PuppetDB is not reachable: {e}")))?;
    let unknown: Vec<&String> = certnames.iter().filter(|c| !active.contains(*c)).collect();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(BoltError::BadRequest(format!(
            "Unknown or inactive nodes: {}",
            unknown
                .iter()
                .take(5)
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }
}

/// Approves or rejects a job. Nobody approves their own request.
pub async fn decide(
    ctx: &BoltCtx,
    actor: &Actor,
    job_id: &str,
    approved: bool,
    notes: Option<&str>,
) -> Result<BoltJob, BoltError> {
    ctx.ensure_enabled()?;
    let store = ctx.store();
    let job = store
        .job(actor.organization_id, job_id)
        .await?
        .ok_or_else(|| BoltError::NotFound(format!("Job '{job_id}' not found")))?;
    if job.requested_by_id == actor.id.to_string() {
        return Err(BoltError::Forbidden(
            "You cannot approve or reject your own request".into(),
        ));
    }
    if job.status != JobStatus::PendingApproval {
        return Err(BoltError::Conflict(format!(
            "The job is {}, not waiting for approval",
            job.status.as_str()
        )));
    }
    if !store.decide(job_id, approved, &actor.name, notes).await? {
        return Err(BoltError::Conflict(
            "The job changed state meanwhile".into(),
        ));
    }
    let job = store
        .job(actor.organization_id, job_id)
        .await?
        .ok_or_else(|| BoltError::NotFound(format!("Job '{job_id}' not found")))?;
    audit(
        &ctx.pool,
        actor.organization_id,
        Some(actor.id),
        if approved { ACTION_APPROVED } else { ACTION_REJECTED },
        &job.id,
        json!({"actor": actor.name, "requested_by": job.requested_by, "notes": notes, "status": job.status.as_str()}),
    )
    .await;
    if approved {
        emit(
            &ctx.pool,
            EVENT_APPROVED,
            actor.organization_id,
            &job,
            EventActor::user(actor.name.clone()),
        )
        .await;
        spawn_job(ctx.clone(), job.id.clone());
    }
    Ok(job)
}

/// Cancels a job: a waiting one at once, a running one by stopping Bolt.
pub async fn cancel(ctx: &BoltCtx, actor: &Actor, job_id: &str) -> Result<BoltJob, BoltError> {
    ctx.ensure_enabled()?;
    let store = ctx.store();
    let job = store
        .job(actor.organization_id, job_id)
        .await?
        .ok_or_else(|| BoltError::NotFound(format!("Job '{job_id}' not found")))?;
    if job.status.is_terminal() {
        return Err(BoltError::Conflict(format!(
            "The job is already {}",
            job.status.as_str()
        )));
    }
    let cancelled = if job.status == JobStatus::Running {
        signal_cancel(&ctx.cfg, job_id)
    } else {
        store.cancel_waiting(job_id).await?
    };
    if !cancelled {
        return Err(BoltError::Conflict(
            "The job changed state meanwhile".into(),
        ));
    }
    audit(
        &ctx.pool,
        actor.organization_id,
        Some(actor.id),
        ACTION_CANCELLED,
        job_id,
        json!({"actor": actor.name, "requested_by": job.requested_by, "was": job.status.as_str()}),
    )
    .await;
    // A running job turns `cancelled` when its process has stopped.
    store
        .job(actor.organization_id, job_id)
        .await?
        .ok_or_else(|| BoltError::NotFound(format!("Job '{job_id}' not found")))
}

// ----------------------------------------------------------------------------
// Execution
// ----------------------------------------------------------------------------

/// Runs a queued job in the background.
pub fn spawn_job(ctx: BoltCtx, job_id: String) {
    tokio::spawn(async move {
        if let Err(e) = execute_job(&ctx, &job_id).await {
            tracing::error!("Bolt job {} failed to execute: {:#}", job_id, e);
            let _ = ctx
                .store()
                .fail_waiting(&job_id, "internal error before the run")
                .await;
        }
    });
}

/// How long to wait for a maintenance window to open, polling in between.
async fn wait_for_window(
    store: &BoltStore,
    job_id: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<bool, BoltError> {
    loop {
        let now = Utc::now();
        if end.is_some_and(|e| e <= now) {
            store
                .fail_waiting(
                    job_id,
                    "the maintenance window ended before the job could run",
                )
                .await?;
            return Ok(false);
        }
        let Some(s) = start.filter(|s| *s > now) else {
            return Ok(true);
        };
        let wait = (s - now)
            .to_std()
            .unwrap_or(Duration::from_secs(1))
            .min(Duration::from_secs(30));
        tokio::time::sleep(wait).await;
        // Cancelled (or otherwise changed) while waiting?
        match store.runnable(job_id).await? {
            Some(r) if r.job.status == JobStatus::Queued => {}
            _ => return Ok(false),
        }
    }
}

async fn execute_job(ctx: &BoltCtx, job_id: &str) -> Result<(), BoltError> {
    let store = ctx.store();
    let Some(first) = store.runnable(job_id).await? else {
        return Ok(());
    };
    if first.job.status != JobStatus::Queued {
        return Ok(());
    }
    if !wait_for_window(
        &store,
        job_id,
        first.job.maintenance_window_start,
        first.job.maintenance_window_end,
    )
    .await?
    {
        return Ok(());
    }

    let pool = pool(&ctx.cfg);
    let _permit = pool
        .permits
        .acquire()
        .await
        .map_err(|e| BoltError::Internal(anyhow::anyhow!("run pool closed: {e}")))?;
    // The window may have closed while this waited for a free slot.
    if first
        .job
        .maintenance_window_end
        .is_some_and(|e| e <= Utc::now())
    {
        store
            .fail_waiting(
                job_id,
                "the maintenance window ended before the job could run",
            )
            .await?;
        return Ok(());
    }
    if !store
        .transition(job_id, JobStatus::Queued, JobStatus::Running)
        .await?
    {
        return Ok(()); // cancelled while waiting
    }
    let Some(r) = store.runnable(job_id).await? else {
        return Ok(());
    };
    let org = r.organization_id;

    let (tx, rx) = watch::channel(false);
    pool.cancels
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(job_id.to_string(), tx);

    store.mark_targets_running(job_id).await?;
    audit(
        &ctx.pool,
        org,
        None,
        ACTION_STARTED,
        job_id,
        json!({"target_count": r.targets.len(), "kind": r.job.kind.as_str(), "name": r.job.name}),
    )
    .await;

    let targets = resolve_targets(ctx, &r.targets).await;
    let outcome = runner::run(
        &ctx.cfg,
        &RunRequest {
            kind: r.job.kind,
            name: &r.job.name,
            params: &r.parameters,
            targets: &targets,
            noop: r.job.noop,
        },
        rx,
    )
    .await;
    pool.cancels
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(job_id);

    let status = final_status(&outcome);
    store
        .finish(job_id, status, outcome.error.as_deref(), &outcome.targets)
        .await?;
    let succeeded = outcome
        .targets
        .iter()
        .filter(|t| t.status == "success")
        .count();
    audit(
        &ctx.pool,
        org,
        None,
        ACTION_FINISHED,
        job_id,
        json!({
            "status": status.as_str(),
            "succeeded": succeeded,
            "failed": outcome.targets.len() - succeeded,
            "error": outcome.error,
        }),
    )
    .await;
    if let Some(done) = store.job(org, job_id).await? {
        emit(&ctx.pool, EVENT_COMPLETED, org, &done, EventActor::system()).await;
    }
    Ok(())
}

/// The final status of a run from what the process reported.
pub fn final_status(o: &runner::RunOutcome) -> JobStatus {
    if o.cancelled {
        return JobStatus::Cancelled;
    }
    if o.timed_out {
        return JobStatus::TimedOut;
    }
    if o.targets.is_empty() {
        return JobStatus::Failed;
    }
    let ok = o.targets.iter().filter(|t| t.status == "success").count();
    if ok == o.targets.len() {
        JobStatus::Succeeded
    } else if ok == 0 {
        JobStatus::Failed
    } else {
        JobStatus::Partial
    }
}

/// Targets with the transport their `kernel` fact calls for. Without
/// PuppetDB, or without the fact, SSH is assumed.
async fn resolve_targets(ctx: &BoltCtx, certnames: &[String]) -> Vec<BoltTarget> {
    let mut kernels: HashMap<String, String> = HashMap::new();
    if let Some(pdb) = &ctx.puppetdb {
        match pdb.query_facts(Some("kernel")).await {
            Ok(facts) => {
                for f in facts {
                    if let Some(k) = f.value.as_str() {
                        kernels.insert(f.certname, k.to_string());
                    }
                }
            }
            Err(e) => tracing::warn!("Could not read the kernel facts for a Bolt run: {}", e),
        }
    }
    certnames
        .iter()
        .map(|c| BoltTarget {
            certname: c.clone(),
            address: None,
            kernel: kernels.get(c).cloned(),
        })
        .collect()
}

// ----------------------------------------------------------------------------
// Health and maintenance
// ----------------------------------------------------------------------------

/// What the status endpoint and the health check report.
pub async fn status(ctx: &BoltCtx) -> Value {
    let cfg = &ctx.cfg;
    if !cfg.enabled {
        return json!({"enabled": false, "state": "disabled"});
    }
    let (version, error) = match runner::version(cfg).await {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    let project_ok = cfg.project_dir.is_dir();
    let state = if error.is_none() && project_ok {
        "ok"
    } else {
        "degraded"
    };
    json!({
        "enabled": true,
        "state": state,
        "version": version,
        "error": error.or_else(|| (!project_ok).then(|| format!("project directory {} not found", cfg.project_dir.display()))),
        "allow_arbitrary_commands": cfg.allow_arbitrary_commands,
        "limits": {
            "max_targets_per_run": cfg.max_targets_per_run,
            "max_concurrent_runs": cfg.max_concurrent_runs,
            "approval_target_threshold": cfg.approval_target_threshold,
            "timeout_seconds": cfg.timeout_seconds,
            "retention_days": cfg.retention_days,
        },
        "warnings": cfg.security_warnings(),
    })
}

/// Startup: warns about weak settings, recovers jobs a restart interrupted
/// and purges old results once a day.
pub fn start_maintenance(ctx: BoltCtx) {
    if !ctx.cfg.enabled {
        return;
    }
    for w in ctx.cfg.security_warnings() {
        tracing::warn!("{}", w);
    }
    tokio::spawn(async move {
        let store = ctx.store();
        match store.recover().await {
            Ok(queued) => {
                for id in queued {
                    spawn_job(ctx.clone(), id);
                }
            }
            Err(e) => tracing::error!("Bolt job recovery failed: {:#}", e),
        }
        let mut tick = tokio::time::interval(Duration::from_secs(24 * 3600));
        loop {
            tick.tick().await;
            match store.purge(ctx.cfg.retention_days).await {
                Ok(n) if n > 0 => tracing::info!("Purged {} old Bolt jobs", n),
                Ok(_) => {}
                Err(e) => tracing::warn!("Bolt retention purge failed: {:#}", e),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bolt::runner::{RunOutcome, TargetOutcome};

    fn t(status: &str) -> TargetOutcome {
        TargetOutcome {
            certname: "a".into(),
            status: status.into(),
            exit_code: None,
            result: Value::Null,
            output: String::new(),
        }
    }

    #[test]
    fn final_status_covers_every_outcome() {
        let mut o = RunOutcome::default();
        assert_eq!(final_status(&o), JobStatus::Failed); // nothing reported
        o.targets = vec![t("success"), t("success")];
        assert_eq!(final_status(&o), JobStatus::Succeeded);
        o.targets = vec![t("success"), t("failure")];
        assert_eq!(final_status(&o), JobStatus::Partial);
        o.targets = vec![t("failure"), t("skipped")];
        assert_eq!(final_status(&o), JobStatus::Failed);
        o.timed_out = true;
        assert_eq!(final_status(&o), JobStatus::TimedOut);
        o.cancelled = true;
        assert_eq!(final_status(&o), JobStatus::Cancelled);
    }

    #[test]
    fn runnable_names_are_module_style() {
        for ok in ["package", "puppet_agent::run", "a1_b2"] {
            assert!(valid_runnable_name(ok), "{ok}");
        }
        for bad in [
            "", "Package", "a b", "a;b", "::a", "a::", "../x", "a-b", "$(x)",
        ] {
            assert!(!valid_runnable_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn parses_task_and_plan_metadata() {
        let task = json!({"name": "package", "metadata": {
            "description": "Manage packages", "supports_noop": true,
            "parameters": {"name": {"type": "String"}, "password": {"type": "String", "sensitive": true}}}});
        let r = parse_runnable(JobKind::Task, "package", &task);
        assert!(r.supports_noop);
        assert_eq!(r.description.as_deref(), Some("Manage packages"));
        assert_eq!(
            sensitive_parameter_names(&r.parameters),
            vec!["password".to_string()]
        );

        let plan = json!({"name": "deploy", "description": "Deploy", "parameters": {
            "targets": {"type": "TargetSpec"}, "version": {"type": "String"}}});
        let p = parse_runnable(JobKind::Plan, "deploy", &plan);
        assert!(p.parameters.get("targets").is_none());
        assert!(p.parameters.get("version").is_some());
        assert!(!p.supports_noop);
    }

    #[test]
    fn event_payload_carries_no_parameters_or_output() {
        let job = BoltJob {
            id: "j".into(),
            kind: JobKind::Task,
            name: "package".into(),
            parameters: json!({"password": "[REDACTED]"}),
            target_group_id: None,
            target_count: 3,
            noop: false,
            status: JobStatus::Queued,
            requires_approval: false,
            requested_by: "alice".into(),
            requested_by_id: "u".into(),
            approved_by: None,
            approval_notes: None,
            maintenance_window_start: None,
            maintenance_window_end: None,
            error: None,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        };
        let p = event_payload(Uuid::nil(), &job);
        assert_eq!(p["target_count"], 3);
        assert!(p.get("parameters").is_none() && p.get("output").is_none());
    }

    #[tokio::test]
    async fn a_disabled_integration_refuses_everything() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let ctx = BoltCtx {
            cfg: BoltConfig::default(),
            pool,
            puppetdb: None,
        };
        let actor = Actor {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            name: "a".into(),
        };
        let input = CreateJobInput {
            kind: JobKind::Task,
            name: "package".into(),
            parameters: json!({}),
            certnames: vec!["a".into()],
            group_id: None,
            noop: false,
            window: (None, None),
        };
        assert!(matches!(
            create_job(&ctx, &actor, input).await,
            Err(BoltError::Disabled)
        ));
        assert!(matches!(
            decide(&ctx, &actor, "x", true, None).await,
            Err(BoltError::Disabled)
        ));
        assert!(matches!(
            cancel(&ctx, &actor, "x").await,
            Err(BoltError::Disabled)
        ));
    }

    // ------------------------------------------------------------------ flows

    async fn ctx_with(binary: std::path::PathBuf, mutate: impl FnOnce(&mut BoltConfig)) -> BoltCtx {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let _ = sqlx::raw_sql(include_str!(
            "../../../migrations/20261113000001_bolt_orchestration.sql"
        ))
        .execute(&pool)
        .await;
        let mut cfg = BoltConfig {
            enabled: true,
            binary_path: binary,
            timeout_seconds: 10,
            ..Default::default()
        };
        mutate(&mut cfg);
        BoltCtx {
            cfg,
            pool,
            puppetdb: None,
        }
    }

    fn actor(name: &str) -> Actor {
        Actor {
            id: Uuid::new_v4(),
            organization_id: Uuid::nil(),
            name: name.into(),
        }
    }

    async fn queue(ctx: &BoltCtx, requester: &Actor, status: JobStatus) -> BoltJob {
        ctx.store()
            .create_job(NewJob {
                organization_id: requester.organization_id,
                kind: JobKind::Task,
                name: "package".into(),
                parameters: json!({"name": "nginx", "password": "hunter2"}),
                sensitive_names: vec![],
                targets: vec!["a.example".into()],
                target_group_id: None,
                noop: false,
                status,
                requires_approval: status == JobStatus::PendingApproval,
                requested_by: requester.name.clone(),
                requested_by_id: requester.id,
                window: (None, None),
            })
            .await
            .unwrap()
    }

    #[cfg(unix)]
    fn fake_bolt(dir: &runner::PrivateDir, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.write("bolt", &format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_queued_job_runs_and_keeps_no_secret() {
        let dir = runner::PrivateDir::create().unwrap();
        let bin = fake_bolt(
            &dir,
            r#"echo '{"items":[{"target":"a.example","status":"success","value":{"_output":"installed password=hunter2"}}]}'"#,
        );
        let ctx = ctx_with(bin, |_| {}).await;
        let alice = actor("alice");
        let job = queue(&ctx, &alice, JobStatus::Queued).await;
        execute_job(&ctx, &job.id).await.unwrap();
        let d = ctx
            .store()
            .job_detail(alice.organization_id, &job.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(d.job.status, JobStatus::Succeeded);
        assert_eq!(d.targets[0].status, "success");
        assert!(!d.targets[0].output.clone().unwrap().contains("hunter2"));
        let stored = ctx.store().runnable(&job.id).await.unwrap().unwrap();
        assert_eq!(
            stored.parameters["password"],
            crate::services::bolt::model::REDACTED
        );
        // A finished job is not run twice.
        execute_job(&ctx, &job.id).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failing_target_makes_the_job_partial() {
        let dir = runner::PrivateDir::create().unwrap();
        let bin = fake_bolt(
            &dir,
            r#"echo '{"items":[{"target":"a.example","status":"success","value":{"_output":"ok"}},{"target":"b.example","status":"failure","value":{"_error":{"msg":"no"}}}]}'; exit 2"#,
        );
        let ctx = ctx_with(bin, |_| {}).await;
        let alice = actor("alice");
        let job = ctx
            .store()
            .create_job(NewJob {
                organization_id: alice.organization_id,
                kind: JobKind::Task,
                name: "package".into(),
                parameters: json!({}),
                sensitive_names: vec![],
                targets: vec!["a.example".into(), "b.example".into()],
                target_group_id: None,
                noop: false,
                status: JobStatus::Queued,
                requires_approval: false,
                requested_by: "alice".into(),
                requested_by_id: alice.id,
                window: (None, None),
            })
            .await
            .unwrap();
        execute_job(&ctx, &job.id).await.unwrap();
        let j = ctx
            .store()
            .job(alice.organization_id, &job.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(j.status, JobStatus::Partial);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_a_running_job_stops_bolt() {
        let dir = runner::PrivateDir::create().unwrap();
        let bin = fake_bolt(&dir, "sleep 30");
        let ctx = ctx_with(bin, |c| c.timeout_seconds = 60).await;
        let alice = actor("alice");
        let job = queue(&ctx, &alice, JobStatus::Queued).await;
        let run = {
            let (ctx, id) = (ctx.clone(), job.id.clone());
            tokio::spawn(async move { execute_job(&ctx, &id).await })
        };
        for _ in 0..50 {
            if ctx
                .store()
                .job(alice.organization_id, &job.id)
                .await
                .unwrap()
                .unwrap()
                .status
                == JobStatus::Running
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let after = cancel(&ctx, &alice, &job.id).await.unwrap();
        assert!(matches!(
            after.status,
            JobStatus::Running | JobStatus::Cancelled
        ));
        run.await.unwrap().unwrap();
        let j = ctx
            .store()
            .job(alice.organization_id, &job.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(j.status, JobStatus::Cancelled);
        assert!(cancel(&ctx, &alice, &job.id).await.is_err()); // already final
    }

    #[tokio::test]
    async fn nobody_approves_their_own_request() {
        let ctx = ctx_with(std::path::PathBuf::from("/nonexistent/bolt"), |_| {}).await;
        let alice = actor("alice");
        let job = queue(&ctx, &alice, JobStatus::PendingApproval).await;
        assert!(matches!(
            decide(&ctx, &alice, &job.id, true, None).await,
            Err(BoltError::Forbidden(_))
        ));
        let bob = Actor {
            id: Uuid::new_v4(),
            ..alice.clone()
        };
        let rejected = decide(&ctx, &bob, &job.id, false, Some("no"))
            .await
            .unwrap();
        assert_eq!(rejected.status, JobStatus::Rejected);
        assert_eq!(rejected.approval_notes.as_deref(), Some("no"));
        // Already decided.
        assert!(matches!(
            decide(&ctx, &bob, &job.id, true, None).await,
            Err(BoltError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn approval_queues_the_job() {
        let ctx = ctx_with(std::path::PathBuf::from("/nonexistent/bolt"), |_| {}).await;
        let alice = actor("alice");
        let job = queue(&ctx, &alice, JobStatus::PendingApproval).await;
        let bob = Actor {
            id: Uuid::new_v4(),
            name: "bob".into(),
            ..alice.clone()
        };
        let approved = decide(&ctx, &bob, &job.id, true, Some("ok")).await.unwrap();
        assert_eq!(approved.status, JobStatus::Queued);
        assert_eq!(approved.approved_by.as_deref(), Some("bob"));
    }

    #[tokio::test]
    async fn a_waiting_job_cancels_at_once_and_a_closed_window_fails_it() {
        let ctx = ctx_with(std::path::PathBuf::from("/nonexistent/bolt"), |_| {}).await;
        let alice = actor("alice");
        let waiting = queue(&ctx, &alice, JobStatus::PendingApproval).await;
        let c = cancel(&ctx, &alice, &waiting.id).await.unwrap();
        assert_eq!(c.status, JobStatus::Cancelled);

        let late = ctx
            .store()
            .create_job(NewJob {
                organization_id: alice.organization_id,
                kind: JobKind::Task,
                name: "package".into(),
                parameters: json!({}),
                sensitive_names: vec![],
                targets: vec!["a.example".into()],
                target_group_id: None,
                noop: false,
                status: JobStatus::Queued,
                requires_approval: false,
                requested_by: "alice".into(),
                requested_by_id: alice.id,
                window: (None, Some(Utc::now() - chrono::Duration::minutes(1))),
            })
            .await
            .unwrap();
        execute_job(&ctx, &late.id).await.unwrap();
        let j = ctx
            .store()
            .job(alice.organization_id, &late.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(j.status, JobStatus::Failed);
        assert!(j.error.unwrap().contains("maintenance window"));
    }

    #[tokio::test]
    async fn create_job_enforces_limits_and_names_before_any_network_call() {
        let ctx = ctx_with(std::path::PathBuf::from("/nonexistent/bolt"), |c| {
            c.max_targets_per_run = 2
        })
        .await;
        let a = actor("alice");
        let base = CreateJobInput {
            kind: JobKind::Task,
            name: "package".into(),
            parameters: json!({}),
            certnames: vec!["a.example".into()],
            group_id: None,
            noop: false,
            window: (None, None),
        };
        let bad = |f: &dyn Fn(&mut CreateJobInput)| {
            let mut i = base.clone();
            f(&mut i);
            i
        };
        for input in [
            bad(&|i| i.certnames = vec![]),
            bad(&|i| i.certnames = vec!["a".into(), "b".into(), "c".into()]),
            bad(&|i| i.certnames = vec!["a b".into()]),
            bad(&|i| {
                i.window = (
                    Some(Utc::now() + chrono::Duration::hours(2)),
                    Some(Utc::now() + chrono::Duration::hours(1)),
                )
            }),
            bad(&|i| i.window = (None, Some(Utc::now() - chrono::Duration::hours(1)))),
        ] {
            assert!(matches!(
                create_job(&ctx, &a, input).await,
                Err(BoltError::BadRequest(_))
            ));
        }
        // Without PuppetDB the nodes cannot be checked: refused, not trusted.
        assert!(matches!(
            create_job(&ctx, &a, base.clone()).await,
            Err(BoltError::Forbidden(_)) | Err(BoltError::Unavailable(_))
        ));
        // Commands stay off unless configured.
        let cmd = bad(&|i| {
            i.kind = JobKind::Command;
            i.name = "uptime".into();
        });
        assert!(matches!(
            create_job(&ctx, &a, cmd).await,
            Err(BoltError::Forbidden(_))
        ));
    }
}
