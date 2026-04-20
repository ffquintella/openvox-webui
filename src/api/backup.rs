//! Backup API endpoints
//!
//! Provides REST API for managing server backups, schedules, and restores.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    middleware::AuthUser,
    models::{
        format_file_size, BackupFeatureStatus, BackupRestoreResponse, BackupScheduleResponse,
        CreateBackupRequest, ListBackupsQuery, RestoreBackupRequest, ServerBackupResponse,
        UpdateBackupScheduleRequest, VerifyBackupRequest, VerifyBackupResponse,
    },
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Feature status
        .route("/status", get(get_feature_status))
        // Backups
        .route("/backups", get(list_backups).post(create_backup))
        .route("/backups/{id}", get(get_backup).delete(delete_backup))
        .route("/backups/{id}/download", get(download_backup))
        .route("/backups/{id}/verify", post(verify_backup))
        .route("/backups/{id}/restore", post(restore_backup))
        // Schedule
        .route("/schedule", get(get_schedule).put(update_schedule))
        // Restore history
        .route("/restores", get(list_restores))
}

// ============================================================================
// Feature Status
// ============================================================================

/// Get backup feature status
///
/// Returns whether the feature is enabled and current status information.
async fn get_feature_status(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<BackupFeatureStatus>, AppError> {
    require_permission(&auth_user, "backup_view")?;

    let enabled = state.backup_config.as_ref().is_some_and(|c| c.enabled);

    if !enabled {
        return Ok(Json(BackupFeatureStatus {
            enabled: false,
            backup_dir: String::new(),
            backup_dir_exists: false,
            backup_dir_writable: false,
            encryption_enabled: false,
            schedule_active: false,
            total_backups: 0,
            total_size: 0,
            total_size_formatted: "0 B".to_string(),
            last_backup_at: None,
            next_scheduled_backup: None,
        }));
    }

    let service = state.backup_service()?;
    let (dir_exists, dir_writable) = service.check_backup_dir();
    let (total_backups, total_size) = service.get_stats().await.unwrap_or((0, 0));
    let last_backup = service.get_last_backup().await.ok().flatten();
    let schedule = service.get_schedule().await.ok().flatten();

    Ok(Json(BackupFeatureStatus {
        enabled: true,
        backup_dir: service.backup_dir().to_string_lossy().to_string(),
        backup_dir_exists: dir_exists,
        backup_dir_writable: dir_writable,
        encryption_enabled: service.config().encryption.enabled,
        schedule_active: schedule.as_ref().is_some_and(|s| s.is_active),
        total_backups,
        total_size,
        total_size_formatted: format_file_size(total_size),
        last_backup_at: last_backup.as_ref().and_then(|b| b.completed_at),
        next_scheduled_backup: schedule.and_then(|s| s.next_run_at),
    }))
}

// ============================================================================
// Backup Handlers
// ============================================================================

/// List all backups
async fn list_backups(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ListBackupsQuery>,
) -> Result<Json<Vec<ServerBackupResponse>>, AppError> {
    require_permission(&auth_user, "backup_view")?;

    let service = state.backup_service()?;
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let backups = service
        .list_backups(
            query.status.as_deref(),
            query.trigger_type.as_deref(),
            limit,
            offset,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to list backups: {}", e);
            AppError::internal("Failed to list backups")
        })?;

    // Convert to response type (username lookup could be added later)
    let responses: Vec<ServerBackupResponse> = backups
        .into_iter()
        .map(|b| ServerBackupResponse::from_backup(b, None))
        .collect();

    Ok(Json(responses))
}

/// Get a single backup by ID
async fn get_backup(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerBackupResponse>, AppError> {
    require_permission(&auth_user, "backup_view")?;

    let service = state.backup_service()?;
    let backup = service
        .get_backup(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get backup: {}", e);
            AppError::internal("Failed to get backup")
        })?
        .ok_or_else(|| AppError::not_found("Backup not found"))?;

    Ok(Json(ServerBackupResponse::from_backup(backup, None)))
}

/// Create a new backup
async fn create_backup(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateBackupRequest>,
) -> Result<(StatusCode, Json<ServerBackupResponse>), AppError> {
    require_permission(&auth_user, "backup_create")?;

    let service = state.backup_service()?;

    // Check if encryption is enabled and password is required
    if service.config().encryption.enabled
        && service.config().encryption.require_password
        && payload.password.is_none()
    {
        return Err(AppError::bad_request(
            "Password is required for encrypted backups",
        ));
    }

    let user_id = auth_user.user_id();
    let username = auth_user.username.clone();

    let backup = service
        .create_backup(
            payload.password.as_deref(),
            payload.notes.as_deref(),
            crate::models::BackupTrigger::Manual,
            Some(user_id),
            payload.include_database,
            payload.include_config,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create backup: {}", e);
            AppError::internal(format!("Failed to create backup: {}", e))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(ServerBackupResponse::from_backup(backup, Some(username))),
    ))
}

/// Delete a backup
async fn delete_backup(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_permission(&auth_user, "backup_delete")?;

    let service = state.backup_service()?;
    let deleted = service.delete_backup(id).await.map_err(|e| {
        tracing::error!("Failed to delete backup: {}", e);
        AppError::internal("Failed to delete backup")
    })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Backup not found"))
    }
}

