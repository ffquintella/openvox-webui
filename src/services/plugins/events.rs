//! Event bus with a durable outbox and a dispatcher (T77).
//!
//! `publish` only writes to `event_outbox`, so callers never wait for a
//! handler. The dispatcher fans each event out to its targets (enabled plugin
//! subscriptions and outbound webhook endpoints), then delivers with
//! at-least-once semantics: retry with exponential backoff, dead-letter after
//! `max_attempts`, a per-plugin concurrency bulkhead and a handler timeout.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{SqliteConnection, SqlitePool};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::filter::{pattern_matches, valid_event_type, Filter};
use super::invoke::{invoke_with_timeout, HandlerKind, HookIndex, InvokeError};
use super::manager::PluginManager;
use super::webhooks::{sign, HEADER_DELIVERY, HEADER_EVENT, HEADER_SIGNATURE, HEADER_TIMESTAMP};
use crate::config::PluginEventsConfig;
use crate::db::plugin_events_repository::{
    insert_event, ClaimedDelivery, NewDelivery, WebhookEndpointRecord,
};
use crate::db::PluginEventsRepository;
use crate::models::{EventActor, EventEnvelope, EVENT_SCHEMA_VERSION};

pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct EventBus {
    pool: SqlitePool,
}

fn build_envelope(
    event_type: &str,
    payload: Value,
    actor: EventActor,
    correlation_id: Option<String>,
) -> Result<EventEnvelope> {
    if !valid_event_type(event_type) {
        return Err(anyhow!("'{}' is not a valid event type", event_type));
    }
    if payload.to_string().len() > MAX_PAYLOAD_BYTES {
        return Err(anyhow!(
            "event payload is larger than {} bytes",
            MAX_PAYLOAD_BYTES
        ));
    }
    Ok(EventEnvelope {
        id: Uuid::new_v4().to_string(),
        event_type: event_type.to_string(),
        schema_version: EVENT_SCHEMA_VERSION,
        occurred_at: Utc::now(),
        actor,
        payload,
        correlation_id,
    })
}

impl EventBus {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn publish(
        &self,
        event_type: &str,
        payload: Value,
        actor: EventActor,
    ) -> Result<EventEnvelope> {
        let e = build_envelope(event_type, payload, actor, None)?;
        let mut conn = self.pool.acquire().await?;
        insert_event(&mut conn, &e).await?;
        Ok(e)
    }

    /// Publishes on the caller's transaction: the event exists only if the
    /// state change commits.
    pub async fn publish_in_tx(
        conn: &mut SqliteConnection,
        event_type: &str,
        payload: Value,
        actor: EventActor,
    ) -> Result<EventEnvelope> {
        let e = build_envelope(event_type, payload, actor, None)?;
        insert_event(conn, &e).await?;
        Ok(e)
    }

    /// Fire-and-forget for call sites that must not fail because of the bus.
    pub async fn emit(&self, event_type: &str, payload: Value, actor: EventActor) {
        if let Err(e) = self.publish(event_type, payload, actor).await {
            tracing::warn!("could not publish event '{}': {}", event_type, e);
        }
    }

    /// Publish on behalf of a plugin: only names it declared in
    /// `events.publish` are accepted (the manifest keeps them under
    /// `<plugin-id>.`), which is what the host API `events.publish` calls.
    pub async fn publish_from_plugin(
        &self,
        hooks: &HookIndex,
        plugin_id: &str,
        event_type: &str,
        payload: Value,
    ) -> Result<EventEnvelope> {
        let declared = hooks
            .get(plugin_id)
            .is_some_and(|h| h.publish.iter().any(|p| p == event_type));
        if !declared {
            return Err(anyhow!(
                "plugin '{}' did not declare the event '{}'",
                plugin_id,
                event_type
            ));
        }
        self.publish(event_type, payload, EventActor::plugin(plugin_id))
            .await
    }
}

pub struct Dispatcher {
    manager: Arc<PluginManager>,
    http: reqwest::Client,
    cfg: PluginEventsConfig,
    bulkheads: Mutex<HashMap<String, Arc<Semaphore>>>,
}

