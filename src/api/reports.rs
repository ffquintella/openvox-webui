//! Reports API endpoints
//!
//! Provides endpoints for querying reports from PuppetDB.

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{
    db::{ActivityHeatmapCell, ReportDailySummary, ReportHourlySummary, ReportSummaryRepository},
    models::{Report, ResourceEvent},
    services::puppetdb::{QueryBuilder, QueryParams},
    utils::error::{AppError, AppResult},
    AppState,
};

/// Create routes for report endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(query_reports))
        .route("/daily-summary", get(get_daily_summary))
        .route("/hourly-summary", get(get_hourly_summary))
        .route("/activity-heatmap", get(get_activity_heatmap))
        .route("/{hash}", get(get_report))
        .route("/{hash}/events", get(get_report_events))
}

/// Query parameters for the daily summary endpoint.
#[derive(Debug, Deserialize)]
pub struct DailySummaryQuery {
    /// Number of days back from today (UTC) to return. Defaults to 7,
    /// capped at 90 to bound response size.
    pub days: Option<u32>,
}

/// GET /api/v1/reports/daily-summary
///
/// Returns per-day report counts (changed/unchanged/failed/noop/total) for
/// the last `days` UTC days, oldest first. Backed by the
/// `report_daily_summary` table populated hourly by the summary scheduler.
/// Days with no recorded summary yet are returned as zero rows so the
/// frontend can render a continuous time series without gap handling.
async fn get_daily_summary(
    State(state): State<AppState>,
    Query(query): Query<DailySummaryQuery>,
) -> AppResult<Json<Vec<ReportDailySummary>>> {
    let days = query.days.unwrap_or(7).clamp(1, 90);
    let today = chrono::Utc::now().date_naive();
    let start = today - chrono::Duration::days((days - 1) as i64);

    let repo = ReportSummaryRepository::new(state.db.clone());
    let stored = repo
        .range(&start.to_string(), &today.to_string())
        .await
        .map_err(|e| AppError::Internal(format!("Failed to load daily summary: {}", e)))?;

    // Fill missing days with zeroes so callers get a dense series.
    let by_date: std::collections::HashMap<String, ReportDailySummary> =
        stored.into_iter().map(|r| (r.date.clone(), r)).collect();
    let mut out = Vec::with_capacity(days as usize);
    for i in 0..days as i64 {
        let d = start + chrono::Duration::days(i);
        let key = d.to_string();
        out.push(by_date.get(&key).cloned().unwrap_or(ReportDailySummary {
            date: key,
            changed: 0,
            unchanged: 0,
            failed: 0,
            noop: 0,
            total: 0,
            updated_at: String::new(),
        }));
    }
    Ok(Json(out))
}

/// Query parameters for reports query
#[derive(Debug, Deserialize)]
pub struct ReportsQuery {
    /// Filter by certname
    pub certname: Option<String>,
    /// Filter by status (changed, unchanged, failed)
    pub status: Option<String>,
    /// Filter by environment
    pub environment: Option<String>,
    /// Only show reports newer than this timestamp (ISO 8601)
    pub since: Option<String>,
    /// Only show reports older than this timestamp (ISO 8601)
    pub until: Option<String>,
    /// Maximum number of results
    pub limit: Option<u32>,
    /// Number of results to skip
    pub offset: Option<u32>,
    /// Field to order by (default: end_time)
    pub order_by: Option<String>,
    /// Order direction (asc/desc, default: desc)
    pub order_dir: Option<String>,
}

/// Query reports
///
/// GET /api/v1/reports
///
/// Query parameters:
/// - `certname`: Filter by certname
/// - `status`: Filter by status (changed, unchanged, failed)
/// - `environment`: Filter by environment
/// - `since`: Only show reports newer than this timestamp (ISO 8601)
/// - `until`: Only show reports older than this timestamp (ISO 8601)
/// - `limit`: Maximum number of results (default: 50)
/// - `offset`: Number of results to skip
/// - `order_by`: Field to order by (default: end_time)
/// - `order_dir`: Order direction (asc/desc, default: desc)
async fn query_reports(
    State(state): State<AppState>,
    Query(query): Query<ReportsQuery>,
) -> AppResult<Json<Vec<Report>>> {
    // If PuppetDB is not configured, return empty list (stub behavior expected by tests)
    let Some(puppetdb) = state.puppetdb.as_ref() else {
        return Ok(Json(vec![]));
    };

    // Build query
    let mut qb = QueryBuilder::new();

    if let Some(ref certname) = query.certname {
        qb = qb.equals("certname", certname);
    }

    if let Some(ref status) = query.status {
        qb = qb.equals("status", status);
    }

    if let Some(ref env) = query.environment {
        qb = qb.equals("environment", env);
    }

    if let Some(ref since) = query.since {
        qb = qb.greater_than("end_time", since);
    }

    if let Some(ref until) = query.until {
        qb = qb.less_than("end_time", until);
    }

    // Build pagination params
    let mut params = QueryParams::new();
    if let Some(limit) = query.limit {
        params = params.limit(limit);
    } else {
        params = params.limit(50); // Default limit
    }
    if let Some(offset) = query.offset {
        params = params.offset(offset);
    }

    // Add ordering (default: newest first)
    let order_field = query.order_by.as_deref().unwrap_or("end_time");
    let ascending = query.order_dir.as_deref() == Some("asc");
    params = params.order_by(order_field, ascending).include_total();

    // Execute query
    let reports = puppetdb
        .query_reports_advanced(&qb, params)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query reports: {}", e)))?;

    Ok(Json(reports))
}

