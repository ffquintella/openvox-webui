//! Telemetry and protection of the host against plugins (M22).
//!
//! The [`Governor`] sits in front of every handler call (events, jobs,
//! webhooks and lifecycle hooks all go through `PluginManager::invoke`). It
//! admits or refuses the call (circuit breaker, calls per minute), records the
//! outcome and latency, and decides when the breaker opens or closes. It is
//! pure in-memory state; the manager applies the side effects of a transition
//! (plugin state, audit, events) and a periodic tick flushes the hourly
//! rollups to the database.

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, DurationRound, Utc};
use serde_json::{json, Value};

use super::events::EventBus;
use super::invoke::InvokeError;
use super::manager::{PluginError, PluginManager, AUDIT_PLUGINS};
use crate::config::PluginGovernanceConfig;
use crate::db::{PluginEventsRepository, PluginGovernanceRepository, PluginRepository};
use crate::models::*;

/// `plugin_state.state_reason` of a plugin the breaker took out of play.
pub const CIRCUIT_REASON: &str = "circuit_open";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Error,
    Trap,
    Timeout,
}

/// Maps a handler result to what the metrics count. `Unavailable` (bulkhead
/// full, runtime missing) is the host's condition, not the plugin's, so it
/// is neither a failure nor a success.
pub fn classify(r: &Result<Value, InvokeError>) -> Option<Outcome> {
    match r {
        Ok(_) => Some(Outcome::Ok),
        Err(InvokeError::Timeout(_)) => Some(Outcome::Timeout),
        Err(InvokeError::Unavailable(_)) => None,
        Err(InvokeError::Failed(m)) => Some(
            if m.starts_with("fuel exhausted")
                || m.starts_with("guest trapped")
                || m.starts_with("memory limit")
                || m.starts_with("host call limit")
                || m.starts_with("invalid guest message")
            {
                Outcome::Trap
            } else {
                Outcome::Error
            },
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("circuit breaker open, retry after {0}")]
    Circuit(DateTime<Utc>),
    #[error("quota exceeded: {0} calls per minute")]
    Quota(u32),
}

/// What changed in the breaker because of a recorded call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    Opened {
        trips: u32,
        retry_at: DateTime<Utc>,
        error: String,
    },
    Closed,
}

#[derive(Debug, Clone, Copy)]
enum Circuit {
    Closed,
    Open { retry_at: DateTime<Utc> },
    HalfOpen { successes: u32 },
}

struct PluginGov {
    calls: VecDeque<DateTime<Utc>>,
    failures: VecDeque<DateTime<Utc>>,
    circuit: Circuit,
    /// Trips in a row without closing in between (drives the backoff)
    streak: u32,
    trips_total: u64,
    opened_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

impl Default for PluginGov {
    fn default() -> Self {
        Self {
            calls: VecDeque::new(),
            failures: VecDeque::new(),
            circuit: Circuit::Closed,
            streak: 0,
            trips_total: 0,
            opened_at: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Default)]
struct Agg {
    calls: u64,
    errors: u64,
    traps: u64,
    timeouts: u64,
    quota_rejections: u64,
    circuit_rejections: u64,
    total_ms: u64,
    max_ms: u64,
    buckets: [u64; LATENCY_BUCKET_COUNT],
}

impl Agg {
    fn add(&mut self, o: &Agg) {
        self.calls += o.calls;
        self.errors += o.errors;
        self.traps += o.traps;
        self.timeouts += o.timeouts;
        self.quota_rejections += o.quota_rejections;
        self.circuit_rejections += o.circuit_rejections;
        self.total_ms += o.total_ms;
        self.max_ms = self.max_ms.max(o.max_ms);
        for (a, b) in self.buckets.iter_mut().zip(o.buckets.iter()) {
            *a += b;
        }
    }

    fn to_row(&self, plugin: &str, version: &str, kind: &str, hour: &str) -> UsageRow {
        UsageRow {
            plugin_id: plugin.to_string(),
            version: version.to_string(),
            kind: kind.to_string(),
            hour: hour.to_string(),
            calls: self.calls,
            errors: self.errors,
            traps: self.traps,
            timeouts: self.timeouts,
            quota_rejections: self.quota_rejections,
            circuit_rejections: self.circuit_rejections,
            total_ms: self.total_ms,
            max_ms: self.max_ms,
            latency_buckets: self.buckets.to_vec(),
        }
    }

