//! Update schedule scheduler service.
//!
//! Background task that processes due group update schedules
//! and creates UpdateJob records via the existing orchestration pipeline.

use std::sync::Arc;

use chrono::Utc;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use crate::db::{repository::GroupRepository, InventoryRepository, SettingsRepository};
use crate::services::notification::NotificationService;
use crate::services::puppetdb::PuppetDbClient;
use crate::services::scheduler::calculate_next_run;
use crate::services::AlertingService;

type DbPool = SqlitePool;

/// Default maximum update-job runtime (minutes) when the setting is unset.
const DEFAULT_MAX_RUNTIME_MINUTES: i64 = 240;

#[derive(Clone)]
pub struct UpdateScheduleSchedulerState {
    running: Arc<RwLock<bool>>,
    /// Main application DB (node_groups, classification_rules, pinned_nodes).
    main_pool: DbPool,
    /// Dedicated inventory DB (update_jobs, update_job_targets,
    /// update_job_results, group_update_schedules).
    inventory_pool: DbPool,
    /// PuppetDB client used to classify rule-matched group members. When
    /// absent, only pinned nodes are resolved.
    puppetdb: Option<Arc<PuppetDbClient>>,
    /// Notification service used to deliver update-job alerts.
    notification_service: Arc<NotificationService>,
}

impl UpdateScheduleSchedulerState {
    pub fn new(
        main_pool: DbPool,
        inventory_pool: DbPool,
        puppetdb: Option<Arc<PuppetDbClient>>,
        notification_service: Arc<NotificationService>,
    ) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            main_pool,
            inventory_pool,
            puppetdb,
            notification_service,
        }
    }

    #[allow(dead_code)]
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Update schedule scheduler stop requested");
    }
}

pub fn start_update_schedule_scheduler(
    main_pool: DbPool,
    inventory_pool: DbPool,
    puppetdb: Option<Arc<PuppetDbClient>>,
    notification_service: Arc<NotificationService>,
) -> UpdateScheduleSchedulerState {
    let state = UpdateScheduleSchedulerState::new(
        main_pool,
        inventory_pool,
        puppetdb,
        notification_service,
    );
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut running = state_clone.running.write().await;
        *running = true;
        drop(running);
    });

    let task_state = state.clone();
    tokio::spawn(async move {
        schedule_check_task(task_state).await;
    });

    info!("Update schedule scheduler started");
    state
}

async fn schedule_check_task(state: UpdateScheduleSchedulerState) {
    // Wait a bit before first check to let the app fully start
    tokio::time::sleep(Duration::from_secs(30)).await;

    let mut tick = interval(Duration::from_secs(60));

    loop {
        tick.tick().await;

        let running = state.running.read().await;
        if !*running {
            info!("Update schedule scheduler stopping");
            break;
        }
        drop(running);

        if let Err(e) = process_due_schedules(
            &state.main_pool,
            &state.inventory_pool,
            state.puppetdb.as_deref(),
        )
        .await
        {
            error!("Update schedule check failed: {}", e);
        }

        if let Err(e) = enforce_update_job_limits_and_alerts(&state).await {
            error!("Update job limit/alert check failed: {}", e);
        }
    }
}

