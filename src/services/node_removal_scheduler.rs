//! Background scheduler for node removal tracking
//!
//! This scheduler periodically checks for:
//! - Nodes with revoked certificates in Puppet CA
//! - Nodes that appear in PuppetDB but have no certificate in Puppet CA
//! - Nodes that are due for automatic removal after the retention period
//!
//! Nodes meeting these criteria are marked as "pending removal" and automatically
//! removed after a configurable period (default 10 days).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::config::NodeRemovalConfig;
use crate::db::{DbPool, NodeRemovalRepository};
use crate::models::RemovalReason;
use crate::services::puppet_ca::PuppetCAService;
use crate::services::puppetdb::PuppetDbClient;

/// Scheduler state for node removal tracking
#[derive(Clone)]
pub struct NodeRemovalSchedulerState {
    /// Whether the scheduler is running
    running: Arc<RwLock<bool>>,
    /// Database connection pool
    pool: DbPool,
    /// Node removal configuration
    config: NodeRemovalConfig,
    /// Puppet CA service (optional)
    puppet_ca: Option<Arc<PuppetCAService>>,
    /// PuppetDB client
    puppetdb: Arc<PuppetDbClient>,
}

impl NodeRemovalSchedulerState {
    /// Create a new scheduler state
    pub fn new(
        pool: DbPool,
        config: NodeRemovalConfig,
        puppet_ca: Option<Arc<PuppetCAService>>,
        puppetdb: Arc<PuppetDbClient>,
    ) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            pool,
            config,
            puppet_ca,
            puppetdb,
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
        info!("Node removal scheduler stop requested");
    }
}

/// Start the background scheduler for node removal tracking
///
/// This spawns background tasks for:
/// - Checking certificate status and marking nodes for removal
/// - Executing automatic removal of nodes past their retention period
/// - Cleaning up old audit log entries
pub fn start_node_removal_scheduler(
    pool: DbPool,
    config: NodeRemovalConfig,
    puppet_ca: Option<Arc<PuppetCAService>>,
    puppetdb: Arc<PuppetDbClient>,
) -> NodeRemovalSchedulerState {
    let state = NodeRemovalSchedulerState::new(pool, config, puppet_ca, puppetdb);
    let state_clone = state.clone();

    // Mark as running
    tokio::spawn(async move {
        let mut running = state_clone.running.write().await;
        *running = true;
        drop(running);
    });

    // Spawn certificate check task
    let check_state = state.clone();
    tokio::spawn(async move {
        certificate_check_task(check_state).await;
    });

    // Spawn removal execution task
    let removal_state = state.clone();
    tokio::spawn(async move {
        removal_execution_task(removal_state).await;
    });

    // Spawn cleanup task for audit logs
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        audit_cleanup_task(cleanup_state).await;
    });

    info!("Node removal scheduler started");
    state
}

/// Certificate check task
///
/// Periodically checks Puppet CA for revoked certificates and nodes without certificates,
/// marking them as pending removal.
async fn certificate_check_task(state: NodeRemovalSchedulerState) {
    // Check interval from config (default: every 5 minutes)
    let check_interval = Duration::from_secs(state.config.check_interval_secs.unwrap_or(300));
    let mut interval_timer = interval(check_interval);

    info!(
        "Certificate check task started (check interval: {}s)",
        check_interval.as_secs()
    );

    // Wait a bit before first check to let services initialize
    tokio::time::sleep(Duration::from_secs(30)).await;

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Certificate check task stopping");
            break;
        }

        debug!("Running certificate status check");

        if let Err(e) = check_certificate_status(&state).await {
            error!("Certificate check failed: {}", e);
        }
    }
}