/// Download a backup file
async fn download_backup(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response<Body>, AppError> {
    require_permission(&auth_user, "backup_view")?;

    let service = state.backup_service()?;
    let backup = service
        .get_backup(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get backup: {}", e);
            AppError::internal("Failed to get backup")
        })?
        .ok_or_else(|| AppError::not_found("Backup not found"))?;

    let file_path = service
        .get_backup_path(&backup)
        .ok_or_else(|| AppError::not_found("Backup file not found"))?;

    let file = tokio::fs::File::open(&file_path).await.map_err(|e| {
        tracing::error!("Failed to open backup file: {}", e);
        AppError::internal("Failed to open backup file")
    })?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", backup.filename),
        )
        .header(header::CONTENT_LENGTH, backup.file_size)
        .body(body)
        .map_err(|e| {
            tracing::error!("Failed to build response: {}", e);
            AppError::internal("Failed to build response")
        })?;

    Ok(response)
}

/// Verify backup integrity
async fn verify_backup(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<VerifyBackupRequest>,
) -> Result<Json<VerifyBackupResponse>, AppError> {
    require_permission(&auth_user, "backup_view")?;

    let service = state.backup_service()?;
    let result = service
        .verify_backup(id, &payload.password)
        .await
        .map_err(|e| {
            tracing::error!("Failed to verify backup: {}", e);
            AppError::internal("Failed to verify backup")
        })?;

    Ok(Json(result))
}

/// Restore from a backup
async fn restore_backup(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<RestoreBackupRequest>,
) -> Result<Json<BackupRestoreResponse>, AppError> {
    require_permission(&auth_user, "backup_admin")?;

    if !payload.confirm {
        return Err(AppError::bad_request(
            "Confirmation required to restore backup",
        ));
    }

    let service = state.backup_service()?;

    // Get the backup to include filename in response
    let backup = service
        .get_backup(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get backup: {}", e);
            AppError::internal("Failed to get backup")
        })?
        .ok_or_else(|| AppError::not_found("Backup not found"))?;

    let user_id = auth_user.user_id();
    let username = auth_user.username.clone();

    let restore = service
        .restore_backup(id, &payload.password, Some(user_id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to restore backup: {}", e);
            AppError::internal(format!("Failed to restore backup: {}", e))
        })?;

    Ok(Json(BackupRestoreResponse::from_restore(
        restore,
        Some(backup.filename),
        Some(username),
    )))
}

// ============================================================================
// Schedule Handlers
// ============================================================================

/// Get backup schedule
async fn get_schedule(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Option<BackupScheduleResponse>>, AppError> {
    require_permission(&auth_user, "backup_view")?;

    let service = state.backup_service()?;
    let schedule = service.get_schedule().await.map_err(|e| {
        tracing::error!("Failed to get schedule: {}", e);
        AppError::internal("Failed to get schedule")
    })?;

    Ok(Json(schedule.map(BackupScheduleResponse::from)))
}

/// Update backup schedule
async fn update_schedule(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<UpdateBackupScheduleRequest>,
) -> Result<Json<BackupScheduleResponse>, AppError> {
    require_permission(&auth_user, "backup_admin")?;

    let service = state.backup_service()?;

    // Get existing schedule
    let mut schedule = service
        .get_schedule()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get schedule: {}", e);
            AppError::internal("Failed to get schedule")
        })?
        .ok_or_else(|| AppError::not_found("Schedule not found"))?;

    // Update fields
    if let Some(is_active) = payload.is_active {
        schedule.is_active = is_active;
    }
    if let Some(ref frequency) = payload.frequency {
        schedule.frequency = frequency.clone();
    }
    if let Some(ref cron) = payload.cron_expression {
        schedule.cron_expression = Some(cron.clone());
    }
    if let Some(ref time) = payload.time_of_day {
        schedule.time_of_day = time.clone();
    }
    if let Some(day) = payload.day_of_week {
        schedule.day_of_week = day;
    }
    if let Some(count) = payload.retention_count {
        schedule.retention_count = count;
    }

    service.update_schedule(&schedule).await.map_err(|e| {
        tracing::error!("Failed to update schedule: {}", e);
        AppError::internal("Failed to update schedule")
    })?;

    Ok(Json(BackupScheduleResponse::from(schedule)))
}

// ============================================================================
// Restore History Handlers
// ============================================================================

/// List restore history
async fn list_restores(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Vec<BackupRestoreResponse>>, AppError> {
    require_permission(&auth_user, "backup_view")?;

    let service = state.backup_service()?;
    let restores = service.list_restores(100).await.map_err(|e| {
        tracing::error!("Failed to list restores: {}", e);
        AppError::internal("Failed to list restores")
    })?;

    // Convert to response type
    let responses: Vec<BackupRestoreResponse> = restores
        .into_iter()
        .map(|r| BackupRestoreResponse::from_restore(r, None, None))
        .collect();

    Ok(Json(responses))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if user has required permission for backup operations
fn require_permission(auth_user: &AuthUser, _permission: &str) -> Result<(), AppError> {
    // Super admins have all permissions
    if auth_user.is_super_admin() {
        return Ok(());
    }

    // Check if user is admin (has backup permissions)
    // The actual permission checking is done via RBAC in the database
    // For backups, we allow admin role
    if auth_user.roles.iter().any(|r| r == "admin") {
        return Ok(());
    }

    // Operators have read-only access to some endpoints
    // This is a simplified check - full RBAC would query the database
    Err(AppError::forbidden(
        "Insufficient permissions for backup operations",
    ))
}
