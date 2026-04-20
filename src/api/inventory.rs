//! Inventory analytics and version intelligence endpoints.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    db::{repository::GroupRepository, CveRepository},
    middleware::AuthUser,
    models::{
        ApproveUpdateJobRequest, ComplianceCategoryNode, CreateUpdateJobRequest,
        FleetRepositoryConfig, InventoryDashboardReport, InventoryFleetStatusSummary,
        OutdatedSoftwareNodeDetail, RepositoryVersionCatalogEntry, UpdateJob, UpdateOperationType,
        UpdatePreviewPackage, UpdatePreviewRequest, UpdatePreviewResponse, UpdatePreviewTarget,
    },
    utils::error::{AppError, AppResult},
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/updates", get(list_update_jobs).post(create_update_job))
        .route("/updates/preview", post(preview_update_job))
        .route("/updates/{job_id}", get(get_update_job))
        .route("/updates/{job_id}/approve", post(approve_update_job))
        .route("/updates/{job_id}/cancel", post(cancel_update_job))
        .route("/dashboard", get(get_inventory_dashboard))
        .route(
            "/dashboard/outdated-software/{name}",
            get(get_outdated_software_nodes),
        )
        .route(
            "/dashboard/compliance/{category}",
            get(get_compliance_category_nodes),
        )
        .route("/summary", get(get_inventory_summary))
        .route("/catalog", get(list_version_catalog))
        .route("/repositories", get(list_fleet_repositories))
        .route("/repositories/check", post(trigger_repo_check))
}

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    pub software_type: Option<String>,
    pub platform_family: Option<String>,
    pub distribution: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateJobsQuery {
    pub limit: Option<usize>,
}

fn require_inventory_update_read(auth_user: &AuthUser) -> AppResult<()> {
    if auth_user.is_super_admin()
        || auth_user
            .roles
            .iter()
            .any(|role| role == "admin" || role == "operator" || role == "viewer")
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "Insufficient permissions for inventory update operations",
        ))
    }
}

fn require_inventory_update_write(auth_user: &AuthUser) -> AppResult<()> {
    if auth_user.is_super_admin()
        || auth_user
            .roles
            .iter()
            .any(|role| role == "admin" || role == "operator")
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "Insufficient permissions to manage inventory update jobs",
        ))
    }
}

async fn get_inventory_summary(
    State(state): State<AppState>,
) -> AppResult<Json<InventoryFleetStatusSummary>> {
    let repo = state.inventory_repository();
    let summary = repo
        .get_fleet_status_summary()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch inventory summary: {}", e)))?;

    Ok(Json(summary))
}