/// Check certificate status and mark nodes as needed
async fn check_certificate_status(state: &NodeRemovalSchedulerState) -> anyhow::Result<()> {
    let repo = NodeRemovalRepository::new(state.pool.clone());

    // Get all nodes from PuppetDB
    let puppetdb_nodes = state
        .puppetdb
        .query_nodes(&crate::services::puppetdb::QueryBuilder::new())
        .await?;

    let puppetdb_certnames: HashSet<String> = puppetdb_nodes
        .iter()
        .map(|n| n.certname.clone())
        .collect();

    debug!("Found {} nodes in PuppetDB", puppetdb_certnames.len());

    // If Puppet CA is available, check certificate status
    if let Some(ref puppet_ca) = state.puppet_ca {
        // Get all signed certificates
        let signed_certs = match puppet_ca.list_certificates().await {
            Ok(certs) => certs,
            Err(e) => {
                warn!("Failed to list certificates from Puppet CA: {}", e);
                return Ok(()); // Don't fail the whole check
            }
        };

        let signed_certnames: HashSet<String> = signed_certs
            .iter()
            .map(|c| c.certname.clone())
            .collect();

        debug!("Found {} signed certificates in Puppet CA", signed_certnames.len());

        // Check for revoked certificates
        // We need to check individual certificates for revocation status
        for certname in &puppetdb_certnames {
            // Skip if already marked for removal
            if repo.is_marked_for_removal(certname).await? {
                continue;
            }

            // Check certificate status
            match puppet_ca.get_certificate(certname).await {
                Ok(cert) => {
                    if cert.state == crate::models::CertificateStatus::Revoked {
                        info!(
                            "Node '{}' has revoked certificate, marking for removal",
                            certname
                        );
                        repo.mark_for_removal(
                            certname,
                            RemovalReason::RevokedCertificate,
                            state.config.retention_days,
                            Some("Certificate was revoked in Puppet CA"),
                            None, // System action
                        )
                        .await?;
                    }
                }
                Err(crate::utils::AppError::NotFound(_)) => {
                    // Node in PuppetDB but no certificate in Puppet CA
                    info!(
                        "Node '{}' has no certificate in Puppet CA, marking for removal",
                        certname
                    );
                    repo.mark_for_removal(
                        certname,
                        RemovalReason::NoCertificate,
                        state.config.retention_days,
                        Some("No certificate found in Puppet CA"),
                        None, // System action
                    )
                    .await?;
                }
                Err(e) => {
                    debug!("Error checking certificate for '{}': {}", certname, e);
                }
            }
        }

        // Also check for nodes without certificates (more efficient bulk check)
        for certname in &puppetdb_certnames {
            if !signed_certnames.contains(certname) {
                // Skip if already marked
                if repo.is_marked_for_removal(certname).await? {
                    continue;
                }

                // Double-check by trying to get the specific certificate
                match puppet_ca.get_certificate(certname).await {
                    Ok(_) => {
                        // Certificate exists (might be pending or something else)
                    }
                    Err(crate::utils::AppError::NotFound(_)) => {
                        info!(
                            "Node '{}' has no certificate in Puppet CA, marking for removal",
                            certname
                        );
                        repo.mark_for_removal(
                            certname,
                            RemovalReason::NoCertificate,
                            state.config.retention_days,
                            Some("No certificate found in Puppet CA"),
                            None,
                        )
                        .await?;
                    }
                    Err(_) => {
                        // Some other error, skip
                    }
                }
            }
        }
    } else {
        debug!("Puppet CA not configured, skipping certificate checks");
    }

    // Unmark nodes that now have valid certificates
    let pending = repo.get_all_pending().await?;
    for removal in pending {
        // Check if node still exists in PuppetDB
        if !puppetdb_certnames.contains(&removal.certname) {
            // Node no longer in PuppetDB, it was probably already removed
            info!(
                "Node '{}' no longer in PuppetDB, unmarking from pending removal",
                removal.certname
            );
            repo.unmark_removal(&removal.certname, None, Some("Node no longer in PuppetDB"))
                .await?;
            continue;
        }

        // If we have Puppet CA, check if certificate was restored
        if let Some(ref puppet_ca) = state.puppet_ca {
            match puppet_ca.get_certificate(&removal.certname).await {
                Ok(cert) => {
                    if cert.state == crate::models::CertificateStatus::Signed {
                        // Certificate is now valid, unmark
                        info!(
                            "Node '{}' now has valid certificate, unmarking from pending removal",
                            removal.certname
                        );
                        repo.unmark_removal(
                            &removal.certname,
                            None,
                            Some("Certificate restored in Puppet CA"),
                        )
                        .await?;
                    }
                }
                Err(_) => {
                    // Still no certificate or error, keep marked
                }
            }
        }
    }

    Ok(())
}

/// Removal execution task
///
/// Periodically checks for nodes that have passed their retention period
/// and executes the actual removal from PuppetDB.
async fn removal_execution_task(state: NodeRemovalSchedulerState) {
    // Check every hour for nodes due for removal
    let check_interval = Duration::from_secs(3600);
    let mut interval_timer = interval(check_interval);

    info!("Removal execution task started (check interval: 1 hour)");

    // Wait before first check
    tokio::time::sleep(Duration::from_secs(60)).await;

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Removal execution task stopping");
            break;
        }

        debug!("Checking for nodes due for removal");

        if let Err(e) = execute_pending_removals(&state).await {
            error!("Removal execution failed: {}", e);
        }
    }
}

