//! Backup models
//!
//! Data types for server backup and restore functionality.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Enums
// =============================================================================

/// Backup status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum BackupStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Deleted,
}

impl std::fmt::Display for BackupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupStatus::Pending => write!(f, "pending"),
            BackupStatus::InProgress => write!(f, "in_progress"),
            BackupStatus::Completed => write!(f, "completed"),
            BackupStatus::Failed => write!(f, "failed"),
            BackupStatus::Deleted => write!(f, "deleted"),
        }
    }
}

impl From<String> for BackupStatus {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => BackupStatus::Pending,
            "in_progress" => BackupStatus::InProgress,
            "completed" => BackupStatus::Completed,
            "failed" => BackupStatus::Failed,
            "deleted" => BackupStatus::Deleted,
            _ => BackupStatus::Pending,
        }
    }
}

/// Backup trigger type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum BackupTrigger {
    Manual,
    Scheduled,
}

impl std::fmt::Display for BackupTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupTrigger::Manual => write!(f, "manual"),
            BackupTrigger::Scheduled => write!(f, "scheduled"),
        }
    }
}

impl From<String> for BackupTrigger {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "scheduled" => BackupTrigger::Scheduled,
            _ => BackupTrigger::Manual,
        }
    }
}

// =============================================================================
// Database Models
// =============================================================================

/// Server backup record from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerBackup {
    pub id: Uuid,
    pub filename: String,
    pub file_path: String,
    pub file_size: i64,
    pub checksum: String,
    pub uncompressed_size: Option<i64>,
    pub is_encrypted: bool,
    pub encryption_salt: Option<String>,
    pub encryption_nonce: Option<String>,
    pub trigger_type: BackupTrigger,
    pub status: BackupStatus,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub includes_database: bool,
    pub includes_config: bool,
    pub database_version: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Backup schedule record from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub frequency: String,
    pub cron_expression: Option<String>,
    pub time_of_day: String,
    pub day_of_week: i32,
    pub retention_count: i32,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Backup restore record from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRestore {
    pub id: Uuid,
    pub backup_id: Uuid,
    pub status: BackupStatus,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub restored_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// =============================================================================
// API Response Types
// =============================================================================

/// API response for server backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerBackupResponse {
    pub id: Uuid,
    pub filename: String,
    pub file_size: i64,
    pub file_size_formatted: String,
    pub is_encrypted: bool,
    pub trigger_type: BackupTrigger,
    pub status: BackupStatus,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub created_by: Option<Uuid>,
    pub created_by_username: Option<String>,
    pub includes_database: bool,
    pub includes_config: bool,
    pub database_version: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ServerBackupResponse {
    pub fn from_backup(backup: ServerBackup, username: Option<String>) -> Self {
        let duration_seconds = match (backup.started_at, backup.completed_at) {
            (Some(start), Some(end)) => Some((end - start).num_seconds()),
            _ => None,
        };

        Self {
            id: backup.id,
            filename: backup.filename,
            file_size: backup.file_size,
            file_size_formatted: format_file_size(backup.file_size),
            is_encrypted: backup.is_encrypted,
            trigger_type: backup.trigger_type,
            status: backup.status,
            error_message: backup.error_message,
            started_at: backup.started_at,
            completed_at: backup.completed_at,
            duration_seconds,
            created_by: backup.created_by,
            created_by_username: username,
            includes_database: backup.includes_database,
            includes_config: backup.includes_config,
            database_version: backup.database_version,
            notes: backup.notes,
            created_at: backup.created_at,
        }
    }
}

/// API response for backup schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupScheduleResponse {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub frequency: String,
    pub cron_expression: Option<String>,
    pub time_of_day: String,
    pub day_of_week: i32,
    pub day_of_week_name: String,
    pub retention_count: i32,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
}

impl From<BackupSchedule> for BackupScheduleResponse {
    fn from(schedule: BackupSchedule) -> Self {
        let day_name = match schedule.day_of_week {
            0 => "Sunday",
            1 => "Monday",
            2 => "Tuesday",
            3 => "Wednesday",
            4 => "Thursday",
            5 => "Friday",
            6 => "Saturday",
            _ => "Unknown",
        };

        Self {
            id: schedule.id,
            name: schedule.name,
            is_active: schedule.is_active,
            frequency: schedule.frequency,
            cron_expression: schedule.cron_expression,
            time_of_day: schedule.time_of_day,
            day_of_week: schedule.day_of_week,
            day_of_week_name: day_name.to_string(),
            retention_count: schedule.retention_count,
            last_run_at: schedule.last_run_at,
            next_run_at: schedule.next_run_at,
        }
    }
}

