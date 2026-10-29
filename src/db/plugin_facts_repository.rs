//! Persistence for plugin facts (M25, T89): values, the administrator's
//! opt-in per fact, provider status, the last classification computed with
//! plugin facts and the last export digest per node.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use super::plugin_events_repository::fmt;
use crate::models::{FactProviderStatus, PluginFactRecord, PluginFactSetting};

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn opt_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.as_deref().map(ts)
}

/// One value to store.
pub struct FactWrite<'a> {
    pub name: &'a str,
    pub value: &'a Value,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Last classification computed with plugin facts for a node and scope.
#[derive(Debug, Clone)]
pub struct StoredClassification {
    pub result_json: String,
    pub signature: String,
    pub influenced: bool,
    pub facts_json: String,
    pub updated_at: DateTime<Utc>,
}

pub struct PluginFactsRepository<'a> {
    pool: &'a SqlitePool,
}

fn record(r: &sqlx::sqlite::SqliteRow) -> PluginFactRecord {
    let value: String = r.get("value_json");
    PluginFactRecord {
        plugin_id: r.get("plugin_id"),
        certname: r.get("certname"),
        name: r.get("name"),
        value: serde_json::from_str(&value).unwrap_or(Value::Null),
        computed_at: ts(&r.get::<String, _>("computed_at")),
        expires_at: opt_ts(r.get("expires_at")),
    }
}

macro_rules! select_facts {
    ($rest:literal) => {
        concat!(
            "SELECT plugin_id, certname, name, value_json, computed_at, expires_at FROM plugin_facts ",
            $rest
        )
    };
}

