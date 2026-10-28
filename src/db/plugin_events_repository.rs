//! Persistence for the plugin event outbox, deliveries, jobs and webhooks (M21).

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::models::{
    EventActor, EventDeliveryView, EventEnvelope, EventStats, OutboundWebhookView, PluginJobRunView,
};

/// Fixed-width UTC format so string comparison orders like time.
pub fn fmt(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[derive(Debug, Clone)]
pub struct NewDelivery {
    pub target_kind: &'static str,
    pub target_id: String,
    pub handler: Option<String>,
    pub priority: i32,
}

#[derive(Debug, Clone)]
pub struct ClaimedDelivery {
    pub id: i64,
    pub event: EventEnvelope,
    pub target_kind: String,
    pub target_id: String,
    pub handler: Option<String>,
    pub attempts: u32,
}

#[derive(Debug, Clone)]
pub struct WebhookEndpointRecord {
    pub view: OutboundWebhookView,
    pub secret: String,
}

#[derive(Debug, Clone)]
pub struct JobState {
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub run_count: u64,
    pub failure_count: u64,
}

#[derive(Debug, Clone)]
pub struct WebhookSecretMeta {
    pub hook_id: String,
    pub rotated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub struct PluginEventsRepository<'a> {
    pool: &'a SqlitePool,
}

fn envelope_from_row(r: &sqlx::sqlite::SqliteRow) -> EventEnvelope {
    EventEnvelope {
        id: r.get("id"),
        event_type: r.get("type"),
        schema_version: r.get::<i64, _>("schema_version") as u32,
        occurred_at: ts(&r.get::<String, _>("occurred_at")),
        actor: EventActor {
            kind: r.get("actor_kind"),
            id: r.get("actor_id"),
        },
        payload: serde_json::from_str(&r.get::<String, _>("payload")).unwrap_or_default(),
        correlation_id: r.get("correlation_id"),
    }
}

fn endpoint_from_row(r: &sqlx::sqlite::SqliteRow) -> WebhookEndpointRecord {
    WebhookEndpointRecord {
        view: OutboundWebhookView {
            id: r.get("id"),
            name: r.get("name"),
            url: r.get("url"),
            event_types: serde_json::from_str(&r.get::<String, _>("event_types"))
                .unwrap_or_default(),
            filter: r.get("filter"),
            enabled: r.get::<i64, _>("enabled") != 0,
            created_by: r.get("created_by"),
            created_at: ts(&r.get::<String, _>("created_at")),
            updated_at: ts(&r.get::<String, _>("updated_at")),
        },
        secret: r.get("secret"),
    }
}

/// Writes an event to the outbox on the caller's connection, so it commits
/// (or rolls back) together with the state change that caused it.
pub async fn insert_event(conn: &mut SqliteConnection, e: &EventEnvelope) -> Result<()> {
    sqlx::query(
        "INSERT INTO event_outbox (id, type, schema_version, payload, actor_kind, actor_id, \
         correlation_id, occurred_at, created_at, state) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(&e.id)
    .bind(&e.event_type)
    .bind(e.schema_version as i64)
    .bind(e.payload.to_string())
    .bind(&e.actor.kind)
    .bind(&e.actor.id)
    .bind(&e.correlation_id)
    .bind(fmt(e.occurred_at))
    .bind(fmt(Utc::now()))
    .execute(conn)
    .await
    .context("insert event")?;
    Ok(())
}

impl<'a> PluginEventsRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ---- outbox and deliveries ----------------------------------------------

    pub async fn pending_events(&self, limit: i64) -> Result<Vec<EventEnvelope>> {
        let rows = sqlx::query(
            "SELECT * FROM event_outbox WHERE state = 'pending' ORDER BY created_at, id LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(envelope_from_row).collect())
    }

    /// Creates the deliveries and marks the event fanned out in one transaction.
    pub async fn fan_out(&self, event_id: &str, targets: &[NewDelivery]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = fmt(Utc::now());
        for t in targets {
            sqlx::query(
                "INSERT OR IGNORE INTO event_deliveries (event_id, target_kind, target_id, handler, \
                 priority, status, attempts, next_attempt_at, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, 'pending', 0, ?, ?, ?)",
            )
            .bind(event_id)
            .bind(t.target_kind)
            .bind(&t.target_id)
            .bind(&t.handler)
            .bind(t.priority as i64)
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("UPDATE event_outbox SET state = 'fanned_out' WHERE id = ?")
            .bind(event_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Deliveries interrupted by a restart go back to the queue.
    pub async fn reset_in_flight(&self) -> Result<u64> {
        let r = sqlx::query(
            "UPDATE event_deliveries SET status = 'pending', updated_at = ? WHERE status = 'in_flight'",
        )
        .bind(fmt(Utc::now()))
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    /// Takes up to `limit` due deliveries (lowest priority number first) and
    /// marks them in flight so no other tick picks them up.
    pub async fn claim_due(&self, limit: i64) -> Result<Vec<ClaimedDelivery>> {
        let mut tx = self.pool.begin().await?;
        let now = fmt(Utc::now());
        let rows = sqlx::query(
            "SELECT d.id AS delivery_id, d.target_kind, d.target_id, d.handler, d.attempts, e.* \
             FROM event_deliveries d JOIN event_outbox e ON e.id = d.event_id \
             WHERE d.status = 'pending' AND d.next_attempt_at <= ? \
             ORDER BY d.priority, d.target_id, d.id LIMIT ?",
        )
        .bind(&now)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let id: i64 = r.get("delivery_id");
            sqlx::query(
                "UPDATE event_deliveries SET status = 'in_flight', updated_at = ? WHERE id = ?",
            )
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            out.push(ClaimedDelivery {
                id,
                event: envelope_from_row(r),
                target_kind: r.get("target_kind"),
                target_id: r.get("target_id"),
                handler: r.get("handler"),
                attempts: r.get::<i64, _>("attempts") as u32,
            });
        }
        tx.commit().await?;
        Ok(out)
    }

    pub async fn mark_delivered(&self, id: i64) -> Result<()> {
        let now = fmt(Utc::now());
        sqlx::query(
            "UPDATE event_deliveries SET status = 'delivered', attempts = attempts + 1, \
             last_error = NULL, delivered_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Records a failed attempt: retry at `retry_at`, or dead-letter when `None`.
    pub async fn mark_failed(
        &self,
        id: i64,
        error: &str,
        retry_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let now = fmt(Utc::now());
        let error: String = error.chars().take(1000).collect();
        match retry_at {
            Some(at) => sqlx::query(
                "UPDATE event_deliveries SET status = 'pending', attempts = attempts + 1, \
                 last_error = ?, next_attempt_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(error)
            .bind(fmt(at))
            .bind(&now)
            .bind(id),
            None => sqlx::query(
                "UPDATE event_deliveries SET status = 'dead_letter', attempts = attempts + 1, \
                 last_error = ?, updated_at = ? WHERE id = ?",
            )
            .bind(error)
            .bind(&now)
            .bind(id),
        }
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_dropped(&self, id: i64, reason: &str) -> Result<()> {
        sqlx::query(
            "UPDATE event_deliveries SET status = 'dropped', last_error = ?, updated_at = ? WHERE id = ?",
        )
        .bind(reason)
        .bind(fmt(Utc::now()))
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Puts a dead-lettered delivery back in the queue. Returns false when it
    /// does not exist or is not dead-lettered.
    pub async fn retry_dead_letter(&self, id: i64) -> Result<bool> {
        let now = fmt(Utc::now());
        let r = sqlx::query(
            "UPDATE event_deliveries SET status = 'pending', attempts = 0, next_attempt_at = ?, \
             updated_at = ? WHERE id = ? AND status = 'dead_letter'",
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn list_deliveries(
        &self,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<EventDeliveryView>> {
        let rows = sqlx::query(
            "SELECT d.*, e.type AS event_type FROM event_deliveries d \
             JOIN event_outbox e ON e.id = d.event_id \
             WHERE (? IS NULL OR d.status = ?) ORDER BY d.id DESC LIMIT ?",
        )
        .bind(status)
        .bind(status)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| EventDeliveryView {
                id: r.get("id"),
                event_id: r.get("event_id"),
                event_type: r.get("event_type"),
                target_kind: r.get("target_kind"),
                target_id: r.get("target_id"),
                handler: r.get("handler"),
                status: r.get("status"),
                attempts: r.get::<i64, _>("attempts") as u32,
                next_attempt_at: ts(&r.get::<String, _>("next_attempt_at")),
                last_error: r.get("last_error"),
                created_at: ts(&r.get::<String, _>("created_at")),
                delivered_at: r.get::<Option<String>, _>("delivered_at").map(|s| ts(&s)),
            })
            .collect())
    }

    pub async fn stats(&self) -> Result<EventStats> {
        let count = |q: &'static str| async move {
            sqlx::query_scalar::<_, i64>(q)
                .fetch_one(self.pool)
                .await
                .map(|n| n as u64)
        };
        let pending_events =
            count("SELECT COUNT(*) FROM event_outbox WHERE state = 'pending'").await?;
        let pending_deliveries =
            count("SELECT COUNT(*) FROM event_deliveries WHERE status = 'pending'").await?;
        let in_flight =
            count("SELECT COUNT(*) FROM event_deliveries WHERE status = 'in_flight'").await?;
        let dead_letters =
            count("SELECT COUNT(*) FROM event_deliveries WHERE status = 'dead_letter'").await?;
        let delivered =
            count("SELECT COUNT(*) FROM event_deliveries WHERE status = 'delivered'").await?;
        let oldest: Option<String> = sqlx::query_scalar(
            "SELECT MIN(t) FROM (SELECT created_at AS t FROM event_outbox WHERE state = 'pending' \
             UNION ALL SELECT created_at FROM event_deliveries WHERE status IN ('pending','in_flight'))",
        )
        .fetch_one(self.pool)
        .await?;
        Ok(EventStats {
            pending_events,
            pending_deliveries,
            in_flight,
            dead_letters,
            delivered,
            oldest_pending_age_secs: oldest.map(|s| (Utc::now() - ts(&s)).num_seconds().max(0)),
        })
    }

    /// Removes fully handled events older than `retention_days` (deliveries
    /// cascade). Dead letters are kept until an administrator deals with them.
    pub async fn purge_events(&self, retention_days: i64) -> Result<u64> {
        let cutoff = fmt(Utc::now() - Duration::days(retention_days));
        let r = sqlx::query(
            "DELETE FROM event_outbox WHERE state = 'fanned_out' AND created_at < ? \
             AND NOT EXISTS (SELECT 1 FROM event_deliveries d WHERE d.event_id = event_outbox.id \
             AND d.status IN ('pending', 'in_flight', 'dead_letter'))",
        )
        .bind(cutoff)
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    pub async fn delete_plugin_events(&self, plugin_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM event_deliveries WHERE target_kind = 'plugin' AND target_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM plugin_jobs WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM plugin_job_runs WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM plugin_webhook_secrets WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    // ---- jobs ---------------------------------------------------------------

    pub async fn job_state(&self, plugin_id: &str, job_id: &str) -> Result<Option<JobState>> {
        let row = sqlx::query("SELECT * FROM plugin_jobs WHERE plugin_id = ? AND job_id = ?")
            .bind(plugin_id)
            .bind(job_id)
            .fetch_optional(self.pool)
            .await?;
        Ok(row.map(|r| JobState {
            next_run_at: r.get::<Option<String>, _>("next_run_at").map(|s| ts(&s)),
            last_run_at: r.get::<Option<String>, _>("last_run_at").map(|s| ts(&s)),
            last_status: r.get("last_status"),
            last_error: r.get("last_error"),
            run_count: r.get::<i64, _>("run_count") as u64,
            failure_count: r.get::<i64, _>("failure_count") as u64,
        }))
    }

    pub async fn set_next_run(
        &self,
        plugin_id: &str,
        job_id: &str,
        next: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_jobs (plugin_id, job_id, next_run_at) VALUES (?, ?, ?) \
             ON CONFLICT(plugin_id, job_id) DO UPDATE SET next_run_at = excluded.next_run_at",
        )
        .bind(plugin_id)
        .bind(job_id)
        .bind(next.map(fmt))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn start_run(&self, plugin_id: &str, job_id: &str, trigger: &str) -> Result<i64> {
        let r = sqlx::query(
            "INSERT INTO plugin_job_runs (plugin_id, job_id, trigger, started_at, status) \
             VALUES (?, ?, ?, ?, 'running')",
        )
        .bind(plugin_id)
        .bind(job_id)
        .bind(trigger)
        .bind(fmt(Utc::now()))
        .execute(self.pool)
        .await?;
        Ok(r.last_insert_rowid())
    }

    pub async fn finish_run(
        &self,
        run_id: i64,
        plugin_id: &str,
        job_id: &str,
        status: &str,
        error: Option<&str>,
        duration_ms: i64,
    ) -> Result<()> {
        let now = fmt(Utc::now());
        let error = error.map(|e| e.chars().take(1000).collect::<String>());
        sqlx::query(
            "UPDATE plugin_job_runs SET finished_at = ?, status = ?, error = ?, duration_ms = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(status)
        .bind(&error)
        .bind(duration_ms)
        .bind(run_id)
        .execute(self.pool)
        .await?;
        let failed = i64::from(status != "ok");
        sqlx::query(
            "INSERT INTO plugin_jobs (plugin_id, job_id, last_run_at, last_status, last_error, run_count, failure_count) \
             VALUES (?, ?, ?, ?, ?, 1, ?) \
             ON CONFLICT(plugin_id, job_id) DO UPDATE SET last_run_at = excluded.last_run_at, \
             last_status = excluded.last_status, last_error = excluded.last_error, \
             run_count = run_count + 1, failure_count = failure_count + excluded.failure_count",
        )
        .bind(plugin_id)
        .bind(job_id)
        .bind(&now)
        .bind(status)
        .bind(&error)
        .bind(failed)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Runs left `running` by a crash are closed as errors.
    pub async fn close_orphan_runs(&self) -> Result<u64> {
        let r = sqlx::query(
            "UPDATE plugin_job_runs SET status = 'error', error = 'interrupted by a restart', \
             finished_at = ? WHERE status = 'running'",
        )
        .bind(fmt(Utc::now()))
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    pub async fn list_runs(
        &self,
        plugin_id: &str,
        job_id: &str,
        limit: i64,
    ) -> Result<Vec<PluginJobRunView>> {
        let rows = sqlx::query(
            "SELECT * FROM plugin_job_runs WHERE plugin_id = ? AND job_id = ? ORDER BY id DESC LIMIT ?",
        )
        .bind(plugin_id)
        .bind(job_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| PluginJobRunView {
                id: r.get("id"),
                plugin_id: r.get("plugin_id"),
                job_id: r.get("job_id"),
                trigger: r.get("trigger"),
                started_at: ts(&r.get::<String, _>("started_at")),
                finished_at: r.get::<Option<String>, _>("finished_at").map(|s| ts(&s)),
                status: r.get("status"),
                error: r.get("error"),
                duration_ms: r.get("duration_ms"),
            })
            .collect())
    }

    pub async fn purge_runs(&self, retention_days: i64) -> Result<u64> {
        let r =
            sqlx::query("DELETE FROM plugin_job_runs WHERE started_at < ? AND status != 'running'")
                .bind(fmt(Utc::now() - Duration::days(retention_days)))
                .execute(self.pool)
                .await?;
        Ok(r.rows_affected())
    }

    // ---- inbound webhook secrets ------------------------------------------------

    pub async fn webhook_secret(&self, plugin_id: &str, hook_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT secret FROM plugin_webhook_secrets WHERE plugin_id = ? AND hook_id = ?",
        )
        .bind(plugin_id)
        .bind(hook_id)
        .fetch_optional(self.pool)
        .await?)
    }

    pub async fn set_webhook_secret(
        &self,
        plugin_id: &str,
        hook_id: &str,
        secret: &str,
    ) -> Result<()> {
        let now = fmt(Utc::now());
        sqlx::query(
            "INSERT INTO plugin_webhook_secrets (plugin_id, hook_id, secret, created_at, rotated_at) \
             VALUES (?, ?, ?, ?, ?) ON CONFLICT(plugin_id, hook_id) DO UPDATE SET \
             secret = excluded.secret, rotated_at = excluded.rotated_at, last_used_at = NULL",
        )
        .bind(plugin_id)
        .bind(hook_id)
        .bind(secret)
        .bind(&now)
        .bind(&now)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn touch_webhook(&self, plugin_id: &str, hook_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE plugin_webhook_secrets SET last_used_at = ? WHERE plugin_id = ? AND hook_id = ?",
        )
        .bind(fmt(Utc::now()))
        .bind(plugin_id)
        .bind(hook_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn webhook_secret_meta(&self, plugin_id: &str) -> Result<Vec<WebhookSecretMeta>> {
        let rows = sqlx::query(
            "SELECT hook_id, rotated_at, last_used_at FROM plugin_webhook_secrets WHERE plugin_id = ?",
        )
        .bind(plugin_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| WebhookSecretMeta {
                hook_id: r.get("hook_id"),
                rotated_at: ts(&r.get::<String, _>("rotated_at")),
                last_used_at: r.get::<Option<String>, _>("last_used_at").map(|s| ts(&s)),
            })
            .collect())
    }

    // ---- outbound webhook endpoints ---------------------------------------------

    pub async fn list_endpoints(&self) -> Result<Vec<WebhookEndpointRecord>> {
        let rows = sqlx::query("SELECT * FROM plugin_webhook_endpoints ORDER BY created_at, id")
            .fetch_all(self.pool)
            .await?;
        Ok(rows.iter().map(endpoint_from_row).collect())
    }

    pub async fn get_endpoint(&self, id: &str) -> Result<Option<WebhookEndpointRecord>> {
        let row = sqlx::query("SELECT * FROM plugin_webhook_endpoints WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool)
            .await?;
        Ok(row.as_ref().map(endpoint_from_row))
    }

    pub async fn insert_endpoint(&self, e: &WebhookEndpointRecord) -> Result<()> {
        let v = &e.view;
        sqlx::query(
            "INSERT INTO plugin_webhook_endpoints (id, name, url, secret, event_types, filter, \
             enabled, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&v.id)
        .bind(&v.name)
        .bind(&v.url)
        .bind(&e.secret)
        .bind(serde_json::to_string(&v.event_types)?)
        .bind(&v.filter)
        .bind(i64::from(v.enabled))
        .bind(&v.created_by)
        .bind(fmt(v.created_at))
        .bind(fmt(v.updated_at))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_endpoint(&self, e: &WebhookEndpointRecord) -> Result<()> {
        let v = &e.view;
        sqlx::query(
            "UPDATE plugin_webhook_endpoints SET name = ?, url = ?, secret = ?, event_types = ?, \
             filter = ?, enabled = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&v.name)
        .bind(&v.url)
        .bind(&e.secret)
        .bind(serde_json::to_string(&v.event_types)?)
        .bind(&v.filter)
        .bind(i64::from(v.enabled))
        .bind(fmt(Utc::now()))
        .bind(&v.id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_endpoint(&self, id: &str) -> Result<bool> {
        let r = sqlx::query("DELETE FROM plugin_webhook_endpoints WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM event_deliveries WHERE target_kind = 'webhook' AND target_id = ? AND status != 'delivered'")
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }
}
