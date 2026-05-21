//! Background maintenance scheduler for the dedicated inventory database.
//!
//! Runs two independent timers against the inventory DB pool:
//!
//!   1. **Maintenance cycle** (default: hourly) — prunes
//!      `host_inventory_snapshots` down to `snapshot_retention_per_node` per
//!      certname, checkpoints the WAL, and runs `PRAGMA optimize`.
//!   2. **VACUUM cycle** (default: weekly) — reclaims free pages after
//!      deletions. Acquires an exclusive lock and rewrites the DB file, so
//!      it runs on a longer cadence than the maintenance cycle.
//!
//! Patterned after [`crate::services::backup_scheduler`]. All heavy lifting
//! lives on [`crate::db::InventoryRepository`]; this module is thin glue.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::config::InventoryConfig;
use crate::db::{DbPool, InventoryRepository};
use crate::services::puppetdb::PuppetDbClient;

/// Handle for starting/stopping the inventory maintenance scheduler.
#[derive(Clone)]
pub struct InventoryMaintenanceState {
    running: Arc<RwLock<bool>>,
    pool: DbPool,
    config: InventoryConfig,
    puppetdb: Option<Arc<PuppetDbClient>>,
}

impl InventoryMaintenanceState {
    pub fn new(
        pool: DbPool,
        config: InventoryConfig,
        puppetdb: Option<Arc<PuppetDbClient>>,
    ) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            pool,
            config,
            puppetdb,
        }
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Request the scheduler to stop at its next tick. Fire-and-forget;
    /// returns immediately.
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Inventory maintenance scheduler stop requested");
    }
}

/// Spawn the scheduler. Returns a handle that can be used to request shutdown.
pub fn start_inventory_maintenance(
    pool: DbPool,
    config: InventoryConfig,
    puppetdb: Option<Arc<PuppetDbClient>>,
) -> InventoryMaintenanceState {
    let state = InventoryMaintenanceState::new(pool, config, puppetdb);

    // Flip the running flag.
    let flag_state = state.clone();
    tokio::spawn(async move {
        let mut running = flag_state.running.write().await;
        *running = true;
    });

    // Short-cadence maintenance: prune + WAL checkpoint + PRAGMA optimize.
    let maint_state = state.clone();
    tokio::spawn(async move {
        maintenance_loop(maint_state).await;
    });

    // Long-cadence VACUUM (optional; 0 disables).
    if state.config.vacuum_interval_secs > 0 {
        let vac_state = state.clone();
        tokio::spawn(async move {
            vacuum_loop(vac_state).await;
        });
    } else {
        info!("Inventory VACUUM loop disabled (vacuum_interval_secs = 0)");
    }

    info!("Inventory maintenance scheduler started");
    state
}

async fn maintenance_loop(state: InventoryMaintenanceState) {
    // Clamp to at least 60s so a misconfiguration can't hot-loop the DB.
    let tick = Duration::from_secs(state.config.maintenance_interval_secs.max(60));
    let mut timer = interval(tick);
    info!(
        "Inventory maintenance loop started (interval: {}s, retention: {} snapshots/node)",
        tick.as_secs(),
        state.config.snapshot_retention_per_node
    );

    loop {
        timer.tick().await;

        if !*state.running.read().await {
            info!("Inventory maintenance loop stopping");
            break;
        }

        if let Err(e) = run_maintenance_cycle(&state).await {
            error!("Inventory maintenance cycle failed: {}", e);
        }
    }
}

async fn vacuum_loop(state: InventoryMaintenanceState) {
    // VACUUM must run at least once an hour (clamped) to avoid accidents from
    // misconfiguration. Default is weekly.
    let tick = Duration::from_secs(state.config.vacuum_interval_secs.max(3600));
    let mut timer = interval(tick);
    info!(
        "Inventory VACUUM loop started (interval: {}s)",
        tick.as_secs()
    );

    loop {
        timer.tick().await;

        if !*state.running.read().await {
            info!("Inventory VACUUM loop stopping");
            break;
        }

        let repo = InventoryRepository::new(state.pool.clone());
        match repo.vacuum().await {
            Ok(()) => info!("Inventory DB VACUUM completed"),
            Err(e) => warn!("Inventory DB VACUUM failed: {}", e),
        }
    }
}

/// Prune snapshots, checkpoint the WAL, then refresh planner stats.
async fn run_maintenance_cycle(state: &InventoryMaintenanceState) -> anyhow::Result<()> {
    let repo = InventoryRepository::new(state.pool.clone());

    // Prune inventory rows whose certnames no longer appear in PuppetDB's
    // active roster (deactivated or expired nodes). Without this the
    // dashboard's "reporting nodes" count drifts above the live node count.
    if let Some(ref pdb) = state.puppetdb {
        match prune_inactive_nodes(&repo, pdb).await {
            Ok(0) => {}
            Ok(n) => info!("Pruned inventory for {} inactive node(s)", n),
            Err(e) => warn!("Failed to prune inactive nodes from inventory: {}", e),
        }
    }

    let retention = state.config.snapshot_retention_per_node;
    if retention == 0 {
        warn!("Inventory snapshot retention is 0; pruning is disabled");
    } else {
        match repo.prune_snapshots(retention).await {
            Ok(stats) => {
                if stats.snapshots_deleted > 0 {
                    info!(
                        "Pruned {} inventory snapshots beyond retention {}",
                        stats.snapshots_deleted, retention
                    );
                }
            }
            Err(e) => warn!("Failed to prune inventory snapshots: {}", e),
        }
    }

    if let Err(e) = repo.checkpoint_wal().await {
        warn!("Failed to checkpoint inventory WAL: {}", e);
    }

    if let Err(e) = repo.optimize().await {
        warn!("Failed to run PRAGMA optimize on inventory DB: {}", e);
    }

    Ok(())
}

/// Drop inventory rows whose certnames are no longer in PuppetDB's active
/// roster. Returns the number of certnames purged.
///
/// Guarded against a PuppetDB outage: if the active roster comes back empty
/// we treat it as untrusted and skip pruning, otherwise a transient outage
/// would wipe every inventory row on the first cycle.
async fn prune_inactive_nodes(
    repo: &InventoryRepository,
    puppetdb: &PuppetDbClient,
) -> anyhow::Result<u64> {
    let active = puppetdb.get_active_certnames().await?;
    if active.is_empty() {
        warn!("PuppetDB returned no active nodes; skipping inventory prune");
        return Ok(0);
    }

    let inventory = repo.list_inventory_certnames().await?;
    let stale: Vec<String> = inventory
        .into_iter()
        .filter(|c| !active.contains(c))
        .collect();

    if stale.is_empty() {
        return Ok(0);
    }

    repo.prune_nodes_by_certname(&stale).await
}