    fn from_row(r: &UsageRow) -> Agg {
        let mut a = Agg {
            calls: r.calls,
            errors: r.errors,
            traps: r.traps,
            timeouts: r.timeouts,
            quota_rejections: r.quota_rejections,
            circuit_rejections: r.circuit_rejections,
            total_ms: r.total_ms,
            max_ms: r.max_ms,
            ..Default::default()
        };
        for (i, n) in r
            .latency_buckets
            .iter()
            .enumerate()
            .take(LATENCY_BUCKET_COUNT)
        {
            a.buckets[i] = *n;
        }
        a
    }
}

fn bucket_of(ms: u64) -> usize {
    LATENCY_BUCKETS_MS
        .iter()
        .position(|b| ms <= *b)
        .unwrap_or(LATENCY_BUCKETS_MS.len())
}

pub fn hour_of(now: DateTime<Utc>) -> String {
    crate::db::plugin_events_repository::fmt(now.duration_trunc(Duration::hours(1)).unwrap_or(now))
}

#[derive(Default)]
struct Inner {
    plugins: HashMap<String, PluginGov>,
    /// (plugin, version, kind, hour) not yet flushed to the database
    pending: HashMap<(String, String, String, String), Agg>,
    /// (plugin, version, kind) since the process started (Prometheus counters)
    lifetime: HashMap<(String, String, String), Agg>,
}

pub struct Governor {
    cfg: PluginGovernanceConfig,
    inner: Mutex<Inner>,
}

impl Governor {
    pub fn new(cfg: PluginGovernanceConfig) -> Self {
        Self {
            cfg,
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn config(&self) -> &PluginGovernanceConfig {
        &self.cfg
    }

    /// The lower of the host ceiling and the manifest's request; `None` is
    /// unlimited.
    pub fn effective_quota(&self, requested: Option<u32>) -> Option<u32> {
        let host = (self.cfg.max_invocations_per_minute > 0)
            .then_some(self.cfg.max_invocations_per_minute);
        match (host, requested) {
            (Some(h), Some(r)) => Some(h.min(r)),
            (h, r) => h.or(r),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn bump(
        inner: &mut Inner,
        plugin: &str,
        version: &str,
        kind: &str,
        now: DateTime<Utc>,
        f: impl Fn(&mut Agg),
    ) {
        let hour = hour_of(now);
        f(inner
            .pending
            .entry((
                plugin.to_string(),
                version.to_string(),
                kind.to_string(),
                hour,
            ))
            .or_default());
        f(inner
            .lifetime
            .entry((plugin.to_string(), version.to_string(), kind.to_string()))
            .or_default());
    }

    /// Decides whether a call may run. A refusal is counted. Lifecycle hooks
    /// are exempt from the call quota (an administrator is waiting for them).
    pub fn admit(
        &self,
        plugin: &str,
        version: &str,
        kind: &str,
        requested_quota: Option<u32>,
        now: DateTime<Utc>,
    ) -> Result<(), Rejection> {
        let quota = self.effective_quota(requested_quota);
        let mut inner = self.lock();
        let g = inner.plugins.entry(plugin.to_string()).or_default();
        let verdict = match g.circuit {
            Circuit::Open { retry_at } => Err(Rejection::Circuit(retry_at)),
            _ => match quota {
                Some(limit) if kind != "lifecycle" => {
                    let cutoff = now - Duration::seconds(60);
                    while g.calls.front().is_some_and(|t| *t <= cutoff) {
                        g.calls.pop_front();
                    }
                    if g.calls.len() as u32 >= limit {
                        Err(Rejection::Quota(limit))
                    } else {
                        g.calls.push_back(now);
                        Ok(())
                    }
                }
                _ => Ok(()),
            },
        };
        if let Err(r) = &verdict {
            let circuit = matches!(r, Rejection::Circuit(_));
            Self::bump(&mut inner, plugin, version, kind, now, |a| {
                if circuit {
                    a.circuit_rejections += 1
                } else {
                    a.quota_rejections += 1
                }
            });
        }
        verdict
    }

    /// Records a finished call and returns the breaker transition, if any.
    /// Lifecycle hooks are measured but never trip the breaker.
    pub fn record(
        &self,
        plugin: &str,
        version: &str,
        kind: &str,
        outcome: Outcome,
        elapsed_ms: u64,
        error: Option<&str>,
        now: DateTime<Utc>,
    ) -> Option<Transition> {
        let mut inner = self.lock();
        Self::bump(&mut inner, plugin, version, kind, now, |a| {
            a.calls += 1;
            a.total_ms += elapsed_ms;
            a.max_ms = a.max_ms.max(elapsed_ms);
            a.buckets[bucket_of(elapsed_ms)] += 1;
            match outcome {
                Outcome::Ok => {}
                Outcome::Error => a.errors += 1,
                Outcome::Trap => a.traps += 1,
                Outcome::Timeout => a.timeouts += 1,
            }
        });
        if kind == "lifecycle" {
            return None;
        }
        let cfg = &self.cfg;
        let g = inner.plugins.entry(plugin.to_string()).or_default();
        if outcome == Outcome::Ok {
            if let Circuit::HalfOpen { successes } = g.circuit {
                let successes = successes + 1;
                if successes >= cfg.breaker_close_after_successes.max(1) {
                    g.circuit = Circuit::Closed;
                    g.streak = 0;
                    g.failures.clear();
                    g.opened_at = None;
                    return Some(Transition::Closed);
                }
                g.circuit = Circuit::HalfOpen { successes };
            }
            return None;
        }
        let message = error.unwrap_or("failed").to_string();
        g.last_error = Some(message.clone());
        let trip = match g.circuit {
            Circuit::Open { .. } => return None,
            Circuit::HalfOpen { .. } => true,
            Circuit::Closed => {
                let cutoff = now - Duration::seconds(cfg.breaker_window_secs.max(1) as i64);
                while g.failures.front().is_some_and(|t| *t <= cutoff) {
                    g.failures.pop_front();
                }
                g.failures.push_back(now);
                g.failures.len() as u32 >= cfg.breaker_failure_threshold.max(1)
            }
        };
        if !trip {
            return None;
        }
        g.streak += 1;
        g.trips_total += 1;
        g.failures.clear();
        let wait = cfg
            .breaker_backoff_base_secs
            .saturating_mul(1u64 << (g.streak - 1).min(20))
            .min(cfg.breaker_backoff_max_secs.max(1));
        let retry_at = now + Duration::seconds(wait as i64);
        g.circuit = Circuit::Open { retry_at };
        g.opened_at = Some(now);
        Some(Transition::Opened {
            trips: g.streak,
            retry_at,
            error: message,
        })
    }

    /// Plugins whose backoff has elapsed.
    pub fn due_for_retry(&self, now: DateTime<Utc>) -> Vec<String> {
        let inner = self.lock();
        let mut v: Vec<String> = inner
            .plugins
            .iter()
            .filter(|(_, g)| matches!(g.circuit, Circuit::Open { retry_at } if retry_at <= now))
            .map(|(id, _)| id.clone())
            .collect();
        v.sort();
        v
    }

    pub fn mark_half_open(&self, plugin: &str) {
        if let Some(g) = self.lock().plugins.get_mut(plugin) {
            g.circuit = Circuit::HalfOpen { successes: 0 };
        }
    }

    /// Closes the breaker (administrator reset or an explicit enable).
    pub fn reset(&self, plugin: &str) {
        if let Some(g) = self.lock().plugins.get_mut(plugin) {
            g.circuit = Circuit::Closed;
            g.streak = 0;
            g.failures.clear();
            g.calls.clear();
            g.opened_at = None;
        }
    }

    pub fn forget(&self, plugin: &str) {
        self.lock().plugins.remove(plugin);
    }

    /// Restores breakers persisted before a restart.
    pub fn load(&self, records: &[CircuitRecord]) {
        let mut inner = self.lock();
        for r in records {
            let g = inner.plugins.entry(r.plugin_id.clone()).or_default();
            g.circuit = match r.state {
                CircuitState::HalfOpen => Circuit::HalfOpen { successes: 0 },
                _ => Circuit::Open {
                    retry_at: r.retry_at,
                },
            };
            g.streak = r.trips;
            g.trips_total = g.trips_total.max(r.trips as u64);
            g.opened_at = Some(r.opened_at);
            g.last_error = r.last_error.clone();
        }
    }

    pub fn circuit_view(&self, plugin: &str) -> CircuitView {
        let inner = self.lock();
        let Some(g) = inner.plugins.get(plugin) else {
            return CircuitView::closed();
        };
        let (state, retry_at) = match g.circuit {
            Circuit::Closed => (CircuitState::Closed, None),
            Circuit::Open { retry_at } => (CircuitState::Open, Some(retry_at)),
            Circuit::HalfOpen { .. } => (CircuitState::HalfOpen, None),
        };
        CircuitView {
            state,
            trips: g.streak,
            retry_at,
            last_error: g
                .last_error
                .clone()
                .filter(|_| state != CircuitState::Closed),
        }
    }

    pub fn circuit_record(&self, plugin: &str) -> Option<CircuitRecord> {
        let inner = self.lock();
        let g = inner.plugins.get(plugin)?;
        let (state, retry_at) = match g.circuit {
            Circuit::Closed => return None,
            Circuit::Open { retry_at } => (CircuitState::Open, retry_at),
            Circuit::HalfOpen { .. } => (CircuitState::HalfOpen, Utc::now()),
        };
        Some(CircuitRecord {
            plugin_id: plugin.to_string(),
            state,
            trips: g.streak,
            opened_at: g.opened_at.unwrap_or_else(Utc::now),
            retry_at,
            last_error: g.last_error.clone(),
        })
    }

    /// Drains the rollups that were not stored yet.
    pub fn take_pending(&self) -> Vec<UsageRow> {
        let mut inner = self.lock();
        std::mem::take(&mut inner.pending)
            .into_iter()
            .map(|((p, v, k, h), a)| a.to_row(&p, &v, &k, &h))
            .collect()
    }

    /// Puts rows back after a failed flush.
    pub fn requeue(&self, rows: Vec<UsageRow>) {
        let mut inner = self.lock();
        for r in rows {
            let key = (
                r.plugin_id.clone(),
                r.version.clone(),
                r.kind.clone(),
                r.hour.clone(),
            );
            inner
                .pending
                .entry(key)
                .or_default()
                .add(&Agg::from_row(&r));
        }
    }

    /// Prometheus text exposition of the counters, the latency histogram,
    /// the breaker state and the event bus gauges.
    pub fn render_prometheus(
        &self,
        plugins: &[(String, Option<String>, String)],
        events: &EventStats,
    ) -> String {
        let inner = self.lock();
        let mut out = String::new();
        let mut keys: Vec<_> = inner.lifetime.keys().cloned().collect();
        keys.sort();

        out.push_str("# HELP openvox_plugin_invocations_total Handler calls by outcome.\n");
        out.push_str("# TYPE openvox_plugin_invocations_total counter\n");
        for k in &keys {
            let a = &inner.lifetime[k];
            let ok = a.calls - (a.errors + a.traps + a.timeouts).min(a.calls);
            for (outcome, n) in [
                ("ok", ok),
                ("error", a.errors),
                ("trap", a.traps),
                ("timeout", a.timeouts),
            ] {
                let _ = writeln!(
                    out,
                    "openvox_plugin_invocations_total{{plugin=\"{}\",version=\"{}\",kind=\"{}\",outcome=\"{}\"}} {}",
                    esc(&k.0), esc(&k.1), esc(&k.2), outcome, n
                );
            }
        }

        out.push_str("# HELP openvox_plugin_rejections_total Calls refused before running.\n");
        out.push_str("# TYPE openvox_plugin_rejections_total counter\n");
        for k in &keys {
            let a = &inner.lifetime[k];
            for (reason, n) in [
                ("quota", a.quota_rejections),
                ("circuit", a.circuit_rejections),
            ] {
                let _ = writeln!(
                    out,
                    "openvox_plugin_rejections_total{{plugin=\"{}\",version=\"{}\",kind=\"{}\",reason=\"{}\"}} {}",
                    esc(&k.0), esc(&k.1), esc(&k.2), reason, n
                );
            }
        }

        out.push_str("# HELP openvox_plugin_invocation_duration_seconds Handler call latency.\n");
        out.push_str("# TYPE openvox_plugin_invocation_duration_seconds histogram\n");
        let mut per_version: std::collections::BTreeMap<(String, String), Agg> = Default::default();
        for k in &keys {
            per_version
                .entry((k.0.clone(), k.1.clone()))
                .or_default()
                .add(&inner.lifetime[k]);
        }
        for ((plugin, version), a) in &per_version {
            let mut cumulative = 0;
            for (i, bound) in LATENCY_BUCKETS_MS.iter().enumerate() {
                cumulative += a.buckets[i];
                let _ = writeln!(
                    out,
                    "openvox_plugin_invocation_duration_seconds_bucket{{plugin=\"{}\",version=\"{}\",le=\"{}\"}} {}",
                    esc(plugin), esc(version), *bound as f64 / 1000.0, cumulative
                );
            }
            let _ = writeln!(
                out,
                "openvox_plugin_invocation_duration_seconds_bucket{{plugin=\"{}\",version=\"{}\",le=\"+Inf\"}} {}",
                esc(plugin), esc(version), a.calls
            );
            let _ =
                writeln!(
                out,
                "openvox_plugin_invocation_duration_seconds_sum{{plugin=\"{}\",version=\"{}\"}} {}",
                esc(plugin), esc(version), a.total_ms as f64 / 1000.0
            );
            let _ = writeln!(
                out,
                "openvox_plugin_invocation_duration_seconds_count{{plugin=\"{}\",version=\"{}\"}} {}",
                esc(plugin), esc(version), a.calls
            );
        }

        out.push_str(
            "# HELP openvox_plugin_circuit_state Breaker state: 0 closed, 1 half open, 2 open.\n",
        );
        out.push_str("# TYPE openvox_plugin_circuit_state gauge\n");
        out.push_str(
            "# HELP openvox_plugin_circuit_trips_total Times the breaker opened since start.\n",
        );
        out.push_str("# TYPE openvox_plugin_circuit_trips_total counter\n");
        out.push_str("# HELP openvox_plugin_info Installed plugins and their state.\n");
        out.push_str("# TYPE openvox_plugin_info gauge\n");
        for (id, version, state) in plugins {
            let (value, trips) = inner
                .plugins
                .get(id)
                .map(|g| {
                    (
                        match g.circuit {
                            Circuit::Closed => 0,
                            Circuit::HalfOpen { .. } => 1,
                            Circuit::Open { .. } => 2,
                        },
                        g.trips_total,
                    )
                })
                .unwrap_or((0, 0));
            let _ = writeln!(
                out,
                "openvox_plugin_circuit_state{{plugin=\"{}\"}} {}",
                esc(id),
                value
            );
            let _ = writeln!(
                out,
                "openvox_plugin_circuit_trips_total{{plugin=\"{}\"}} {}",
                esc(id),
                trips
            );
            let _ = writeln!(
                out,
                "openvox_plugin_info{{plugin=\"{}\",version=\"{}\",state=\"{}\"}} 1",
                esc(id),
                esc(version.as_deref().unwrap_or("")),
                esc(state)
            );
        }

        for (name, help, value) in [
            (
                "openvox_plugin_events_pending",
                "Events and deliveries waiting.",
                events.pending_events + events.pending_deliveries,
            ),
            (
                "openvox_plugin_events_in_flight",
                "Deliveries being handled.",
                events.in_flight,
            ),
            (
                "openvox_plugin_events_dead_letter",
                "Dead-lettered deliveries.",
                events.dead_letters,
            ),
        ] {
            let _ = writeln!(
                out,
                "# HELP {} {}\n# TYPE {} gauge\n{} {}",
                name, help, name, name, value
            );
        }
        let _ = writeln!(
            out,
            "# HELP openvox_plugin_events_oldest_pending_seconds Outbox lag.\n\
             # TYPE openvox_plugin_events_oldest_pending_seconds gauge\n\
             openvox_plugin_events_oldest_pending_seconds {}",
            events.oldest_pending_age_secs.unwrap_or(0).max(0)
        );
        out
    }
}

/// Escapes a Prometheus label value.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

// ---- manager side: effects of transitions, flushing, dashboard ------------

impl PluginManager {
    /// Restores persisted breakers; call once before serving traffic.
    pub async fn load_circuits(&self) {
        match PluginGovernanceRepository::new(self.pool())
            .list_circuits()
            .await
        {
            Ok(records) => self.governor().load(&records),
            Err(e) => tracing::warn!("could not load plugin circuit breakers: {}", e),
        }
    }

    async fn store_circuit(&self, plugin_id: &str) {
        let repo = PluginGovernanceRepository::new(self.pool());
        let r = match self.governor().circuit_record(plugin_id) {
            Some(c) => repo.upsert_circuit(&c).await,
            None => repo.delete_circuit(plugin_id).await,
        };
        if let Err(e) = r {
            tracing::warn!("could not store the breaker of {}: {}", plugin_id, e);
        }
    }

    /// Takes the plugin out of play (state `error`, no extension points),
    /// tells the administrators and anyone subscribed to the event.
    pub(super) async fn on_transition(&self, plugin_id: &str, version: &str, t: Transition) {
        match t {
            Transition::Opened {
                trips,
                retry_at,
                error,
            } => {
                tracing::error!(
                    "plugin {} {} isolated by the circuit breaker (trip {}): {}",
                    plugin_id,
                    version,
                    trips,
                    error
                );
                if let Err(e) = PluginRepository::new(self.pool())
                    .set_state(plugin_id, PluginState::Error, Some(CIRCUIT_REASON))
                    .await
                {
                    tracing::error!("could not mark {} as failed: {}", plugin_id, e);
                }
                self.store_circuit(plugin_id).await;
                if let Err(e) = self.reload().await {
                    tracing::error!("reload after circuit trip failed: {}", e);
                }
                self.audit(
                    default_organization_uuid(),
                    None,
                    AUDIT_PLUGINS,
                    "plugin.circuit.opened",
                    plugin_id,
                    json!({"version": version, "trips": trips, "retry_at": retry_at, "error": error}),
                )
                .await;
                self.emit_circuit_event(
                    "plugin.circuit.opened",
                    plugin_id,
                    version,
                    json!({
                        "trips": trips, "retry_at": retry_at, "error": error
                    }),
                )
                .await;
            }
            Transition::Closed => {
                self.store_circuit(plugin_id).await;
                self.audit(
                    default_organization_uuid(),
                    None,
                    AUDIT_PLUGINS,
                    "plugin.circuit.closed",
                    plugin_id,
                    json!({"version": version}),
                )
                .await;
                self.emit_circuit_event("plugin.circuit.closed", plugin_id, version, json!({}))
                    .await;
            }
        }
    }

    async fn emit_circuit_event(&self, kind: &str, plugin_id: &str, version: &str, extra: Value) {
        let mut payload = json!({"plugin_id": plugin_id, "version": version});
        if let (Some(p), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        EventBus::new(self.pool().clone())
            .emit(kind, payload, EventActor::system())
            .await;
    }

    /// Stores the usage collected since the last flush.
    pub async fn flush_usage(&self) -> anyhow::Result<()> {
        let rows = self.governor().take_pending();
        if rows.is_empty() {
            return Ok(());
        }
        if let Err(e) = PluginGovernanceRepository::new(self.pool())
            .add_usage(&rows)
            .await
        {
            self.governor().requeue(rows);
            return Err(e);
        }
        Ok(())
    }

    /// Periodic work: flush usage, retire old rollups and try plugins whose
    /// backoff has elapsed again (half-open).
    pub async fn governance_tick(&self) {
        if let Err(e) = self.flush_usage().await {
            tracing::warn!("could not store plugin usage: {}", e);
        }
        let keep = Duration::days(self.governor().config().usage_retention_days.max(1) as i64);
        if let Err(e) = PluginGovernanceRepository::new(self.pool())
            .purge_usage_before(Utc::now() - keep)
            .await
        {
            tracing::warn!("could not purge plugin usage: {}", e);
        }
        for id in self.governor().due_for_retry(Utc::now()) {
            if let Err(e) = self.try_again(&id).await {
                tracing::warn!("could not retry plugin {}: {}", id, e);
            }
        }
    }

    async fn try_again(&self, id: &str) -> Result<(), PluginError> {
        let repo = PluginRepository::new(self.pool());
        let still_ours = repo.get_plugin(id).await?.is_some_and(|p| {
            p.state == PluginState::Error && p.state_reason.as_deref() == Some(CIRCUIT_REASON)
        });
        if !still_ours {
            // Disabled, removed or re-enabled by an administrator meanwhile.
            self.governor().forget(id);
            PluginGovernanceRepository::new(self.pool())
                .delete_circuit(id)
                .await?;
            return Ok(());
        }
        repo.set_state(id, PluginState::Enabled, None).await?;
        self.governor().mark_half_open(id);
        self.store_circuit(id).await;
        self.reload().await?;
        self.audit(
            default_organization_uuid(),
            None,
            AUDIT_PLUGINS,
            "plugin.circuit.half_open",
            id,
            json!({}),
        )
        .await;
        self.emit_circuit_event("plugin.circuit.half_open", id, "", json!({}))
            .await;
        Ok(())
    }

    /// Administrator action: close the breaker and bring the plugin back.
    pub async fn reset_circuit(&self, id: &str) -> Result<CircuitView, PluginError> {
        let repo = PluginRepository::new(self.pool());
        let rec = repo
            .get_plugin(id)
            .await?
            .ok_or_else(|| PluginError::NotFound(format!("plugin '{}' not found", id)))?;
        self.governor().reset(id);
        PluginGovernanceRepository::new(self.pool())
            .delete_circuit(id)
            .await?;
        if rec.state == PluginState::Error && rec.state_reason.as_deref() == Some(CIRCUIT_REASON) {
            repo.set_state(id, PluginState::Enabled, None).await?;
            self.reload().await?;
        }
        Ok(self.governor().circuit_view(id))
    }

    /// Called when a plugin is enabled by hand, disabled or removed.
    pub(super) async fn clear_governance(&self, id: &str, drop_usage: bool) {
        self.governor().forget(id);
        let repo = PluginGovernanceRepository::new(self.pool());
        let r = if drop_usage {
            repo.delete_plugin_data(id).await
        } else {
            repo.delete_circuit(id).await
        };
        if let Err(e) = r {
            tracing::warn!("could not clear governance state of {}: {}", id, e);
        }
    }

    pub async fn governance_overview(&self) -> Result<GovernanceOverview, PluginError> {
        self.flush_usage().await?;
        let now = Utc::now();
        let day = PluginGovernanceRepository::new(self.pool())
            .usage_since(None, now - Duration::hours(24))
            .await?;
        let hour_floor = hour_of(now - Duration::hours(1));
        let mut entries = Vec::new();
        for p in self.list().await? {
            let rows: Vec<&UsageRow> = day.iter().filter(|r| r.plugin_id == p.id).collect();
            let last_hour =
                UsageTotals::from_rows(rows.iter().copied().filter(|r| r.hour >= hour_floor));
            let last_24h = UsageTotals::from_rows(rows.iter().copied());
            let circuit = self.governor().circuit_view(&p.id);
            let state = p.state.as_str().to_string();
            let health = if circuit.state != CircuitState::Closed {
                PluginHealth::CircuitOpen
            } else {
                match p.state {
                    PluginState::Enabled if last_hour.calls >= 10 && last_hour.error_rate > 0.2 => {
                        PluginHealth::Degraded
                    }
                    PluginState::Enabled => PluginHealth::Healthy,
                    PluginState::Error => PluginHealth::Error,
                    _ => PluginHealth::Disabled,
                }
            };
            let limit = self
                .active_limits_quota(&p.id, p.active_version.as_deref())
                .await;
            let revoked_versions = PluginRepository::new(self.pool())
                .list_versions(&p.id)
                .await?
                .into_iter()
                .filter(|v| v.revoked)
                .map(|v| v.version)
                .collect();
            entries.push(PluginGovernanceEntry {
                id: p.id,
                name: p.name,
                version: p.active_version,
                state,
                state_reason: p.state_reason,
                health,
                circuit,
                invocations_per_minute_limit: self.governor().effective_quota(limit),
                last_hour,
                last_24h,
                revoked_versions,
            });
        }
        let mut by_time: Vec<(&str, u64)> = day
            .iter()
            .fold(HashMap::<&str, u64>::new(), |mut m, r| {
                *m.entry(r.plugin_id.as_str()).or_default() += r.total_ms;
                m
            })
            .into_iter()
            .filter(|(_, ms)| *ms > 0)
            .collect();
        by_time.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let open_circuits = entries
            .iter()
            .filter(|e| e.circuit.state == CircuitState::Open)
            .count() as u32;
        Ok(GovernanceOverview {
            plugins: entries,
            top_consumers: by_time
                .iter()
                .take(5)
                .map(|(id, _)| id.to_string())
                .collect(),
            open_circuits,
            generated_at: now,
        })
    }

    async fn active_limits_quota(&self, id: &str, version: Option<&str>) -> Option<u32> {
        let v = PluginRepository::new(self.pool())
            .get_version(id, version?)
            .await
            .ok()??;
        serde_json::from_str::<PluginManifest>(&v.manifest_json)
            .ok()?
            .runtime
            .limits?
            .invocations_per_minute
    }

    pub async fn plugin_metrics(
        &self,
        id: &str,
        hours: u32,
    ) -> Result<PluginMetricsView, PluginError> {
        if PluginRepository::new(self.pool())
            .get_plugin(id)
            .await?
            .is_none()
        {
            return Err(PluginError::NotFound(format!("plugin '{}' not found", id)));
        }
        self.flush_usage().await?;
        let hours = hours.clamp(1, 24 * 30);
        let rows = PluginGovernanceRepository::new(self.pool())
            .usage_since(Some(id), Utc::now() - Duration::hours(hours as i64))
            .await?;
        let mut versions: Vec<&str> = rows.iter().map(|r| r.version.as_str()).collect();
        versions.sort();
        versions.dedup();
        let by_version = versions
            .into_iter()
            .map(|v| VersionUsage {
                version: v.to_string(),
                totals: UsageTotals::from_rows(rows.iter().filter(|r| r.version == v)),
            })
            .collect();
        let mut hour_keys: Vec<&str> = rows.iter().map(|r| r.hour.as_str()).collect();
        hour_keys.sort();
        hour_keys.dedup();
        let series = hour_keys
            .into_iter()
            .map(|h| {
                let t = UsageTotals::from_rows(rows.iter().filter(|r| r.hour == h));
                UsagePoint {
                    hour: h.to_string(),
                    calls: t.calls,
                    errors: t.errors,
                    traps: t.traps,
                    timeouts: t.timeouts,
                    quota_rejections: t.quota_rejections,
                    circuit_rejections: t.circuit_rejections,
                    p95_ms: t.p95_ms,
                }
            })
            .collect();
        Ok(PluginMetricsView {
            plugin_id: id.to_string(),
            hours,
            circuit: self.governor().circuit_view(id),
            totals: UsageTotals::from_rows(rows.iter()),
            by_version,
            series,
        })
    }

    /// Prometheus exposition of every plugin (no payload content, only counts).
    pub async fn prometheus(&self) -> Result<String, PluginError> {
        let plugins: Vec<(String, Option<String>, String)> = self
            .list()
            .await?
            .into_iter()
            .map(|p| (p.id, p.active_version, p.state.as_str().to_string()))
            .collect();
        let stats = PluginEventsRepository::new(self.pool()).stats().await?;
        Ok(self.governor().render_prometheus(&plugins, &stats))
    }
}

/// Starts the periodic flush, retention and breaker retry loop.
pub fn start_governance(manager: Arc<PluginManager>) {
    let secs = manager.governor().config().tick_secs.max(1);
    tokio::spawn(async move {
        manager.load_circuits().await;
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(secs));
        tick.tick().await;
        loop {
            tick.tick().await;
            manager.governance_tick().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PluginGovernanceConfig {
        PluginGovernanceConfig {
            max_invocations_per_minute: 3,
            breaker_failure_threshold: 3,
            breaker_window_secs: 60,
            breaker_backoff_base_secs: 10,
            breaker_backoff_max_secs: 25,
            breaker_close_after_successes: 2,
            ..Default::default()
        }
    }

    fn t(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + s, 0).unwrap()
    }

    fn fail(g: &Governor, now: DateTime<Utc>) -> Option<Transition> {
        g.record("p", "1.0.0", "event", Outcome::Error, 5, Some("boom"), now)
    }

    #[test]
    fn classifies_results() {
        assert_eq!(classify(&Ok(Value::Null)), Some(Outcome::Ok));
        assert_eq!(
            classify(&Err(InvokeError::Timeout(5))),
            Some(Outcome::Timeout)
        );
        assert_eq!(classify(&Err(InvokeError::Unavailable("x".into()))), None);
        assert_eq!(
            classify(&Err(InvokeError::Failed("fuel exhausted".into()))),
            Some(Outcome::Trap)
        );
        assert_eq!(
            classify(&Err(InvokeError::Failed("handler said no".into()))),
            Some(Outcome::Error)
        );
    }

    #[test]
    fn quota_is_per_minute_and_the_lower_value_applies() {
        let g = Governor::new(cfg());
        assert_eq!(g.effective_quota(Some(2)), Some(2));
        assert_eq!(g.effective_quota(Some(50)), Some(3));
        assert_eq!(g.effective_quota(None), Some(3));
        for i in 0..3 {
            assert!(g.admit("p", "1", "event", None, t(i)).is_ok());
        }
        assert_eq!(
            g.admit("p", "1", "event", None, t(5)),
            Err(Rejection::Quota(3))
        );
        // lifecycle hooks are exempt, and the window slides
        assert!(g.admit("p", "1", "lifecycle", None, t(5)).is_ok());
        assert!(g.admit("p", "1", "event", None, t(62)).is_ok());
        let rows = g.take_pending();
        assert_eq!(rows.iter().map(|r| r.quota_rejections).sum::<u64>(), 1);
    }

    #[test]
    fn breaker_opens_at_the_threshold_and_refuses_calls() {
        let g = Governor::new(cfg());
        assert!(fail(&g, t(0)).is_none());
        assert!(fail(&g, t(1)).is_none());
        let tr = fail(&g, t(2)).expect("third failure trips");
        assert_eq!(
            tr,
            Transition::Opened {
                trips: 1,
                retry_at: t(12),
                error: "boom".into()
            }
        );
        assert_eq!(g.circuit_view("p").state, CircuitState::Open);
        assert_eq!(
            g.admit("p", "1.0.0", "event", None, t(3)),
            Err(Rejection::Circuit(t(12)))
        );
        assert!(g.due_for_retry(t(11)).is_empty());
        assert_eq!(g.due_for_retry(t(12)), vec!["p".to_string()]);
    }

    #[test]
    fn failures_outside_the_window_do_not_count() {
        let g = Governor::new(cfg());
        fail(&g, t(0));
        fail(&g, t(1));
        assert!(fail(&g, t(100)).is_none());
        assert_eq!(g.circuit_view("p").state, CircuitState::Closed);
    }

    #[test]
    fn half_open_failure_reopens_with_a_longer_backoff_and_successes_close() {
        let g = Governor::new(cfg());
        for i in 0..3 {
            fail(&g, t(i));
        }
        g.mark_half_open("p");
        assert!(g.admit("p", "1.0.0", "event", None, t(20)).is_ok());
        let tr = fail(&g, t(20)).unwrap();
        // second trip: 10s * 2 = 20s
        assert_eq!(
            tr,
            Transition::Opened {
                trips: 2,
                retry_at: t(40),
                error: "boom".into()
            }
        );
        // third trip is capped at the maximum (25s)
        g.mark_half_open("p");
        match fail(&g, t(50)).unwrap() {
            Transition::Opened { retry_at, .. } => assert_eq!(retry_at, t(75)),
            other => panic!("{:?}", other),
        }
        g.mark_half_open("p");
        let ok = |s| g.record("p", "1.0.0", "event", Outcome::Ok, 1, None, t(s));
        assert!(ok(80).is_none());
        assert_eq!(ok(81), Some(Transition::Closed));
        assert_eq!(g.circuit_view("p").state, CircuitState::Closed);
        assert_eq!(g.circuit_view("p").trips, 0);
    }

    #[test]
    fn lifecycle_failures_and_unavailable_never_trip_the_breaker() {
        let g = Governor::new(cfg());
        for i in 0..10 {
            assert!(g
                .record("p", "1", "lifecycle", Outcome::Error, 1, Some("x"), t(i))
                .is_none());
        }
        assert_eq!(g.circuit_view("p").state, CircuitState::Closed);
    }

    #[test]
    fn reset_closes_and_persisted_state_is_restored() {
        let g = Governor::new(cfg());
        for i in 0..3 {
            fail(&g, t(i));
        }
        let rec = g.circuit_record("p").unwrap();
        assert_eq!(rec.state, CircuitState::Open);
        g.reset("p");
        assert_eq!(g.circuit_view("p").state, CircuitState::Closed);
        assert!(g.circuit_record("p").is_none());
        let other = Governor::new(cfg());
        other.load(&[rec]);
        assert_eq!(other.circuit_view("p").state, CircuitState::Open);
        assert_eq!(other.circuit_view("p").trips, 1);
    }

    #[test]
    fn usage_rolls_up_per_hour_with_a_latency_histogram() {
        let g = Governor::new(cfg());
        g.record("p", "1", "job", Outcome::Ok, 3, None, t(0));
        g.record("p", "1", "job", Outcome::Timeout, 9_000, Some("t"), t(1));
        let mut rows = g.take_pending();
        assert_eq!(rows.len(), 1);
        let r = rows.pop().unwrap();
        assert_eq!((r.calls, r.timeouts, r.max_ms), (2, 1, 9_000));
        assert_eq!(r.latency_buckets[0], 1);
        assert_eq!(r.latency_buckets[10], 1);
        assert!(g.take_pending().is_empty());
        g.requeue(vec![r]);
        assert_eq!(g.take_pending()[0].calls, 2);
    }

    #[test]
    fn prometheus_output_has_labels_escaped_and_all_families() {
        let g = Governor::new(cfg());
        g.record("p", "1", "event", Outcome::Ok, 3, None, t(0));
        g.record("p", "1", "event", Outcome::Error, 30, Some("x"), t(1));
        let text = g.render_prometheus(
            &[
                ("p".into(), Some("1".into()), "enabled".into()),
                ("q\"x".into(), None, "disabled".into()),
            ],
            &EventStats {
                pending_events: 1,
                pending_deliveries: 2,
                in_flight: 0,
                dead_letters: 4,
                delivered: 9,
                oldest_pending_age_secs: Some(7),
            },
        );
        assert!(text.contains(
            "openvox_plugin_invocations_total{plugin=\"p\",version=\"1\",kind=\"event\",outcome=\"error\"} 1"
        ));
        assert!(text.contains("outcome=\"ok\"} 1"));
        assert!(text.contains("openvox_plugin_invocation_duration_seconds_bucket{plugin=\"p\",version=\"1\",le=\"+Inf\"} 2"));
        assert!(text.contains("le=\"0.005\"} 1"));
        assert!(text.contains("openvox_plugin_circuit_state{plugin=\"q\\\"x\"} 0"));
        assert!(text.contains("openvox_plugin_events_pending 3"));
        assert!(text.contains("openvox_plugin_events_dead_letter 4"));
        assert!(text.contains("openvox_plugin_events_oldest_pending_seconds 7"));
    }

    // ---- manager level ------------------------------------------------------

    use crate::services::plugins::invoke::{HandlerCall, HandlerInvoker, HandlerKind};
    use crate::services::plugins::manager::BuiltinPlugin;
    use crate::services::plugins::package::build_signed_package;
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicBool, Ordering};

    const SEED: [u8; 32] = [3; 32];

    #[derive(Default)]
    struct Flaky {
        fail: AtomicBool,
    }

    impl BuiltinPlugin for Flaky {
        fn handle(&self, _call: &HandlerCall) -> anyhow::Result<Value> {
            if self.fail.load(Ordering::SeqCst) {
                anyhow::bail!("boom");
            }
            Ok(json!({"ok": true}))
        }
    }

    fn yaml(limits: &str) -> String {
        format!(
            "api_version: 2\nid: demo\nname: Demo\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {{ kind: builtin, entry: demo{limits} }}\n\
             capabilities: [events:subscribe]\n\
             events:\n  subscribe:\n    - {{ type: node.classified }}\n"
        )
    }

    struct Env {
        manager: Arc<PluginManager>,
        flaky: Arc<Flaky>,
        pool: sqlx::SqlitePool,
    }

    async fn env(gov: PluginGovernanceConfig, limits: &str) -> Env {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let flaky = Arc::new(Flaky::default());
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let dir = std::env::temp_dir().join(format!("ovx_m22_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let manager = Arc::new(
            PluginManager::new(pool.clone(), dir, 8)
                .with_builtin_keys(vec![("OpenVox".into(), pk)])
                .with_builtin_runtime("demo", flaky.clone())
                .with_governance(gov),
        );
        manager.reload().await.unwrap();
        let y = yaml(limits);
        let pkg = build_signed_package(&[("plugin.yaml", y.as_bytes())], "demo", "1.0.0", 2, SEED);
        manager.install(&pkg, "admin", None).await.unwrap();
        manager.enable("demo").await.unwrap();
        Env {
            manager,
            flaky,
            pool,
        }
    }

    fn gov(threshold: u32) -> PluginGovernanceConfig {
        PluginGovernanceConfig {
            breaker_failure_threshold: threshold,
            breaker_backoff_base_secs: 0,
            breaker_close_after_successes: 2,
            ..Default::default()
        }
    }

    async fn call(e: &Env) -> Result<Value, InvokeError> {
        let hooks = e.manager.hooks();
        let c = hooks.get("demo").expect("plugin is enabled").call(
            HandlerKind::Event,
            "on_event",
            json!({}),
        );
        e.manager.invoke(c).await
    }

    async fn state(e: &Env) -> (PluginState, Option<String>) {
        let p = PluginRepository::new(&e.pool)
            .get_plugin("demo")
            .await
            .unwrap()
            .unwrap();
        (p.state, p.state_reason)
    }

    #[tokio::test]
    async fn a_failing_plugin_is_isolated_and_the_trip_is_audited_and_published() {
        let e = env(gov(3), "").await;
        e.flaky.fail.store(true, Ordering::SeqCst);
        for _ in 0..3 {
            assert!(matches!(call(&e).await, Err(InvokeError::Failed(_))));
        }
        assert_eq!(
            state(&e).await,
            (PluginState::Error, Some(CIRCUIT_REASON.to_string()))
        );
        // out of play: no extension points left, and the persisted breaker
        assert!(e.manager.hooks().get("demo").is_none());
        let rec = PluginGovernanceRepository::new(&e.pool)
            .list_circuits()
            .await
            .unwrap();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].state, CircuitState::Open);
        let audited: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'plugin.circuit.opened' AND resource_id = 'demo'",
        )
        .fetch_one(&e.pool)
        .await
        .unwrap();
        assert_eq!(audited, 1);
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_outbox WHERE type = 'plugin.circuit.opened'",
        )
        .fetch_one(&e.pool)
        .await
        .unwrap();
        assert_eq!(events, 1);
        // a call that still arrives is refused without reaching the plugin
        let c = HandlerCall {
            plugin_id: "demo".into(),
            version: "1.0.0".into(),
            runtime: crate::models::RuntimeSpec {
                kind: crate::models::RuntimeKind::Builtin,
                entry: Some("demo".into()),
                endpoint: None,
                limits: None,
            },
            handler: "on_event".into(),
            kind: HandlerKind::Event,
            input: json!({}),
        };
        let r = e.manager.invoke(c).await;
        assert!(
            matches!(r, Err(InvokeError::Unavailable(m)) if m.contains("circuit breaker open"))
        );
    }

    #[tokio::test]
    async fn the_tripped_plugin_is_tried_again_and_closes_after_successes() {
        let e = env(gov(2), "").await;
        e.flaky.fail.store(true, Ordering::SeqCst);
        call(&e).await.unwrap_err();
        call(&e).await.unwrap_err();
        assert_eq!(state(&e).await.0, PluginState::Error);

        // half-open: back in play, one failure reopens it
        e.manager.governance_tick().await;
        assert_eq!(state(&e).await.0, PluginState::Enabled);
        assert_eq!(
            e.manager.governor().circuit_view("demo").state,
            CircuitState::HalfOpen
        );
        call(&e).await.unwrap_err();
        assert_eq!(state(&e).await.0, PluginState::Error);
        assert_eq!(e.manager.governor().circuit_view("demo").trips, 2);

        // half-open again: two successes close it and remove the stored row
        e.flaky.fail.store(false, Ordering::SeqCst);
        e.manager.governance_tick().await;
        call(&e).await.unwrap();
        call(&e).await.unwrap();
        assert_eq!(
            e.manager.governor().circuit_view("demo").state,
            CircuitState::Closed
        );
        assert!(PluginGovernanceRepository::new(&e.pool)
            .list_circuits()
            .await
            .unwrap()
            .is_empty());
        assert_eq!(state(&e).await, (PluginState::Enabled, None));
    }

    #[tokio::test]
    async fn an_administrator_can_reset_or_enable_a_tripped_plugin() {
        let e = env(gov(1), "").await;
        e.flaky.fail.store(true, Ordering::SeqCst);
        call(&e).await.unwrap_err();
        assert_eq!(state(&e).await.0, PluginState::Error);
        let view = e.manager.reset_circuit("demo").await.unwrap();
        assert_eq!(view.state, CircuitState::Closed);
        assert_eq!(state(&e).await, (PluginState::Enabled, None));

        call(&e).await.unwrap_err();
        assert_eq!(state(&e).await.0, PluginState::Error);
        e.manager.enable("demo").await.unwrap();
        assert_eq!(
            e.manager.governor().circuit_view("demo").state,
            CircuitState::Closed
        );
        assert!(e.manager.reset_circuit("nope").await.is_err());
    }

    #[tokio::test]
    async fn a_manifest_may_ask_for_a_lower_call_quota() {
        let e = env(
            PluginGovernanceConfig {
                max_invocations_per_minute: 100,
                ..gov(50)
            },
            ", limits: { invocations_per_minute: 2 }",
        )
        .await;
        call(&e).await.unwrap();
        call(&e).await.unwrap();
        let r = call(&e).await;
        assert!(matches!(r, Err(InvokeError::Unavailable(m)) if m.contains("quota exceeded")));
        // refusals are not failures: the plugin stays enabled
        assert_eq!(state(&e).await.0, PluginState::Enabled);
        let o = e.manager.governance_overview().await.unwrap();
        assert_eq!(o.plugins[0].invocations_per_minute_limit, Some(2));
        assert_eq!(o.plugins[0].last_hour.quota_rejections, 1);
    }

    #[tokio::test]
    async fn dashboard_metrics_and_prometheus_reflect_usage() {
        let e = env(gov(50), "").await;
        call(&e).await.unwrap();
        call(&e).await.unwrap();
        e.flaky.fail.store(true, Ordering::SeqCst);
        call(&e).await.unwrap_err();

        let o = e.manager.governance_overview().await.unwrap();
        assert_eq!(o.plugins.len(), 1);
        let p = &o.plugins[0];
        assert_eq!(p.id, "demo");
        assert_eq!(p.health, PluginHealth::Healthy);
        assert_eq!((p.last_24h.calls, p.last_24h.errors), (3, 1));
        assert!((p.last_24h.error_rate - 1.0 / 3.0).abs() < 1e-9);
        assert!(p.last_24h.p95_ms.is_some());

        let m = e.manager.plugin_metrics("demo", 24).await.unwrap();
        assert_eq!(m.totals.calls, 3);
        assert_eq!(m.by_version.len(), 1);
        assert_eq!(m.series.len(), 1);
        assert!(e.manager.plugin_metrics("nope", 24).await.is_err());

        // flushing twice does not double count
        e.manager.flush_usage().await.unwrap();
        assert_eq!(
            e.manager
                .plugin_metrics("demo", 24)
                .await
                .unwrap()
                .totals
                .calls,
            3
        );

        let text = e.manager.prometheus().await.unwrap();
        assert!(text.contains("outcome=\"error\"} 1"));
        assert!(text.contains(
            "openvox_plugin_info{plugin=\"demo\",version=\"1.0.0\",state=\"enabled\"} 1"
        ));
    }

    #[tokio::test]
    async fn uninstall_removes_usage_and_breaker_rows() {
        let e = env(gov(1), "").await;
        e.flaky.fail.store(true, Ordering::SeqCst);
        call(&e).await.unwrap_err();
        e.manager.flush_usage().await.unwrap();
        e.manager.uninstall("demo", false).await.unwrap();
        for n in [
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM plugin_usage_hourly"),
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM plugin_circuit"),
        ] {
            assert_eq!(n.fetch_one(&e.pool).await.unwrap(), 0);
        }
    }
}