impl<'a> PluginFactsRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ---- values -----------------------------------------------------------

    /// Stores a batch for one node in one transaction; `delete` names are
    /// removed. A write resets the stale notification of the fact.
    pub async fn write_batch(
        &self,
        plugin_id: &str,
        certname: &str,
        writes: &[FactWrite<'_>],
        delete: &[String],
        computed_at: DateTime<Utc>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin fact write")?;
        for w in writes {
            sqlx::query(
                "INSERT INTO plugin_facts (plugin_id, certname, name, value_json, computed_at, \
                 expires_at, stale_notified) VALUES (?, ?, ?, ?, ?, ?, 0) \
                 ON CONFLICT(plugin_id, certname, name) DO UPDATE SET \
                 value_json = excluded.value_json, computed_at = excluded.computed_at, \
                 expires_at = excluded.expires_at, stale_notified = 0",
            )
            .bind(plugin_id)
            .bind(certname)
            .bind(w.name)
            .bind(serde_json::to_string(w.value)?)
            .bind(fmt(computed_at))
            .bind(w.expires_at.map(fmt))
            .execute(&mut *tx)
            .await?;
        }
        for name in delete {
            sqlx::query(
                "DELETE FROM plugin_facts WHERE plugin_id = ? AND certname = ? AND name = ?",
            )
            .bind(plugin_id)
            .bind(certname)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Names the plugin holds for one node.
    pub async fn names_for_node(&self, plugin_id: &str, certname: &str) -> Result<Vec<String>> {
        Ok(
            sqlx::query_scalar(
                "SELECT name FROM plugin_facts WHERE plugin_id = ? AND certname = ?",
            )
            .bind(plugin_id)
            .bind(certname)
            .fetch_all(self.pool)
            .await?,
        )
    }

    /// Every plugin fact stored for one node.
    pub async fn node_facts(&self, certname: &str) -> Result<Vec<PluginFactRecord>> {
        let rows = sqlx::query(select_facts!("WHERE certname = ? ORDER BY plugin_id, name"))
            .bind(certname)
            .fetch_all(self.pool)
            .await?;
        Ok(rows.iter().map(record).collect())
    }

    /// Values of one fact across nodes, newest first.
    pub async fn values_of(
        &self,
        plugin_id: &str,
        name: &str,
        limit: Option<u32>,
    ) -> Result<Vec<PluginFactRecord>> {
        let rows = sqlx::query(select_facts!(
            "WHERE plugin_id = ? AND name = ? \
             ORDER BY computed_at DESC, certname LIMIT ?"
        ))
        .bind(plugin_id)
        .bind(name)
        .bind(limit.map(i64::from).unwrap_or(-1))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(record).collect())
    }

    /// Filtered query used by `GET /facts?source=plugin:<id>`.
    pub async fn query(
        &self,
        plugin_id: &str,
        name: Option<&str>,
        certname: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<PluginFactRecord>> {
        let rows = sqlx::query(select_facts!(
            "WHERE plugin_id = ? \
             AND (? IS NULL OR name = ?) AND (? IS NULL OR certname = ?) \
             ORDER BY certname, name LIMIT ? OFFSET ?"
        ))
        .bind(plugin_id)
        .bind(name)
        .bind(name)
        .bind(certname)
        .bind(certname)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(record).collect())
    }

    pub async fn count_values(&self, plugin_id: &str, name: &str) -> Result<u64> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_facts WHERE plugin_id = ? AND name = ?",
        )
        .bind(plugin_id)
        .bind(name)
        .fetch_one(self.pool)
        .await?;
        Ok(n.max(0) as u64)
    }

    /// Per fact: nodes with a value and nodes whose value is older than `stale_before`.
    pub async fn stats(
        &self,
        plugin_id: &str,
        stale_before: DateTime<Utc>,
    ) -> Result<HashMap<String, (u64, u64)>> {
        let rows = sqlx::query(
            "SELECT name, COUNT(*) AS nodes, \
             SUM(CASE WHEN computed_at < ? THEN 1 ELSE 0 END) AS stale \
             FROM plugin_facts WHERE plugin_id = ? GROUP BY name",
        )
        .bind(fmt(stale_before))
        .bind(plugin_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                let nodes: i64 = r.get("nodes");
                let stale: Option<i64> = r.get("stale");
                (
                    r.get::<String, _>("name"),
                    (nodes.max(0) as u64, stale.unwrap_or(0).max(0) as u64),
                )
            })
            .collect())
    }

    /// Values older than `before` that were not reported stale yet.
    pub async fn newly_stale(
        &self,
        plugin_id: &str,
        before: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<PluginFactRecord>> {
        let rows = sqlx::query(select_facts!(
            "WHERE plugin_id = ? AND stale_notified = 0 \
             AND computed_at < ? ORDER BY certname, name LIMIT ?"
        ))
        .bind(plugin_id)
        .bind(fmt(before))
        .bind(i64::from(limit))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(record).collect())
    }

    pub async fn mark_stale_notified(
        &self,
        plugin_id: &str,
        certname: &str,
        names: &[String],
    ) -> Result<()> {
        for name in names {
            sqlx::query(
                "UPDATE plugin_facts SET stale_notified = 1 \
                 WHERE plugin_id = ? AND certname = ? AND name = ?",
            )
            .bind(plugin_id)
            .bind(certname)
            .bind(name)
            .execute(self.pool)
            .await?;
        }
        Ok(())
    }

    // ---- settings ---------------------------------------------------------

    pub async fn settings(&self, plugin_id: &str) -> Result<HashMap<String, PluginFactSetting>> {
        let rows = sqlx::query(
            "SELECT name, usable_in_rules, exportable, approved_by, approved_at \
             FROM plugin_fact_settings WHERE plugin_id = ?",
        )
        .bind(plugin_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get::<String, _>("name"),
                    PluginFactSetting {
                        usable_in_rules: r.get::<i64, _>("usable_in_rules") != 0,
                        exportable: r.get::<i64, _>("exportable") != 0,
                        approved_by: r.get("approved_by"),
                        approved_at: opt_ts(r.get("approved_at")),
                    },
                )
            })
            .collect())
    }

    /// Facts with at least one use enabled: `(plugin_id, name) -> setting`.
    pub async fn enabled_settings(&self) -> Result<HashMap<(String, String), PluginFactSetting>> {
        let rows = sqlx::query(
            "SELECT plugin_id, name, usable_in_rules, exportable, approved_by, approved_at \
             FROM plugin_fact_settings WHERE usable_in_rules = 1 OR exportable = 1",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    (r.get::<String, _>("plugin_id"), r.get::<String, _>("name")),
                    PluginFactSetting {
                        usable_in_rules: r.get::<i64, _>("usable_in_rules") != 0,
                        exportable: r.get::<i64, _>("exportable") != 0,
                        approved_by: r.get("approved_by"),
                        approved_at: opt_ts(r.get("approved_at")),
                    },
                )
            })
            .collect())
    }

    pub async fn put_setting(
        &self,
        plugin_id: &str,
        name: &str,
        s: &PluginFactSetting,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_fact_settings (plugin_id, name, usable_in_rules, exportable, \
             approved_by, approved_at) VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(plugin_id, name) DO UPDATE SET usable_in_rules = excluded.usable_in_rules, \
             exportable = excluded.exportable, approved_by = excluded.approved_by, \
             approved_at = excluded.approved_at",
        )
        .bind(plugin_id)
        .bind(name)
        .bind(s.usable_in_rules as i64)
        .bind(s.exportable as i64)
        .bind(&s.approved_by)
        .bind(s.approved_at.map(fmt))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    // ---- provider status --------------------------------------------------

    pub async fn provider_status(&self, plugin_id: &str) -> Result<FactProviderStatus> {
        let row = sqlx::query(
            "SELECT last_success_at, last_failure_at, last_error, failures, rejected \
             FROM plugin_fact_providers WHERE plugin_id = ?",
        )
        .bind(plugin_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row
            .map(|r| FactProviderStatus {
                last_success_at: opt_ts(r.get("last_success_at")),
                last_failure_at: opt_ts(r.get("last_failure_at")),
                last_error: r.get("last_error"),
                failures: r.get::<i64, _>("failures").max(0) as u64,
                rejected: r.get::<i64, _>("rejected").max(0) as u64,
            })
            .unwrap_or_default())
    }

    pub async fn record_success(&self, plugin_id: &str, at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_fact_providers (plugin_id, last_success_at) VALUES (?, ?) \
             ON CONFLICT(plugin_id) DO UPDATE SET last_success_at = excluded.last_success_at",
        )
        .bind(plugin_id)
        .bind(fmt(at))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// `rejected`: the plugin sent values that do not fit the schema;
    /// otherwise the provider failed (error or timeout).
    pub async fn record_failure(
        &self,
        plugin_id: &str,
        error: &str,
        rejected: bool,
        at: DateTime<Utc>,
    ) -> Result<()> {
        let msg: String = error.chars().take(500).collect();
        sqlx::query(
            "INSERT INTO plugin_fact_providers (plugin_id, last_failure_at, last_error, failures, rejected) \
             VALUES (?, ?, ?, ?, ?) ON CONFLICT(plugin_id) DO UPDATE SET \
             last_failure_at = excluded.last_failure_at, last_error = excluded.last_error, \
             failures = failures + excluded.failures, rejected = rejected + excluded.rejected",
        )
        .bind(plugin_id)
        .bind(fmt(at))
        .bind(msg)
        .bind((!rejected) as i64)
        .bind(rejected as i64)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    // ---- classification and export bookkeeping ----------------------------

    pub async fn get_classification(
        &self,
        certname: &str,
        scope: &str,
    ) -> Result<Option<StoredClassification>> {
        let row = sqlx::query(
            "SELECT result_json, signature, influenced, facts_json, updated_at \
             FROM plugin_fact_classifications WHERE certname = ? AND scope = ?",
        )
        .bind(certname)
        .bind(scope)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| StoredClassification {
            result_json: r.get("result_json"),
            signature: r.get("signature"),
            influenced: r.get::<i64, _>("influenced") != 0,
            facts_json: r.get("facts_json"),
            updated_at: ts(&r.get::<String, _>("updated_at")),
        }))
    }

    pub async fn put_classification(
        &self,
        certname: &str,
        scope: &str,
        c: &StoredClassification,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_fact_classifications (certname, scope, result_json, signature, \
             influenced, facts_json, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(certname, scope) DO UPDATE SET result_json = excluded.result_json, \
             signature = excluded.signature, influenced = excluded.influenced, \
             facts_json = excluded.facts_json, updated_at = excluded.updated_at",
        )
        .bind(certname)
        .bind(scope)
        .bind(&c.result_json)
        .bind(&c.signature)
        .bind(c.influenced as i64)
        .bind(&c.facts_json)
        .bind(fmt(c.updated_at))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn export_digest(&self, certname: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT digest FROM plugin_fact_exports WHERE certname = ?")
                .bind(certname)
                .fetch_optional(self.pool)
                .await?,
        )
    }

    pub async fn put_export(&self, certname: &str, digest: &str, names: &[String]) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_fact_exports (certname, digest, names_json, exported_at) \
             VALUES (?, ?, ?, ?) ON CONFLICT(certname) DO UPDATE SET digest = excluded.digest, \
             names_json = excluded.names_json, exported_at = excluded.exported_at",
        )
        .bind(certname)
        .bind(digest)
        .bind(serde_json::to_string(names)?)
        .bind(fmt(Utc::now()))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Uninstall: settings and provider status always go (the opt-ins are
    /// approvals and start from scratch); values only with `purge`.
    pub async fn delete_plugin(&self, plugin_id: &str, purge: bool) -> Result<()> {
        sqlx::query("DELETE FROM plugin_fact_settings WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM plugin_fact_providers WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await?;
        if purge {
            sqlx::query("DELETE FROM plugin_facts WHERE plugin_id = ?")
                .bind(plugin_id)
                .execute(self.pool)
                .await?;
        }
        Ok(())
    }
}
