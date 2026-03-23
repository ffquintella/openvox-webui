//! CVE vulnerability API endpoints.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};

use crate::{
    db::CveRepository,
    middleware::AuthUser,
    models::{
        CreateCveFeedSourceRequest, CveDetailResponse, CveEntry, CveFeedSource, CveSearchQuery,
        FeedSyncResult, HostVulnerabilityMatch, NodeVulnerabilitySummary,
        UpdateCveFeedSourceRequest, VulnerabilityDashboardReport, VulnerableNodesQuery,
    },
    services::cve_feed::CveFeedService,
    utils::error::{AppError, AppResult},
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(get_vulnerability_dashboard))
        .route("/nodes", get(list_vulnerable_nodes))
        .route("/nodes/{certname}", get(get_node_vulnerabilities))
        .route("/entries", get(search_cve_entries))
        .route("/entries/{cve_id}", get(get_cve_detail))
        .route("/feeds", get(list_cve_feeds).post(create_cve_feed))
        .route("/feeds/{id}", put(update_cve_feed).delete(delete_cve_feed))
        .route("/feeds/{id}/sync", post(trigger_feed_sync))
        .route("/refresh-matches", post(trigger_match_refresh))
}

fn require_cve_read(auth_user: &AuthUser) -> AppResult<()> {
    if auth_user.is_super_admin()
        || auth_user
            .roles
            .iter()
            .any(|role| role == "admin" || role == "operator" || role == "viewer")
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "Insufficient permissions for CVE operations",
        ))
    }
}

fn require_cve_write(auth_user: &AuthUser) -> AppResult<()> {
    if auth_user.is_super_admin()
        || auth_user
            .roles
            .iter()
            .any(|role| role == "admin" || role == "operator")
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "Insufficient permissions to manage CVE feeds",
        ))
    }
}

async fn get_vulnerability_dashboard(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<VulnerabilityDashboardReport>> {
    require_cve_read(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let report = repo
        .get_fleet_vulnerability_dashboard()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get vulnerability dashboard: {}", e)))?;
    Ok(Json(report))
}

async fn list_vulnerable_nodes(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<VulnerableNodesQuery>,
) -> AppResult<Json<Vec<NodeVulnerabilitySummary>>> {
    require_cve_read(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());

    // Get all vulnerable node certnames, then get summaries
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT certname FROM host_vulnerability_matches ORDER BY certname",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("Failed to list vulnerable nodes: {}", e)))?;

    let limit = query.limit.unwrap_or(100).min(500);
    let mut summaries = Vec::new();
    for certname in rows.iter().take(limit) {
        let summary = repo
            .get_vulnerability_summary(certname)
            .await
            .map_err(|e| {
                AppError::Internal(format!("Failed to get vulnerability summary: {}", e))
            })?;
        if summary.total_count > 0 {
            if let Some(ref sev) = query.severity {
                let matches = match sev.as_str() {
                    "critical" => summary.critical_count > 0,
                    "high" => summary.high_count > 0,
                    "medium" => summary.medium_count > 0,
                    "low" => summary.low_count > 0,
                    _ => true,
                };
                if matches {
                    summaries.push(summary);
                }
            } else {
                summaries.push(summary);
            }
        }
    }

    Ok(Json(summaries))
}

async fn get_node_vulnerabilities(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(certname): Path<String>,
) -> AppResult<Json<Vec<HostVulnerabilityMatch>>> {
    require_cve_read(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let vulns = repo
        .get_node_vulnerabilities(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get node vulnerabilities: {}", e)))?;
    Ok(Json(vulns))
}

async fn search_cve_entries(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<CveSearchQuery>,
) -> AppResult<Json<Vec<CveEntry>>> {
    require_cve_read(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let entries = repo
        .search_cves(
            query.query.as_deref(),
            query.severity.as_deref(),
            query.is_kev,
            query.limit.unwrap_or(100),
        )
        .await
        .map_err(|e| AppError::Internal(format!("Failed to search CVEs: {}", e)))?;
    Ok(Json(entries))
}

async fn get_cve_detail(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(cve_id): Path<String>,
) -> AppResult<Json<CveDetailResponse>> {
    require_cve_read(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let detail = repo
        .get_cve_detail(&cve_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get CVE detail: {}", e)))?
        .ok_or_else(|| AppError::not_found(format!("CVE '{}' not found", cve_id)))?;
    Ok(Json(detail))
}

async fn list_cve_feeds(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<Vec<CveFeedSource>>> {
    require_cve_read(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let feeds = repo
        .list_feed_sources()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list CVE feeds: {}", e)))?;
    Ok(Json(feeds))
}

async fn create_cve_feed(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateCveFeedSourceRequest>,
) -> AppResult<(StatusCode, Json<CveFeedSource>)> {
    require_cve_write(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let feed = repo
        .create_feed_source(
            &payload.name,
            &payload.feed_url,
            payload.feed_type,
            payload.enabled,
            payload.sync_interval_secs,
        )
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create CVE feed: {}", e)))?;
    Ok((StatusCode::CREATED, Json(feed)))
}

async fn update_cve_feed(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateCveFeedSourceRequest>,
) -> AppResult<Json<CveFeedSource>> {
    require_cve_write(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let feed = repo
        .update_feed_source(
            &id,
            payload.name.as_deref(),
            payload.feed_url.as_deref(),
            payload.enabled,
            payload.sync_interval_secs,
        )
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update CVE feed: {}", e)))?
        .ok_or_else(|| AppError::not_found(format!("CVE feed '{}' not found", id)))?;
    Ok(Json(feed))
}

async fn delete_cve_feed(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    require_cve_write(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let deleted = repo
        .delete_feed_source(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete CVE feed: {}", e)))?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found(format!("CVE feed '{}' not found", id)))
    }
}

async fn trigger_feed_sync(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<FeedSyncResult>> {
    require_cve_write(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let feed = repo
        .get_feed_source(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get CVE feed: {}", e)))?
        .ok_or_else(|| AppError::not_found(format!("CVE feed '{}' not found", id)))?;

    let service = CveFeedService::new(CveRepository::new(state.db.clone()));
    let result = service
        .sync_feed(&feed)
        .await
        .map_err(|e| AppError::Internal(format!("Feed sync failed: {}", e)))?;
    Ok(Json(result))
}

async fn trigger_match_refresh(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    require_cve_write(&auth_user)?;
    let repo = CveRepository::new(state.db.clone());
    let count = repo
        .refresh_host_vulnerability_matches()
        .await
        .map_err(|e| AppError::Internal(format!("Match refresh failed: {}", e)))?;
    Ok(Json(serde_json::json!({
        "matches_refreshed": count
    })))
}
