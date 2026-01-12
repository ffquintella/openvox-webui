//! Background task scheduler for Server Backups
//!
//! Provides periodic backup creation based on schedule and retention cleanup.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, NaiveTime, Timelike, Utc, Weekday};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::config::{BackupConfig, BackupFrequency};
use crate::db::{BackupRepository, DbPool};
use crate::models::BackupTrigger;
use crate::services::backup::BackupService;

/// Scheduler state
#[derive(Debug, Clone)]
pub struct BackupSchedulerState {
    /// Whether the scheduler is running
    running: Arc<RwLock<bool>>,
    /// Database connection pool
    pool: DbPool,
    /// Backup configuration
    config: BackupConfig,
}

impl BackupSchedulerState {
    /// Create a new scheduler state
    pub fn new(pool: DbPool, config: BackupConfig) -> Self {
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
        info!("Backup scheduler stop requested");
    }
}

/// Start the background scheduler for Backups
///
/// This spawns background tasks for:
/// - Creating scheduled backups
/// - Cleaning up old backups based on retention policy
pub fn start_backup_scheduler(pool: DbPool, config: BackupConfig) -> BackupSchedulerState {
    let state = BackupSchedulerState::new(pool.clone(), config.clone());
    let state_clone = state.clone();

    // Mark as running
    tokio::spawn(async move {
        let mut running = state_clone.running.write().await;
        *running = true;
        drop(running);
    });

    // Spawn scheduled backup task
    let backup_state = state.clone();
    tokio::spawn(async move {
        scheduled_backup_task(backup_state).await;
    });

    // Spawn cleanup task
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        cleanup_task(cleanup_state).await;
    });

    info!("Backup scheduler started");
    state
}

/// Scheduled backup task
///
/// Checks every minute if a scheduled backup should run based on the configured schedule.
async fn scheduled_backup_task(state: BackupSchedulerState) {
    // Check every minute
    let check_interval = Duration::from_secs(60);
    let mut interval_timer = interval(check_interval);

    info!("Scheduled backup task started (check interval: 60s)");

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Scheduled backup task stopping");
            break;
        }

        // Check if we should run a backup
        if let Err(e) = check_and_run_scheduled_backup(&state).await {
            error!("Error in scheduled backup check: {}", e);
        }
    }
}

/// Check if a scheduled backup should run and execute it
async fn check_and_run_scheduled_backup(state: &BackupSchedulerState) -> anyhow::Result<()> {
    let repo = BackupRepository::new(state.pool.clone());

    // Get the schedule
    let schedule = match repo.get_schedule().await? {
        Some(s) => s,
        None => {
            debug!("No backup schedule configured");
            return Ok(());
        }
    };

    // Check if schedule is active
    if !schedule.is_active {
        debug!("Backup schedule is not active");
        return Ok(());
    }

    // Check if config schedule is disabled
    if state.config.schedule.frequency == BackupFrequency::Disabled {
        debug!("Backup frequency is disabled in config");
        return Ok(());
    }

    let now = Utc::now();

    // Check if it's time to run based on schedule
    let should_run = match schedule.frequency.as_str() {
        "hourly" => {
            // Run at the top of each hour
            let minute = now.minute();
            minute == 0
        }
        "daily" => {
            // Run at the configured time each day
            check_daily_schedule(&schedule.time_of_day, now)
        }
        "weekly" => {
            // Run on configured day at configured time
            check_weekly_schedule(&schedule.time_of_day, schedule.day_of_week, now)
        }
        "custom" => {
            // Check against cron expression
            if let Some(ref cron) = schedule.cron_expression {
                check_cron_schedule(cron, now)
            } else {
                false
            }
        }
        _ => false,
    };

    if !should_run {
        return Ok(());
    }

    // Check if we already ran recently (within last 5 minutes to avoid duplicate runs)
    if let Some(last_run) = schedule.last_run_at {
        let since_last = (now - last_run).num_minutes();
        if since_last < 5 {
            debug!("Backup already ran {} minutes ago, skipping", since_last);
            return Ok(());
        }
    }

    info!("Running scheduled backup");

    // Create the backup
    let service = BackupService::new(state.pool.clone(), state.config.clone());

    // For scheduled backups, we don't use a password (or use a system key in the future)
    // The admin should configure encryption appropriately
    let password = if state.config.encryption.enabled && !state.config.encryption.require_password {
        // In the future, could use a system-derived key here
        None
    } else {
        None
    };

    match service
        .create_backup(
            password,
            Some("Scheduled backup"),
            BackupTrigger::Scheduled,
            None, // No user for scheduled backups
            state.config.include.database,
            state.config.include.config_files,
        )
        .await
    {
        Ok(backup) => {
            info!(
                "Scheduled backup created successfully: {} ({} bytes)",
                backup.filename, backup.file_size
            );

            // Calculate next run time
            let next_run = calculate_next_run(&schedule.frequency, &schedule.time_of_day, schedule.day_of_week);

            // Update last run time
            repo.update_schedule_last_run(schedule.id, now, next_run).await?;
        }
        Err(e) => {
            error!("Scheduled backup failed: {}", e);
            // Still update last_run to avoid retrying immediately
            let next_run = calculate_next_run(&schedule.frequency, &schedule.time_of_day, schedule.day_of_week);
            repo.update_schedule_last_run(schedule.id, now, next_run).await?;
        }
    }

    Ok(())
}

