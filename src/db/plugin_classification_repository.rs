//! Persistence for classification extensions (M27): members of dynamic
//! groups, provider state, updates held by the churn guard and group
//! template instances.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::plugin_events_repository::fmt;
use crate::models::{GroupTemplateInstance, HeldMembership};

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn opt_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.as_deref().map(ts)
}

fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap_or_default()
}

/// Last refresh outcome of one dynamic group.
#[derive(Debug, Clone)]
pub struct MembershipState {
    pub group_id: Uuid,
    pub plugin_id: String,
    pub provider_id: String,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_refresh_at: Option<DateTime<Utc>>,
    pub last_status: String,
    pub last_error: Option<String>,
    pub stale_alerted: bool,
}

pub struct PluginClassificationRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> PluginClassificationRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ---- members ----------------------------------------------------------

    /// Members of one group with the time the provider last confirmed them.
    pub async fn members(&self, group_id: Uuid) -> Result<Vec<(String, DateTime<Utc>)>> {
        let rows = sqlx::query(
            "SELECT certname, computed_at FROM plugin_group_members WHERE group_id = ? ORDER BY certname",
        )
        .bind(group_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("list group members")?;
        Ok(rows
            .iter()
            .map(|r| (r.get("certname"), ts(&r.get::<String, _>("computed_at"))))
            .collect())
    }

    /// The members of every dynamic group.
    pub async fn all_members(&self) -> Result<HashMap<Uuid, Vec<(String, DateTime<Utc>)>>> {
        let rows = sqlx::query("SELECT group_id, certname, computed_at FROM plugin_group_members")
            .fetch_all(self.pool)
            .await
            .context("list members")?;
        let mut out: HashMap<Uuid, Vec<(String, DateTime<Utc>)>> = HashMap::new();
        for r in rows {
            out.entry(uuid(&r.get::<String, _>("group_id")))
                .or_default()
                .push((r.get("certname"), ts(&r.get::<String, _>("computed_at"))));
        }
        Ok(out)
    }

    /// Applies a membership change in one transaction: adds, removes and
    /// (when `confirm_all`) refreshes the time of every kept member.
    pub async fn apply(
        &self,
        group_id: Uuid,
        add: &[String],
        remove: &[String],
        confirm_all: bool,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let g = group_id.to_string();
        let mut tx = self.pool.begin().await?;
        for c in remove {
            sqlx::query("DELETE FROM plugin_group_members WHERE group_id = ? AND certname = ?")
                .bind(&g)
                .bind(c)
                .execute(&mut *tx)
                .await?;
        }
        for c in add {
            sqlx::query(
                "INSERT INTO plugin_group_members (group_id, certname, computed_at) VALUES (?, ?, ?) \
                 ON CONFLICT(group_id, certname) DO UPDATE SET computed_at = excluded.computed_at",
            )
            .bind(&g)
            .bind(c)
            .bind(fmt(now))
            .execute(&mut *tx)
            .await?;
        }
        if confirm_all {
            sqlx::query("UPDATE plugin_group_members SET computed_at = ? WHERE group_id = ?")
                .bind(fmt(now))
                .bind(&g)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Deletes everything stored for a group (group deleted or purged).
    pub async fn forget_group(&self, group_id: Uuid) -> Result<()> {
        let g = group_id.to_string();
        sqlx::query("DELETE FROM plugin_group_members WHERE group_id = ?")
            .bind(&g)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM plugin_membership_state WHERE group_id = ?")
            .bind(&g)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM plugin_membership_held WHERE group_id = ?")
            .bind(&g)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    // ---- provider state ---------------------------------------------------

    pub async fn state(&self, group_id: Uuid) -> Result<Option<MembershipState>> {
        let row = sqlx::query("SELECT * FROM plugin_membership_state WHERE group_id = ?")
            .bind(group_id.to_string())
            .fetch_optional(self.pool)
            .await?;
        Ok(row.map(|r| state_of(&r)))
    }

    pub async fn states(&self) -> Result<Vec<MembershipState>> {
        let rows = sqlx::query("SELECT * FROM plugin_membership_state")
            .fetch_all(self.pool)
            .await?;
        Ok(rows.iter().map(state_of).collect())
    }

    /// Records an attempt. `status` is `ok`, `failed` or `held`; only `ok`
    /// moves `last_refresh_at`.
    pub async fn record_attempt(
        &self,
        group_id: Uuid,
        plugin_id: &str,
        provider_id: &str,
        status: &str,
        error: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let refreshed = (status == "ok").then(|| fmt(now));
        sqlx::query(
            "INSERT INTO plugin_membership_state \
               (group_id, plugin_id, provider_id, last_attempt_at, last_refresh_at, last_status, last_error) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(group_id) DO UPDATE SET \
               plugin_id = excluded.plugin_id, provider_id = excluded.provider_id, \
               last_attempt_at = excluded.last_attempt_at, \
               last_refresh_at = COALESCE(excluded.last_refresh_at, plugin_membership_state.last_refresh_at), \
               last_status = excluded.last_status, last_error = excluded.last_error, \
               stale_alerted = CASE WHEN excluded.last_status = 'ok' THEN 0 ELSE plugin_membership_state.stale_alerted END",
        )
        .bind(group_id.to_string())
        .bind(plugin_id)
        .bind(provider_id)
        .bind(fmt(now))
        .bind(refreshed)
        .bind(status)
        .bind(error)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_stale_alerted(&self, group_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE plugin_membership_state SET stale_alerted = 1 WHERE group_id = ?")
            .bind(group_id.to_string())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    // ---- held updates -----------------------------------------------------

    pub async fn put_held(&self, h: &HeldMembership) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_membership_held \
               (group_id, plugin_id, provider_id, added_json, removed_json, current_count, \
                proposed_count, churn_pct, limit_pct, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(group_id) DO UPDATE SET \
               plugin_id = excluded.plugin_id, provider_id = excluded.provider_id, \
               added_json = excluded.added_json, removed_json = excluded.removed_json, \
               current_count = excluded.current_count, proposed_count = excluded.proposed_count, \
               churn_pct = excluded.churn_pct, limit_pct = excluded.limit_pct, \
               created_at = excluded.created_at",
        )
        .bind(h.group_id.to_string())
        .bind(&h.plugin_id)
        .bind(&h.provider_id)
        .bind(serde_json::to_string(&h.added)?)
        .bind(serde_json::to_string(&h.removed)?)
        .bind(h.current as i64)
        .bind(h.proposed as i64)
        .bind(h.churn_pct)
        .bind(h.limit_pct as i64)
        .bind(fmt(h.created_at))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn held(&self, group_id: Uuid) -> Result<Option<HeldMembership>> {
        let row = sqlx::query("SELECT * FROM plugin_membership_held WHERE group_id = ?")
            .bind(group_id.to_string())
            .fetch_optional(self.pool)
            .await?;
        Ok(row.map(|r| held_of(&r)))
    }

    pub async fn all_held(&self) -> Result<Vec<HeldMembership>> {
        let rows = sqlx::query("SELECT * FROM plugin_membership_held ORDER BY created_at")
            .fetch_all(self.pool)
            .await?;
        Ok(rows.iter().map(held_of).collect())
    }

    pub async fn delete_held(&self, group_id: Uuid) -> Result<bool> {
        let r = sqlx::query("DELETE FROM plugin_membership_held WHERE group_id = ?")
            .bind(group_id.to_string())
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    // ---- template instances -----------------------------------------------

    pub async fn instances(&self) -> Result<Vec<GroupTemplateInstance>> {
        let rows = sqlx::query("SELECT * FROM plugin_group_template_instances ORDER BY name")
            .fetch_all(self.pool)
            .await?;
        Ok(rows.iter().map(instance_of).collect())
    }

    pub async fn instance(&self, id: Uuid) -> Result<Option<GroupTemplateInstance>> {
        let row = sqlx::query("SELECT * FROM plugin_group_template_instances WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool)
            .await?;
        Ok(row.map(|r| instance_of(&r)))
    }

    pub async fn insert_instance(&self, i: &GroupTemplateInstance) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_group_template_instances \
               (id, plugin_id, template_id, name, parent_id, classes_json, variables_json, \
                approved_by, approved_at, enabled, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(i.id.to_string())
        .bind(&i.plugin_id)
        .bind(&i.template_id)
        .bind(&i.name)
        .bind(i.parent_id.map(|p| p.to_string()))
        .bind(i.classes.to_string())
        .bind(i.variables.to_string())
        .bind(&i.approved_by)
        .bind(i.approved_at.map(fmt))
        .bind(i.enabled as i64)
        .bind(fmt(i.created_at))
        .execute(self.pool)
        .await
        .context("create template instance")?;
        Ok(())
    }

    pub async fn approve_instance(&self, id: Uuid, by: &str, now: DateTime<Utc>) -> Result<bool> {
        let r = sqlx::query(
            "UPDATE plugin_group_template_instances SET approved_by = ?, approved_at = ? WHERE id = ?",
        )
        .bind(by)
        .bind(fmt(now))
        .bind(id.to_string())
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn set_instance_enabled(&self, id: Uuid, enabled: bool) -> Result<bool> {
        let r = sqlx::query("UPDATE plugin_group_template_instances SET enabled = ? WHERE id = ?")
            .bind(enabled as i64)
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn record_instance_run(
        &self,
        id: Uuid,
        status: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE plugin_group_template_instances SET last_run_at = ?, last_status = ? WHERE id = ?",
        )
        .bind(fmt(now))
        .bind(status)
        .bind(id.to_string())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_instance(&self, id: Uuid) -> Result<bool> {
        let r = sqlx::query("DELETE FROM plugin_group_template_instances WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn delete_instances_of(&self, plugin_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM plugin_group_template_instances WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}

fn state_of(r: &sqlx::sqlite::SqliteRow) -> MembershipState {
    MembershipState {
        group_id: uuid(&r.get::<String, _>("group_id")),
        plugin_id: r.get("plugin_id"),
        provider_id: r.get("provider_id"),
        last_attempt_at: opt_ts(r.get("last_attempt_at")),
        last_refresh_at: opt_ts(r.get("last_refresh_at")),
        last_status: r.get("last_status"),
        last_error: r.get("last_error"),
        stale_alerted: r.get::<i64, _>("stale_alerted") != 0,
    }
}

fn held_of(r: &sqlx::sqlite::SqliteRow) -> HeldMembership {
    let list = |c: &str| -> Vec<String> {
        serde_json::from_str(&r.get::<String, _>(c)).unwrap_or_default()
    };
    HeldMembership {
        group_id: uuid(&r.get::<String, _>("group_id")),
        plugin_id: r.get("plugin_id"),
        provider_id: r.get("provider_id"),
        added: list("added_json"),
        removed: list("removed_json"),
        current: r.get::<i64, _>("current_count") as u64,
        proposed: r.get::<i64, _>("proposed_count") as u64,
        churn_pct: r.get("churn_pct"),
        limit_pct: r.get::<i64, _>("limit_pct") as u32,
        created_at: ts(&r.get::<String, _>("created_at")),
    }
}

fn instance_of(r: &sqlx::sqlite::SqliteRow) -> GroupTemplateInstance {
    let json =
        |c: &str| -> Value { serde_json::from_str(&r.get::<String, _>(c)).unwrap_or(Value::Null) };
    GroupTemplateInstance {
        id: uuid(&r.get::<String, _>("id")),
        plugin_id: r.get("plugin_id"),
        template_id: r.get("template_id"),
        name: r.get("name"),
        parent_id: r.get::<Option<String>, _>("parent_id").as_deref().map(uuid),
        classes: json("classes_json"),
        variables: json("variables_json"),
        approved: r.get::<Option<String>, _>("approved_at").is_some(),
        approved_by: r.get("approved_by"),
        approved_at: opt_ts(r.get("approved_at")),
        enabled: r.get::<i64, _>("enabled") != 0,
        last_run_at: opt_ts(r.get("last_run_at")),
        last_status: r.get("last_status"),
        generated_groups: 0,
        created_at: ts(&r.get::<String, _>("created_at")),
    }
}
