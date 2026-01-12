//! Backup repository
//!
//! Database operations for server backups, schedules, and restore history.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{
    BackupRestore, BackupSchedule, BackupStatus, BackupTrigger, ServerBackup,
};

/// Repository for backup-related database operations
pub struct BackupRepository {
    pool: SqlitePool,
}

impl BackupRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // =========================================================================
    // Server Backups
    // =========================================================================

    /// Create a new backup record
    pub async fn create_backup(&self, backup: &ServerBackup) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO server_backups (
                id, filename, file_path, file_size, checksum, uncompressed_size,
                is_encrypted, encryption_salt, encryption_nonce, trigger_type,
                status, error_message, started_at, completed_at, created_by,
                includes_database, includes_config, database_version, notes,
                created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21
            )
            "#,
        )
        .bind(backup.id.to_string())
        .bind(&backup.filename)
        .bind(&backup.file_path)
        .bind(backup.file_size)
        .bind(&backup.checksum)
        .bind(backup.uncompressed_size)
        .bind(backup.is_encrypted)
        .bind(&backup.encryption_salt)
        .bind(&backup.encryption_nonce)
        .bind(backup.trigger_type.to_string())
        .bind(backup.status.to_string())
        .bind(&backup.error_message)
        .bind(backup.started_at.map(|dt| dt.to_rfc3339()))
        .bind(backup.completed_at.map(|dt| dt.to_rfc3339()))
        .bind(backup.created_by.map(|id| id.to_string()))
        .bind(backup.includes_database)
        .bind(backup.includes_config)
        .bind(&backup.database_version)
        .bind(&backup.notes)
        .bind(backup.created_at.to_rfc3339())
        .bind(backup.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to create backup record")?;

        Ok(())
    }

    /// Get a backup by ID
    pub async fn get_backup(&self, id: Uuid) -> Result<Option<ServerBackup>> {
        let row = sqlx::query_as::<_, BackupRow>(
            r#"
            SELECT * FROM server_backups WHERE id = ?1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch backup")?;

        Ok(row.map(|r| r.into()))
    }

    /// Update backup status
    pub async fn update_backup_status(
        &self,
        id: Uuid,
        status: BackupStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE server_backups
            SET status = ?1, error_message = ?2, updated_at = ?3
            WHERE id = ?4
            "#,
        )
        .bind(status.to_string())
        .bind(error_message)
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("Failed to update backup status")?;

        Ok(())
    }

    /// Mark backup as completed
    pub async fn complete_backup(
        &self,
        id: Uuid,
        file_size: i64,
        checksum: &str,
        uncompressed_size: Option<i64>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE server_backups
            SET status = 'completed', file_size = ?1, checksum = ?2,
                uncompressed_size = ?3, completed_at = ?4, updated_at = ?5
            WHERE id = ?6
            "#,
        )
        .bind(file_size)
        .bind(checksum)
        .bind(uncompressed_size)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("Failed to complete backup")?;

        Ok(())
    }

    /// Mark backup as failed
    pub async fn fail_backup(&self, id: Uuid, error_message: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE server_backups
            SET status = 'failed', error_message = ?1, completed_at = ?2, updated_at = ?3
            WHERE id = ?4
            "#,
        )
        .bind(error_message)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("Failed to mark backup as failed")?;

        Ok(())
    }

    /// List backups with optional filters
    pub async fn list_backups(
        &self,
        status: Option<&str>,
        trigger_type: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ServerBackup>> {
        let mut query = String::from(
            "SELECT * FROM server_backups WHERE 1=1",
        );

        if status.is_some() {
            query.push_str(" AND status = ?1");
        }
        if trigger_type.is_some() {
            query.push_str(" AND trigger_type = ?2");
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ?3 OFFSET ?4");

        let rows = sqlx::query_as::<_, BackupRow>(&query)
            .bind(status.unwrap_or(""))
            .bind(trigger_type.unwrap_or(""))
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .context("Failed to list backups")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get all backups ordered by creation date
    pub async fn get_all_backups(&self) -> Result<Vec<ServerBackup>> {
        let rows = sqlx::query_as::<_, BackupRow>(
            "SELECT * FROM server_backups ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch all backups")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Delete a backup record (doesn't delete the file)
    pub async fn delete_backup(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM server_backups WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("Failed to delete backup")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get backup statistics
    pub async fn get_backup_stats(&self) -> Result<(i64, i64)> {
        let row: (i64, i64) = sqlx::query_as(
            r#"
            SELECT
                COUNT(*) as total_backups,
                COALESCE(SUM(file_size), 0) as total_size
            FROM server_backups
            WHERE status = 'completed'
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to get backup stats")?;

        Ok(row)
    }

    /// Get the most recent completed backup
    pub async fn get_last_backup(&self) -> Result<Option<ServerBackup>> {
        let row = sqlx::query_as::<_, BackupRow>(
            r#"
            SELECT * FROM server_backups
            WHERE status = 'completed'
            ORDER BY completed_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch last backup")?;

        Ok(row.map(|r| r.into()))
    }

    /// Get old backups that exceed retention count
    pub async fn get_backups_exceeding_retention(&self, retention_count: u32) -> Result<Vec<ServerBackup>> {
        let rows = sqlx::query_as::<_, BackupRow>(
            r#"
            SELECT * FROM server_backups
            WHERE status = 'completed'
            ORDER BY created_at DESC
            LIMIT -1 OFFSET ?1
            "#,
        )
        .bind(retention_count)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch backups exceeding retention")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    // =========================================================================
    // Backup Schedules
    // =========================================================================

    /// Get the default backup schedule
    pub async fn get_schedule(&self) -> Result<Option<BackupSchedule>> {
        let row = sqlx::query_as::<_, ScheduleRow>(
            "SELECT * FROM backup_schedules WHERE name = 'default' LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch backup schedule")?;

        Ok(row.map(|r| r.into()))
    }

    /// Update backup schedule
    pub async fn update_schedule(&self, schedule: &BackupSchedule) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE backup_schedules
            SET is_active = ?1, frequency = ?2, cron_expression = ?3,
                time_of_day = ?4, day_of_week = ?5, retention_count = ?6,
                next_run_at = ?7, updated_at = ?8
            WHERE id = ?9
            "#,
        )
        .bind(schedule.is_active)
        .bind(&schedule.frequency)
        .bind(&schedule.cron_expression)
        .bind(&schedule.time_of_day)
        .bind(schedule.day_of_week)
        .bind(schedule.retention_count)
        .bind(schedule.next_run_at.map(|dt| dt.to_rfc3339()))
        .bind(Utc::now().to_rfc3339())
        .bind(schedule.id.to_string())
        .execute(&self.pool)
        .await
        .context("Failed to update backup schedule")?;

        Ok(())
    }

    /// Update schedule last run time
    pub async fn update_schedule_last_run(&self, id: Uuid, last_run: DateTime<Utc>, next_run: Option<DateTime<Utc>>) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE backup_schedules
            SET last_run_at = ?1, next_run_at = ?2, updated_at = ?3
            WHERE id = ?4
            "#,
        )
        .bind(last_run.to_rfc3339())
        .bind(next_run.map(|dt| dt.to_rfc3339()))
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("Failed to update schedule last run")?;

        Ok(())
    }

    // =========================================================================
    // Backup Restores
    // =========================================================================

    /// Create a restore record
    pub async fn create_restore(&self, restore: &BackupRestore) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO backup_restores (
                id, backup_id, status, error_message, started_at,
                completed_at, restored_by, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(restore.id.to_string())
        .bind(restore.backup_id.to_string())
        .bind(restore.status.to_string())
        .bind(&restore.error_message)
        .bind(restore.started_at.map(|dt| dt.to_rfc3339()))
        .bind(restore.completed_at.map(|dt| dt.to_rfc3339()))
        .bind(restore.restored_by.map(|id| id.to_string()))
        .bind(restore.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to create restore record")?;

        Ok(())
    }

    /// Update restore status
    pub async fn update_restore_status(
        &self,
        id: Uuid,
        status: BackupStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let completed_at = if status == BackupStatus::Completed || status == BackupStatus::Failed {
            Some(Utc::now())
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE backup_restores
            SET status = ?1, error_message = ?2, completed_at = ?3
            WHERE id = ?4
            "#,
        )
        .bind(status.to_string())
        .bind(error_message)
        .bind(completed_at.map(|dt| dt.to_rfc3339()))
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("Failed to update restore status")?;

        Ok(())
    }

    /// List restore history
    pub async fn list_restores(&self, limit: u32) -> Result<Vec<BackupRestore>> {
        let rows = sqlx::query_as::<_, RestoreRow>(
            r#"
            SELECT * FROM backup_restores
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list restores")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get a restore by ID
    pub async fn get_restore(&self, id: Uuid) -> Result<Option<BackupRestore>> {
        let row = sqlx::query_as::<_, RestoreRow>(
            "SELECT * FROM backup_restores WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch restore")?;

        Ok(row.map(|r| r.into()))
    }
}

// =============================================================================
// Row Types for SQLx
// =============================================================================

#[derive(Debug, sqlx::FromRow)]
struct BackupRow {
    id: String,
    filename: String,
    file_path: String,
    file_size: i64,
    checksum: String,
    uncompressed_size: Option<i64>,
    is_encrypted: i32,
    encryption_salt: Option<String>,
    encryption_nonce: Option<String>,
    trigger_type: String,
    status: String,
    error_message: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    created_by: Option<String>,
    includes_database: i32,
    includes_config: i32,
    database_version: Option<String>,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<BackupRow> for ServerBackup {
    fn from(row: BackupRow) -> Self {
        Self {
            id: Uuid::parse_str(&row.id).unwrap_or_default(),
            filename: row.filename,
            file_path: row.file_path,
            file_size: row.file_size,
            checksum: row.checksum,
            uncompressed_size: row.uncompressed_size,
            is_encrypted: row.is_encrypted != 0,
            encryption_salt: row.encryption_salt,
            encryption_nonce: row.encryption_nonce,
            trigger_type: BackupTrigger::from(row.trigger_type),
            status: BackupStatus::from(row.status),
            error_message: row.error_message,
            started_at: row.started_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
            completed_at: row.completed_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
            created_by: row.created_by.and_then(|s| Uuid::parse_str(&s).ok()),
            includes_database: row.includes_database != 0,
            includes_config: row.includes_config != 0,
            database_version: row.database_version,
            notes: row.notes,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ScheduleRow {
    id: String,
    name: String,
    is_active: i32,
    frequency: String,
    cron_expression: Option<String>,
    time_of_day: String,
    day_of_week: i32,
    retention_count: i32,
    last_run_at: Option<String>,
    next_run_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<ScheduleRow> for BackupSchedule {
    fn from(row: ScheduleRow) -> Self {
        Self {
            id: Uuid::parse_str(&row.id).unwrap_or_default(),
            name: row.name,
            is_active: row.is_active != 0,
            frequency: row.frequency,
            cron_expression: row.cron_expression,
            time_of_day: row.time_of_day,
            day_of_week: row.day_of_week,
            retention_count: row.retention_count,
            last_run_at: row.last_run_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
            next_run_at: row.next_run_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RestoreRow {
    id: String,
    backup_id: String,
    status: String,
    error_message: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    restored_by: Option<String>,
    created_at: String,
}

impl From<RestoreRow> for BackupRestore {
    fn from(row: RestoreRow) -> Self {
        Self {
            id: Uuid::parse_str(&row.id).unwrap_or_default(),
            backup_id: Uuid::parse_str(&row.backup_id).unwrap_or_default(),
            status: BackupStatus::from(row.status),
            error_message: row.error_message,
            started_at: row.started_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
            completed_at: row.completed_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
            restored_by: row.restored_by.and_then(|s| Uuid::parse_str(&s).ok()),
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}