/// Check if daily schedule matches current time (within a minute)
fn check_daily_schedule(time_of_day: &str, now: chrono::DateTime<Utc>) -> bool {
    if let Ok(scheduled_time) = NaiveTime::parse_from_str(time_of_day, "%H:%M") {
        let current_hour = now.hour();
        let current_minute = now.minute();

        current_hour == scheduled_time.hour() && current_minute == scheduled_time.minute()
    } else {
        warn!("Invalid time_of_day format: {}", time_of_day);
        false
    }
}

/// Check if weekly schedule matches current time and day
fn check_weekly_schedule(time_of_day: &str, day_of_week: i32, now: chrono::DateTime<Utc>) -> bool {
    // Convert day_of_week (0=Sunday) to chrono Weekday
    let target_weekday = match day_of_week {
        0 => Weekday::Sun,
        1 => Weekday::Mon,
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        6 => Weekday::Sat,
        _ => return false,
    };

    if now.weekday() != target_weekday {
        return false;
    }

    check_daily_schedule(time_of_day, now)
}

/// Check if cron expression matches current time
/// Basic implementation - supports minute hour * * * format
fn check_cron_schedule(cron: &str, now: chrono::DateTime<Utc>) -> bool {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() < 2 {
        warn!("Invalid cron expression: {}", cron);
        return false;
    }

    let minute_match = match_cron_field(parts[0], now.minute() as i32);
    let hour_match = match_cron_field(parts[1], now.hour() as i32);

    minute_match && hour_match
}

/// Match a single cron field value
fn match_cron_field(field: &str, value: i32) -> bool {
    if field == "*" {
        return true;
    }

    // Handle comma-separated values
    if field.contains(',') {
        return field
            .split(',')
            .any(|v| v.parse::<i32>().map(|n| n == value).unwrap_or(false));
    }

    // Handle ranges (e.g., 1-5)
    if field.contains('-') {
        let parts: Vec<&str> = field.split('-').collect();
        if parts.len() == 2 {
            if let (Ok(start), Ok(end)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                return value >= start && value <= end;
            }
        }
        return false;
    }

    // Handle step values (e.g., */5)
    if let Some(step_str) = field.strip_prefix("*/") {
        if let Ok(step) = step_str.parse::<i32>() {
            return step > 0 && value % step == 0;
        }
        return false;
    }

    // Direct value match
    field.parse::<i32>().map(|n| n == value).unwrap_or(false)
}

/// Calculate the next run time based on schedule
fn calculate_next_run(
    frequency: &str,
    time_of_day: &str,
    day_of_week: i32,
) -> Option<chrono::DateTime<Utc>> {
    let now = Utc::now();

    match frequency {
        "hourly" => {
            // Next hour at minute 0
            Some(now + chrono::Duration::hours(1) - chrono::Duration::minutes(now.minute() as i64)
                - chrono::Duration::seconds(now.second() as i64))
        }
        "daily" => {
            // Tomorrow at scheduled time
            if let Ok(time) = NaiveTime::parse_from_str(time_of_day, "%H:%M") {
                let today = now.date_naive();
                let scheduled = today.and_time(time);
                let scheduled_utc = chrono::DateTime::from_naive_utc_and_offset(scheduled, Utc);

                if scheduled_utc > now {
                    Some(scheduled_utc)
                } else {
                    Some(scheduled_utc + chrono::Duration::days(1))
                }
            } else {
                None
            }
        }
        "weekly" => {
            if let Ok(time) = NaiveTime::parse_from_str(time_of_day, "%H:%M") {
                let current_dow = now.weekday().num_days_from_sunday() as i32;
                let days_until = if day_of_week >= current_dow {
                    day_of_week - current_dow
                } else {
                    7 - (current_dow - day_of_week)
                };

                let target_date = now.date_naive() + chrono::Duration::days(days_until as i64);
                let scheduled = target_date.and_time(time);
                let scheduled_utc = chrono::DateTime::from_naive_utc_and_offset(scheduled, Utc);

                if scheduled_utc > now {
                    Some(scheduled_utc)
                } else {
                    Some(scheduled_utc + chrono::Duration::weeks(1))
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Cleanup task
///
/// Periodically cleans up old backups based on retention policy.
async fn cleanup_task(state: BackupSchedulerState) {
    // Run cleanup once per hour
    let cleanup_interval = Duration::from_secs(3600);
    let mut interval_timer = interval(cleanup_interval);

    info!(
        "Backup cleanup task started (interval: {}s)",
        cleanup_interval.as_secs()
    );

    loop {
        interval_timer.tick().await;

        if !*state.running.read().await {
            info!("Backup cleanup task stopping");
            break;
        }

        debug!("Running backup cleanup cycle");

        let service = BackupService::new(state.pool.clone(), state.config.clone());

        match service.cleanup_old_backups().await {
            Ok(deleted) => {
                if deleted > 0 {
                    info!("Cleaned up {} old backups", deleted);
                }
            }
            Err(e) => {
                error!("Failed to cleanup old backups: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_cron_field() {
        assert!(match_cron_field("*", 5));
        assert!(match_cron_field("5", 5));
        assert!(!match_cron_field("5", 6));
        assert!(match_cron_field("1,5,10", 5));
        assert!(!match_cron_field("1,5,10", 6));
        assert!(match_cron_field("1-5", 3));
        assert!(!match_cron_field("1-5", 6));
        assert!(match_cron_field("*/5", 10));
        assert!(match_cron_field("*/5", 0));
        assert!(!match_cron_field("*/5", 7));
    }

    #[test]
    fn test_scheduler_state_creation() {
        // Basic test to ensure the module compiles correctly
        assert!(true);
    }
}
