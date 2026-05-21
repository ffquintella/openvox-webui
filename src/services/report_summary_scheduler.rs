//! Hourly scheduler that refreshes the `report_daily_summary` table.
//!
//! For each day in a rolling lookback window, queries PuppetDB for the count
//! of reports per status (`changed`, `unchanged`, `failed`, `noop`) and
//! upserts a single row per day. Counting via PQL `[count()]` keeps the
//! request payloads tiny — no report bodies are fetched — so the chart
//! stays cheap to populate regardless of fleet size.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use serde::Deserialize;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::db::{DbPool, ReportSummaryRepository};
use crate::services::puppetdb::PuppetDbClient;

/// How many days of history to keep up to date. The dashboard chart shows
/// the last 7 days; we keep a slightly wider window so late-arriving
/// reports for "yesterday" still land in the summary.
const LOOKBACK_DAYS: i64 = 14;

/// How often the loop fires.
const REFRESH_INTERVAL_SECS: u64 = 60 * 60;

/// Statuses we track separately. Anything outside this set is ignored —
/// PuppetDB occasionally surfaces `unknown` etc., which don't belong in the
/// chart's stacked totals.
const TRACKED_STATUSES: &[&str] = &["changed", "unchanged", "failed", "noop"];

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
struct PqlCountRow {
    count: i64,
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
        // First tick fires immediately; skip it since we just ran.
        timer.tick().await;

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
    let today = Utc::now().date_naive();
    let start = today - ChronoDuration::days(LOOKBACK_DAYS - 1);

    let mut days_updated = 0usize;
    let mut day = start;
    while day <= today {
        match refresh_day(&repo, puppetdb, day).await {
            Ok(()) => days_updated += 1,
            Err(e) => warn!("Failed to refresh report summary for {}: {}", day, e),
        }
        day += ChronoDuration::days(1);
    }
    debug!(
        "Report summary refresh complete ({} day(s) updated)",
        days_updated
    );
    Ok(())
}

async fn refresh_day(
    repo: &ReportSummaryRepository,
    puppetdb: &PuppetDbClient,
    day: NaiveDate,
) -> anyhow::Result<()> {
    let next = day + ChronoDuration::days(1);
    let start_iso = format!("{}T00:00:00.000Z", day);
    let end_iso = format!("{}T00:00:00.000Z", next);

    let mut counts = [0i64; 4]; // changed, unchanged, failed, noop
    for (idx, status) in TRACKED_STATUSES.iter().enumerate() {
        counts[idx] = count_reports(puppetdb, &start_iso, &end_iso, status).await?;
    }

    repo.upsert(
        &day.to_string(),
        counts[0],
        counts[1],
        counts[2],
        counts[3],
    )
    .await?;
    Ok(())
}

async fn count_reports(
    puppetdb: &PuppetDbClient,
    start_iso: &str,
    end_iso: &str,
    status: &str,
) -> anyhow::Result<i64> {
    // PQL: returns [{"count": N}]. `end_time` is the field PuppetDB
    // indexes most reliably across versions for report-completion time.
    let pql = format!(
        r#"reports[count()] {{ end_time >= "{}" and end_time < "{}" and status = "{}" }}"#,
        start_iso, end_iso, status
    );
    let rows: Vec<PqlCountRow> = puppetdb.query(&pql).await?;
    Ok(rows.first().map(|r| r.count).unwrap_or(0))
}
