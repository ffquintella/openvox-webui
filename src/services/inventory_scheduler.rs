//! Background scheduler for inventory freshness and version intelligence.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info};

use crate::config::InventoryConfig;
use crate::db::{DbPool, InventoryRepository};

#[derive(Debug, Clone)]
pub struct InventorySchedulerState {
    running: Arc<RwLock<bool>>,
    pool: DbPool,
    config: InventoryConfig,
}

impl InventorySchedulerState {
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
        info!("Inventory scheduler stop requested");
    }
}

pub fn start_inventory_scheduler(pool: DbPool, config: InventoryConfig) -> InventorySchedulerState {
    let state = InventorySchedulerState::new(pool, config);
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut running = state_clone.running.write().await;
        *running = true;
    });

    let catalog_state = state.clone();
    tokio::spawn(async move {
        catalog_refresh_task(catalog_state).await;
    });

    let status_state = state.clone();
    tokio::spawn(async move {
        status_refresh_task(status_state).await;
    });

    info!("Inventory scheduler started");
    state
}

async fn catalog_refresh_task(state: InventorySchedulerState) {
    let mut timer = interval(Duration::from_secs(
        state.config.catalog_refresh_interval_secs.max(60),
    ));
    info!(
        "Inventory catalog refresh task started (interval: {}s)",
        state.config.catalog_refresh_interval_secs.max(60)
    );

    loop {
        timer.tick().await;

        if !*state.running.read().await {
            info!("Inventory catalog refresh task stopping");
            break;
        }

        let repo = InventoryRepository::new(state.pool.clone());
        match repo.refresh_version_catalog().await {
            Ok(entries) => info!("Inventory version catalog refreshed: {} entries", entries),
            Err(err) => error!("Inventory version catalog refresh failed: {}", err),
        }
    }
}

async fn status_refresh_task(state: InventorySchedulerState) {
    let mut timer = interval(Duration::from_secs(
        state.config.status_refresh_interval_secs.max(60),
    ));
    info!(
        "Inventory status refresh task started (interval: {}s)",
        state.config.status_refresh_interval_secs.max(60)
    );

    loop {
        timer.tick().await;

        if !*state.running.read().await {
            info!("Inventory status refresh task stopping");
            break;
        }

        let repo = InventoryRepository::new(state.pool.clone());
        match repo
            .refresh_host_update_statuses(state.config.stale_after_hours)
            .await
        {
            Ok(summary) => info!(
                "Inventory status refreshed: {} nodes, {} stale, {} outdated",
                summary.total_nodes, summary.stale_nodes, summary.outdated_nodes
            ),
            Err(err) => error!("Inventory status refresh failed: {}", err),
        }
    }
}
