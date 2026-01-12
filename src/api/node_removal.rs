//! Node removal API endpoints
//!
//! Provides REST API for managing nodes pending removal due to revoked or missing certificates.

use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};

use crate::{
    config::NodeRemovalConfig,
    db::NodeRemovalRepository,
    middleware::AuthUser,
    models::{
        ExtendRemovalDeadlineRequest, MarkNodeForRemovalRequest, NodeRemovalAudit,
        PendingNodeRemovalResponse, PendingRemovalStats, RemovalReason,
    },
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Feature status
        .route("/status", get(get_feature_status))
        // Pending removals
        .route("/pending", get(list_pending_removals))
        .route("/pending/{certname}", get(get_pending_removal))
        .route("/pending/{certname}", delete(unmark_removal))
        // Manual marking
        .route("/mark", post(mark_for_removal))
        // Extend deadline
        .route("/extend", post(extend_deadline))
        // Statistics
        .route("/stats", get(get_stats))
        // Audit log
        .route("/audit", get(list_audit_log))
        .route("/audit/{certname}", get(get_node_audit_log))
}

/// Check if user has read permission for node removal
fn require_read_permission(auth_user: &AuthUser) -> Result<(), AppError> {
    // Super admins have all permissions
    if auth_user.is_super_admin() {
        return Ok(());
    }

    // Check if user has admin or operator role
    if auth_user.roles.iter().any(|r| r == "admin" || r == "operator") {
        return Ok(());
    }

    Err(AppError::forbidden(
        "Insufficient permissions for node removal operations",
    ))
}

/// Check if user has write permission for node removal
fn require_write_permission(auth_user: &AuthUser) -> Result<(), AppError> {
    // Super admins have all permissions
    if auth_user.is_super_admin() {
        return Ok(());
    }

    // Only admin role can modify node removal settings
    if auth_user.roles.iter().any(|r| r == "admin") {
        return Ok(());
    }

    Err(AppError::forbidden(
        "Insufficient permissions for node removal write operations",
    ))
}

// ============================================================================
// Feature Status
// ============================================================================

/// Response for node removal feature status
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeRemovalFeatureStatus {
    pub enabled: bool,
    pub retention_days: i64,
    pub check_interval_secs: u64,
    pub puppetdb_connected: bool,
    pub puppet_ca_connected: bool,
}

/// Get node removal feature status
async fn get_feature_status(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<NodeRemovalFeatureStatus>, AppError> {
    require_read_permission(&auth_user)?;

    let config = state.config.node_removal.clone().unwrap_or_default();

    Ok(Json(NodeRemovalFeatureStatus {
        enabled: config.enabled,
        retention_days: config.retention_days,
        check_interval_secs: config.check_interval_secs.unwrap_or(300),
        puppetdb_connected: state.puppetdb.is_some(),
        puppet_ca_connected: state.puppet_ca.is_some(),
    }))
}

// ============================================================================
// Pending Removals
// ============================================================================

/// List all nodes pending removal
async fn list_pending_removals(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Vec<PendingNodeRemovalResponse>>, AppError> {
    require_read_permission(&auth_user)?;

    let repo = NodeRemovalRepository::new(state.db.clone());
    let pending = repo.get_all_pending().await.map_err(|e| {
        AppError::Internal(format!("Failed to fetch pending removals: {}", e))
    })?;

    let responses: Vec<PendingNodeRemovalResponse> = pending.into_iter().map(Into::into).collect();

    Ok(Json(responses))
}

/// Get a specific pending removal by certname
async fn get_pending_removal(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(certname): Path<String>,
) -> Result<Json<PendingNodeRemovalResponse>, AppError> {
    require_read_permission(&auth_user)?;

    let repo = NodeRemovalRepository::new(state.db.clone());
    let pending = repo
        .get_pending_removal(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch pending removal: {}", e)))?
        .ok_or_else(|| {
            AppError::NotFound(format!("No pending removal found for node: {}", certname))
        })?;

    Ok(Json(pending.into()))
}

/// Unmark a node (cancel pending removal)
async fn unmark_removal(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(certname): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_write_permission(&auth_user)?;

    let repo = NodeRemovalRepository::new(state.db.clone());
    let removed = repo
        .unmark_removal(&certname, Some(&auth_user.username), None)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to unmark node: {}", e)))?;

    if removed {
        Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Node '{}' unmarked for removal", certname)
        })))
    } else {
        Err(AppError::NotFound(format!(
            "No pending removal found for node: {}",
            certname
        )))
    }
}

// ============================================================================
// Manual Marking
// ============================================================================

/// Manually mark a node for removal
async fn mark_for_removal(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(request): Json<MarkNodeForRemovalRequest>,
) -> Result<Json<PendingNodeRemovalResponse>, AppError> {
    require_write_permission(&auth_user)?;

    let config: NodeRemovalConfig = state.config.node_removal.clone().unwrap_or_default();

    let repo = NodeRemovalRepository::new(state.db.clone());
    let reason = request.reason.unwrap_or(RemovalReason::Manual);

    let pending = repo
        .mark_for_removal(
            &request.certname,
            reason,
            config.retention_days,
            request.notes.as_deref(),
            Some(&auth_user.username),
        )
        .await
        .map_err(|e| AppError::Internal(format!("Failed to mark node: {}", e)))?;

    Ok(Json(pending.into()))
}

// ============================================================================
// Extend Deadline
// ============================================================================

/// Extend the removal deadline for a node
async fn extend_deadline(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(request): Json<ExtendRemovalDeadlineRequest>,
) -> Result<Json<PendingNodeRemovalResponse>, AppError> {
    require_write_permission(&auth_user)?;

    let repo = NodeRemovalRepository::new(state.db.clone());
    let pending = repo
        .extend_deadline(
            &request.certname,
            request.extend_days as i64,
            Some(&auth_user.username),
            request.notes.as_deref(),
        )
        .await
        .map_err(|e| AppError::Internal(format!("Failed to extend deadline: {}", e)))?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "No pending removal found for node: {}",
                request.certname
            ))
        })?;

    Ok(Json(pending.into()))
}

// ============================================================================
// Statistics
// ============================================================================

/// Get statistics for pending removals
async fn get_stats(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<PendingRemovalStats>, AppError> {
    require_read_permission(&auth_user)?;

    let repo = NodeRemovalRepository::new(state.db.clone());
    let stats = repo
        .get_stats()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch stats: {}", e)))?;

    Ok(Json(stats))
}

// ============================================================================
// Audit Log
// ============================================================================

/// List recent audit log entries
async fn list_audit_log(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Vec<NodeRemovalAudit>>, AppError> {
    require_read_permission(&auth_user)?;

    let repo = NodeRemovalRepository::new(state.db.clone());
    let audit = repo
        .get_recent_audit(100)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch audit log: {}", e)))?;

    Ok(Json(audit))
}

/// Get audit log for a specific node
async fn get_node_audit_log(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(certname): Path<String>,
) -> Result<Json<Vec<NodeRemovalAudit>>, AppError> {
    require_read_permission(&auth_user)?;

    let repo = NodeRemovalRepository::new(state.db.clone());
    let audit = repo
        .get_audit_log(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch audit log: {}", e)))?;

    Ok(Json(audit))
}
