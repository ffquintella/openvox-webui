//! Report scheduler service
//!
//! This module provides scheduled report execution functionality.
//! It can be run as a background task or invoked via cron.

use anyhow::Result;
use chrono::{DateTime, Utc};
use cron::Schedule;
use sqlx::SqlitePool;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::db::repository::{ReportScheduleRepository, SavedReportRepository};
use crate::models::{ExecuteReportRequest, ReportSchedule};
use crate::services::{PuppetDbClient, ReportingService};

/// Report scheduler that executes due scheduled reports
pub struct ReportScheduler {
    pool: SqlitePool,
    puppetdb: Option<Arc<PuppetDbClient>>,
}

impl ReportScheduler {
    pub fn new(pool: SqlitePool, puppetdb: Option<Arc<PuppetDbClient>>) -> Self {
        Self { pool, puppetdb }
    }

    /// Run all due scheduled reports
    ///
    /// This method checks all enabled schedules and executes any that are due.
    /// It updates the last_run_at and next_run_at timestamps after execution.
    pub async fn run_due_schedules(&self) -> Result<Vec<ScheduleExecutionResult>> {
        let schedule_repo = ReportScheduleRepository::new(&self.pool);
        let report_repo = SavedReportRepository::new(&self.pool);
        let reporting_service = ReportingService::new(self.pool.clone(), self.puppetdb.clone());

        let schedules = schedule_repo.get_due().await?;
        let mut results = Vec::new();

        info!("Found {} due schedules to execute", schedules.len());

        for schedule in schedules {
            let result = self
                .execute_schedule(&schedule, &report_repo, &reporting_service, &schedule_repo)
                .await;

            results.push(result);
        }

        Ok(results)
    }

    /// Execute a single schedule
    async fn execute_schedule(
        &self,
        schedule: &ReportSchedule,
        report_repo: &SavedReportRepository<'_>,
        reporting_service: &ReportingService,
        schedule_repo: &ReportScheduleRepository<'_>,
    ) -> ScheduleExecutionResult {
        info!(
            "Executing scheduled report: {} (schedule: {})",
            schedule.report_id, schedule.id
        );

        // Get the associated report
        let report = match report_repo.get_by_id(schedule.report_id).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                warn!("Report {} not found for schedule {}", schedule.report_id, schedule.id);
                return ScheduleExecutionResult {
                    schedule_id: schedule.id,
                    success: false,
                    error: Some("Report not found".to_string()),
                    execution_time_ms: 0,
                };
            }
            Err(e) => {
                error!("Failed to get report {}: {}", schedule.report_id, e);
                return ScheduleExecutionResult {
                    schedule_id: schedule.id,
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms: 0,
                };
            }
        };

        let start = std::time::Instant::now();

        // Execute the report
        let execute_req = ExecuteReportRequest {
            output_format: schedule.output_format,
            query_config_override: None,
        };

        let result = reporting_service
            .execute_report(&report, &execute_req, None)
            .await;

        let execution_time_ms = start.elapsed().as_millis() as i32;

        // Update schedule timestamps
        let now = Utc::now();
        let next_run = calculate_next_run(&schedule.schedule_cron, &schedule.timezone);

        if let Err(e) = schedule_repo.update_run_times(schedule.id, now, next_run).await {
            error!("Failed to update schedule run times: {}", e);
        }

        match result {
            Ok(execution) => {
                info!(
                    "Schedule {} executed successfully (execution_id: {}, rows: {})",
                    schedule.id,
                    execution.id,
                    execution.row_count.unwrap_or(0)
                );
                ScheduleExecutionResult {
                    schedule_id: schedule.id,
                    success: true,
                    error: None,
                    execution_time_ms,
                }
            }
            Err(e) => {
                error!("Schedule {} execution failed: {}", schedule.id, e);
                ScheduleExecutionResult {
                    schedule_id: schedule.id,
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms,
                }
            }
        }
    }

    /// Run a specific schedule by ID
    pub async fn run_schedule(&self, schedule_id: uuid::Uuid) -> Result<ScheduleExecutionResult> {
        let schedule_repo = ReportScheduleRepository::new(&self.pool);
        let report_repo = SavedReportRepository::new(&self.pool);
        let reporting_service = ReportingService::new(self.pool.clone(), self.puppetdb.clone());

        let schedule = schedule_repo
            .get_by_id(schedule_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Schedule not found"))?;

        Ok(self
            .execute_schedule(&schedule, &report_repo, &reporting_service, &schedule_repo)
            .await)
    }
}

/// Result of executing a scheduled report
#[derive(Debug)]
pub struct ScheduleExecutionResult {
    pub schedule_id: uuid::Uuid,
    pub success: bool,
    pub error: Option<String>,
    pub execution_time_ms: i32,
}

/// Calculate the next run time for a cron expression
pub fn calculate_next_run(cron_expr: &str, timezone: &str) -> Option<DateTime<Utc>> {
    let schedule = match Schedule::from_str(cron_expr) {
        Ok(s) => s,
        Err(e) => {
            warn!("Invalid cron expression '{}': {}", cron_expr, e);
            return None;
        }
    };

    // Get the next occurrence
    // Note: The cron crate works with UTC by default
    // For timezone support, we'd need to use chrono-tz
    let _ = timezone; // TODO: Implement timezone support with chrono-tz

    schedule.upcoming(Utc).next()
}

/// Validate a cron expression
pub fn validate_cron_expression(cron_expr: &str) -> Result<(), String> {
    Schedule::from_str(cron_expr)
        .map(|_| ())
        .map_err(|e| format!("Invalid cron expression: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cron_expression_valid() {
        assert!(validate_cron_expression("0 0 * * * *").is_ok()); // Every hour
        assert!(validate_cron_expression("0 0 0 * * *").is_ok()); // Daily at midnight
        assert!(validate_cron_expression("0 30 9 * * MON-FRI").is_ok()); // Weekdays at 9:30
    }

    #[test]
    fn test_validate_cron_expression_invalid() {
        assert!(validate_cron_expression("invalid").is_err());
        assert!(validate_cron_expression("60 * * * * *").is_err()); // Invalid second
    }

    #[test]
    fn test_calculate_next_run() {
        let next = calculate_next_run("0 0 * * * *", "UTC");
        assert!(next.is_some());
        assert!(next.unwrap() > Utc::now());
    }
}