impl Dispatcher {
    pub fn new(manager: Arc<PluginManager>, cfg: PluginEventsConfig) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Arc::new(Self {
            manager,
            http,
            cfg,
            bulkheads: Mutex::new(HashMap::new()),
        })
    }

    fn repo(&self) -> PluginEventsRepository<'_> {
        PluginEventsRepository::new(self.manager.pool())
    }

    fn bulkhead(&self, key: &str) -> Arc<Semaphore> {
        let mut m = self.bulkheads.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(key.to_string())
            .or_insert_with(|| {
                Arc::new(Semaphore::new(
                    self.cfg.per_plugin_concurrency.max(1) as usize
                ))
            })
            .clone()
    }

    /// Fans out new events, claims due deliveries and starts them. Returns the
    /// started delivery tasks (the loop does not wait for them).
    pub async fn tick(self: &Arc<Self>) -> Result<Vec<JoinHandle<()>>> {
        self.fan_out().await?;
        let due = self
            .repo()
            .claim_due(self.cfg.batch_size.max(1) as i64)
            .await?;
        Ok(due
            .into_iter()
            .map(|d| {
                let this = self.clone();
                tokio::spawn(async move { this.deliver(d).await })
            })
            .collect())
    }

    /// One full pass, waiting for every started delivery. Used by tests and
    /// by the administrator's manual retry.
    pub async fn run_once(self: &Arc<Self>) -> Result<()> {
        for h in self.tick().await? {
            let _ = h.await;
        }
        Ok(())
    }

    #[cfg(test)]
    pub async fn fan_out_for_test(&self) {
        self.fan_out().await.unwrap();
    }

    async fn fan_out(&self) -> Result<()> {
        let events = self
            .repo()
            .pending_events(self.cfg.batch_size.max(1) as i64)
            .await?;
        if events.is_empty() {
            return Ok(());
        }
        let hooks = self.manager.hooks();
        let endpoints = self.repo().list_endpoints().await?;
        for e in events {
            let mut targets = Vec::new();
            for p in hooks.plugins() {
                if let Some(s) = p.subscription_for(&e.event_type, &e.payload) {
                    targets.push(NewDelivery {
                        target_kind: "plugin",
                        target_id: p.plugin_id.clone(),
                        handler: Some(s.handler.clone()),
                        priority: s.priority,
                    });
                }
            }
            for ep in endpoints.iter().filter(|ep| endpoint_wants(ep, &e)) {
                targets.push(NewDelivery {
                    target_kind: "webhook",
                    target_id: ep.view.id.clone(),
                    handler: None,
                    priority: 100,
                });
            }
            self.repo().fan_out(&e.id, &targets).await?;
        }
        Ok(())
    }

    async fn deliver(self: Arc<Self>, d: ClaimedDelivery) {
        let result = match d.target_kind.as_str() {
            "plugin" => self.deliver_plugin(&d).await,
            _ => self.deliver_webhook(&d).await,
        };
        let repo = self.repo();
        let stored = match result {
            Ok(()) => repo.mark_delivered(d.id).await,
            Err(Outcome::Drop(reason)) => repo.mark_dropped(d.id, &reason).await,
            Err(Outcome::Retry(err)) => {
                let attempts = d.attempts + 1;
                let retry_at = (attempts < self.cfg.max_attempts).then(|| {
                    Utc::now() + chrono::Duration::seconds(self.backoff_secs(d.attempts) as i64)
                });
                if retry_at.is_none() {
                    tracing::error!(
                        "event {} to {} '{}' dead-lettered after {} attempts: {}",
                        d.event.event_type,
                        d.target_kind,
                        d.target_id,
                        attempts,
                        err
                    );
                }
                repo.mark_failed(d.id, &err, retry_at).await
            }
        };
        if let Err(e) = stored {
            tracing::error!("could not record delivery {}: {}", d.id, e);
        }
    }

    pub fn backoff_secs(&self, attempts_so_far: u32) -> u64 {
        let factor = 1u64
            .checked_shl(attempts_so_far.min(20))
            .unwrap_or(u64::MAX);
        self.cfg
            .retry_base_secs
            .saturating_mul(factor)
            .min(self.cfg.retry_max_secs)
    }

    async fn deliver_plugin(&self, d: &ClaimedDelivery) -> Result<(), Outcome> {
        let hooks = self.manager.hooks();
        let Some(p) = hooks.get(&d.target_id) else {
            return Err(Outcome::Drop("plugin is not enabled".into()));
        };
        let handler = d
            .handler
            .clone()
            .unwrap_or_else(|| crate::models::DEFAULT_EVENT_HANDLER.to_string());
        let _permit = self
            .bulkhead(&d.target_id)
            .acquire_owned()
            .await
            .map_err(|e| Outcome::Retry(e.to_string()))?;
        let input = serde_json::to_value(&d.event).unwrap_or_default();
        let call = p.call(HandlerKind::Event, &handler, input);
        invoke_with_timeout(self.manager.as_ref(), call, p.timeout_ms())
            .await
            .map(|_| ())
            .map_err(|e: InvokeError| Outcome::Retry(e.to_string()))
    }

    async fn deliver_webhook(&self, d: &ClaimedDelivery) -> Result<(), Outcome> {
        let ep = self
            .repo()
            .get_endpoint(&d.target_id)
            .await
            .map_err(|e| Outcome::Retry(e.to_string()))?;
        let Some(ep) = ep.filter(|e| e.view.enabled) else {
            return Err(Outcome::Drop("webhook endpoint removed or disabled".into()));
        };
        let body = serde_json::to_vec(&d.event).unwrap_or_default();
        let ts = Utc::now().timestamp().to_string();
        let resp = self
            .http
            .post(&ep.view.url)
            .header("content-type", "application/json")
            .header(HEADER_EVENT, &d.event.event_type)
            .header(HEADER_DELIVERY, &d.event.id)
            .header(HEADER_TIMESTAMP, &ts)
            .header(
                HEADER_SIGNATURE,
                format!("sha256={}", sign(&ep.secret, &ts, &body)),
            )
            .body(body)
            .send()
            .await
            .map_err(|e| Outcome::Retry(format!("request failed: {}", e)))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Outcome::Retry(format!(
                "endpoint answered {}",
                resp.status()
            )))
        }
    }
}

