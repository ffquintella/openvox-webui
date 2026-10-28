//! Audit trail for update job requests, approvals and outcomes.
//!
//! Every lifecycle event of an update job is written to the shared
//! `audit_log` table with `resource_type = "update_jobs"` and the job id as
//! `resource_id`, so it can be queried through `GET /api/v1/audit-logs`.
//! Writing is best-effort: a failed audit insert is logged and never fails
//! the operation being audited.

use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::AuditRepository;
use crate::models::{UpdateJob, UpdateJobStatus};

pub const RESOURCE_TYPE: &str = "update_jobs";

pub const ACTION_REQUESTED: &str = "update_job.requested";
pub const ACTION_APPROVED: &str = "update_job.approved";
pub const ACTION_REJECTED: &str = "update_job.rejected";
pub const ACTION_CANCELLED: &str = "update_job.cancelled";
pub const ACTION_DISPATCHED: &str = "update_job.dispatched";
pub const ACTION_TARGET_RESULT: &str = "update_job.target_result";
pub const ACTION_FINISHED: &str = "update_job.finished";
pub const ACTION_TIMED_OUT: &str = "update_job.timed_out";

/// Records one update job audit event. `user_id` is `None` for events caused
/// by nodes or background services rather than a signed-in user.
pub async fn record(
    pool: &SqlitePool,
    organization_id: Uuid,
    user_id: Option<Uuid>,
    action: &str,
    job_id: &str,
    details: Value,
) {
    if let Err(e) = AuditRepository::new(pool)
        .insert(
            organization_id,
            user_id,
            action,
            RESOURCE_TYPE,
            Some(job_id),
            Some(&details),
            None,
        )
        .await
    {
        tracing::warn!("Failed to write update audit entry '{}': {}", action, e);
    }
}

/// Details recorded when a job is requested, manually or by a schedule.
pub fn requested_details(job: &UpdateJob, actor: &str, source: &str) -> Value {
    json!({
        "actor": actor,
        "source": source,
        "operation_type": job.operation_type,
        "package_names": job.package_names,
        "target_group_id": job.target_group_id,
        "target_count": job.target_nodes.len(),
        "requires_approval": job.requires_approval,
        "scheduled_for": job.scheduled_for,
        "status": job.status.as_str(),
    })
}

/// Whether a job status is final (no further target results will arrive).
pub fn is_terminal(status: UpdateJobStatus) -> bool {
    matches!(
        status,
        UpdateJobStatus::Completed
            | UpdateJobStatus::CompletedWithFailures
            | UpdateJobStatus::Cancelled
            | UpdateJobStatus::Rejected
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{default_organization_uuid, AuditLogQuery};
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn records_are_queryable_by_resource_type_and_action() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let org = default_organization_uuid();
        record(
            &pool,
            org,
            None,
            ACTION_REQUESTED,
            "job-1",
            json!({"actor": "admin"}),
        )
        .await;
        record(
            &pool,
            org,
            None,
            ACTION_APPROVED,
            "job-1",
            json!({"actor": "operator"}),
        )
        .await;

        let repo = AuditRepository::new(&pool);
        let all = repo
            .list(
                org,
                &AuditLogQuery {
                    resource_type: Some(RESOURCE_TYPE.to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("list");
        assert_eq!(all.len(), 2);
        assert!(all
            .iter()
            .all(|e| e.resource_id.as_deref() == Some("job-1")));

        let approved = repo
            .list(
                org,
                &AuditLogQuery {
                    action: Some(ACTION_APPROVED.to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("list");
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].details.as_ref().unwrap()["actor"], "operator");
    }

    #[test]
    fn terminal_statuses() {
        assert!(is_terminal(UpdateJobStatus::Completed));
        assert!(is_terminal(UpdateJobStatus::CompletedWithFailures));
        assert!(is_terminal(UpdateJobStatus::Cancelled));
        assert!(is_terminal(UpdateJobStatus::Rejected));
        assert!(!is_terminal(UpdateJobStatus::InProgress));
        assert!(!is_terminal(UpdateJobStatus::PendingApproval));
        assert!(!is_terminal(UpdateJobStatus::Approved));
    }
}