/// API response for backup restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRestoreResponse {
    pub id: Uuid,
    pub backup_id: Uuid,
    pub backup_filename: Option<String>,
    pub status: BackupStatus,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub restored_by: Option<Uuid>,
    pub restored_by_username: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl BackupRestoreResponse {
    pub fn from_restore(
        restore: BackupRestore,
        backup_filename: Option<String>,
        username: Option<String>,
    ) -> Self {
        let duration_seconds = match (restore.started_at, restore.completed_at) {
            (Some(start), Some(end)) => Some((end - start).num_seconds()),
            _ => None,
        };

        Self {
            id: restore.id,
            backup_id: restore.backup_id,
            backup_filename,
            status: restore.status,
            error_message: restore.error_message,
            started_at: restore.started_at,
            completed_at: restore.completed_at,
            duration_seconds,
            restored_by: restore.restored_by,
            restored_by_username: username,
            created_at: restore.created_at,
        }
    }
}

// =============================================================================
// Request Types
// =============================================================================

/// Request to create a new backup
#[derive(Debug, Clone, Deserialize)]
pub struct CreateBackupRequest {
    /// Password for encryption (required if encryption is enabled)
    pub password: Option<String>,
    /// Optional notes/description
    pub notes: Option<String>,
    /// Include database files (default: true)
    #[serde(default = "default_true")]
    pub include_database: bool,
    /// Include config files (default: true)
    #[serde(default = "default_true")]
    pub include_config: bool,
}

fn default_true() -> bool {
    true
}

/// Request to restore from a backup
#[derive(Debug, Clone, Deserialize)]
pub struct RestoreBackupRequest {
    /// Password to decrypt the backup
    pub password: String,
    /// Explicit confirmation required
    pub confirm: bool,
}

/// Request to verify a backup
#[derive(Debug, Clone, Deserialize)]
pub struct VerifyBackupRequest {
    /// Password to verify decryption
    pub password: String,
}

/// Response from backup verification
#[derive(Debug, Clone, Serialize)]
pub struct VerifyBackupResponse {
    pub valid: bool,
    pub checksum_match: bool,
    pub can_decrypt: bool,
    pub file_count: Option<usize>,
    pub total_size: Option<i64>,
    pub error: Option<String>,
}

/// Request to update backup schedule
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBackupScheduleRequest {
    pub is_active: Option<bool>,
    pub frequency: Option<String>,
    pub cron_expression: Option<String>,
    pub time_of_day: Option<String>,
    pub day_of_week: Option<i32>,
    pub retention_count: Option<i32>,
}

/// Query parameters for listing backups
#[derive(Debug, Clone, Deserialize)]
pub struct ListBackupsQuery {
    pub status: Option<String>,
    pub trigger_type: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

// =============================================================================
// Feature Status
// =============================================================================

/// Backup feature status response
#[derive(Debug, Clone, Serialize)]
pub struct BackupFeatureStatus {
    pub enabled: bool,
    pub backup_dir: String,
    pub backup_dir_exists: bool,
    pub backup_dir_writable: bool,
    pub encryption_enabled: bool,
    pub schedule_active: bool,
    pub total_backups: i64,
    pub total_size: i64,
    pub total_size_formatted: String,
    pub last_backup_at: Option<DateTime<Utc>>,
    pub next_scheduled_backup: Option<DateTime<Utc>>,
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Format file size in human-readable format
pub fn format_file_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(500), "500 B");
        assert_eq!(format_file_size(1024), "1.00 KB");
        assert_eq!(format_file_size(1536), "1.50 KB");
        assert_eq!(format_file_size(1048576), "1.00 MB");
        assert_eq!(format_file_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_backup_status_display() {
        assert_eq!(BackupStatus::Pending.to_string(), "pending");
        assert_eq!(BackupStatus::InProgress.to_string(), "in_progress");
        assert_eq!(BackupStatus::Completed.to_string(), "completed");
    }

    #[test]
    fn test_backup_status_from_string() {
        assert_eq!(BackupStatus::from("pending".to_string()), BackupStatus::Pending);
        assert_eq!(BackupStatus::from("IN_PROGRESS".to_string()), BackupStatus::InProgress);
        assert_eq!(BackupStatus::from("unknown".to_string()), BackupStatus::Pending);
    }
}
