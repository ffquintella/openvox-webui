//! Persistence for plugin usage rollups and circuit breaker state (M22).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use super::plugin_events_repository::fmt;
use crate::models::{CircuitRecord, CircuitState, UsageRow, LATENCY_BUCKET_COUNT};

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

pub struct PluginGovernanceRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> PluginGovernanceRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Adds the rows to the stored hourly rollups.
    pub async fn add_usage(&self, rows: &[UsageRow]) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin usage flush")?;
        for r in rows {
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT latency_buckets FROM plugin_usage_hourly \
                 WHERE plugin_id = ? AND version = ? AND kind = ? AND hour = ?",
            )
            .bind(&r.plugin_id)
            .bind(&r.version)
            .bind(&r.kind)
            .bind(&r.hour)
            .fetch_optional(&mut *tx)
            .await?;
            let mut buckets = vec![0u64; LATENCY_BUCKET_COUNT];
            for (i, n) in r
                .latency_buckets
                .iter()
                .enumerate()
                .take(LATENCY_BUCKET_COUNT)
            {
                buckets[i] = *n;
            }
            if let Some(old) = existing {
                let old: Vec<u64> = serde_json::from_str(&old).unwrap_or_default();
                for (i, n) in old.iter().enumerate().take(LATENCY_BUCKET_COUNT) {
                    buckets[i] += n;
                }
            }
            sqlx::query(
                "INSERT INTO plugin_usage_hourly (plugin_id, version, kind, hour, calls, errors, \
                 traps, timeouts, quota_rejections, circuit_rejections, total_ms, max_ms, \
                 latency_buckets) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(plugin_id, version, kind, hour) DO UPDATE SET \
                 calls = calls + excluded.calls, errors = errors + excluded.errors, \
                 traps = traps + excluded.traps, timeouts = timeouts + excluded.timeouts, \
                 quota_rejections = quota_rejections + excluded.quota_rejections, \
                 circuit_rejections = circuit_rejections + excluded.circuit_rejections, \
                 total_ms = total_ms + excluded.total_ms, \
                 max_ms = MAX(max_ms, excluded.max_ms), \
                 latency_buckets = excluded.latency_buckets",
            )
            .bind(&r.plugin_id)
            .bind(&r.version)
            .bind(&r.kind)
            .bind(&r.hour)
            .bind(r.calls as i64)
            .bind(r.errors as i64)
            .bind(r.traps as i64)
            .bind(r.timeouts as i64)
            .bind(r.quota_rejections as i64)
            .bind(r.circuit_rejections as i64)
            .bind(r.total_ms as i64)
            .bind(r.max_ms as i64)
            .bind(serde_json::to_string(&buckets)?)
            .execute(&mut *tx)
            .await
            .context("store usage row")?;
        }
        tx.commit().await.context("commit usage flush")?;
        Ok(())
    }

    /// Rows since `since` (inclusive); for one plugin or, with `None`, all.
    pub async fn usage_since(
        &self,
        plugin_id: Option<&str>,
        since: DateTime<Utc>,
    ) -> Result<Vec<UsageRow>> {
        let rows = sqlx::query(
            "SELECT plugin_id, version, kind, hour, calls, errors, traps, timeouts, \
             quota_rejections, circuit_rejections, total_ms, max_ms, latency_buckets \
             FROM plugin_usage_hourly WHERE hour >= ? AND (? IS NULL OR plugin_id = ?) \
             ORDER BY hour",
        )
        .bind(fmt(since))
        .bind(plugin_id)
        .bind(plugin_id)
        .fetch_all(self.pool)
        .await
        .context("read usage")?;
        Ok(rows
            .into_iter()
            .map(|r| UsageRow {
                plugin_id: r.get("plugin_id"),
                version: r.get("version"),
                kind: r.get("kind"),
                hour: r.get("hour"),
                calls: r.get::<i64, _>("calls") as u64,
                errors: r.get::<i64, _>("errors") as u64,
                traps: r.get::<i64, _>("traps") as u64,
                timeouts: r.get::<i64, _>("timeouts") as u64,
                quota_rejections: r.get::<i64, _>("quota_rejections") as u64,
                circuit_rejections: r.get::<i64, _>("circuit_rejections") as u64,
                total_ms: r.get::<i64, _>("total_ms") as u64,
                max_ms: r.get::<i64, _>("max_ms") as u64,
                latency_buckets: serde_json::from_str(&r.get::<String, _>("latency_buckets"))
                    .unwrap_or_default(),
            })
            .collect())
    }

    pub async fn purge_usage_before(&self, before: DateTime<Utc>) -> Result<u64> {
        let r = sqlx::query("DELETE FROM plugin_usage_hourly WHERE hour < ?")
            .bind(fmt(before))
            .execute(self.pool)
            .await
            .context("purge usage")?;
        Ok(r.rows_affected())
    }

    pub async fn delete_plugin_data(&self, plugin_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM plugin_usage_hourly WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await
            .context("delete usage")?;
        self.delete_circuit(plugin_id).await
    }

    pub async fn upsert_circuit(&self, c: &CircuitRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_circuit (plugin_id, state, trips, opened_at, retry_at, last_error) \
             VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(plugin_id) DO UPDATE SET state = excluded.state, \
             trips = excluded.trips, opened_at = excluded.opened_at, retry_at = excluded.retry_at, \
             last_error = excluded.last_error",
        )
        .bind(&c.plugin_id)
        .bind(c.state.as_str())
        .bind(c.trips as i64)
        .bind(fmt(c.opened_at))
        .bind(fmt(c.retry_at))
        .bind(&c.last_error)
        .execute(self.pool)
        .await
        .context("store circuit")?;
        Ok(())
    }

    pub async fn delete_circuit(&self, plugin_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM plugin_circuit WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await
            .context("delete circuit")?;
        Ok(())
    }

    pub async fn list_circuits(&self) -> Result<Vec<CircuitRecord>> {
        let rows = sqlx::query(
            "SELECT plugin_id, state, trips, opened_at, retry_at, last_error FROM plugin_circuit",
        )
        .fetch_all(self.pool)
        .await
        .context("list circuits")?;
        Ok(rows
            .into_iter()
            .map(|r| CircuitRecord {
                plugin_id: r.get("plugin_id"),
                state: match r.get::<String, _>("state").as_str() {
                    "half_open" => CircuitState::HalfOpen,
                    _ => CircuitState::Open,
                },
                trips: r.get::<i64, _>("trips") as u32,
                opened_at: ts(&r.get::<String, _>("opened_at")),
                retry_at: ts(&r.get::<String, _>("retry_at")),
                last_error: r.get("last_error"),
            })
            .collect())
    }
}
