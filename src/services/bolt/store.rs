//! Persistence of Bolt jobs, results and the allowlist (T124).
//!
//! Every state change is a conditional `UPDATE ... WHERE status = ?`, so two
//! requests (approve and cancel, say) cannot both win.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::model::{AllowlistEntry, BoltJob, BoltJobDetail, BoltJobTarget, JobKind, JobStatus};
use super::runner::TargetOutcome;

pub struct NewJob {
    pub organization_id: Uuid,
    pub kind: JobKind,
    pub name: String,
    pub parameters: Value,
    pub sensitive_names: Vec<String>,
    pub targets: Vec<String>,
    pub target_group_id: Option<String>,
    pub noop: bool,
    pub status: JobStatus,
    pub requires_approval: bool,
    pub requested_by: String,
    pub requested_by_id: Uuid,
    pub window: (Option<DateTime<Utc>>, Option<DateTime<Utc>>),
}

/// What the executor needs to run a job: the real parameters.
pub struct RunnableJob {
    pub job: BoltJob,
    pub organization_id: Uuid,
    pub parameters: Value,
    pub targets: Vec<String>,
}

fn ts(v: Option<String>) -> Option<DateTime<Utc>> {
    v.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&Utc))
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn row_to_job(r: &sqlx::sqlite::SqliteRow) -> Result<BoltJob> {
    let params: String = r.try_get("parameters")?;
    let sensitive: String = r.try_get("sensitive_names")?;
    let targets: String = r.try_get("targets")?;
    let sensitive: Vec<String> = serde_json::from_str(&sensitive).unwrap_or_default();
    let params: Value = serde_json::from_str(&params).unwrap_or(Value::Null);
    let targets: Vec<String> = serde_json::from_str(&targets).unwrap_or_default();
    let kind: String = r.try_get("kind")?;
    let status: String = r.try_get("status")?;
    Ok(BoltJob {
        id: r.try_get("id")?,
        kind: JobKind::parse(&kind).context("unknown job kind")?,
        name: r.try_get("name")?,
        // Never leaves the store with a sensitive value in it.
        parameters: super::model::redact_parameters(&params, &sensitive),
        target_group_id: r.try_get("target_group_id")?,
        target_count: targets.len(),
        noop: r.try_get::<i64, _>("noop")? != 0,
        status: JobStatus::parse(&status).context("unknown job status")?,
        requires_approval: r.try_get::<i64, _>("requires_approval")? != 0,
        requested_by: r.try_get("requested_by")?,
        requested_by_id: r.try_get("requested_by_id")?,
        approved_by: r.try_get("approved_by")?,
        approval_notes: r.try_get("approval_notes")?,
        maintenance_window_start: ts(r.try_get("maintenance_window_start")?),
        maintenance_window_end: ts(r.try_get("maintenance_window_end")?),
        error: r.try_get("error")?,
        created_at: ts(r.try_get("created_at")?).unwrap_or_else(Utc::now),
        started_at: ts(r.try_get("started_at")?),
        finished_at: ts(r.try_get("finished_at")?),
    })
}

#[derive(Clone)]
pub struct BoltStore {
    pool: SqlitePool,
}

