//! Organization (tenant) API endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::{
    db::{AuditRepository, OrganizationRepository},
    middleware::AuthUser,
    models::{CreateOrganizationRequest, Organization, UpdateOrganizationRequest},
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_organizations).post(create_organization))
        .route("/current", get(get_current_organization))
        .route(
            "/{id}",
            get(get_organization)
                .put(update_organization)
                .delete(delete_organization),
        )
}

fn require_super_admin(auth_user: &AuthUser) -> Result<(), AppError> {
    if auth_user.is_super_admin() {
        Ok(())
    } else {
        Err(AppError::forbidden("super_admin role required"))
    }
}

async fn list_organizations(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Vec<Organization>>, AppError> {
    require_super_admin(&auth_user)?;

    let repo = OrganizationRepository::new(&state.db);
    let orgs = repo.list().await.map_err(|e| {
        tracing::error!("Failed to list organizations: {}", e);
        AppError::internal("Failed to list organizations")
    })?;

    Ok(Json(orgs))
}

async fn get_current_organization(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Organization>, AppError> {
    let repo = OrganizationRepository::new(&state.db);
    let org = repo
        .get_by_id(auth_user.organization_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get current organization: {}", e);
            AppError::internal("Failed to get current organization")
        })?
        .ok_or_else(|| AppError::not_found("Organization not found"))?;

    Ok(Json(org))
}

async fn get_organization(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Organization>, AppError> {
    let uuid =
        Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid organization ID"))?;

    // Allow users to read their own org; cross-tenant reads require super_admin.
    if uuid != auth_user.organization_id {
        require_super_admin(&auth_user)?;
    }

    let repo = OrganizationRepository::new(&state.db);
    let org = repo.get_by_id(uuid).await.map_err(|e| {
        tracing::error!("Failed to get organization: {}", e);
        AppError::internal("Failed to get organization")
    })?;

    match org {
        Some(o) => Ok(Json(o)),
        None => Err(AppError::not_found("Organization not found")),
    }
}

async fn create_organization(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateOrganizationRequest>,
) -> Result<(StatusCode, Json<Organization>), AppError> {
    require_super_admin(&auth_user)?;

    let repo = OrganizationRepository::new(&state.db);
    let org = repo.create(&payload).await.map_err(|e| {
        tracing::error!("Failed to create organization: {}", e);
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::conflict("Organization name/slug already exists")
        } else {
            AppError::internal("Failed to create organization")
        }
    })?;

    let audit_repo = AuditRepository::new(&state.db);
    let _ = audit_repo
        .insert(
            auth_user.organization_id,
            Some(auth_user.user_id()),
            "organization.create",
            "organizations",
            Some(&org.id.to_string()),
            Some(&serde_json::json!({ "name": org.name, "slug": org.slug })),
            None,
        )
        .await;

    Ok((StatusCode::CREATED, Json(org)))
}

async fn update_organization(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateOrganizationRequest>,
) -> Result<Json<Organization>, AppError> {
    require_super_admin(&auth_user)?;
    let uuid =
        Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid organization ID"))?;

    let repo = OrganizationRepository::new(&state.db);
    let updated = repo.update(uuid, &payload).await.map_err(|e| {
        tracing::error!("Failed to update organization: {}", e);
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::conflict("Organization name/slug already exists")
        } else {
            AppError::internal("Failed to update organization")
        }
    })?;

    match updated {
        Some(org) => {
            let audit_repo = AuditRepository::new(&state.db);
            let _ = audit_repo
                .insert(
                    auth_user.organization_id,
                    Some(auth_user.user_id()),
                    "organization.update",
                    "organizations",
                    Some(&org.id.to_string()),
                    Some(&serde_json::json!({ "name": org.name, "slug": org.slug })),
                    None,
                )
                .await;
            Ok(Json(org))
        }
        None => Err(AppError::not_found("Organization not found")),
    }
}

async fn delete_organization(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<bool>, AppError> {
    require_super_admin(&auth_user)?;
    let uuid =
        Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid organization ID"))?;

    if uuid.to_string() == crate::models::DEFAULT_ORGANIZATION_ID {
        return Err(AppError::bad_request(
            "Default organization cannot be deleted",
        ));
    }

    let repo = OrganizationRepository::new(&state.db);
    let deleted = repo.delete(uuid).await.map_err(|e| {
        tracing::error!("Failed to delete organization: {}", e);
        AppError::internal("Failed to delete organization")
    })?;

    if deleted {
        let audit_repo = AuditRepository::new(&state.db);
        let _ = audit_repo
            .insert(
                auth_user.organization_id,
                Some(auth_user.user_id()),
                "organization.delete",
                "organizations",
                Some(&uuid.to_string()),
                None,
                None,
            )
            .await;
    }

    Ok(Json(deleted))
}
