//! Repository for the `report_daily_summary` table.
//!
//! Pre-aggregated per-day Puppet report counts. A background scheduler
//! refreshes these rows hourly so the dashboard's "Weekly Activity Trend"
//! chart doesn't have to scan every report on each page load.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

use crate::db::DbPool;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ReportDailySummary {
    pub date: String,
    pub changed: i64,
    pub unchanged: i64,
    pub failed: i64,
    pub noop: i64,
    pub total: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ReportHourlySummary {
    pub hour: String,
    pub changed: i64,
    pub unchanged: i64,
    pub failed: i64,
    pub noop: i64,
    pub total: i64,
    pub updated_at: String,
}

/// One cell of the activity heatmap.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityHeatmapCell {
    /// 0 = Sunday, 6 = Saturday (UTC).
    pub day_of_week: i64,
    /// 0..=23 in UTC.
    pub hour_of_day: i64,
    /// Sum of completed reports in that (dow, hour) bucket over the window.
    pub total: i64,
    /// Sum of `changed`-status reports — the activity signal the chart maps
    /// to color intensity. Resource-level change counts aren't tracked in
    /// the summary, so we use changed-report counts as the proxy.
    pub changed: i64,
}

pub struct ReportSummaryRepository {
    pool: DbPool,
}

impl ReportSummaryRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Upsert a single day's counts. `date` is the UTC day in `YYYY-MM-DD`.
    pub async fn upsert(
        &self,
        date: &str,
        changed: i64,
        unchanged: i64,
        failed: i64,
        noop: i64,
    ) -> Result<()> {
        let total = changed + unchanged + failed + noop;
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO report_daily_summary (date, changed, unchanged, failed, noop, total, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(date) DO UPDATE SET
                changed = excluded.changed,
                unchanged = excluded.unchanged,
                failed = excluded.failed,
                noop = excluded.noop,
                total = excluded.total,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(date)
        .bind(changed)
        .bind(unchanged)
        .bind(failed)
        .bind(noop)
        .bind(total)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to upsert report_daily_summary row")?;
        Ok(())
    }

    /// Upsert a single hour's counts. `hour` is the UTC bucket start,
    /// formatted as RFC3339 (`YYYY-MM-DDTHH:00:00Z`).
    pub async fn upsert_hourly(
        &self,
        hour: &str,
        changed: i64,
        unchanged: i64,
        failed: i64,
        noop: i64,
    ) -> Result<()> {
        let total = changed + unchanged + failed + noop;
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO report_hourly_summary (hour, changed, unchanged, failed, noop, total, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(hour) DO UPDATE SET
                changed = excluded.changed,
                unchanged = excluded.unchanged,
                failed = excluded.failed,
                noop = excluded.noop,
                total = excluded.total,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(hour)
        .bind(changed)
        .bind(unchanged)
        .bind(failed)
        .bind(noop)
        .bind(total)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to upsert report_hourly_summary row")?;
        Ok(())
    }

    /// Fetch hourly summaries with bucket hour in `[start_hour, end_hour]`
    /// inclusive (both RFC3339 UTC strings). Returned ordered ascending.
    pub async fn range_hourly(
        &self,
        start_hour: &str,
        end_hour: &str,
    ) -> Result<Vec<ReportHourlySummary>> {
        let rows = sqlx::query_as::<_, ReportHourlySummary>(
            r#"
            SELECT hour, changed, unchanged, failed, noop, total, updated_at
            FROM report_hourly_summary
            WHERE hour >= ?1 AND hour <= ?2
            ORDER BY hour ASC
            "#,
        )
        .bind(start_hour)
        .bind(end_hour)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query report_hourly_summary range")?;
        Ok(rows)
    }

    /// Aggregate the hourly summary into a 7x24 day-of-week × hour-of-day
    /// grid covering the window `[start_hour, end_hour]`. Cells with no
    /// matching rows are simply absent from the result — the caller is
    /// expected to densify the grid.
    pub async fn heatmap_grid(
        &self,
        start_hour: &str,
        end_hour: &str,
    ) -> Result<Vec<ActivityHeatmapCell>> {
        // `hour` is stored as ISO-8601 UTC with literal `T` and `Z` (e.g.
        // `2026-05-21T13:00:00Z`). SQLite's strftime handles that format
        // directly: `%w` → day of week 0..6 (Sunday=0), `%H` → hour 00..23.
        let rows = sqlx::query_as::<_, (i64, i64, i64, i64)>(
            r#"
            SELECT
                CAST(strftime('%w', hour) AS INTEGER) AS day_of_week,
                CAST(strftime('%H', hour) AS INTEGER) AS hour_of_day,
                COALESCE(SUM(total), 0) AS total,
                COALESCE(SUM(changed), 0) AS changed
            FROM report_hourly_summary
            WHERE hour >= ?1 AND hour <= ?2
            GROUP BY day_of_week, hour_of_day
            ORDER BY day_of_week, hour_of_day
            "#,
        )
        .bind(start_hour)
        .bind(end_hour)
        .fetch_all(&self.pool)
        .await
        .context("Failed to aggregate activity heatmap")?;

        Ok(rows
            .into_iter()
            .map(|(dow, hod, total, changed)| ActivityHeatmapCell {
                day_of_week: dow,
                hour_of_day: hod,
                total,
                changed,
            })
            .collect())
    }

    /// Fetch summaries for dates in `[start_date, end_date]` inclusive,
    /// ordered ascending.
    pub async fn range(&self, start_date: &str, end_date: &str) -> Result<Vec<ReportDailySummary>> {
        let rows = sqlx::query_as::<_, ReportDailySummary>(
            r#"
            SELECT date, changed, unchanged, failed, noop, total, updated_at
            FROM report_daily_summary
            WHERE date >= ?1 AND date <= ?2
            ORDER BY date ASC
            "#,
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query report_daily_summary range")?;
        Ok(rows)
    }
}
