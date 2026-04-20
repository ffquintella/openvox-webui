//! Background scheduler for CVE feed synchronization and vulnerability matching.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info};

use crate::config::CveConfig;
use crate::db::{CveRepository, DbPool};
use crate::models::{CreateNotificationRequest, NotificationType};
use crate::services::cve_feed::CveFeedService;
use crate::services::notification::NotificationService;

#[derive(Clone)]
pub struct CveSchedulerState {
    running: Arc<RwLock<bool>>,
    pool: DbPool,
    config: CveConfig,
    notification_service: Option<Arc<NotificationService>>,
}

impl CveSchedulerState {
    pub fn new(
        pool: DbPool,
        config: CveConfig,
        notification_service: Option<Arc<NotificationService>>,
    ) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            pool,
            config,
            notification_service,
        }
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("CVE scheduler stop requested");
    }
}

pub fn start_cve_scheduler(
    pool: DbPool,
    config: CveConfig,
    notification_service: Option<Arc<NotificationService>>,
) -> CveSchedulerState {
    let state = CveSchedulerState::new(pool, config, notification_service);
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut running = state_clone.running.write().await;
        *running = true;
    });

    // Seed default feeds on startup
    let seed_state = state.clone();
    tokio::spawn(async move {
        let repo = CveRepository::new(seed_state.pool.clone());
        if let Err(e) = repo.seed_default_feeds().await {
            error!("Failed to seed default CVE feeds: {}", e);
        }
    });

    let sync_state = state.clone();
    tokio::spawn(async move {
        feed_sync_task(sync_state).await;
    });

    let match_state = state.clone();
    tokio::spawn(async move {
        vulnerability_match_task(match_state).await;
    });

    info!("CVE scheduler started");
    state
}

async fn feed_sync_task(state: CveSchedulerState) {
    let interval_secs = state.config.sync_interval_secs.max(300); // minimum 5 minutes
    let mut timer = interval(Duration::from_secs(interval_secs));
    info!("CVE feed sync task started (interval: {}s)", interval_secs);

    loop {
        timer.tick().await;

        if !*state.running.read().await {
            info!("CVE feed sync task stopping");
            break;
        }

        let repo = CveRepository::new(state.pool.clone());
        let feeds = match repo.list_feed_sources().await {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to list CVE feeds: {}", e);
                continue;
            }
        };

        let enabled_feeds: Vec<_> = feeds.into_iter().filter(|f| f.enabled).collect();
        if enabled_feeds.is_empty() {
            continue;
        }

        let service = CveFeedService::new(CveRepository::new(state.pool.clone()));
        for feed in &enabled_feeds {
            match service.sync_feed(feed).await {
                Ok(result) => {
                    info!(
                        "CVE feed '{}' synced: {} entries ({} new, {} updated, {} matches)",
                        feed.name,
                        result.entries_processed,
                        result.entries_new,
                        result.entries_updated,
                        result.package_matches_created
                    );
                    if !result.errors.is_empty() {
                        for err in &result.errors {
                            error!("CVE feed '{}' error: {}", feed.name, err);
                        }
                    }
                }
                Err(e) => {
                    error!("CVE feed '{}' sync failed: {}", feed.name, e);
                }
            }
        }
    }
}

async fn vulnerability_match_task(state: CveSchedulerState) {
    let interval_secs = state.config.match_refresh_interval_secs.max(300);
    let mut timer = interval(Duration::from_secs(interval_secs));
    info!(
        "CVE vulnerability match task started (interval: {}s)",
        interval_secs
    );

    loop {
        timer.tick().await;

        if !*state.running.read().await {
            info!("CVE vulnerability match task stopping");
            break;
        }

        let repo = CveRepository::new(state.pool.clone());
        match repo.refresh_host_vulnerability_matches().await {
            Ok(count) => {
                info!("CVE vulnerability matches refreshed: {} matches", count);

                // Emit notifications for critical/KEV vulnerabilities
                if count > 0 {
                    if let Some(ns) = &state.notification_service {
                        match repo.get_fleet_vulnerability_dashboard().await {
                            Ok(dashboard) => {
                                let critical = dashboard
                                    .severity_distribution
                                    .iter()
                                    .find(|s| s.severity == "critical")
                                    .map(|s| s.count)
                                    .unwrap_or(0);
                                let kev = dashboard.kev_count;

                                if (state.config.alert_on_critical && critical > 0)
                                    || (state.config.alert_on_kev && kev > 0)
                                {
                                    let mut parts = Vec::new();
                                    if critical > 0 {
                                        parts.push(format!("{} critical", critical));
                                    }
                                    if kev > 0 {
                                        parts.push(format!("{} known-exploited", kev));
                                    }
                                    let message = format!(
                                        "Vulnerability scan found {} CVEs across {} nodes: {}",
                                        dashboard.total_cves_matched,
                                        dashboard.total_vulnerable_nodes,
                                        parts.join(", ")
                                    );

                                    let req = CreateNotificationRequest {
                                        user_id: "system".to_string(),
                                        organization_id: None,
                                        title: "Vulnerabilities Detected".to_string(),
                                        message,
                                        r#type: NotificationType::Warning,
                                        category: Some("vulnerability".to_string()),
                                        link: Some("/updates".to_string()),
                                        expires_at: None,
                                        metadata: None,
                                    };

                                    if let Err(e) = ns.create_notification(req).await {
                                        error!(
                                            "Failed to create vulnerability notification: {}",
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Failed to get vulnerability dashboard for notification: {}",
                                    e
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("CVE vulnerability match refresh failed: {}", e);
            }
        }
    }
}
