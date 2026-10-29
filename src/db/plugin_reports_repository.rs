//! Persistence for plugin reports (M25, T90): report annotations, plugin
//! datasets, the `report.received` watcher cursor and the retention of
//! plugin report executions.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use super::plugin_events_repository::fmt;
use crate::models::AnnotationRecord;

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Most equality filters a dataset query takes (the SQL is static).
pub const MAX_DATASET_FILTERS: usize = 4;

pub struct PluginReportsRepository<'a> {
    pool: &'a SqlitePool,
}

fn annotation(r: &sqlx::sqlite::SqliteRow) -> AnnotationRecord {
    let value: String = r.get("value_json");
    AnnotationRecord {
        report_hash: r.get("report_hash"),
        plugin_id: r.get("plugin_id"),
        key: r.get("key"),
        value: serde_json::from_str(&value).unwrap_or(Value::Null),
        severity: r.get("severity"),
        created_at: ts(&r.get::<String, _>("created_at")),
    }
}

impl<'a> PluginReportsRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ---- annotations ------------------------------------------------------

    pub async fn put_annotation(&self, a: &AnnotationRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO report_annotations (report_hash, plugin_id, key, value_json, severity, \
             created_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(report_hash, plugin_id, key) \
             DO UPDATE SET value_json = excluded.value_json, severity = excluded.severity, \
             created_at = excluded.created_at",
        )
        .bind(&a.report_hash)
        .bind(&a.plugin_id)
        .bind(&a.key)
        .bind(serde_json::to_string(&a.value)?)
        .bind(&a.severity)
        .bind(fmt(a.created_at))
        .execute(self.pool)
        .await
        .context("store report annotation")?;
        Ok(())
    }

    pub async fn delete_annotation(
        &self,
        report_hash: &str,
        plugin_id: &str,
        key: &str,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM report_annotations WHERE report_hash = ? AND plugin_id = ? AND key = ?",
        )
        .bind(report_hash)
        .bind(plugin_id)
        .bind(key)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn annotations_for(&self, report_hash: &str) -> Result<Vec<AnnotationRecord>> {
        let rows = sqlx::query(
            "SELECT report_hash, plugin_id, key, value_json, severity, created_at \
             FROM report_annotations WHERE report_hash = ? ORDER BY plugin_id, key",
        )
        .bind(report_hash)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(annotation).collect())
    }

    // ---- datasets ---------------------------------------------------------

    /// Writes rows of one dataset in one transaction; with `replace` the
    /// rows not in this batch are removed first. Refused (nothing written)
    /// when the dataset would hold more than `max_rows` rows.
    pub async fn put_dataset_rows(
        &self,
        plugin_id: &str,
        dataset_id: &str,
        schema_json: &str,
        rows: &[(String, Value)],
        replace: bool,
        max_rows: u64,
    ) -> Result<()> {
        let now = fmt(Utc::now());
        let mut tx = self.pool.begin().await.context("begin dataset write")?;
        if replace {
            sqlx::query("DELETE FROM plugin_dataset_rows WHERE plugin_id = ? AND dataset_id = ?")
                .bind(plugin_id)
                .bind(dataset_id)
                .execute(&mut *tx)
                .await?;
        }
        for (key, row) in rows {
            sqlx::query(
                "INSERT INTO plugin_dataset_rows (plugin_id, dataset_id, row_key, row_json, \
                 updated_at) VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT(plugin_id, dataset_id, row_key) DO UPDATE SET \
                 row_json = excluded.row_json, updated_at = excluded.updated_at",
            )
            .bind(plugin_id)
            .bind(dataset_id)
            .bind(key)
            .bind(serde_json::to_string(row)?)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        let held: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_dataset_rows WHERE plugin_id = ? AND dataset_id = ?",
        )
        .bind(plugin_id)
        .bind(dataset_id)
        .fetch_one(&mut *tx)
        .await?;
        if held.max(0) as u64 > max_rows {
            tx.rollback().await?;
            anyhow::bail!("a dataset holds at most {} rows", max_rows);
        }
        sqlx::query(
            "INSERT INTO plugin_datasets (plugin_id, dataset_id, schema_json, updated_at) \
             VALUES (?, ?, ?, ?) ON CONFLICT(plugin_id, dataset_id) DO UPDATE SET \
             schema_json = excluded.schema_json, updated_at = excluded.updated_at",
        )
        .bind(plugin_id)
        .bind(dataset_id)
        .bind(schema_json)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_dataset_rows(
        &self,
        plugin_id: &str,
        dataset_id: &str,
        keys: &[String],
    ) -> Result<()> {
        for key in keys {
            sqlx::query(
                "DELETE FROM plugin_dataset_rows WHERE plugin_id = ? AND dataset_id = ? \
                 AND row_key = ?",
            )
            .bind(plugin_id)
            .bind(dataset_id)
            .bind(key)
            .execute(self.pool)
            .await?;
        }
        Ok(())
    }

    /// Rows held and the time of the last write.
    pub async fn dataset_stats(
        &self,
        plugin_id: &str,
        dataset_id: &str,
    ) -> Result<(u64, Option<DateTime<Utc>>)> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_dataset_rows WHERE plugin_id = ? AND dataset_id = ?",
        )
        .bind(plugin_id)
        .bind(dataset_id)
        .fetch_one(self.pool)
        .await?;
        let updated: Option<String> = sqlx::query_scalar(
            "SELECT updated_at FROM plugin_datasets WHERE plugin_id = ? AND dataset_id = ?",
        )
        .bind(plugin_id)
        .bind(dataset_id)
        .fetch_optional(self.pool)
        .await?;
        Ok((n.max(0) as u64, updated.as_deref().map(ts)))
    }

    /// Rows matching up to [`MAX_DATASET_FILTERS`] equality filters on
    /// top-level columns, ordered by row key, with the total match count.
    pub async fn query_dataset(
        &self,
        plugin_id: &str,
        dataset_id: &str,
        filters: &[(String, Value)],
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<Value>, u64)> {
        anyhow::ensure!(
            filters.len() <= MAX_DATASET_FILTERS,
            "at most {} filters",
            MAX_DATASET_FILTERS
        );
        let mut paths: Vec<Option<String>> = vec![None; MAX_DATASET_FILTERS];
        let mut values: Vec<Option<String>> = vec![None; MAX_DATASET_FILTERS];
        for (i, (col, v)) in filters.iter().enumerate() {
            paths[i] = Some(format!("$.\"{}\"", col.replace('"', "")));
            // Compared through json_extract on both sides, so types match.
            values[i] = Some(v.to_string());
        }
        macro_rules! bind_where {
            ($q:expr) => {{
                let mut q = $q.bind(plugin_id).bind(dataset_id);
                for i in 0..MAX_DATASET_FILTERS {
                    q = q
                        .bind(paths[i].clone())
                        .bind(paths[i].clone())
                        .bind(values[i].clone());
                }
                q
            }};
        }
        let total: i64 = bind_where!(sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_dataset_rows WHERE plugin_id = ? AND dataset_id = ? \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$')) \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$')) \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$')) \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$'))"
        ))
        .fetch_one(self.pool)
        .await?;
        let rows: Vec<String> = bind_where!(sqlx::query_scalar(
            "SELECT row_json FROM plugin_dataset_rows WHERE plugin_id = ? AND dataset_id = ? \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$')) \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$')) \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$')) \
             AND (? IS NULL OR json_extract(row_json, ?) = json_extract(?, '$')) \
             ORDER BY row_key LIMIT ? OFFSET ?"
        ))
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(self.pool)
        .await?;
        Ok((
            rows.iter()
                .map(|r| serde_json::from_str(r).unwrap_or(Value::Null))
                .collect(),
            total.max(0) as u64,
        ))
    }

    // ---- report.received watcher -----------------------------------------

    pub async fn watch_cursor(&self) -> Result<Option<DateTime<Utc>>> {
        let v: Option<String> =
            sqlx::query_scalar("SELECT last_receive_time FROM plugin_report_watch WHERE id = 1")
                .fetch_optional(self.pool)
                .await?;
        Ok(v.as_deref().map(ts))
    }

    pub async fn set_watch_cursor(&self, at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_report_watch (id, last_receive_time) VALUES (1, ?) \
             ON CONFLICT(id) DO UPDATE SET last_receive_time = excluded.last_receive_time",
        )
        .bind(fmt(at))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    // ---- retention and uninstall -----------------------------------------

    /// Deletes executions of plugin report types started more than `days`
    /// days ago; core report types are never touched.
    pub async fn purge_plugin_executions(&self, days: u32) -> Result<u64> {
        let r = sqlx::query(
            "DELETE FROM report_executions WHERE started_at < datetime('now', ?) \
             AND report_id IN (SELECT id FROM saved_reports WHERE report_type LIKE 'plugin:%')",
        )
        .bind(format!("-{} days", days))
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    /// `purge` of an uninstalled plugin: its annotations and datasets.
    pub async fn delete_plugin(&self, plugin_id: &str) -> Result<()> {
        for sql in [
            "DELETE FROM report_annotations WHERE plugin_id = ?",
            "DELETE FROM plugin_dataset_rows WHERE plugin_id = ?",
            "DELETE FROM plugin_datasets WHERE plugin_id = ?",
        ] {
            sqlx::query(sql).bind(plugin_id).execute(self.pool).await?;
        }
        Ok(())
    }
}