async fn process_due_schedules(
    main_pool: &SqlitePool,
    inventory_pool: &SqlitePool,
    puppetdb: Option<&PuppetDbClient>,
) -> anyhow::Result<()> {
    let inv_repo = InventoryRepository::new(inventory_pool.clone());
    let due_schedules = inv_repo.get_due_update_schedules().await?;

    if due_schedules.is_empty() {
        return Ok(());
    }

    info!("Processing {} due update schedules", due_schedules.len());
    // `GroupRepository` resolves node membership against the main DB.
    let group_repo = GroupRepository::new(main_pool);

    for schedule in due_schedules {
        let group_id = match uuid::Uuid::parse_str(&schedule.group_id) {
            Ok(id) => id,
            Err(e) => {
                warn!(
                    "Invalid group_id '{}' in schedule '{}': {}",
                    schedule.group_id, schedule.id, e
                );
                continue;
            }
        };

        // Resolve the group's organization so classification can consider the
        // full group hierarchy in that org.
        let org_id = match group_repo.get_group_organization_id(group_id).await {
            Ok(Some(org_id)) => org_id,
            Ok(None) => {
                warn!(
                    "Group '{}' in schedule '{}' no longer exists, skipping",
                    schedule.group_id, schedule.id
                );
                continue;
            }
            Err(e) => {
                warn!(
                    "Failed to resolve organization for group '{}' in schedule '{}': {}",
                    schedule.group_id, schedule.id, e
                );
                continue;
            }
        };

        let certnames = match crate::api::groups::classify_group_members(
            &group_repo,
            puppetdb,
            org_id,
            group_id,
        )
        .await
        {
            Ok(nodes) => nodes,
            Err(e) => {
                warn!(
                    "Failed to resolve nodes for group '{}' in schedule '{}': {}",
                    schedule.group_id, schedule.id, e
                );
                continue;
            }
        };

        if certnames.is_empty() {
            warn!(
                "No nodes found for group '{}' in schedule '{}', skipping",
                schedule.group_id, schedule.id
            );
            // Still advance the schedule to prevent re-firing
            let next_run = compute_next_run(&schedule);
            let _ = inv_repo
                .update_schedule_after_run(
                    &schedule.id,
                    Utc::now(),
                    next_run,
                    "",
                    schedule.schedule_type == "one_time",
                )
                .await;
            continue;
        }

        match inv_repo
            .create_update_job(
                schedule.operation_type,
                &schedule.package_names,
                Some(&schedule.group_id),
                &certnames,
                schedule.requires_approval,
                None, // immediate execution
                None,
                None,
                "update-schedule-scheduler",
                Some(&format!("Auto-created from schedule '{}'", schedule.name)),
            )
            .await
        {
            Ok(job) => {
                info!(
                    "Created update job '{}' from schedule '{}' for {} nodes",
                    job.id,
                    schedule.name,
                    certnames.len()
                );

                let next_run = compute_next_run(&schedule);
                let is_one_time = schedule.schedule_type == "one_time";

                if let Err(e) = inv_repo
                    .update_schedule_after_run(
                        &schedule.id,
                        Utc::now(),
                        next_run,
                        &job.id,
                        is_one_time,
                    )
                    .await
                {
                    error!(
                        "Failed to update schedule '{}' after run: {}",
                        schedule.id, e
                    );
                }
            }
            Err(e) => {
                error!(
                    "Failed to create update job from schedule '{}': {}",
                    schedule.id, e
                );
            }
        }
    }

    Ok(())
}

fn compute_next_run(
    schedule: &crate::models::GroupUpdateSchedule,
) -> Option<chrono::DateTime<Utc>> {
    match schedule.schedule_type.as_str() {
        "recurring" => schedule
            .cron_expression
            .as_deref()
            .and_then(|cron| calculate_next_run(cron, "UTC")),
        "one_time" => None, // one-time schedules don't have a next run
        _ => None,
    }
}

/// Reads the configured maximum update-job runtime (minutes) from settings,
/// defaulting to [`DEFAULT_MAX_RUNTIME_MINUTES`].
async fn read_max_runtime_minutes(main_pool: &SqlitePool) -> i64 {
    let settings_repo = SettingsRepository::new(main_pool.clone());
    match settings_repo.get_setting("update_jobs.max_runtime_minutes").await {
        Ok(Some(setting)) => setting
            .value
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|m| *m > 0)
            .unwrap_or(DEFAULT_MAX_RUNTIME_MINUTES),
        Ok(None) => DEFAULT_MAX_RUNTIME_MINUTES,
        Err(e) => {
            warn!(
                "Failed to read update_jobs.max_runtime_minutes setting, using default {}: {}",
                DEFAULT_MAX_RUNTIME_MINUTES, e
            );
            DEFAULT_MAX_RUNTIME_MINUTES
        }
    }
}

/// Fails update jobs that exceed the configured maximum runtime and then
/// evaluates user-defined `update_job` alert rules against recent jobs.
async fn enforce_update_job_limits_and_alerts(
    state: &UpdateScheduleSchedulerState,
) -> anyhow::Result<()> {
    let max_minutes = read_max_runtime_minutes(&state.main_pool).await;

    // Fail jobs that have been running longer than the configured limit.
    let inv_repo = InventoryRepository::new(state.inventory_pool.clone());
    let failed = inv_repo.fail_overrunning_jobs(max_minutes).await?;
    if !failed.is_empty() {
        warn!(
            "Failed {} update job(s) exceeding the maximum runtime of {} minutes",
            failed.len(),
            max_minutes
        );
    }

    // Evaluate update-job alert rules (failures and timeouts).
    let alerting = AlertingService::new(
        state.main_pool.clone(),
        state.puppetdb.clone(),
        Some(state.notification_service.clone()),
    )
    .with_inventory_pool(state.inventory_pool.clone());

    match alerting.evaluate_update_job_rules().await {
        Ok(alerts) if !alerts.is_empty() => {
            info!("Triggered {} update job alert(s)", alerts.len());
        }
        Ok(_) => {}
        Err(e) => error!("Failed to evaluate update job alert rules: {}", e),
    }

    Ok(())
}
