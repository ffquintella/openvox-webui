//! Background scheduler for repository metadata checking.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info};

use crate::config::InventoryConfig;
use crate::db::{DbPool, InventoryRepository};
use crate::services::repo_checker::RepoCheckerService;

#[derive(Clone)]
pub struct RepoCheckerSchedulerState {
    running: Arc<RwLock<bool>>,
    pool: DbPool,
    config: InventoryConfig,
}

impl RepoCheckerSchedulerState {
    pub fn new(pool: DbPool, config: InventoryConfig) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            pool,
            config,
        }
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Repo checker scheduler stop requested");
    }
}

pub fn start_repo_checker_scheduler(
    pool: DbPool,
    config: InventoryConfig,
) -> RepoCheckerSchedulerState {
    let state = RepoCheckerSchedulerState::new(pool, config);
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut running = state_clone.running.write().await;
        *running = true;
    });

    let check_state = state.clone();
    tokio::spawn(async move {
        repo_check_task(check_state).await;
    });

    info!("Repo checker scheduler started");
    state
}

async fn repo_check_task(state: RepoCheckerSchedulerState) {
    let interval_secs = state.config.repo_check_interval_secs.max(300); // minimum 5 minutes
    let mut timer = interval(Duration::from_secs(interval_secs));
    info!(
        "Repo check task started (interval: {}s)",
        interval_secs
    );

    loop {
        timer.tick().await;

        if !*state.running.read().await {
            info!("Repo check task stopping");
            break;
        }

        let repo = InventoryRepository::new(state.pool.clone());
        let service = RepoCheckerService::new(
            repo,
            state.config.repo_check_timeout_secs,
            state.config.repo_check_max_concurrent,
        );

        match service.check_all_repos().await {
            Ok(summary) => {
                info!(
                    "Repo check complete: {}/{} repos succeeded, {} catalog entries upserted",
                    summary.repos_succeeded, summary.repos_checked, summary.catalog_entries_upserted
                );
            }
            Err(err) => {
                error!("Repo check task failed: {}", err);
            }
        }
    }
}
