//! Reports API endpoints

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{models::Report, AppState};

/// Create routes for report endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(query_reports))
        .route("/:hash", get(get_report))
        .route("/:hash/events", get(get_report_events))
}

#[derive(Deserialize)]
pub struct ReportsQuery {
    pub certname: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Query reports
async fn query_reports(
    State(_state): State<AppState>,
    Query(_query): Query<ReportsQuery>,
) -> Json<Vec<Report>> {
    // TODO: Implement PuppetDB query
    Json(vec![])
}

/// Get a specific report by hash
async fn get_report(
    State(_state): State<AppState>,
    Path(_hash): Path<String>,
) -> Json<Option<Report>> {
    // TODO: Implement PuppetDB query
    Json(None)
}

/// Get events from a specific report
async fn get_report_events(
    State(_state): State<AppState>,
    Path(_hash): Path<String>,
) -> Json<Vec<serde_json::Value>> {
    // TODO: Implement PuppetDB query
    Json(vec![])
}