enum Outcome {
    Retry(String),
    Drop(String),
}

fn endpoint_wants(ep: &WebhookEndpointRecord, e: &EventEnvelope) -> bool {
    ep.view.enabled
        && ep
            .view
            .event_types
            .iter()
            .any(|p| pattern_matches(p, &e.event_type))
        && ep
            .view
            .filter
            .as_deref()
            .and_then(|f| Filter::parse(f).ok())
            .is_none_or(|f| f.matches(&e.payload))
}

/// Starts the dispatcher loop and the hourly purge of handled events.
pub fn start_dispatcher(dispatcher: Arc<Dispatcher>, cfg: PluginEventsConfig) {
    let d = dispatcher;
    tokio::spawn(async move {
        if let Err(e) = d.repo().reset_in_flight().await {
            tracing::warn!("could not requeue interrupted deliveries: {}", e);
        }
        let mut tick =
            tokio::time::interval(Duration::from_millis(cfg.dispatch_interval_ms.max(100)));
        let mut last_purge = std::time::Instant::now();
        loop {
            tick.tick().await;
            if let Err(e) = d.tick().await {
                tracing::warn!("event dispatcher tick failed: {}", e);
            }
            if last_purge.elapsed() >= Duration::from_secs(3600) {
                last_purge = std::time::Instant::now();
                let days = cfg.retention_days.max(1) as i64;
                if let Err(e) = d.repo().purge_events(days).await {
                    tracing::warn!("event purge failed: {}", e);
                }
                let _ = d.repo().purge_runs(days.max(30)).await;
            }
        }
    });
}

pub fn lifecycle_payload(plugin_id: &str, version: Option<&str>) -> Value {
    json!({ "plugin_id": plugin_id, "version": version })
}
