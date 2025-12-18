//! Audit log API endpoints

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};

use crate::{
    db::AuditRepository,
    middleware::AuthUser,
    models::{AuditLogEntry, AuditLogQuery},
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(list_audit_logs))
}

fn can_view_audit_logs(auth_user: &AuthUser) -> bool {
    auth_user.is_super_admin()
        || auth_user.roles.iter().any(|r| r == "admin")
        || auth_user.roles.iter().any(|r| r == "auditor")
}

async fn list_audit_logs(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<Vec<AuditLogEntry>>, AppError> {
    if !can_view_audit_logs(&auth_user) {
        return Err(AppError::forbidden("Not allowed to view audit logs"));
    }

    let org_id = match query.organization_id {
        Some(org_id) if !auth_user.is_super_admin() => {
            return Err(AppError::forbidden(
                "organization_id can only be specified by super_admin",
            ));
        }
        Some(org_id) => org_id,
        None => auth_user.organization_id,
    };

    let repo = AuditRepository::new(&state.db);
    let logs = repo.list(org_id, &query).await.map_err(|e| {
        tracing::error!("Failed to list audit logs: {}", e);
        AppError::internal("Failed to list audit logs")
    })?;

    Ok(Json(logs))
}
