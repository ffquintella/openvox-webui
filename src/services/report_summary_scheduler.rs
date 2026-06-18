//! Hourly scheduler that refreshes the report summary tables.
//!
//! One PQL fetch per cycle pulls `[end_time, status]` projections for every
//! report in the rolling 31-day window, then we bucket them into
//! `report_hourly_summary` (UTC hour granularity) and
//! `report_daily_summary` (UTC day granularity). The Dashboard's weekly
//! trend, the Analytics time-series chart, and the activity heatmap all
//! read from those tables instead of hitting PuppetDB.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, Timelike, Utc};
use serde::Deserialize;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::db::{DbPool, ReportSummaryRepository};
use crate::services::puppetdb::PuppetDbClient;

/// How many days of history to keep up to date. The Analytics chart goes
/// back 30 days; we keep a slightly wider window so late-arriving reports
/// for the oldest day still land in the summary.
const LOOKBACK_DAYS: i64 = 31;

/// How often the loop fires.
const REFRESH_INTERVAL_SECS: u64 = 60 * 60;

#[derive(Debug, Clone)]
pub struct ReportSummarySchedulerState {
    running: Arc<RwLock<bool>>,
}

impl ReportSummarySchedulerState {
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Report summary scheduler stop requested");
    }
}

#[derive(Debug, Deserialize)]
struct ReportRow {
    end_time: Option<String>,
    status: Option<String>,
}

pub fn start_report_summary_scheduler(
    pool: DbPool,
    puppetdb: Arc<PuppetDbClient>,
) -> ReportSummarySchedulerState {
    let running = Arc::new(RwLock::new(true));
    let state = ReportSummarySchedulerState {
        running: running.clone(),
    };

    tokio::spawn(async move {
        // Run once at startup so a fresh server has data immediately rather
        // than waiting an hour for the first chart to populate.
        if let Err(e) = refresh_summary(&pool, &puppetdb).await {
            warn!("Initial report summary refresh failed: {}", e);
        }

        let mut timer = interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
        timer.tick().await; // First tick fires immediately; skip it.

        loop {
            timer.tick().await;
            if !*running.read().await {
                info!("Report summary scheduler stopping");
                break;
            }
            if let Err(e) = refresh_summary(&pool, &puppetdb).await {
                error!("Report summary refresh failed: {}", e);
            }
        }
    });

    info!(
        "Report summary scheduler started (every {}s, {} day window)",
        REFRESH_INTERVAL_SECS, LOOKBACK_DAYS
    );
    state
}

async fn refresh_summary(pool: &DbPool, puppetdb: &PuppetDbClient) -> anyhow::Result<()> {
    let repo = ReportSummaryRepository::new(pool.clone());
    let now = Utc::now();
    let window_start = now - ChronoDuration::days(LOOKBACK_DAYS);
    let window_start_iso = window_start.to_rfc3339();

    // One PQL fetch per cycle. PuppetDB projects just the two fields we
    // need, so the payload is small even for thousands of reports.
    let pql = format!(
        r#"reports[end_time, status] {{ end_time >= "{}" }}"#,
        window_start_iso
    );
    let rows: Vec<ReportRow> = puppetdb.query(&pql).await?;
    debug!(
        "Report summary refresh: fetched {} report rows over {} days",
        rows.len(),
        LOOKBACK_DAYS
    );

    // Bucket each report into the (hour, status) and (day, status) it
    // landed in. We keep `unknown`-style statuses out of the counts —
    // dashboards only stack the four known statuses.
    let mut hourly: HashMap<String, [i64; 4]> = HashMap::new();
    let mut daily: HashMap<NaiveDate, [i64; 4]> = HashMap::new();

    for row in &rows {
        let (end_time, status) = match (&row.end_time, &row.status) {
            (Some(t), Some(s)) => (t, s),
            _ => continue,
        };
        let Some(idx) = status_index(status) else {
            continue;
        };
        let Ok(ts) = DateTime::parse_from_rfc3339(end_time) else {
            continue;
        };
        let ts = ts.with_timezone(&Utc);

        let hour_key = format!(
            "{:04}-{:02}-{:02}T{:02}:00:00Z",
            ts.year(),
            ts.month(),
            ts.day(),
            ts.hour()
        );
        hourly.entry(hour_key).or_insert([0; 4])[idx] += 1;

        let day_key = ts.date_naive();
        daily.entry(day_key).or_insert([0; 4])[idx] += 1;
    }

    // Touch every hour and day in the window even if it has no reports, so
    // the API responses are dense and old non-zero buckets get cleared if
    // the underlying reports are GC'd by PuppetDB.
    let mut day = (now - ChronoDuration::days(LOOKBACK_DAYS - 1)).date_naive();
    let today = now.date_naive();
    while day <= today {
        daily.entry(day).or_insert([0; 4]);
        day += ChronoDuration::days(1);
    }
    let mut hour_cursor = now - ChronoDuration::hours(24 * LOOKBACK_DAYS);
    let hour_cursor_floor = hour_cursor
        .with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(hour_cursor);
    hour_cursor = hour_cursor_floor;
    while hour_cursor <= now {
        let key = format!(
            "{:04}-{:02}-{:02}T{:02}:00:00Z",
            hour_cursor.year(),
            hour_cursor.month(),
            hour_cursor.day(),
            hour_cursor.hour()
        );
        hourly.entry(key).or_insert([0; 4]);
        hour_cursor += ChronoDuration::hours(1);
    }

    for (hour, [c, u, f, n]) in &hourly {
        if let Err(e) = repo.upsert_hourly(hour, *c, *u, *f, *n).await {
            warn!("Failed to upsert hourly summary for {}: {}", hour, e);
        }
    }
    for (date, [c, u, f, n]) in &daily {
        if let Err(e) = repo.upsert(&date.to_string(), *c, *u, *f, *n).await {
            warn!("Failed to upsert daily summary for {}: {}", date, e);
        }
    }

    debug!(
        "Report summary refresh complete: {} hour bucket(s), {} day bucket(s)",
        hourly.len(),
        daily.len()
    );
    Ok(())
}

fn status_index(status: &str) -> Option<usize> {
    match status {
        "changed" => Some(0),
        "unchanged" => Some(1),
        "failed" => Some(2),
        "noop" => Some(3),
        _ => None,
    }
}