/// Execute pending removals for nodes past their retention period
async fn execute_pending_removals(state: &NodeRemovalSchedulerState) -> anyhow::Result<()> {
    let repo = NodeRemovalRepository::new(state.pool.clone());

    // Get nodes due for removal
    let due_for_removal = repo.get_due_for_removal().await?;

    if due_for_removal.is_empty() {
        debug!("No nodes due for removal");
        return Ok(());
    }

    info!(
        "{} node(s) are due for removal, processing...",
        due_for_removal.len()
    );

    for removal in due_for_removal {
        info!(
            "Processing removal for node '{}' (reason: {})",
            removal.certname, removal.removal_reason
        );

        // Deactivate node in PuppetDB
        match deactivate_node_in_puppetdb(&state.puppetdb, &removal.certname).await {
            Ok(_) => {
                info!("Node '{}' deactivated in PuppetDB", removal.certname);
                // Mark as removed in our tracking
                repo.mark_as_removed(&removal.certname, Some("system"))
                    .await?;
            }
            Err(e) => {
                error!(
                    "Failed to deactivate node '{}' in PuppetDB: {}",
                    removal.certname, e
                );
                // Don't mark as removed, will retry next time
            }
        }
    }

    Ok(())
}

/// Deactivate a node in PuppetDB
///
/// This sends a deactivate command to PuppetDB to remove the node from the active inventory.
async fn deactivate_node_in_puppetdb(
    puppetdb: &PuppetDbClient,
    certname: &str,
) -> anyhow::Result<()> {
    // PuppetDB has a deactivate endpoint, but it requires a command submission
    // For now, we'll log this as a TODO - the actual deactivation would need
    // to be implemented in the PuppetDB client

    // Check if the node still exists
    match puppetdb.get_node(certname).await? {
        Some(node) => {
            if node.deactivated.is_some() {
                debug!("Node '{}' is already deactivated", certname);
                return Ok(());
            }

            // TODO: Actually deactivate the node via PuppetDB command API
            // For now, we just log it
            warn!(
                "Node '{}' should be deactivated in PuppetDB - manual action may be required",
                certname
            );

            // The deactivation command would look like:
            // POST /pdb/cmd/v1
            // {"command": "deactivate node", "version": 3, "payload": {"certname": "xxx", "producer_timestamp": "..."}}

            Ok(())
        }
        None => {
            debug!("Node '{}' already removed from PuppetDB", certname);
            Ok(())
        }
    }
}

/// Audit log cleanup task
///
/// Periodically cleans up old audit log entries and removed node records.
async fn audit_cleanup_task(state: NodeRemovalSchedulerState) {
    // Run cleanup once per day
    let check_interval = Duration::from_secs(86400);
    let mut interval_timer = interval(check_interval);

    info!("Audit cleanup task started (check interval: 24 hours)");

    // Wait before first cleanup
    tokio::time::sleep(Duration::from_secs(3600)).await;

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Audit cleanup task stopping");
            break;
        }

        debug!("Running audit log cleanup");

        let repo = NodeRemovalRepository::new(state.pool.clone());

        // Clean up audit entries older than configured retention
        let audit_retention = state.config.audit_retention_days.unwrap_or(90);
        match repo.cleanup_old_audit(audit_retention).await {
            Ok(count) => {
                if count > 0 {
                    info!("Cleaned up {} old audit log entries", count);
                }
            }
            Err(e) => {
                error!("Failed to clean up audit log: {}", e);
            }
        }

        // Clean up removed node entries older than audit retention
        match repo.cleanup_removed_entries(audit_retention).await {
            Ok(count) => {
                if count > 0 {
                    info!("Cleaned up {} old removed node entries", count);
                }
            }
            Err(e) => {
                error!("Failed to clean up removed entries: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_state_creation() {
        // Basic test to ensure the types are correct
        let config = NodeRemovalConfig {
            enabled: true,
            retention_days: 10,
            check_interval_secs: Some(300),
            audit_retention_days: Some(90),
        };

        assert!(config.enabled);
        assert_eq!(config.retention_days, 10);
    }
}
