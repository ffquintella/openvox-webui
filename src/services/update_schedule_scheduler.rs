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

use crate::db::{repository::GroupRepository, InventoryRepository};
use crate::services::scheduler::calculate_next_run;

type DbPool = SqlitePool;

#[derive(Clone)]
pub struct UpdateScheduleSchedulerState {
    running: Arc<RwLock<bool>>,
    pool: DbPool,
}

impl UpdateScheduleSchedulerState {
    pub fn new(pool: DbPool) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            pool,
        }
    }

    #[allow(dead_code)]
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Update schedule scheduler stop requested");
    }
}

pub fn start_update_schedule_scheduler(pool: DbPool) -> UpdateScheduleSchedulerState {
    let state = UpdateScheduleSchedulerState::new(pool);
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

        if let Err(e) = process_due_schedules(&state.pool).await {
            error!("Update schedule check failed: {}", e);
        }
    }
}

async fn process_due_schedules(pool: &SqlitePool) -> anyhow::Result<()> {
    let inv_repo = InventoryRepository::new(pool.clone());
    let due_schedules = inv_repo.get_due_update_schedules().await?;

    if due_schedules.is_empty() {
        return Ok(());
    }

    info!("Processing {} due update schedules", due_schedules.len());
    let group_repo = GroupRepository::new(pool);

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

        let certnames = match group_repo.get_group_nodes(group_id).await {
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