#[derive(Debug, Deserialize)]
pub struct HourlySummaryQuery {
    /// Number of hours back from now (inclusive of the current hour) to
    /// return. Defaults to 24, capped at 30 days × 24 = 720 to bound the
    /// payload.
    pub hours: Option<u32>,
}

/// GET /api/v1/reports/hourly-summary
///
/// Returns one row per UTC hour over the requested window, oldest first.
/// Missing hours are filled with zeros so the frontend can render a
/// continuous time series without gap handling.
async fn get_hourly_summary(
    State(state): State<AppState>,
    Query(query): Query<HourlySummaryQuery>,
) -> AppResult<Json<Vec<ReportHourlySummary>>> {
    let hours = query.hours.unwrap_or(24).clamp(1, 720);
    let now = chrono::Utc::now();
    let end_hour = floor_to_hour(now);
    let start_hour = end_hour - chrono::Duration::hours((hours - 1) as i64);
    let start_iso = format_hour(start_hour);
    let end_iso = format_hour(end_hour);

    let repo = ReportSummaryRepository::new(state.db.clone());
    let stored = repo
        .range_hourly(&start_iso, &end_iso)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to load hourly summary: {}", e)))?;

    let by_hour: std::collections::HashMap<String, ReportHourlySummary> =
        stored.into_iter().map(|r| (r.hour.clone(), r)).collect();
    let mut out = Vec::with_capacity(hours as usize);
    for i in 0..hours as i64 {
        let h = start_hour + chrono::Duration::hours(i);
        let key = format_hour(h);
        out.push(by_hour.get(&key).cloned().unwrap_or(ReportHourlySummary {
            hour: key,
            changed: 0,
            unchanged: 0,
            failed: 0,
            noop: 0,
            total: 0,
            updated_at: String::new(),
        }));
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct ActivityHeatmapQuery {
    /// How many days back from today to fold into the heatmap. Defaults
    /// to 30, capped at the scheduler's lookback window.
    pub days: Option<u32>,
}

/// GET /api/v1/reports/activity-heatmap
///
/// Returns a dense 7×24 (UTC day-of-week × hour-of-day) grid summarising
/// report activity across the requested window. Empty cells are still
/// emitted with zero counts.
async fn get_activity_heatmap(
    State(state): State<AppState>,
    Query(query): Query<ActivityHeatmapQuery>,
) -> AppResult<Json<Vec<ActivityHeatmapCell>>> {
    let days = query.days.unwrap_or(30).clamp(1, 31);
    let now = chrono::Utc::now();
    let end_hour = floor_to_hour(now);
    let start_hour = end_hour - chrono::Duration::days(days as i64);
    let start_iso = format_hour(start_hour);
    let end_iso = format_hour(end_hour);

    let repo = ReportSummaryRepository::new(state.db.clone());
    let cells = repo
        .heatmap_grid(&start_iso, &end_iso)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to load activity heatmap: {}", e)))?;

    // Densify the grid so every (dow, hour) is present.
    let mut by_cell: std::collections::HashMap<(i64, i64), ActivityHeatmapCell> = cells
        .into_iter()
        .map(|c| ((c.day_of_week, c.hour_of_day), c))
        .collect();
    let mut out = Vec::with_capacity(7 * 24);
    for dow in 0..7i64 {
        for hod in 0..24i64 {
            out.push(by_cell.remove(&(dow, hod)).unwrap_or(ActivityHeatmapCell {
                day_of_week: dow,
                hour_of_day: hod,
                total: 0,
                changed: 0,
            }));
        }
    }
    Ok(Json(out))
}

fn floor_to_hour(ts: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    use chrono::Timelike;
    ts.with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(ts)
}

fn format_hour(ts: chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Datelike;
    use chrono::Timelike;
    format!(
        "{:04}-{:02}-{:02}T{:02}:00:00Z",
        ts.year(),
        ts.month(),
        ts.day(),
        ts.hour()
    )
}

/// Get a specific report by hash
///
/// GET /api/v1/reports/:hash
async fn get_report(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> AppResult<Json<Report>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    let report = puppetdb
        .get_report(&hash)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch report: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Report '{}' not found", hash)))?;

    Ok(Json(report))
}

/// Query parameters for report events
#[derive(Debug, Deserialize)]
pub struct ReportEventsQuery {
    /// Filter by status (success, failure, noop, skipped)
    pub status: Option<String>,
    /// Filter by resource type
    #[serde(rename = "type")]
    pub resource_type: Option<String>,
}

/// Get events from a specific report
///
/// GET /api/v1/reports/:hash/events
///
/// Query parameters:
/// - `status`: Filter by status (success, failure, noop, skipped)
/// - `type`: Filter by resource type
async fn get_report_events(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(query): Query<ReportEventsQuery>,
) -> AppResult<Json<Vec<ResourceEvent>>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // Build query for events - filter by report hash
    let mut qb = QueryBuilder::new();
    qb = qb.equals("report", &hash);

    if let Some(ref status) = query.status {
        qb = qb.equals("status", status);
    }

    if let Some(ref rtype) = query.resource_type {
        qb = qb.equals("resource_type", rtype);
    }

    let events = puppetdb
        .query_events(&qb)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch events: {}", e)))?;

    Ok(Json(events))
}
