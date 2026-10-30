//! Scheduled directory synchronization (M30).
//!
//! Every minute the loop checks which directories with `sync.enabled` are due
//! (their `sync.interval_secs` elapsed since the last run in
//! `directory_sync_runs`) and runs them one after the other.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::db::DbPool;
use crate::services::directory::service::{self, DirectoryContext};
use crate::services::directory::DirectoryRegistry;
use crate::services::DbRbacService;

const TICK_SECS: u64 = 60;

#[derive(Debug, Clone)]
pub struct DirectorySyncSchedulerState {
    running: Arc<RwLock<bool>>,
}

impl DirectorySyncSchedulerState {
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Directory sync scheduler stop requested");
    }
}

async fn last_start(pool: &DbPool, dir: &str) -> Option<DateTime<Utc>> {
    let ts: Option<String> = sqlx::query_scalar(
        "SELECT MAX(started_at) FROM directory_sync_runs WHERE directory_id = ?",
    )
    .bind(dir)
    .fetch_one(pool)
    .await
    .ok()
    .flatten();
    ts.and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
        .map(|t| t.with_timezone(&Utc))
}

/// Runs the directories that are due. Public for tests.
pub async fn run_due(
    pool: &DbPool,
    registry: &DirectoryRegistry,
    rbac: Option<&DbRbacService>,
) -> usize {
    let mut ran = 0;
    for dir in registry.enabled().filter(|d| d.config.sync.enabled) {
        let due = match last_start(pool, dir.id()).await {
            Some(t) => (Utc::now() - t).num_seconds() >= dir.config.sync_interval_secs() as i64,
            None => true,
        };
        if !due {
            continue;
        }
        let ctx = DirectoryContext::new(pool, rbac);
        match service::run_sync(ctx, dir, "schedule", None).await {
            Ok(run) => {
                info!(
                    "Directory '{}' sync {}: {} created, {} updated, {} deactivated, {} errors",
                    dir.id(),
                    run.status,
                    run.created,
                    run.updated,
                    run.deactivated,
                    run.errors
                );
                ran += 1;
            }
            Err(e) => warn!("Directory '{}' sync not run: {:#}", dir.id(), e),
        }
    }
    ran
}

/// Starts the loop when at least one directory has synchronization enabled.
pub fn start_directory_sync_scheduler(
    pool: DbPool,
    registry: Arc<DirectoryRegistry>,
    rbac: Arc<DbRbacService>,
) -> Option<DirectorySyncSchedulerState> {
    if !registry.enabled().any(|d| d.config.sync.enabled) {
        return None;
    }
    let running = Arc::new(RwLock::new(true));
    let state = DirectorySyncSchedulerState {
        running: running.clone(),
    };
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(TICK_SECS));
        loop {
            timer.tick().await;
            if !*running.read().await {
                info!("Directory sync scheduler stopping");
                break;
            }
            run_due(&pool, &registry, Some(&rbac)).await;
        }
    });
    Some(state)
}