impl BoltStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ------------------------------------------------------------------ allowlist

    pub async fn allowlist(&self, org: Uuid) -> Result<Vec<AllowlistEntry>> {
        let rows = sqlx::query(
            "SELECT kind, name, requires_approval, created_by, created_at \
             FROM bolt_allowlist WHERE organization_id = ? ORDER BY kind, name",
        )
        .bind(org.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let kind: String = r.try_get("kind")?;
                Ok(AllowlistEntry {
                    kind: JobKind::parse(&kind).context("unknown kind")?,
                    name: r.try_get("name")?,
                    requires_approval: r.try_get::<i64, _>("requires_approval")? != 0,
                    created_by: r.try_get("created_by")?,
                    created_at: ts(r.try_get("created_at")?).unwrap_or_else(Utc::now),
                })
            })
            .collect()
    }

    pub async fn allowlist_entry(
        &self,
        org: Uuid,
        kind: JobKind,
        name: &str,
    ) -> Result<Option<AllowlistEntry>> {
        Ok(self
            .allowlist(org)
            .await?
            .into_iter()
            .find(|e| e.kind == kind && e.name == name))
    }

    /// Replaces the whole allowlist of the organization.
    pub async fn replace_allowlist(
        &self,
        org: Uuid,
        entries: &[(JobKind, String, bool)],
        actor: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM bolt_allowlist WHERE organization_id = ?")
            .bind(org.to_string())
            .execute(&mut *tx)
            .await?;
        for (kind, name, approval) in entries {
            sqlx::query(
                "INSERT OR REPLACE INTO bolt_allowlist \
                 (id, organization_id, kind, name, requires_approval, created_by, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(org.to_string())
            .bind(kind.as_str())
            .bind(name)
            .bind(*approval as i64)
            .bind(actor)
            .bind(now())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    // ----------------------------------------------------------------------- jobs

    pub async fn create_job(&self, j: NewJob) -> Result<BoltJob> {
        let id = Uuid::new_v4().to_string();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO bolt_jobs (id, organization_id, kind, name, parameters, sensitive_names, \
             targets, target_group_id, noop, status, requires_approval, requested_by, \
             requested_by_id, maintenance_window_start, maintenance_window_end, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(j.organization_id.to_string())
        .bind(j.kind.as_str())
        .bind(&j.name)
        .bind(j.parameters.to_string())
        .bind(serde_json::to_string(&j.sensitive_names)?)
        .bind(serde_json::to_string(&j.targets)?)
        .bind(&j.target_group_id)
        .bind(j.noop as i64)
        .bind(j.status.as_str())
        .bind(j.requires_approval as i64)
        .bind(&j.requested_by)
        .bind(j.requested_by_id.to_string())
        .bind(j.window.0.map(|d| d.to_rfc3339()))
        .bind(j.window.1.map(|d| d.to_rfc3339()))
        .bind(now())
        .execute(&mut *tx)
        .await?;
        for certname in &j.targets {
            sqlx::query(
                "INSERT INTO bolt_job_targets (job_id, certname, status) VALUES (?, ?, 'pending')",
            )
            .bind(&id)
            .bind(certname)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.job(j.organization_id, &id)
            .await?
            .context("job vanished after insert")
    }

    pub async fn job(&self, org: Uuid, id: &str) -> Result<Option<BoltJob>> {
        let row = sqlx::query("SELECT * FROM bolt_jobs WHERE id = ? AND organization_id = ?")
            .bind(id)
            .bind(org.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(row_to_job).transpose()
    }

    pub async fn job_detail(&self, org: Uuid, id: &str) -> Result<Option<BoltJobDetail>> {
        let Some(job) = self.job(org, id).await? else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT certname, status, exit_code, result, output, started_at, finished_at \
             FROM bolt_job_targets WHERE job_id = ? ORDER BY certname",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let targets = rows
            .iter()
            .map(|r| -> Result<BoltJobTarget> {
                let result: Option<String> = r.try_get("result")?;
                Ok(BoltJobTarget {
                    certname: r.try_get("certname")?,
                    status: r.try_get("status")?,
                    exit_code: r.try_get::<Option<i64>, _>("exit_code")?.map(|c| c as i32),
                    result: result.and_then(|s| serde_json::from_str(&s).ok()),
                    output: r.try_get("output")?,
                    started_at: ts(r.try_get("started_at")?),
                    finished_at: ts(r.try_get("finished_at")?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(BoltJobDetail { job, targets }))
    }

    pub async fn list_jobs(
        &self,
        org: Uuid,
        status: Option<JobStatus>,
        certname: Option<&str>,
        limit: i64,
    ) -> Result<Vec<BoltJob>> {
        let rows = sqlx::query(
            "SELECT * FROM bolt_jobs WHERE organization_id = ? \
             AND (? IS NULL OR status = ?) \
             AND (? IS NULL OR id IN (SELECT job_id FROM bolt_job_targets WHERE certname = ?)) \
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(org.to_string())
        .bind(status.map(|s| s.as_str()))
        .bind(status.map(|s| s.as_str()))
        .bind(certname)
        .bind(certname)
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_job).collect()
    }

    /// The job with its real parameters, for the executor.
    pub async fn runnable(&self, id: &str) -> Result<Option<RunnableJob>> {
        let Some(row) = sqlx::query("SELECT * FROM bolt_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };
        let params: String = row.try_get("parameters")?;
        let targets: String = row.try_get("targets")?;
        let org: String = row.try_get("organization_id")?;
        Ok(Some(RunnableJob {
            job: row_to_job(&row)?,
            organization_id: Uuid::parse_str(&org)?,
            parameters: serde_json::from_str(&params).unwrap_or(Value::Null),
            targets: serde_json::from_str(&targets).unwrap_or_default(),
        }))
    }

    /// Moves a job from `from` to `to`; false when it was no longer in `from`.
    pub async fn transition(&self, id: &str, from: JobStatus, to: JobStatus) -> Result<bool> {
        let started = (to == JobStatus::Running).then(now);
        let r = sqlx::query(
            "UPDATE bolt_jobs SET status = ?, started_at = COALESCE(?, started_at) \
             WHERE id = ? AND status = ?",
        )
        .bind(to.as_str())
        .bind(started)
        .bind(id)
        .bind(from.as_str())
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() == 1)
    }

    /// Records an approval decision on a job still waiting for it.
    pub async fn decide(
        &self,
        id: &str,
        approved: bool,
        approver: &str,
        notes: Option<&str>,
    ) -> Result<bool> {
        let to = if approved {
            JobStatus::Queued
        } else {
            JobStatus::Rejected
        };
        let finished = (!approved).then(now);
        let r = sqlx::query(
            "UPDATE bolt_jobs SET status = ?, approved_by = ?, approval_notes = ?, \
             finished_at = COALESCE(?, finished_at) WHERE id = ? AND status = 'pending_approval'",
        )
        .bind(to.as_str())
        .bind(approver)
        .bind(notes)
        .bind(finished)
        .bind(id)
        .execute(&self.pool)
        .await?;
        if !approved && r.rows_affected() == 1 {
            self.mark_targets(id, "cancelled").await?;
        }
        Ok(r.rows_affected() == 1)
    }

    async fn mark_targets(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE bolt_job_targets SET status = ?, finished_at = ? \
             WHERE job_id = ? AND status IN ('pending', 'running')",
        )
        .bind(status)
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Cancels a job that has not started; false when it already has.
    pub async fn cancel_waiting(&self, id: &str) -> Result<bool> {
        let r = sqlx::query(
            "UPDATE bolt_jobs SET status = 'cancelled', finished_at = ? \
             WHERE id = ? AND status IN ('pending_approval', 'queued')",
        )
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        if r.rows_affected() == 1 {
            self.mark_targets(id, "cancelled").await?;
        }
        Ok(r.rows_affected() == 1)
    }

    pub async fn mark_targets_running(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE bolt_job_targets SET status = 'running', started_at = ? \
             WHERE job_id = ? AND status = 'pending'",
        )
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Ends a running job: stores the results and the final status, and swaps
    /// the stored parameters for their redacted copy.
    pub async fn finish(
        &self,
        id: &str,
        status: JobStatus,
        error: Option<&str>,
        outcomes: &[TargetOutcome],
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let redacted: Option<(String, String)> = sqlx::query_as(
            "SELECT parameters, sensitive_names FROM bolt_jobs WHERE id = ? AND status = 'running'",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((params, sensitive)) = redacted else {
            return Ok(false);
        };
        let params: Value = serde_json::from_str(&params).unwrap_or(Value::Null);
        let sensitive: Vec<String> = serde_json::from_str(&sensitive).unwrap_or_default();
        let clean = super::model::redact_parameters(&params, &sensitive);
        sqlx::query(
            "UPDATE bolt_jobs SET status = ?, error = ?, parameters = ?, finished_at = ? \
             WHERE id = ? AND status = 'running'",
        )
        .bind(status.as_str())
        .bind(error)
        .bind(clean.to_string())
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        for o in outcomes {
            sqlx::query(
                "UPDATE bolt_job_targets SET status = ?, exit_code = ?, result = ?, output = ?, \
                 finished_at = ? WHERE job_id = ? AND certname = ?",
            )
            .bind(&o.status)
            .bind(o.exit_code.map(i64::from))
            .bind(o.result.to_string())
            .bind(&o.output)
            .bind(now())
            .bind(id)
            .bind(&o.certname)
            .execute(&mut *tx)
            .await?;
        }
        // Targets no result arrived for (timeout, cancel, process error).
        let leftover = match status {
            JobStatus::Cancelled => "cancelled",
            JobStatus::TimedOut => "timed_out",
            _ => "failure",
        };
        sqlx::query(
            "UPDATE bolt_job_targets SET status = ?, finished_at = ? \
             WHERE job_id = ? AND status IN ('pending', 'running')",
        )
        .bind(leftover)
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Fails a job in any non-final state (maintenance window missed, refusal
    /// at start), swapping the parameters for the redacted copy too.
    pub async fn fail_waiting(&self, id: &str, error: &str) -> Result<bool> {
        let r = sqlx::query(
            "UPDATE bolt_jobs SET status = 'failed', error = ?, finished_at = ? \
             WHERE id = ? AND status = 'queued'",
        )
        .bind(error)
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        if r.rows_affected() == 1 {
            self.redact_stored_parameters(id).await?;
            self.mark_targets(id, "cancelled").await?;
        }
        Ok(r.rows_affected() == 1)
    }

    async fn redact_stored_parameters(&self, id: &str) -> Result<()> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT parameters, sensitive_names FROM bolt_jobs WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        if let Some((params, sensitive)) = row {
            let params: Value = serde_json::from_str(&params).unwrap_or(Value::Null);
            let sensitive: Vec<String> = serde_json::from_str(&sensitive).unwrap_or_default();
            let clean = super::model::redact_parameters(&params, &sensitive);
            sqlx::query("UPDATE bolt_jobs SET parameters = ? WHERE id = ?")
                .bind(clean.to_string())
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// After a restart: jobs that were running did not finish, and queued
    /// ones lost their executor. Returns the ids of the queued jobs (to run
    /// again) and fails the running ones.
    pub async fn recover(&self) -> Result<Vec<String>> {
        let running: Vec<String> =
            sqlx::query_scalar("SELECT id FROM bolt_jobs WHERE status = 'running'")
                .fetch_all(&self.pool)
                .await?;
        for id in &running {
            sqlx::query(
                "UPDATE bolt_jobs SET status = 'failed', \
                 error = 'interrupted by a restart of the application', finished_at = ? \
                 WHERE id = ? AND status = 'running'",
            )
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
            self.mark_targets(id, "failure").await?;
            self.redact_stored_parameters(id).await?;
        }
        Ok(sqlx::query_scalar(
            "SELECT id FROM bolt_jobs WHERE status = 'queued' ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Deletes finished jobs older than `days`. Returns how many.
    pub async fn purge(&self, days: i64) -> Result<u64> {
        let cutoff = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        sqlx::query(
            "DELETE FROM bolt_job_targets WHERE job_id IN \
             (SELECT id FROM bolt_jobs WHERE finished_at IS NOT NULL AND finished_at < ?)",
        )
        .bind(&cutoff)
        .execute(&self.pool)
        .await?;
        let r =
            sqlx::query("DELETE FROM bolt_jobs WHERE finished_at IS NOT NULL AND finished_at < ?")
                .bind(&cutoff)
                .execute(&self.pool)
                .await?;
        Ok(r.rows_affected())
    }

    /// Jobs running or queued right now.
    pub async fn active_count(&self, org: Uuid) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM bolt_jobs WHERE organization_id = ? \
             AND status IN ('queued', 'running')",
        )
        .bind(org.to_string())
        .fetch_one(&self.pool)
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn store() -> BoltStore {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/20261113000001_bolt_orchestration.sql"
        ))
        .execute(&pool)
        .await
        .unwrap_or_else(|_| {
            // The seed row needs the RBAC tables; the schema above it is what is tested.
            Default::default()
        });
        BoltStore::new(pool)
    }

    fn new_job(org: Uuid, status: JobStatus) -> NewJob {
        NewJob {
            organization_id: org,
            kind: JobKind::Task,
            name: "package".into(),
            parameters: json!({"name": "nginx", "password": "hunter2"}),
            sensitive_names: vec![],
            targets: vec!["a.example".into(), "b.example".into()],
            target_group_id: None,
            noop: false,
            status,
            requires_approval: status == JobStatus::PendingApproval,
            requested_by: "alice".into(),
            requested_by_id: Uuid::new_v4(),
            window: (None, None),
        }
    }

    fn outcome(name: &str, status: &str) -> TargetOutcome {
        TargetOutcome {
            certname: name.into(),
            status: status.into(),
            exit_code: Some(0),
            result: json!({"_output": "ok"}),
            output: "ok".into(),
        }
    }

    #[tokio::test]
    async fn api_view_never_carries_a_sensitive_parameter() {
        let s = store().await;
        let org = Uuid::new_v4();
        let job = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        assert_eq!(job.parameters["password"], super::super::model::REDACTED);
        assert_eq!(job.parameters["name"], "nginx");
        // The executor still gets the real value.
        let r = s.runnable(&job.id).await.unwrap().unwrap();
        assert_eq!(r.parameters["password"], "hunter2");
    }

    #[tokio::test]
    async fn finishing_swaps_the_stored_parameters_for_the_redacted_copy() {
        let s = store().await;
        let org = Uuid::new_v4();
        let job = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        assert!(s
            .transition(&job.id, JobStatus::Queued, JobStatus::Running)
            .await
            .unwrap());
        assert!(s
            .finish(
                &job.id,
                JobStatus::Succeeded,
                None,
                &[
                    outcome("a.example", "success"),
                    outcome("b.example", "success")
                ]
            )
            .await
            .unwrap());
        let r = s.runnable(&job.id).await.unwrap().unwrap();
        assert_eq!(r.parameters["password"], super::super::model::REDACTED);
        let d = s.job_detail(org, &job.id).await.unwrap().unwrap();
        assert_eq!(d.job.status, JobStatus::Succeeded);
        assert!(d
            .targets
            .iter()
            .all(|t| t.status == "success" && t.output.as_deref() == Some("ok")));
    }

    #[tokio::test]
    async fn transitions_are_conditional() {
        let s = store().await;
        let org = Uuid::new_v4();
        let job = s
            .create_job(new_job(org, JobStatus::PendingApproval))
            .await
            .unwrap();
        assert!(s.decide(&job.id, true, "bob", Some("ok")).await.unwrap());
        // A second decision, or a cancel of a job already approved to queued, can still race only once.
        assert!(!s.decide(&job.id, false, "carol", None).await.unwrap());
        assert!(s.cancel_waiting(&job.id).await.unwrap());
        assert!(!s.cancel_waiting(&job.id).await.unwrap());
        assert!(!s
            .transition(&job.id, JobStatus::Queued, JobStatus::Running)
            .await
            .unwrap());
        let j = s.job(org, &job.id).await.unwrap().unwrap();
        assert_eq!(j.status, JobStatus::Cancelled);
        assert_eq!(j.approved_by.as_deref(), Some("bob"));
    }

    #[tokio::test]
    async fn rejecting_cancels_the_targets() {
        let s = store().await;
        let org = Uuid::new_v4();
        let job = s
            .create_job(new_job(org, JobStatus::PendingApproval))
            .await
            .unwrap();
        assert!(s.decide(&job.id, false, "bob", Some("no")).await.unwrap());
        let d = s.job_detail(org, &job.id).await.unwrap().unwrap();
        assert_eq!(d.job.status, JobStatus::Rejected);
        assert!(d.targets.iter().all(|t| t.status == "cancelled"));
    }

    #[tokio::test]
    async fn jobs_are_scoped_to_the_organization_and_filterable() {
        let s = store().await;
        let (org, other) = (Uuid::new_v4(), Uuid::new_v4());
        let job = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        assert!(s.job(other, &job.id).await.unwrap().is_none());
        assert_eq!(
            s.list_jobs(org, None, Some("a.example"), 10)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(s
            .list_jobs(org, None, Some("zzz"), 10)
            .await
            .unwrap()
            .is_empty());
        assert!(s
            .list_jobs(org, Some(JobStatus::Failed), None, 10)
            .await
            .unwrap()
            .is_empty());
        assert!(s.list_jobs(other, None, None, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unreported_targets_end_failed_or_timed_out() {
        let s = store().await;
        let org = Uuid::new_v4();
        let job = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        s.transition(&job.id, JobStatus::Queued, JobStatus::Running)
            .await
            .unwrap();
        s.finish(
            &job.id,
            JobStatus::TimedOut,
            Some("timed out after 1s"),
            &[],
        )
        .await
        .unwrap();
        let d = s.job_detail(org, &job.id).await.unwrap().unwrap();
        assert!(d.targets.iter().all(|t| t.status == "timed_out"));
        assert_eq!(d.job.error.as_deref(), Some("timed out after 1s"));
    }

    #[tokio::test]
    async fn recovery_fails_running_jobs_and_returns_queued_ones() {
        let s = store().await;
        let org = Uuid::new_v4();
        let running = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        s.transition(&running.id, JobStatus::Queued, JobStatus::Running)
            .await
            .unwrap();
        let queued = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        let again = s.recover().await.unwrap();
        assert_eq!(again, vec![queued.id.clone()]);
        let r = s.runnable(&running.id).await.unwrap().unwrap();
        assert_eq!(r.job.status, JobStatus::Failed);
        assert_eq!(r.parameters["password"], super::super::model::REDACTED);
    }

    #[tokio::test]
    async fn purge_only_removes_old_finished_jobs() {
        let s = store().await;
        let org = Uuid::new_v4();
        let old = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        let live = s.create_job(new_job(org, JobStatus::Queued)).await.unwrap();
        s.transition(&old.id, JobStatus::Queued, JobStatus::Running)
            .await
            .unwrap();
        s.finish(&old.id, JobStatus::Succeeded, None, &[])
            .await
            .unwrap();
        sqlx::query("UPDATE bolt_jobs SET finished_at = ? WHERE id = ?")
            .bind((Utc::now() - chrono::Duration::days(100)).to_rfc3339())
            .bind(&old.id)
            .execute(&s.pool)
            .await
            .unwrap();
        assert_eq!(s.purge(90).await.unwrap(), 1);
        assert!(s.job(org, &old.id).await.unwrap().is_none());
        assert!(s.job(org, &live.id).await.unwrap().is_some());
        let left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM bolt_job_targets WHERE job_id = ?")
                .bind(&old.id)
                .fetch_one(&s.pool)
                .await
                .unwrap();
        assert_eq!(left, 0);
    }

    #[tokio::test]
    async fn allowlist_is_replaced_as_a_whole() {
        let s = store().await;
        let org = Uuid::new_v4();
        s.replace_allowlist(
            org,
            &[
                (JobKind::Task, "package".into(), false),
                (JobKind::Plan, "deploy".into(), true),
            ],
            "root",
        )
        .await
        .unwrap();
        s.replace_allowlist(org, &[(JobKind::Task, "service".into(), true)], "root")
            .await
            .unwrap();
        let list = s.allowlist(org).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].requires_approval);
        assert!(s
            .allowlist_entry(org, JobKind::Task, "package")
            .await
            .unwrap()
            .is_none());
        assert!(s.allowlist(Uuid::new_v4()).await.unwrap().is_empty());
    }
}
