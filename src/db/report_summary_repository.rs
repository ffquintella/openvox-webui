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

    /// Fetch summaries for dates in `[start_date, end_date]` inclusive,
    /// ordered ascending.
    pub async fn range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<ReportDailySummary>> {
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
