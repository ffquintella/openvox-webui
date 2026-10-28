//! Scheduled plugin jobs (T78).
//!
//! Jobs come from `contributes.jobs` of enabled plugins and run on a UTC cron
//! schedule. A job never overlaps itself, a missed run (host down) is not
//! replayed, and every run is recorded with its outcome.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use tokio::task::JoinHandle;

use super::invoke::{invoke_with_timeout, HandlerKind, InvokeError, PluginHooks, PluginJob};
use super::manager::PluginManager;
use crate::db::PluginEventsRepository;
use crate::models::{PluginJobRunView, PluginJobView};

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("the plugin or job does not exist or the plugin is not enabled")]
    NotFound,
    #[error("the job is already running")]
    AlreadyRunning,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct JobScheduler {
    manager: Arc<PluginManager>,
    running: Mutex<HashSet<(String, String)>>,
}

struct RunGuard {
    owner: Arc<JobScheduler>,
    key: (String, String),
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Ok(mut r) = self.owner.running.lock() {
            r.remove(&self.key);
        }
    }
}

impl JobScheduler {
    pub fn new(manager: Arc<PluginManager>) -> Arc<Self> {
        Arc::new(Self {
            manager,
            running: Mutex::new(HashSet::new()),
        })
    }

    fn repo(&self) -> PluginEventsRepository<'_> {
        PluginEventsRepository::new(self.manager.pool())
    }

    fn try_lock(self: &Arc<Self>, plugin_id: &str, job_id: &str) -> Option<RunGuard> {
        let key = (plugin_id.to_string(), job_id.to_string());
        let mut r = self.running.lock().ok()?;
        r.insert(key.clone()).then(|| RunGuard {
            owner: self.clone(),
            key,
        })
    }

    fn is_running(&self, plugin_id: &str, job_id: &str) -> bool {
        self.running
            .lock()
            .is_ok_and(|r| r.contains(&(plugin_id.to_string(), job_id.to_string())))
    }

    /// Starts every job that is due. Returns the started runs.
    pub async fn tick(self: &Arc<Self>) -> Result<Vec<JoinHandle<()>>> {
        let hooks = self.manager.hooks();
        let now = Utc::now();
        let mut started = Vec::new();
        for p in hooks.plugins() {
            for job in &p.jobs {
                let id = &job.decl.id;
                let state = self.repo().job_state(&p.plugin_id, id).await?;
                let next = state.and_then(|s| s.next_run_at);
                let upcoming = job.schedule.after(&now).next();
                match next {
                    Some(due) if due <= now => {
                        self.repo().set_next_run(&p.plugin_id, id, upcoming).await?;
                        let Some(guard) = self.try_lock(&p.plugin_id, id) else {
                            tracing::debug!(
                                "job {}/{} still running, run skipped",
                                p.plugin_id,
                                id
                            );
                            continue;
                        };
                        let this = self.clone();
                        let (p, job) = (p.clone(), job.clone());
                        started.push(tokio::spawn(async move {
                            let _guard = guard;
                            if let Err(e) = this.execute(&p, &job, "schedule").await {
                                tracing::warn!(
                                    "job {}/{} could not be recorded: {}",
                                    p.plugin_id,
                                    job.decl.id,
                                    e
                                );
                            }
                        }));
                    }
                    Some(_) => {}
                    None => self.repo().set_next_run(&p.plugin_id, id, upcoming).await?,
                }
            }
        }
        Ok(started)
    }

    /// Runs a job now and waits for the outcome (administrator action).
    pub async fn run_now(
        self: &Arc<Self>,
        plugin_id: &str,
        job_id: &str,
    ) -> Result<PluginJobRunView, JobError> {
        let hooks = self.manager.hooks();
        let p = hooks.get(plugin_id).ok_or(JobError::NotFound)?;
        let job = p
            .jobs
            .iter()
            .find(|j| j.decl.id == job_id)
            .ok_or(JobError::NotFound)?;
        let _guard = self
            .try_lock(plugin_id, job_id)
            .ok_or(JobError::AlreadyRunning)?;
        let run_id = self.execute(p, job, "manual").await?;
        let runs = self.repo().list_runs(plugin_id, job_id, 1).await?;
        runs.into_iter()
            .find(|r| r.id == run_id)
            .ok_or(JobError::NotFound)
    }

    async fn execute(&self, p: &PluginHooks, job: &PluginJob, trigger: &str) -> Result<i64> {
        let repo = self.repo();
        let run_id = repo.start_run(&p.plugin_id, &job.decl.id, trigger).await?;
        let started = Instant::now();
        let input = json!({
            "job_id": job.decl.id,
            "trigger": trigger,
            "started_at": Utc::now(),
        });
        let call = p.call(HandlerKind::Job, &job.decl.handler, input);
        let result = invoke_with_timeout(self.manager.as_ref(), call, p.timeout_ms()).await;
        let (status, error) = match &result {
            Ok(_) => ("ok", None),
            Err(InvokeError::Timeout(_)) => {
                ("timeout", result.as_ref().err().map(|e| e.to_string()))
            }
            Err(e) => ("error", Some(e.to_string())),
        };
        repo.finish_run(
            run_id,
            &p.plugin_id,
            &job.decl.id,
            status,
            error.as_deref(),
            started.elapsed().as_millis() as i64,
        )
        .await?;
        if status != "ok" {
            tracing::warn!(
                "job {}/{} finished with {}",
                p.plugin_id,
                job.decl.id,
                status
            );
        }
        Ok(run_id)
    }

    pub async fn list(&self) -> Result<Vec<PluginJobView>> {
        let hooks = self.manager.hooks();
        let mut out = Vec::new();
        for p in hooks.plugins() {
            for job in &p.jobs {
                let s = self.repo().job_state(&p.plugin_id, &job.decl.id).await?;
                out.push(PluginJobView {
                    plugin_id: p.plugin_id.clone(),
                    job_id: job.decl.id.clone(),
                    cron: job.decl.cron.clone(),
                    handler: job.decl.handler.clone(),
                    next_run_at: s.as_ref().and_then(|s| s.next_run_at),
                    last_run_at: s.as_ref().and_then(|s| s.last_run_at),
                    last_status: s.as_ref().and_then(|s| s.last_status.clone()),
                    last_error: s.as_ref().and_then(|s| s.last_error.clone()),
                    run_count: s.as_ref().map_or(0, |s| s.run_count),
                    failure_count: s.as_ref().map_or(0, |s| s.failure_count),
                    running: self.is_running(&p.plugin_id, &job.decl.id),
                });
            }
        }
        Ok(out)
    }

    pub async fn runs(
        &self,
        plugin_id: &str,
        job_id: &str,
        limit: i64,
    ) -> Result<Vec<PluginJobRunView>> {
        self.repo().list_runs(plugin_id, job_id, limit).await
    }
}

pub fn start_job_scheduler(scheduler: Arc<JobScheduler>, tick_secs: u64) {
    let s = scheduler;
    tokio::spawn(async move {
        if let Err(e) = s.repo().close_orphan_runs().await {
            tracing::warn!("could not close interrupted job runs: {}", e);
        }
        let mut tick = tokio::time::interval(Duration::from_secs(tick_secs.max(1)));
        loop {
            tick.tick().await;
            if let Err(e) = s.tick().await {
                tracing::warn!("plugin job scheduler tick failed: {}", e);
            }
        }
    });
}
