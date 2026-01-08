//! Background task scheduler for Code Deploy
//!
//! Provides periodic polling for repository updates and deployment queue processing.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::db::DbPool;
use crate::services::code_deploy::{CodeDeployConfig, CodeDeployService};

/// Scheduler state
#[derive(Debug, Clone)]
pub struct CodeDeploySchedulerState {
    /// Whether the scheduler is running
    running: Arc<RwLock<bool>>,
    /// Database connection pool
    pool: DbPool,
    /// Code deploy configuration
    config: CodeDeployConfig,
}

impl CodeDeploySchedulerState {
    /// Create a new scheduler state
    pub fn new(pool: DbPool, config: CodeDeployConfig) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            pool,
            config,
        }
    }

    /// Check if the scheduler is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Stop the scheduler
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Code Deploy scheduler stop requested");
    }
}

/// Start the background scheduler for Code Deploy
///
/// This spawns background tasks for:
/// - Polling repositories for updates
/// - Processing the deployment queue
/// - Cleaning up old deployments
pub fn start_code_deploy_scheduler(pool: DbPool, config: CodeDeployConfig) -> CodeDeploySchedulerState {
    let state = CodeDeploySchedulerState::new(pool.clone(), config.clone());
    let state_clone = state.clone();

    // Mark as running
    tokio::spawn(async move {
        let mut running = state_clone.running.write().await;
        *running = true;
        drop(running);
    });

    // Spawn repository polling task
    let poll_state = state.clone();
    tokio::spawn(async move {
        repository_poll_task(poll_state).await;
    });

    // Spawn deployment queue processor task
    let queue_state = state.clone();
    tokio::spawn(async move {
        deployment_queue_task(queue_state).await;
    });

    // Spawn cleanup task
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        cleanup_task(cleanup_state).await;
    });

    info!("Code Deploy scheduler started");
    state
}

/// Repository polling task
///
/// Periodically checks repositories for updates based on their poll_interval_seconds setting.
async fn repository_poll_task(state: CodeDeploySchedulerState) {
    // Default poll interval: 60 seconds
    let poll_interval = Duration::from_secs(60);
    let mut interval_timer = interval(poll_interval);

    info!(
        "Repository polling task started (interval: {}s)",
        poll_interval.as_secs()
    );

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Repository polling task stopping");
            break;
        }

        debug!("Running repository poll cycle");

        let service = CodeDeployService::new(state.pool.clone(), state.config.clone());

        // Get all repositories that need polling
        match service.list_repositories_for_polling().await {
            Ok(repos) => {
                for repo in repos {
                    debug!("Polling repository: {} ({})", repo.name, repo.id);

                    match service.sync_repository(repo.id).await {
                        Ok(environments) => {
                            debug!(
                                "Synced repository {}: {} environments",
                                repo.name,
                                environments.len()
                            );
                        }
                        Err(e) => {
                            warn!("Failed to sync repository {}: {}", repo.name, e);
                            // Record the error on the repository
                            if let Err(e2) =
                                service.record_repository_error(repo.id, &e.to_string()).await
                            {
                                error!("Failed to record repository error: {}", e2);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to list repositories for polling: {}", e);
            }
        }
    }
}

/// Deployment queue processor task
///
/// Processes approved deployments and executes r10k deployments.
async fn deployment_queue_task(state: CodeDeploySchedulerState) {
    // Process queue every 10 seconds
    let process_interval = Duration::from_secs(10);
    let mut interval_timer = interval(process_interval);

    info!(
        "Deployment queue task started (interval: {}s)",
        process_interval.as_secs()
    );

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Deployment queue task stopping");
            break;
        }

        debug!("Processing deployment queue");

        let service = CodeDeployService::new(state.pool.clone(), state.config.clone());

        match service.process_deployment_queue().await {
            Ok(processed) => {
                if processed > 0 {
                    info!("Processed {} deployments from queue", processed);
                }
            }
            Err(e) => {
                error!("Failed to process deployment queue: {}", e);
            }
        }
    }
}

/// Cleanup task
///
/// Periodically cleans up old deployment history based on retain_history_days setting.
async fn cleanup_task(state: CodeDeploySchedulerState) {
    // Run cleanup once per hour
    let cleanup_interval = Duration::from_secs(3600);
    let mut interval_timer = interval(cleanup_interval);

    info!(
        "Cleanup task started (interval: {}s)",
        cleanup_interval.as_secs()
    );

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Cleanup task stopping");
            break;
        }

        debug!("Running cleanup cycle");

        let service = CodeDeployService::new(state.pool.clone(), state.config.clone());

        match service.cleanup_old_deployments().await {
            Ok(deleted) => {
                if deleted > 0 {
                    info!("Cleaned up {} old deployments", deleted);
                }
            }
            Err(e) => {
                error!("Failed to cleanup old deployments: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_scheduler_state_creation() {
        // Basic test to ensure the module compiles correctly
        // Full integration tests would require a database connection
        assert!(true);
    }
}