async fn get_inventory_dashboard(
    State(state): State<AppState>,
) -> AppResult<Json<InventoryDashboardReport>> {
    let repo = state.inventory_repository();
    let report = repo
        .get_dashboard_report()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch inventory dashboard: {}", e)))?;

    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
pub struct OutdatedSoftwareQuery {
    pub software_type: Option<String>,
}

async fn get_outdated_software_nodes(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(name): Path<String>,
    Query(query): Query<OutdatedSoftwareQuery>,
) -> AppResult<Json<Vec<OutdatedSoftwareNodeDetail>>> {
    require_inventory_update_read(&auth_user)?;
    let repo = state.inventory_repository();
    let nodes = repo
        .get_nodes_for_outdated_software(&name, query.software_type.as_deref())
        .await
        .map_err(|e| {
            AppError::Internal(format!("Failed to fetch outdated software nodes: {}", e))
        })?;
    Ok(Json(nodes))
}

async fn get_compliance_category_nodes(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(category): Path<String>,
) -> AppResult<Json<Vec<ComplianceCategoryNode>>> {
    require_inventory_update_read(&auth_user)?;
    let repo = state.inventory_repository();
    let nodes = repo
        .get_nodes_for_compliance_category(&category)
        .await
        .map_err(|e| {
            AppError::Internal(format!("Failed to fetch compliance category nodes: {}", e))
        })?;
    Ok(Json(nodes))
}

async fn list_version_catalog(
    State(state): State<AppState>,
    Query(query): Query<CatalogQuery>,
) -> AppResult<Json<Vec<RepositoryVersionCatalogEntry>>> {
    let repo = state.inventory_repository();
    let mut entries = repo
        .list_version_catalog()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch version catalog: {}", e)))?;

    if let Some(ref software_type) = query.software_type {
        entries.retain(|entry| entry.software_type == *software_type);
    }
    if let Some(ref platform_family) = query.platform_family {
        entries.retain(|entry| entry.platform_family == *platform_family);
    }
    if let Some(ref distribution) = query.distribution {
        entries.retain(|entry| entry.distribution == *distribution);
    }

    Ok(Json(entries))
}

async fn list_update_jobs(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<UpdateJobsQuery>,
) -> AppResult<Json<Vec<UpdateJob>>> {
    require_inventory_update_read(&auth_user)?;

    let repo = state.inventory_repository();
    let jobs = repo
        .list_update_jobs(query.limit.unwrap_or(50).min(200))
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch update jobs: {}", e)))?;

    Ok(Json(jobs))
}

async fn get_update_job(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(job_id): Path<String>,
) -> AppResult<Json<UpdateJob>> {
    require_inventory_update_read(&auth_user)?;

    let repo = state.inventory_repository();
    let job = repo
        .get_update_job(&job_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch update job: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Update job '{}' not found", job_id)))?;

    Ok(Json(job))
}

async fn create_update_job(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateUpdateJobRequest>,
) -> AppResult<(StatusCode, Json<UpdateJob>)> {
    require_inventory_update_write(&auth_user)?;

    let mut certnames = payload.certnames.clone();
    if let Some(group_id) = payload.group_id.as_deref() {
        let group_uuid = Uuid::parse_str(group_id)
            .map_err(|_| AppError::bad_request("group_id must be a valid UUID"))?;
        let group_repo = GroupRepository::new(&state.db);
        let mut group_nodes = group_repo
            .get_group_nodes(group_uuid)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to resolve target group: {}", e)))?;
        certnames.append(&mut group_nodes);
    }

    certnames.sort();
    certnames.dedup();
    certnames.retain(|certname| !certname.trim().is_empty());

    if certnames.is_empty() {
        return Err(AppError::bad_request(
            "At least one target node or a non-empty group is required",
        ));
    }

    // For SecurityPatch, resolve vulnerable packages from CVE matches
    let mut package_names = payload.package_names.clone();
    if payload.operation_type == UpdateOperationType::SecurityPatch && package_names.is_empty() {
        let cve_repo = CveRepository::new(state.db.clone());
        let vuln_packages = cve_repo
            .get_vulnerable_packages_for_nodes(&certnames)
            .await
            .map_err(|e| {
                AppError::Internal(format!("Failed to resolve security packages: {}", e))
            })?;

        // Collect all unique vulnerable package names across all targets
        let mut all_packages: Vec<String> = vuln_packages
            .into_iter()
            .flat_map(|(_, pkgs)| pkgs)
            .collect();
        all_packages.sort();
        all_packages.dedup();
        package_names = all_packages;

        if package_names.is_empty() {
            return Err(AppError::bad_request(
                "No vulnerable packages found for the selected nodes",
            ));
        }
    }

    let repo = state.inventory_repository();
    let job = repo
        .create_update_job(
            payload.operation_type,
            &package_names,
            payload.group_id.as_deref(),
            &certnames,
            payload.requires_approval,
            payload.scheduled_for,
            payload.maintenance_window_start,
            payload.maintenance_window_end,
            &auth_user.username,
            payload.approval_notes.as_deref(),
        )
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create update job: {}", e)))?;

    Ok((StatusCode::CREATED, Json(job)))
}

async fn approve_update_job(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(job_id): Path<String>,
    Json(payload): Json<ApproveUpdateJobRequest>,
) -> AppResult<Json<UpdateJob>> {
    require_inventory_update_write(&auth_user)?;

    let repo = state.inventory_repository();
    let job = repo
        .approve_update_job(
            &job_id,
            payload.approved,
            &auth_user.username,
            payload.notes.as_deref(),
        )
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update job approval state: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Update job '{}' not found", job_id)))?;

    Ok(Json(job))
}

async fn cancel_update_job(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(job_id): Path<String>,
) -> AppResult<Json<UpdateJob>> {
    require_inventory_update_write(&auth_user)?;

    let repo = state.inventory_repository();
    let job = repo
        .cancel_update_job(&job_id, &auth_user.username)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to cancel update job: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Update job '{}' not found", job_id)))?;

    Ok(Json(job))
}

async fn preview_update_job(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<UpdatePreviewRequest>,
) -> AppResult<Json<UpdatePreviewResponse>> {
    require_inventory_update_read(&auth_user)?;

    let mut certnames = payload.certnames.clone();
    if let Some(group_id) = payload.group_id.as_deref() {
        let group_uuid = Uuid::parse_str(group_id)
            .map_err(|_| AppError::bad_request("group_id must be a valid UUID"))?;
        let group_repo = GroupRepository::new(&state.db);
        let mut group_nodes = group_repo
            .get_group_nodes(group_uuid)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to resolve target group: {}", e)))?;
        certnames.append(&mut group_nodes);
    }
    certnames.sort();
    certnames.dedup();
    certnames.retain(|c| !c.trim().is_empty());

    let inv_repo = state.inventory_repository();
    let cve_repo = CveRepository::new(state.db.clone());

    let mut targets: Vec<UpdatePreviewTarget> = Vec::new();
    let mut total_packages = 0;

    for certname in &certnames {
        let update_status = inv_repo
            .get_host_update_status(certname)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get update status: {}", e)))?;

        let outdated_items = update_status.map(|s| s.outdated_items).unwrap_or_default();

        let mut packages_to_update: Vec<UpdatePreviewPackage> = Vec::new();

        for item in &outdated_items {
            match payload.operation_type {
                UpdateOperationType::SecurityPatch => {
                    let cve_ids = cve_repo
                        .get_cve_ids_for_package(certname, &item.name)
                        .await
                        .unwrap_or_default();
                    if !cve_ids.is_empty() {
                        packages_to_update.push(UpdatePreviewPackage {
                            name: item.name.clone(),
                            from_version: item.installed_version.clone(),
                            to_version: item.latest_version.clone(),
                            cve_ids,
                        });
                    }
                }
                UpdateOperationType::PackageUpdate => {
                    if payload.package_names.is_empty()
                        || payload.package_names.iter().any(|p| p == &item.name)
                    {
                        let cve_ids = cve_repo
                            .get_cve_ids_for_package(certname, &item.name)
                            .await
                            .unwrap_or_default();
                        packages_to_update.push(UpdatePreviewPackage {
                            name: item.name.clone(),
                            from_version: item.installed_version.clone(),
                            to_version: item.latest_version.clone(),
                            cve_ids,
                        });
                    }
                }
                UpdateOperationType::SystemPatch => {
                    let cve_ids = cve_repo
                        .get_cve_ids_for_package(certname, &item.name)
                        .await
                        .unwrap_or_default();
                    packages_to_update.push(UpdatePreviewPackage {
                        name: item.name.clone(),
                        from_version: item.installed_version.clone(),
                        to_version: item.latest_version.clone(),
                        cve_ids,
                    });
                }
                _ => {}
            }
        }

        total_packages += packages_to_update.len();
        if !packages_to_update.is_empty() {
            targets.push(UpdatePreviewTarget {
                certname: certname.clone(),
                packages_to_update,
            });
        }
    }

    Ok(Json(UpdatePreviewResponse {
        total_nodes: targets.len(),
        total_packages,
        targets,
    }))
}

async fn list_fleet_repositories(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<Vec<FleetRepositoryConfig>>> {
    require_inventory_update_read(&auth_user)?;
    let repo = state.inventory_repository();
    let configs = repo
        .list_fleet_repository_configs()
        .await
        .map_err(|e| AppError::internal(format!("Failed to list fleet repositories: {}", e)))?;
    Ok(Json(configs))
}

async fn trigger_repo_check(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    require_inventory_update_write(&auth_user)?;
    let repo = state.inventory_repository();
    let service = crate::services::RepoCheckerService::new(repo, 120, 4);
    let summary = service
        .check_all_repos()
        .await
        .map_err(|e| AppError::internal(format!("Repo check failed: {}", e)))?;
    Ok(Json(serde_json::json!({
        "repos_checked": summary.repos_checked,
        "repos_succeeded": summary.repos_succeeded,
        "repos_failed": summary.repos_failed,
        "catalog_entries_upserted": summary.catalog_entries_upserted,
    })))
}
