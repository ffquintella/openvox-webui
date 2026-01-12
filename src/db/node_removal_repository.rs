//! Node removal repository
//!
//! Database operations for tracking nodes pending removal due to revoked or missing certificates.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::models::{
    NodeRemovalAudit, PendingNodeRemoval, PendingRemovalStats, RemovalAuditAction, RemovalReason,
};

/// Repository for node removal tracking operations
pub struct NodeRemovalRepository {
    pool: SqlitePool,
}

impl NodeRemovalRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // =========================================================================
    // Pending Node Removals
    // =========================================================================

    /// Mark a node for removal
    pub async fn mark_for_removal(
        &self,
        certname: &str,
        reason: RemovalReason,
        retention_days: i64,
        notes: Option<&str>,
        marked_by: Option<&str>,
    ) -> Result<PendingNodeRemoval> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let scheduled_removal_at = now + Duration::days(retention_days);

        sqlx::query(
            r#"
            INSERT INTO pending_node_removals (
                id, certname, removal_reason, marked_at, scheduled_removal_at,
                notes, marked_by, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(certname) DO UPDATE SET
                removal_reason = ?3,
                marked_at = ?4,
                scheduled_removal_at = ?5,
                notes = ?6,
                marked_by = ?7,
                updated_at = ?9,
                removed_at = NULL
            "#,
        )
        .bind(id.to_string())
        .bind(certname)
        .bind(reason.as_str())
        .bind(now.to_rfc3339())
        .bind(scheduled_removal_at.to_rfc3339())
        .bind(notes)
        .bind(marked_by)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to mark node for removal")?;

        // Log the action
        self.log_audit(certname, RemovalAuditAction::Marked, Some(reason.as_str()), marked_by, notes)
            .await?;

        Ok(PendingNodeRemoval {
            id: id.to_string(),
            certname: certname.to_string(),
            removal_reason: reason,
            marked_at: now,
            scheduled_removal_at,
            removed_at: None,
            notes: notes.map(String::from),
            marked_by: marked_by.map(String::from),
            created_at: now,
            updated_at: now,
        })
    }

    /// Check if a node is already marked for removal
    pub async fn is_marked_for_removal(&self, certname: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM pending_node_removals
            WHERE certname = ?1 AND removed_at IS NULL
            "#,
        )
        .bind(certname)
        .fetch_one(&self.pool)
        .await
        .context("Failed to check if node is marked for removal")?;

        Ok(count > 0)
    }

    /// Get a pending removal by certname
    pub async fn get_pending_removal(&self, certname: &str) -> Result<Option<PendingNodeRemoval>> {
        let row = sqlx::query_as::<_, PendingRemovalRow>(
            r#"
            SELECT * FROM pending_node_removals
            WHERE certname = ?1 AND removed_at IS NULL
            "#,
        )
        .bind(certname)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch pending removal")?;

        Ok(row.map(|r| r.into()))
    }

    /// Get all pending removals (not yet removed)
    pub async fn get_all_pending(&self) -> Result<Vec<PendingNodeRemoval>> {
        let rows = sqlx::query_as::<_, PendingRemovalRow>(
            r#"
            SELECT * FROM pending_node_removals
            WHERE removed_at IS NULL
            ORDER BY scheduled_removal_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch pending removals")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get pending removals that are due for removal
    pub async fn get_due_for_removal(&self) -> Result<Vec<PendingNodeRemoval>> {
        let now = Utc::now();
        let rows = sqlx::query_as::<_, PendingRemovalRow>(
            r#"
            SELECT * FROM pending_node_removals
            WHERE removed_at IS NULL
              AND datetime(scheduled_removal_at) <= datetime(?1)
            ORDER BY scheduled_removal_at ASC
            "#,
        )
        .bind(now.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch removals due")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Unmark a node (cancel pending removal)
    pub async fn unmark_removal(
        &self,
        certname: &str,
        performed_by: Option<&str>,
        notes: Option<&str>,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM pending_node_removals
            WHERE certname = ?1 AND removed_at IS NULL
            "#,
        )
        .bind(certname)
        .execute(&self.pool)
        .await
        .context("Failed to unmark node")?;

        if result.rows_affected() > 0 {
            self.log_audit(certname, RemovalAuditAction::Unmarked, None, performed_by, notes)
                .await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Mark a node as removed (after actual removal)
    pub async fn mark_as_removed(
        &self,
        certname: &str,
        performed_by: Option<&str>,
    ) -> Result<bool> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE pending_node_removals
            SET removed_at = ?1, updated_at = ?1
            WHERE certname = ?2 AND removed_at IS NULL
            "#,
        )
        .bind(now.to_rfc3339())
        .bind(certname)
        .execute(&self.pool)
        .await
        .context("Failed to mark node as removed")?;

        if result.rows_affected() > 0 {
            self.log_audit(
                certname,
                RemovalAuditAction::Removed,
                None,
                performed_by,
                Some("Node automatically removed after retention period"),
            )
            .await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Extend the removal deadline
    pub async fn extend_deadline(
        &self,
        certname: &str,
        extend_days: i64,
        performed_by: Option<&str>,
        notes: Option<&str>,
    ) -> Result<Option<PendingNodeRemoval>> {
        let now = Utc::now();
        let new_deadline = now + Duration::days(extend_days);

        let result = sqlx::query(
            r#"
            UPDATE pending_node_removals
            SET scheduled_removal_at = ?1, updated_at = ?2, notes = COALESCE(?3, notes)
            WHERE certname = ?4 AND removed_at IS NULL
            "#,
        )
        .bind(new_deadline.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(notes)
        .bind(certname)
        .execute(&self.pool)
        .await
        .context("Failed to extend removal deadline")?;

        if result.rows_affected() > 0 {
            self.log_audit(
                certname,
                RemovalAuditAction::Extended,
                Some(&format!("Extended by {} days", extend_days)),
                performed_by,
                notes,
            )
            .await?;
            self.get_pending_removal(certname).await
        } else {
            Ok(None)
        }
    }

    /// Get statistics for pending removals
    pub async fn get_stats(&self) -> Result<PendingRemovalStats> {
        let now = Utc::now();
        let today_end = now + Duration::days(1);
        let week_end = now + Duration::days(7);

        let stats = sqlx::query_as::<_, StatsRow>(
            r#"
            SELECT
                COUNT(*) as total_pending,
                SUM(CASE WHEN removal_reason = 'revoked_certificate' THEN 1 ELSE 0 END) as revoked_certificates,
                SUM(CASE WHEN removal_reason = 'no_certificate' THEN 1 ELSE 0 END) as no_certificates,
                SUM(CASE WHEN removal_reason = 'manual' THEN 1 ELSE 0 END) as manual,
                SUM(CASE WHEN datetime(scheduled_removal_at) <= datetime(?1) THEN 1 ELSE 0 END) as due_today,
                SUM(CASE WHEN datetime(scheduled_removal_at) <= datetime(?2) THEN 1 ELSE 0 END) as due_this_week
            FROM pending_node_removals
            WHERE removed_at IS NULL
            "#,
        )
        .bind(today_end.to_rfc3339())
        .bind(week_end.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .context("Failed to fetch stats")?;

        Ok(PendingRemovalStats {
            total_pending: stats.total_pending,
            revoked_certificates: stats.revoked_certificates,
            no_certificates: stats.no_certificates,
            manual: stats.manual,
            due_today: stats.due_today,
            due_this_week: stats.due_this_week,
        })
    }

    // =========================================================================
    // Audit Log
    // =========================================================================

    /// Log an audit event
    async fn log_audit(
        &self,
        certname: &str,
        action: RemovalAuditAction,
        reason: Option<&str>,
        performed_by: Option<&str>,
        details: Option<&str>,
    ) -> Result<()> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO node_removal_audit (
                id, certname, action, reason, performed_by, details, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(id.to_string())
        .bind(certname)
        .bind(action.as_str())
        .bind(reason)
        .bind(performed_by)
        .bind(details)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to log audit event")?;

        Ok(())
    }

    /// Get audit log for a specific node
    pub async fn get_audit_log(&self, certname: &str) -> Result<Vec<NodeRemovalAudit>> {
        let rows = sqlx::query_as::<_, AuditRow>(
            r#"
            SELECT * FROM node_removal_audit
            WHERE certname = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch audit log")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get recent audit log entries
    pub async fn get_recent_audit(&self, limit: i64) -> Result<Vec<NodeRemovalAudit>> {
        let rows = sqlx::query_as::<_, AuditRow>(
            r#"
            SELECT * FROM node_removal_audit
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch recent audit log")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Clean up old audit entries (keep last N days)
    pub async fn cleanup_old_audit(&self, retention_days: i64) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(retention_days);

        let result = sqlx::query(
            r#"
            DELETE FROM node_removal_audit
            WHERE datetime(created_at) < datetime(?1)
            "#,
        )
        .bind(cutoff.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to cleanup audit log")?;

        Ok(result.rows_affected())
    }

    /// Clean up old removed entries (keep last N days)
    pub async fn cleanup_removed_entries(&self, retention_days: i64) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(retention_days);

        let result = sqlx::query(
            r#"
            DELETE FROM pending_node_removals
            WHERE removed_at IS NOT NULL
              AND datetime(removed_at) < datetime(?1)
            "#,
        )
        .bind(cutoff.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to cleanup removed entries")?;

        Ok(result.rows_affected())
    }
}

// ============================================================================
// Database Row Types
// ============================================================================

#[derive(Debug, FromRow)]
struct PendingRemovalRow {
    id: String,
    certname: String,
    removal_reason: String,
    marked_at: String,
    scheduled_removal_at: String,
    removed_at: Option<String>,
    notes: Option<String>,
    marked_by: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<PendingRemovalRow> for PendingNodeRemoval {
    fn from(row: PendingRemovalRow) -> Self {
        Self {
            id: row.id,
            certname: row.certname,
            removal_reason: RemovalReason::parse(&row.removal_reason)
                .unwrap_or(RemovalReason::Manual),
            marked_at: DateTime::parse_from_rfc3339(&row.marked_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            scheduled_removal_at: DateTime::parse_from_rfc3339(&row.scheduled_removal_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            removed_at: row.removed_at.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            notes: row.notes,
            marked_by: row.marked_by,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}

#[derive(Debug, FromRow)]
struct AuditRow {
    id: String,
    certname: String,
    action: String,
    reason: Option<String>,
    performed_by: Option<String>,
    details: Option<String>,
    created_at: String,
}

impl From<AuditRow> for NodeRemovalAudit {
    fn from(row: AuditRow) -> Self {
        Self {
            id: row.id,
            certname: row.certname,
            action: RemovalAuditAction::parse(&row.action).unwrap_or(RemovalAuditAction::Marked),
            reason: row.reason,
            performed_by: row.performed_by,
            details: row.details,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}

#[derive(Debug, FromRow)]
struct StatsRow {
    total_pending: i64,
    revoked_certificates: i64,
    no_certificates: i64,
    manual: i64,
    due_today: i64,
    due_this_week: i64,
}
