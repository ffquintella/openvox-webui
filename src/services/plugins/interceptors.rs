//! Restricted interceptors (M31, T104): synchronous before-hooks that may veto
//! or change a core operation, only on events of the host's catalog, with the
//! timeout, fail mode and changeable fields fixed by the event. The rules are
//! in docs/architecture/PLUGIN_PLATFORM.md §9.2 and §17.12.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::governance::CIRCUIT_REASON;
use super::invoke::{invoke_with_timeout, HandlerKind, Interceptor, InvokeError, PluginHooks};
use super::manager::{PluginError, PluginManager, AUDIT_PLUGINS};
use super::manifest::{valid_fact_identifier, RESERVED_FACT_NAMESPACES};
use crate::models::*;
use crate::utils::AppError;

/// Capability that lets a plugin intercept one event: `events:intercept:<event>`.
pub const CAPABILITY_PREFIX: &str = "events:intercept:";
/// Interceptors in play per event (§9.2).
pub const MAX_INTERCEPTORS_PER_EVENT: usize = 3;
/// Wall-clock limit of one interceptor call (§9.2).
pub const INTERCEPTOR_TIMEOUT_MS: u64 = 50;
/// Fuel ceiling of one WASM interceptor call (the plugin may ask for less).
pub const INTERCEPTOR_FUEL_PER_CALL: u64 = 5_000_000;
/// Largest request (event, payload) sent to an interceptor.
pub const MAX_INPUT_BYTES: usize = 512 * 1024;

pub const BEFORE_NODE_CLASSIFY: &str = "before.node.classify";
pub const BEFORE_GROUP_CHANGE: &str = "before.group.change";

const MAX_REASON_CHARS: usize = 200;
const MAX_ERROR_CHARS: usize = 300;
const MAX_CHANGES_BYTES: usize = 16 * 1024;
const MAX_ALTERED_FACTS: usize = 32;
/// Repeated decisions without a user (agent runs) are audited, and fail-open
/// warnings logged, at most once per this window.
const REPEAT_WINDOW: Duration = Duration::from_secs(300);
const MAX_THROTTLE_KEYS: usize = 10_000;

/// What a failing, slow or refused interceptor means for the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailMode {
    /// The interceptor is skipped and the operation goes ahead.
    Open,
    /// The operation is refused.
    Closed,
}

/// One event of the interceptable catalog. Versioned with the plugin API:
/// removing an event or changing its payload needs a major bump.
#[derive(Debug, Clone, Serialize)]
pub struct InterceptableEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    /// The operation, as the approval screen and the admin listing show it.
    pub description: &'static str,
    /// The payload the interceptor receives, for plugin authors.
    pub payload: &'static str,
    pub fail_mode: FailMode,
    pub timeout_ms: u64,
    pub max_interceptors: usize,
    pub can_veto: bool,
    /// Payload fields an interceptor may change; empty means veto only.
    pub alterable: &'static [&'static str],
}

static CATALOG: [InterceptableEvent; 2] = [
    InterceptableEvent {
        event_type: BEFORE_NODE_CLASSIFY,
        description: "a node is classified for its agent, the node page or a fact export \
                      (it receives the node's facts)",
        payload: "{certname, scope, environment, facts}; `facts` are the facts the classifier \
                  will use (PuppetDB facts plus the plugin facts enabled for rules)",
        // The ENC serves every agent run: a slow plugin must not stop it.
        fail_mode: FailMode::Open,
        timeout_ms: INTERCEPTOR_TIMEOUT_MS,
        max_interceptors: MAX_INTERCEPTORS_PER_EVENT,
        can_veto: true,
        alterable: &["facts"],
    },
    InterceptableEvent {
        event_type: BEFORE_GROUP_CHANGE,
        description: "a node group, its rules or its pinned nodes are changed through the API \
                      (it receives the change, including classes and variables)",
        payload: "{action, organization_id, group_id, group_name, actor, change}; `action` is \
                  create, update, delete, add_rule, delete_rule, pin or unpin and `change` the \
                  request (including classes and variables)",
        // A policy gate: without an answer the change does not happen.
        fail_mode: FailMode::Closed,
        timeout_ms: INTERCEPTOR_TIMEOUT_MS,
        max_interceptors: MAX_INTERCEPTORS_PER_EVENT,
        can_veto: true,
        alterable: &[],
    },
];

pub fn catalog() -> &'static [InterceptableEvent] {
    &CATALOG
}

pub fn find(event_type: &str) -> Option<&'static InterceptableEvent> {
    CATALOG.iter().find(|e| e.event_type == event_type)
}

pub fn capability_of(event: &InterceptableEvent) -> String {
    format!("{CAPABILITY_PREFIX}{}", event.event_type)
}

// ============================================================================
// Answers
// ============================================================================

/// `{"decision": "allow"}`, `{"decision": "veto", "reason": "..."}` or
/// `{"decision": "alter", "changes": {<alterable field>: ...}}`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    decision: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    changes: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    Veto(String),
    Alter(Map<String, Value>),
}

/// Parses and checks an answer against what the event allows. Anything else
/// is a failure of the interceptor, handled by the event's fail mode.
pub fn parse_answer(event: &InterceptableEvent, value: Value) -> Result<Decision, String> {
    let a: Answer = serde_json::from_value(value).map_err(|e| format!("invalid answer: {e}"))?;
    match a.decision.as_str() {
        "allow" if a.changes.is_none() => Ok(Decision::Allow),
        "allow" => Err("an allow answer carries no changes".into()),
        "veto" if !event.can_veto => Err(format!("{} cannot be vetoed", event.event_type)),
        "veto" => Ok(Decision::Veto(clean_text(
            a.reason.as_deref().unwrap_or("no reason given"),
            MAX_REASON_CHARS,
        ))),
        "alter" if event.alterable.is_empty() => Err(format!(
            "{} cannot be changed, only vetoed",
            event.event_type
        )),
        "alter" => {
            let Some(Value::Object(changes)) = a.changes else {
                return Err("an alter answer needs a changes object".into());
            };
            if changes.is_empty() {
                return Err("an alter answer needs at least one change".into());
            }
            if let Some(k) = changes
                .keys()
                .find(|k| !event.alterable.contains(&k.as_str()))
            {
                return Err(format!("'{k}' cannot be changed"));
            }
            if Value::Object(changes.clone()).to_string().len() > MAX_CHANGES_BYTES {
                return Err(format!(
                    "the changes are larger than {MAX_CHANGES_BYTES} bytes"
                ));
            }
            check_changes(event, &changes)?;
            Ok(Decision::Alter(changes))
        }
        other => Err(format!("unknown decision '{other}'")),
    }
}

/// Event-specific rules for the changed fields.
fn check_changes(event: &InterceptableEvent, changes: &Map<String, Value>) -> Result<(), String> {
    if event.event_type == BEFORE_NODE_CLASSIFY {
        let facts = changes
            .get("facts")
            .and_then(Value::as_object)
            .ok_or("facts must be an object of top-level fact names")?;
        if facts.is_empty() || facts.len() > MAX_ALTERED_FACTS {
            return Err(format!("change 1 to {MAX_ALTERED_FACTS} facts"));
        }
        for name in facts.keys() {
            if !valid_fact_identifier(name) {
                return Err(format!("'{name}' is not a valid top-level fact name"));
            }
            if RESERVED_FACT_NAMESPACES.contains(&name.as_str()) {
                return Err(format!("the fact '{name}' is protected"));
            }
        }
    }
    Ok(())
}

/// Applies one interceptor's changes to the payload the next one sees: an
/// object field is merged key by key (later wins), anything else replaced.
fn apply_changes(payload: &mut Value, changes: &Map<String, Value>) {
    let Some(root) = payload.as_object_mut() else {
        return;
    };
    for (field, change) in changes {
        match (root.get_mut(field), change) {
            (Some(Value::Object(current)), Value::Object(new)) => {
                for (k, v) in new {
                    current.insert(k.clone(), v.clone());
                }
            }
            _ => {
                root.insert(field.clone(), change.clone());
            }
        }
    }
}

/// Top-level facts changed by `before.node.classify` interceptors.
pub fn apply_fact_changes(facts: &mut Value, changed: &Map<String, Value>) {
    if let Some(root) = facts.as_object_mut() {
        for (k, v) in changed {
            root.insert(k.clone(), v.clone());
        }
    }
}

/// One line of printable text, without markup characters, at most `max` chars.
fn clean_text(s: &str, max: usize) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_control() {
                ' '
            } else if c == '<' || c == '>' {
                '_'
            } else {
                c
            }
        })
        .take(max)
        .collect();
    let t = cleaned.trim();
    if t.is_empty() {
        "no reason given".into()
    } else {
        t.to_string()
    }
}

// ============================================================================
// Outcome of an interception
// ============================================================================

/// The operation may go ahead, with the payload changes to apply.
#[derive(Debug, Default)]
pub struct Interception {
    /// Field -> accumulated changes of every interceptor that altered it.
    pub changes: Map<String, Value>,
    /// Plugins whose changes were applied, in run order.
    pub altered_by: Vec<String>,
    /// Interceptors skipped because they failed on a fail-open event.
    pub degraded: Vec<String>,
}

/// The operation must not happen.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum InterceptRefusal {
    #[error("{event} was vetoed by the plugin '{plugin_id}': {reason}")]
    Vetoed {
        event: String,
        plugin_id: String,
        reason: String,
    },
    /// A fail-closed event did not get a valid answer in time. The plugin's
    /// error goes to the audit log and the telemetry, not to the caller.
    #[error("{event} was refused: the interceptor of the plugin '{plugin_id}' did not give a valid answer in time")]
    Unavailable {
        event: String,
        plugin_id: String,
        error: String,
    },
}

impl InterceptRefusal {
    /// A veto is a conflict with a policy (409); a missing answer of a
    /// fail-closed event is a temporary condition (503).
    pub fn to_app_error(&self) -> AppError {
        match self {
            InterceptRefusal::Vetoed { .. } => AppError::conflict(self.to_string()),
            InterceptRefusal::Unavailable { .. } => AppError::service_unavailable(self.to_string()),
        }
    }
}

/// Who is behind the intercepted operation, for the audit entries.
#[derive(Debug, Clone, Copy)]
pub struct InterceptActor {
    pub organization_id: Uuid,
    /// `None` for operations without a user (agent runs).
    pub user_id: Option<Uuid>,
}

impl InterceptActor {
    pub fn system() -> Self {
        Self {
            organization_id: default_organization_uuid(),
            user_id: None,
        }
    }
}

// ============================================================================
// Telemetry
// ============================================================================

/// Counters of one interceptor (event, plugin) since the process started.
#[derive(Debug, Clone, Default, Serialize)]
pub struct InterceptorStats {
    /// Calls that reached the plugin (or were refused by the governor).
    pub calls: u64,
    pub allowed: u64,
    pub altered: u64,
    pub vetoed: u64,
    /// Errors, timeouts, refusals by the governor and invalid answers.
    pub failures: u64,
    pub timeouts: u64,
    /// Not called: the filter did not match or the capability is not granted.
    pub skipped: u64,
    /// Failures after which the operation went ahead (fail-open events).
    pub failed_open: u64,
    /// Failures that refused the operation (fail-closed events).
    pub failed_closed: u64,
    pub total_ms: u64,
    pub max_ms: u64,
    pub last_error: Option<String>,
    pub last_called_at: Option<DateTime<Utc>>,
}

enum Recorded<'a> {
    Skipped,
    Allowed,
    Altered,
    Vetoed,
    Failed {
        error: &'a str,
        timeout: bool,
        mode: FailMode,
    },
}

/// In-memory telemetry and audit throttle of the interceptors.
#[derive(Default)]
pub struct InterceptorRuntime {
    stats: Mutex<BTreeMap<(String, String), InterceptorStats>>,
    throttle: Mutex<HashMap<String, Instant>>,
}

impl InterceptorRuntime {
    fn record(&self, event: &str, plugin: &str, what: Recorded<'_>, ms: u64) {
        let mut all = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let s = all
            .entry((event.to_string(), plugin.to_string()))
            .or_default();
        if matches!(what, Recorded::Skipped) {
            s.skipped += 1;
            return;
        }
        s.calls += 1;
        s.total_ms += ms;
        s.max_ms = s.max_ms.max(ms);
        s.last_called_at = Some(Utc::now());
        match what {
            Recorded::Skipped => {}
            Recorded::Allowed => s.allowed += 1,
            Recorded::Altered => s.altered += 1,
            Recorded::Vetoed => s.vetoed += 1,
            Recorded::Failed {
                error,
                timeout,
                mode,
            } => {
                s.failures += 1;
                if timeout {
                    s.timeouts += 1;
                }
                match mode {
                    FailMode::Open => s.failed_open += 1,
                    FailMode::Closed => s.failed_closed += 1,
                }
                s.last_error = Some(clean_text(error, MAX_ERROR_CHARS));
            }
        }
    }

    pub fn stats(&self, event: &str, plugin: &str) -> InterceptorStats {
        self.stats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(event.to_string(), plugin.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    /// Drops the counters of a plugin (uninstall).
    pub fn forget(&self, plugin: &str) {
        self.stats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(_, p), _| p != plugin);
    }

    /// True once per window for a key.
    fn first_in_window(&self, key: &str) -> bool {
        let mut t = self.throttle.lock().unwrap_or_else(|e| e.into_inner());
        if t.len() >= MAX_THROTTLE_KEYS {
            t.retain(|_, at| at.elapsed() < REPEAT_WINDOW);
        }
        if t.get(key).is_some_and(|at| at.elapsed() < REPEAT_WINDOW) {
            return false;
        }
        t.insert(key.to_string(), Instant::now());
        true
    }

    /// Prometheus exposition of the dedicated interceptor counters.
    pub fn render_prometheus(&self) -> String {
        let all = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = String::new();
        out.push_str(
            "# HELP openvox_plugin_interceptor_decisions_total Interceptor outcomes by event and plugin.\n\
             # TYPE openvox_plugin_interceptor_decisions_total counter\n",
        );
        for ((event, plugin), s) in all.iter() {
            for (decision, n) in [
                ("allow", s.allowed),
                ("alter", s.altered),
                ("veto", s.vetoed),
                ("failure", s.failures),
                ("skipped", s.skipped),
            ] {
                let _ = writeln!(
                    out,
                    "openvox_plugin_interceptor_decisions_total{{event=\"{}\",plugin=\"{}\",decision=\"{}\"}} {}",
                    esc(event), esc(plugin), decision, n
                );
            }
        }
        out.push_str(
            "# HELP openvox_plugin_interceptor_fail_mode_total Failures resolved by the event's fail mode.\n\
             # TYPE openvox_plugin_interceptor_fail_mode_total counter\n",
        );
        for ((event, plugin), s) in all.iter() {
            for (mode, n) in [("open", s.failed_open), ("closed", s.failed_closed)] {
                let _ = writeln!(
                    out,
                    "openvox_plugin_interceptor_fail_mode_total{{event=\"{}\",plugin=\"{}\",mode=\"{}\"}} {}",
                    esc(event), esc(plugin), mode, n
                );
            }
        }
        out.push_str(
            "# HELP openvox_plugin_interceptor_duration_seconds Time spent in interceptors.\n\
             # TYPE openvox_plugin_interceptor_duration_seconds summary\n",
        );
        for ((event, plugin), s) in all.iter() {
            let _ = writeln!(
                out,
                "openvox_plugin_interceptor_duration_seconds_sum{{event=\"{}\",plugin=\"{}\"}} {}\n\
                 openvox_plugin_interceptor_duration_seconds_count{{event=\"{}\",plugin=\"{}\"}} {}",
                esc(event),
                esc(plugin),
                s.total_ms as f64 / 1000.0,
                esc(event),
                esc(plugin),
                s.calls
            );
        }
        out
    }
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

// ============================================================================
// Administrator listing
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct InterceptorView {
    pub plugin_id: String,
    pub version: String,
    pub priority: i32,
    pub handler: String,
    pub filter: Option<String>,
    /// 1-based place in the chain.
    pub position: usize,
    /// Within the event's maximum (only these run).
    pub active: bool,
    /// `events:intercept:<event>` approved and not denied by policy.
    pub granted: bool,
    pub stats: InterceptorStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterceptableEventView {
    #[serde(flatten)]
    pub event: InterceptableEvent,
    pub capability: String,
    pub interceptors: Vec<InterceptorView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterceptorsOverview {
    pub events: Vec<InterceptableEventView>,
}

// ============================================================================
// Dispatcher
// ============================================================================

struct Failure {
    error: String,
    timeout: bool,
}

impl PluginManager {
    /// True when an enabled plugin intercepts the event, so callers build the
    /// payload only when someone will read it.
    pub fn has_interceptors(&self, event_type: &str) -> bool {
        self.hooks()
            .plugins()
            .any(|p| p.interceptors.iter().any(|i| i.event_type == event_type))
    }

    /// Runs the interceptors of `event_type` in order on `payload`; each sees
    /// the payload as changed by the previous ones. A veto stops the chain; a
    /// failure is skipped (fail open) or stops it (fail closed), as the event
    /// defines. `subject` (certname, group id) goes to the audit entries.
    pub async fn intercept(
        &self,
        event_type: &str,
        subject: &str,
        actor: InterceptActor,
        mut payload: Value,
    ) -> Result<Interception, InterceptRefusal> {
        let mut out = Interception::default();
        let Some(event) = find(event_type) else {
            tracing::error!("'{}' is not an interceptable event", event_type);
            return Ok(out);
        };
        let hooks = self.hooks();
        let chain = hooks.interceptors_for(event.event_type);
        let rt = self.interceptor_runtime();
        for (plugin, icpt) in chain.into_iter().take(event.max_interceptors) {
            let pid = plugin.plugin_id.as_str();
            if icpt.filter.as_ref().is_some_and(|f| !f.matches(&payload))
                || !self.interceptor_granted(plugin, event).await
            {
                rt.record(event.event_type, pid, Recorded::Skipped, 0);
                continue;
            }
            let started = Instant::now();
            let result = self.call_interceptor(event, plugin, icpt, &payload).await;
            let ms = started.elapsed().as_millis() as u64;
            match result {
                Ok(Decision::Allow) => rt.record(event.event_type, pid, Recorded::Allowed, ms),
                Ok(Decision::Alter(changes)) => {
                    rt.record(event.event_type, pid, Recorded::Altered, ms);
                    apply_changes(&mut payload, &changes);
                    let mut acc = Value::Object(std::mem::take(&mut out.changes));
                    apply_changes(&mut acc, &changes);
                    if let Value::Object(m) = acc {
                        out.changes = m;
                    }
                    out.altered_by.push(pid.to_string());
                    tracing::debug!(
                        "{} on {}: {} changed {:?}",
                        event.event_type,
                        subject,
                        pid,
                        changes.keys().collect::<Vec<_>>()
                    );
                }
                Ok(Decision::Veto(reason)) => {
                    rt.record(event.event_type, pid, Recorded::Vetoed, ms);
                    self.audit_interception(
                        "plugin.interceptor.vetoed",
                        event,
                        plugin,
                        subject,
                        actor,
                        json!({"reason": reason}),
                    )
                    .await;
                    return Err(InterceptRefusal::Vetoed {
                        event: event.event_type.to_string(),
                        plugin_id: pid.to_string(),
                        reason,
                    });
                }
                Err(f) => {
                    rt.record(
                        event.event_type,
                        pid,
                        Recorded::Failed {
                            error: &f.error,
                            timeout: f.timeout,
                            mode: event.fail_mode,
                        },
                        ms,
                    );
                    if event.fail_mode == FailMode::Closed {
                        self.audit_interception(
                            "plugin.interceptor.refused",
                            event,
                            plugin,
                            subject,
                            actor,
                            json!({"error": clean_text(&f.error, MAX_ERROR_CHARS)}),
                        )
                        .await;
                        return Err(InterceptRefusal::Unavailable {
                            event: event.event_type.to_string(),
                            plugin_id: pid.to_string(),
                            error: f.error,
                        });
                    }
                    if rt.first_in_window(&format!("warn\0{}\0{}", event.event_type, pid)) {
                        tracing::warn!(
                            "interceptor of {} on {} failed and was skipped (fail open): {}",
                            pid,
                            event.event_type,
                            f.error
                        );
                    }
                    out.degraded.push(format!("{pid}: {}", f.error));
                }
            }
        }
        Ok(out)
    }

    /// The policy ceiling applies to every runtime (`events:intercept:*` in
    /// `plugins.policy.denied_capabilities` turns all interceptors off);
    /// other runtimes than builtin also need the administrator's approval,
    /// re-read on every call so a change applies at once.
    async fn interceptor_granted(&self, plugin: &PluginHooks, event: &InterceptableEvent) -> bool {
        let cap = capability_of(event);
        if self.policy().denies(&cap) {
            return false;
        }
        if plugin.runtime.kind == RuntimeKind::Builtin {
            return true;
        }
        self.repo()
            .list_grants(&plugin.plugin_id)
            .await
            .map(|g| g.iter().any(|g| g.capability == cap && g.scope.is_none()))
            .unwrap_or(false)
    }

    async fn call_interceptor(
        &self,
        event: &InterceptableEvent,
        plugin: &PluginHooks,
        icpt: &Interceptor,
        payload: &Value,
    ) -> Result<Decision, Failure> {
        let input = json!({
            "type": event.event_type,
            "schema_version": EVENT_SCHEMA_VERSION,
            "fail_mode": event.fail_mode,
            "payload": payload,
        });
        if input.to_string().len() > MAX_INPUT_BYTES {
            return Err(Failure {
                error: format!("the payload is larger than {MAX_INPUT_BYTES} bytes"),
                timeout: false,
            });
        }
        let mut call = plugin.call(HandlerKind::Interceptor, &icpt.handler, input);
        let limits = call.runtime.limits.get_or_insert_with(Default::default);
        limits.timeout_ms = Some(event.timeout_ms);
        limits.fuel_per_call = Some(
            limits
                .fuel_per_call
                .map_or(INTERCEPTOR_FUEL_PER_CALL, |f| {
                    f.min(INTERCEPTOR_FUEL_PER_CALL)
                })
                .max(1),
        );
        let started = Instant::now();
        let result = invoke_with_timeout(self, call, event.timeout_ms).await;
        // A late answer is discarded: built-in handlers cannot be preempted
        // and the WASM wait has a short grace period after the deadline.
        if result.is_ok() && started.elapsed() > Duration::from_millis(event.timeout_ms) {
            return Err(Failure {
                error: format!("answered after the {} ms limit", event.timeout_ms),
                timeout: true,
            });
        }
        match result {
            Ok(v) => parse_answer(event, v).map_err(|error| Failure {
                error,
                timeout: false,
            }),
            Err(e) => Err(Failure {
                timeout: matches!(e, InvokeError::Timeout(_)),
                error: e.to_string(),
            }),
        }
    }

    /// Vetoes and fail-closed refusals are audited. Without a user (agent
    /// runs repeat every few minutes) the same decision about the same
    /// subject is audited once per window.
    async fn audit_interception(
        &self,
        action: &str,
        event: &InterceptableEvent,
        plugin: &PluginHooks,
        subject: &str,
        actor: InterceptActor,
        extra: Value,
    ) {
        if actor.user_id.is_none()
            && !self.interceptor_runtime().first_in_window(&format!(
                "{action}\0{}\0{}\0{subject}",
                event.event_type, plugin.plugin_id
            ))
        {
            return;
        }
        let mut details = json!({
            "event": event.event_type,
            "version": plugin.version,
            "subject": subject,
            "fail_mode": event.fail_mode,
        });
        if let (Some(d), Value::Object(e)) = (details.as_object_mut(), extra) {
            d.extend(e);
        }
        self.audit(
            actor.organization_id,
            actor.user_id,
            AUDIT_PLUGINS,
            action,
            &plugin.plugin_id,
            details,
        )
        .await;
    }

    /// Refuses to put a plugin in play when one of its interceptors would
    /// exceed the event's maximum. Plugins isolated by the circuit breaker
    /// keep their place, because they come back on their own.
    pub(super) async fn check_interceptor_limit(
        &self,
        m: &PluginManifest,
    ) -> Result<(), PluginError> {
        let mine = m.events_decl().unwrap_or_default().intercept;
        if mine.is_empty() {
            return Ok(());
        }
        let mut taken: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for p in self.repo().list_plugins().await? {
            let in_play = p.state == PluginState::Enabled
                || (p.state == PluginState::Error
                    && p.state_reason.as_deref() == Some(CIRCUIT_REASON));
            if p.id == m.id || !in_play {
                continue;
            }
            let Some(v) = &p.active_version else {
                continue;
            };
            let Ok(other) = self.manifest_of(&p.id, v).await else {
                continue;
            };
            for i in other.events_decl().unwrap_or_default().intercept {
                taken.entry(i.event_type).or_default().push(p.id.clone());
            }
        }
        for i in &mine {
            let Some(event) = find(&i.event_type) else {
                continue;
            };
            let others = taken.get(event.event_type).cloned().unwrap_or_default();
            if others.len() >= event.max_interceptors {
                return Err(PluginError::Conflict {
                    code: "interceptor_limit",
                    message: format!(
                        "'{}' already has {} interceptors ({}), the maximum; disable one of them before enabling {} {}",
                        event.event_type,
                        others.len(),
                        others.join(", "),
                        m.id,
                        m.version
                    ),
                });
            }
        }
        Ok(())
    }

    /// The interceptable catalog with the interceptors of the enabled
    /// plugins in run order, their place, grant and telemetry.
    pub async fn interceptors_overview(&self) -> InterceptorsOverview {
        let hooks = self.hooks();
        let rt = self.interceptor_runtime();
        let mut events = Vec::new();
        for event in catalog() {
            let mut interceptors = Vec::new();
            for (n, (plugin, icpt)) in hooks
                .interceptors_for(event.event_type)
                .into_iter()
                .enumerate()
            {
                interceptors.push(InterceptorView {
                    plugin_id: plugin.plugin_id.clone(),
                    version: plugin.version.clone(),
                    priority: icpt.priority,
                    handler: icpt.handler.clone(),
                    filter: icpt.filter_src.clone(),
                    position: n + 1,
                    active: n < event.max_interceptors,
                    granted: self.interceptor_granted(plugin, event).await,
                    stats: rt.stats(event.event_type, &plugin.plugin_id),
                });
            }
            events.push(InterceptableEventView {
                event: event.clone(),
                capability: capability_of(event),
                interceptors,
            });
        }
        InterceptorsOverview { events }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PluginRepository;
    use crate::services::plugins::invoke::HandlerCall;
    use crate::services::plugins::package::build_signed_package;
    use crate::services::plugins::BuiltinPlugin;
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::Arc;

    const SEED: [u8; 32] = [21; 32];

    fn classify_event() -> &'static InterceptableEvent {
        find(BEFORE_NODE_CLASSIFY).unwrap()
    }

    fn group_event() -> &'static InterceptableEvent {
        find(BEFORE_GROUP_CHANGE).unwrap()
    }

    #[test]
    fn the_catalog_fixes_fail_mode_timeout_and_maximum() {
        let c = classify_event();
        assert_eq!(c.fail_mode, FailMode::Open);
        assert_eq!(c.timeout_ms, 50);
        assert_eq!(c.max_interceptors, 3);
        assert_eq!(c.alterable, &["facts"]);
        let g = group_event();
        assert_eq!(g.fail_mode, FailMode::Closed);
        assert!(g.alterable.is_empty());
        assert!(find("node.classified").is_none());
        for e in catalog() {
            assert!(super::super::filter::valid_event_type(e.event_type));
        }
    }

    #[test]
    fn answers_are_checked_against_what_the_event_allows() {
        let c = classify_event();
        assert_eq!(
            parse_answer(c, json!({"decision": "allow"})),
            Ok(Decision::Allow)
        );
        assert_eq!(
            parse_answer(
                c,
                json!({"decision": "veto", "reason": " quarantined\n<b> "})
            ),
            Ok(Decision::Veto("quarantined _b_".into()))
        );
        let long = "x".repeat(500);
        match parse_answer(c, json!({"decision": "veto", "reason": long})) {
            Ok(Decision::Veto(r)) => assert_eq!(r.chars().count(), MAX_REASON_CHARS),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse_answer(
                c,
                json!({"decision": "alter", "changes": {"facts": {"role": "web"}}})
            ),
            Ok(Decision::Alter(_))
        ));
        for bad in [
            json!(null),
            json!({}),
            json!({"decision": "maybe"}),
            json!({"decision": "allow", "extra": 1}),
            json!({"decision": "allow", "changes": {"facts": {"a": 1}}}),
            json!({"decision": "alter"}),
            json!({"decision": "alter", "changes": {}}),
            json!({"decision": "alter", "changes": {"certname": "x"}}),
            json!({"decision": "alter", "changes": {"facts": {"trusted": {}}}}),
            json!({"decision": "alter", "changes": {"facts": {"certname": "other"}}}),
            json!({"decision": "alter", "changes": {"facts": {"Bad-Name": 1}}}),
            json!({"decision": "alter", "changes": {"facts": []}}),
            json!({"decision": "alter", "changes": {"facts": {"big": "x".repeat(20_000)}}}),
        ] {
            assert!(parse_answer(c, bad.clone()).is_err(), "{bad}");
        }
        let too_many: Map<String, Value> = (0..33).map(|i| (format!("f{i}"), json!(i))).collect();
        assert!(parse_answer(
            c,
            json!({"decision": "alter", "changes": {"facts": too_many}})
        )
        .is_err());
        // A veto-only event refuses changes.
        assert!(parse_answer(
            group_event(),
            json!({"decision": "alter", "changes": {"change": {}}})
        )
        .is_err());
    }

    #[test]
    fn changes_merge_object_fields_and_accumulate() {
        let mut p = json!({"certname": "n", "facts": {"role": "db", "os": "linux"}});
        let mut ch = Map::new();
        ch.insert("facts".into(), json!({"role": "web", "tier": 1}));
        apply_changes(&mut p, &ch);
        assert_eq!(p["facts"], json!({"role": "web", "os": "linux", "tier": 1}));
        assert_eq!(p["certname"], "n");
        let mut facts = json!({"role": "db"});
        apply_fact_changes(&mut facts, ch["facts"].as_object().unwrap());
        assert_eq!(facts, json!({"role": "web", "tier": 1}));
    }

    #[test]
    fn telemetry_counts_decisions_and_renders_prometheus() {
        let rt = InterceptorRuntime::default();
        rt.record("e", "p", Recorded::Allowed, 3);
        rt.record("e", "p", Recorded::Vetoed, 5);
        rt.record(
            "e",
            "p",
            Recorded::Failed {
                error: "handler timed out after 50 ms",
                timeout: true,
                mode: FailMode::Open,
            },
            50,
        );
        rt.record("e", "p", Recorded::Skipped, 0);
        let s = rt.stats("e", "p");
        assert_eq!(
            (s.calls, s.allowed, s.vetoed, s.failures, s.timeouts, s.skipped),
            (3, 1, 1, 1, 1, 1)
        );
        assert_eq!(
            (s.failed_open, s.failed_closed, s.max_ms, s.total_ms),
            (1, 0, 50, 58)
        );
        let text = rt.render_prometheus();
        assert!(text.contains(
            "openvox_plugin_interceptor_decisions_total{event=\"e\",plugin=\"p\",decision=\"veto\"} 1"
        ));
        assert!(text.contains(
            "openvox_plugin_interceptor_fail_mode_total{event=\"e\",plugin=\"p\",mode=\"open\"} 1"
        ));
        assert!(text.contains(
            "openvox_plugin_interceptor_duration_seconds_count{event=\"e\",plugin=\"p\"} 3"
        ));
        rt.forget("p");
        assert_eq!(rt.stats("e", "p").calls, 0);
    }

    // ---- dispatcher with built-in plugins ---------------------------------

    /// Answers by handler name; `sleepy` exceeds the 50 ms limit.
    #[derive(Default)]
    struct Icpt {
        calls: Mutex<Vec<(String, String, Value)>>,
    }

    impl BuiltinPlugin for Icpt {
        fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
            assert_eq!(call.kind, HandlerKind::Interceptor);
            assert_eq!(call.runtime.limits.as_ref().unwrap().timeout_ms, Some(50));
            self.calls.lock().unwrap().push((
                call.plugin_id.clone(),
                call.handler.clone(),
                call.input.clone(),
            ));
            let facts = &call.input["payload"]["facts"];
            Ok(match call.handler.as_str() {
                "allow" => json!({"decision": "allow"}),
                "veto_db" if facts["role"] == "db" => {
                    json!({"decision": "veto", "reason": "databases are frozen"})
                }
                "veto_db" => json!({"decision": "allow"}),
                "tier" => {
                    json!({"decision": "alter", "changes": {"facts": {"tier": "gold", "role": "web"}}})
                }
                "seen" => {
                    json!({"decision": "alter", "changes": {"facts": {"saw_tier": facts["tier"].clone()}}})
                }
                "garbage" => json!({"verdict": "yes"}),
                "fail" => anyhow::bail!("boom"),
                "sleepy" => {
                    std::thread::sleep(std::time::Duration::from_millis(120));
                    json!({"decision": "allow"})
                }
                other => anyhow::bail!("unknown handler {other}"),
            })
        }
    }

    pub(crate) fn manifest(id: &str, event: &str, priority: i32, handler: &str) -> String {
        format!(
            "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {{ kind: builtin, entry: {id} }}\ncapabilities:\n  - events:intercept:{event}\n\
             events:\n  intercept:\n    - {{ type: {event}, priority: {priority}, handler: {handler} }}\n"
        )
    }

    struct Env {
        manager: Arc<PluginManager>,
        plugin: Arc<Icpt>,
        _dir: std::path::PathBuf,
    }

    async fn env_with(ids: &[&str], policy: Option<crate::config::PluginPolicyConfig>) -> Env {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ovx_icpt_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let plugin = Arc::new(Icpt::default());
        let mut m = PluginManager::new(pool, dir.clone(), 8)
            .with_builtin_keys(vec![("OpenVox".into(), pk)]);
        for id in ids {
            m = m.with_builtin_runtime(id, plugin.clone());
        }
        if let Some(p) = policy {
            m = m.with_policy(p);
        }
        let manager = Arc::new(m);
        manager.reload().await.unwrap();
        Env {
            manager,
            plugin,
            _dir: dir,
        }
    }

    async fn env(ids: &[&str]) -> Env {
        env_with(ids, None).await
    }

    async fn install(m: &PluginManager, id: &str, yaml: &str) {
        let pkg = build_signed_package(&[("plugin.yaml", yaml.as_bytes())], id, "1.0.0", 2, SEED);
        m.install(&pkg, "admin", None).await.unwrap();
    }

    async fn add(m: &PluginManager, id: &str, event: &str, priority: i32, handler: &str) {
        install(m, id, &manifest(id, event, priority, handler)).await;
        m.enable(id).await.unwrap();
    }

    fn payload(role: &str) -> Value {
        json!({"certname": "n1", "scope": "all", "environment": null, "facts": {"role": role}})
    }

    async fn audit_actions(m: &PluginManager) -> Vec<String> {
        sqlx::query_scalar("SELECT action FROM audit_log WHERE action LIKE 'plugin.interceptor.%'")
            .fetch_all(m.pool())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn interceptors_run_in_priority_order_and_see_earlier_changes() {
        let e = env(&["p-seen", "p-tier", "p-allow"]).await;
        let m = &e.manager;
        assert!(!m.has_interceptors(BEFORE_NODE_CLASSIFY));
        add(m, "p-seen", BEFORE_NODE_CLASSIFY, 20, "seen").await;
        add(m, "p-tier", BEFORE_NODE_CLASSIFY, 10, "tier").await;
        add(m, "p-allow", BEFORE_NODE_CLASSIFY, 20, "allow").await;
        assert!(m.has_interceptors(BEFORE_NODE_CLASSIFY));
        assert!(!m.has_interceptors(BEFORE_GROUP_CHANGE));

        let out = m
            .intercept(
                BEFORE_NODE_CLASSIFY,
                "n1",
                InterceptActor::system(),
                payload("db"),
            )
            .await
            .unwrap();
        let order: Vec<String> = e
            .plugin
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.0.clone())
            .collect();
        // priority 10 first, then the tie at 20 by plugin id
        assert_eq!(order, ["p-tier", "p-allow", "p-seen"]);
        assert_eq!(
            out.changes["facts"],
            json!({"tier": "gold", "role": "web", "saw_tier": "gold"})
        );
        assert_eq!(out.altered_by, ["p-tier", "p-seen"]);
        // The interceptor receives the event, the schema and the fail mode.
        let input = e.plugin.calls.lock().unwrap()[0].2.clone();
        assert_eq!(input["type"], BEFORE_NODE_CLASSIFY);
        assert_eq!(input["fail_mode"], "open");
        assert_eq!(input["payload"]["certname"], "n1");
        // Calls went through the governor with their own kind.
        let usage = m.governor().take_pending();
        assert!(usage.iter().any(|u| u.kind == "interceptor" && u.calls > 0));

        let view = m.interceptors_overview().await;
        let ev = view
            .events
            .iter()
            .find(|v| v.event.event_type == BEFORE_NODE_CLASSIFY)
            .unwrap();
        assert_eq!(ev.interceptors.len(), 3);
        assert!(ev.interceptors.iter().all(|i| i.active && i.granted));
        assert_eq!(ev.interceptors[0].plugin_id, "p-tier");
        assert_eq!(ev.interceptors[0].stats.altered, 1);
    }

    #[tokio::test]
    async fn a_veto_stops_the_chain_and_is_audited_once_per_window_without_a_user() {
        let e = env(&["p-veto", "p-later"]).await;
        let m = &e.manager;
        add(m, "p-veto", BEFORE_NODE_CLASSIFY, 1, "veto_db").await;
        add(m, "p-later", BEFORE_NODE_CLASSIFY, 2, "allow").await;
        for _ in 0..3 {
            let err = m
                .intercept(
                    BEFORE_NODE_CLASSIFY,
                    "n1",
                    InterceptActor::system(),
                    payload("db"),
                )
                .await
                .unwrap_err();
            assert_eq!(
                err,
                InterceptRefusal::Vetoed {
                    event: BEFORE_NODE_CLASSIFY.into(),
                    plugin_id: "p-veto".into(),
                    reason: "databases are frozen".into(),
                }
            );
        }
        assert!(e
            .plugin
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|c| c.0 == "p-veto"));
        assert_eq!(audit_actions(m).await, ["plugin.interceptor.vetoed"]);
        // Another node passes.
        assert!(m
            .intercept(
                BEFORE_NODE_CLASSIFY,
                "n2",
                InterceptActor::system(),
                payload("web")
            )
            .await
            .is_ok());
        assert_eq!(
            m.interceptor_runtime()
                .stats(BEFORE_NODE_CLASSIFY, "p-veto")
                .vetoed,
            3
        );
    }

    #[tokio::test]
    async fn failures_follow_the_fail_mode_of_the_event() {
        let e = env(&["p-fail", "p-garbage", "p-slow", "g-fail", "g-garbage"]).await;
        let m = &e.manager;
        // before.node.classify fails open: errors, invalid answers and
        // timeouts are skipped and the operation goes ahead.
        add(m, "p-fail", BEFORE_NODE_CLASSIFY, 1, "fail").await;
        add(m, "p-garbage", BEFORE_NODE_CLASSIFY, 2, "garbage").await;
        add(m, "p-slow", BEFORE_NODE_CLASSIFY, 3, "sleepy").await;
        let out = m
            .intercept(
                BEFORE_NODE_CLASSIFY,
                "n1",
                InterceptActor::system(),
                payload("db"),
            )
            .await
            .unwrap();
        assert_eq!(out.degraded.len(), 3, "{:?}", out.degraded);
        assert!(out.changes.is_empty());
        let slow = m
            .interceptor_runtime()
            .stats(BEFORE_NODE_CLASSIFY, "p-slow");
        assert_eq!((slow.failures, slow.timeouts, slow.failed_open), (1, 1, 1));
        assert!(slow.last_error.unwrap().contains("after the 50 ms limit"));

        // before.group.change fails closed: the first failure refuses.
        add(m, "g-garbage", BEFORE_GROUP_CHANGE, 1, "garbage").await;
        add(m, "g-fail", BEFORE_GROUP_CHANGE, 2, "fail").await;
        // The seeded administrator (audit entries reference a real user).
        let actor = InterceptActor {
            organization_id: default_organization_uuid(),
            user_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()),
        };
        let err = m
            .intercept(
                BEFORE_GROUP_CHANGE,
                "g1",
                actor,
                json!({"action": "create", "change": {}}),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            &err,
            InterceptRefusal::Unavailable { plugin_id, .. } if plugin_id == "g-garbage"
        ));
        assert!(err.to_string().contains("did not give a valid answer"));
        assert!(!err.to_string().contains("verdict"));
        let rt = m.interceptor_runtime();
        assert_eq!(rt.stats(BEFORE_GROUP_CHANGE, "g-garbage").failed_closed, 1);
        // g-fail never ran.
        assert_eq!(rt.stats(BEFORE_GROUP_CHANGE, "g-fail").calls, 0);
        // With a user every refusal is audited.
        let _ = m
            .intercept(
                BEFORE_GROUP_CHANGE,
                "g1",
                actor,
                json!({"action": "create"}),
            )
            .await;
        let n = audit_actions(m)
            .await
            .iter()
            .filter(|a| *a == "plugin.interceptor.refused")
            .count();
        assert_eq!(n, 2);
    }

    #[tokio::test]
    async fn a_filter_keeps_the_plugin_asleep_when_it_does_not_match() {
        let e = env(&["p-filter"]).await;
        let m = &e.manager;
        let yaml = format!(
            "api_version: 2\nid: p-filter\nname: F\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {{ kind: builtin, entry: p-filter }}\ncapabilities:\n  - events:intercept:{BEFORE_NODE_CLASSIFY}\n\
             events:\n  intercept:\n    - {{ type: {BEFORE_NODE_CLASSIFY}, handler: veto_db, filter: \"facts.role == 'db'\" }}\n"
        );
        install(m, "p-filter", &yaml).await;
        m.enable("p-filter").await.unwrap();
        let actor = InterceptActor::system();
        assert!(m
            .intercept(BEFORE_NODE_CLASSIFY, "n", actor, payload("web"))
            .await
            .is_ok());
        assert!(e.plugin.calls.lock().unwrap().is_empty());
        assert!(m
            .intercept(BEFORE_NODE_CLASSIFY, "n", actor, payload("db"))
            .await
            .is_err());
        let s = m
            .interceptor_runtime()
            .stats(BEFORE_NODE_CLASSIFY, "p-filter");
        assert_eq!((s.skipped, s.vetoed), (1, 1));
    }

    #[tokio::test]
    async fn a_denying_policy_turns_interceptors_off() {
        let policy = crate::config::PluginPolicyConfig {
            denied_capabilities: vec!["events:intercept:*".into()],
        };
        let e = env_with(&["p-veto"], Some(policy)).await;
        let m = &e.manager;
        add(m, "p-veto", BEFORE_NODE_CLASSIFY, 1, "veto_db").await;
        assert!(m
            .intercept(
                BEFORE_NODE_CLASSIFY,
                "n",
                InterceptActor::system(),
                payload("db")
            )
            .await
            .is_ok());
        assert!(e.plugin.calls.lock().unwrap().is_empty());
        let view = m.interceptors_overview().await;
        assert!(!view.events[0].interceptors[0].granted);
    }

    /// `before.node.classify` on the real path: the classifier sees the
    /// changed facts, a veto refuses the classification, and an interceptor
    /// that fails leaves the classification as it would have been.
    #[tokio::test]
    async fn the_classification_path_applies_changes_and_vetoes() {
        use crate::services::classification::ClassificationService;
        let e = env(&["p-tier", "p-veto"]).await;
        let m = &e.manager;
        let gold = NodeGroup {
            name: "gold".into(),
            rule_match_type: RuleMatchType::All,
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "tier".into(),
                operator: RuleOperator::Equals,
                value: json!("gold"),
            }],
            ..Default::default()
        };
        let groups = vec![gold];
        let svc = ClassificationService::new(groups.clone());
        let facts = |role: &str| {
            vec![Fact {
                certname: "n1".into(),
                name: "role".into(),
                value: json!(role),
                environment: None,
            }]
        };
        let classify = |role: &'static str| {
            let svc = &svc;
            let groups = &groups;
            async move {
                m.classify_with_plugin_facts("n1", "all", facts(role), None, groups, false, |f| {
                    svc.classify("n1", f)
                })
                .await
            }
        };
        let in_gold = |r: &ClassificationResult| r.groups.iter().any(|g| g.name == "gold");

        // No interceptor: the node is not gold.
        assert!(!in_gold(&classify("db").await.unwrap()));

        add(m, "p-tier", BEFORE_NODE_CLASSIFY, 1, "tier").await;
        assert!(in_gold(&classify("db").await.unwrap()));
        // The interceptor saw the facts the classifier uses.
        let seen = e.plugin.calls.lock().unwrap()[0].2["payload"].clone();
        assert_eq!(seen["facts"]["role"], "db");
        assert_eq!(seen["facts"]["certname"], "n1");
        assert_eq!(seen["scope"], "all");

        // p-tier changes the role to web, so p-veto (later) lets it pass ...
        add(m, "p-veto", BEFORE_NODE_CLASSIFY, 2, "veto_db").await;
        assert!(in_gold(&classify("db").await.unwrap()));
        // ... until it runs first.
        m.disable("p-tier").await.unwrap();
        let err = classify("db").await.unwrap_err();
        assert!(
            matches!(err, InterceptRefusal::Vetoed { ref plugin_id, .. } if plugin_id == "p-veto")
        );
        assert_eq!(err.to_app_error().to_string(), format!("Conflict: {err}"));
        assert!(!in_gold(&classify("web").await.unwrap()));
    }

    // ---- WASM guests: the real deadline and the approval gate -------------

    /// A guest that answers `result` (raw JSON of the guest envelope), after
    /// spinning forever when `spin` is set.
    fn wasm_guest(result: &str, spin: bool) -> Vec<u8> {
        let wat = format!(
            r#"(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 4096))
  (data (i32.const 256) "{data}")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $p))
  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    {body}
    (i64.or (i64.shl (i64.const 256) (i64.const 32)) (i64.const {len}))))"#,
            data = result.replace('\\', "\\\\").replace('"', "\\\""),
            len = result.len(),
            // Requests start with {"kind":"<kind>": spin only for interceptor calls.
            body = if spin {
                "(if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 9))) (i32.const 105)) (then (loop $l (br $l))))"
            } else {
                ""
            },
        );
        wat::parse_str(wat).unwrap()
    }

    async fn wasm_env() -> (Arc<PluginManager>, std::path::PathBuf) {
        use crate::services::plugins::host::{HostApiImpl, PackageModuleSource};
        use crate::services::plugins::storage::{PluginStorage, StorageLimits};
        use crate::services::plugins::wasm::{WasmConfig, WasmInvoker};
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ovx_icptw_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let manager = Arc::new(
            PluginManager::new(pool.clone(), dir.clone(), 8)
                .with_builtin_keys(vec![("T".into(), pk)]),
        );
        manager.reload().await.unwrap();
        let storage = Arc::new(PluginStorage::new(
            pool,
            dir.clone(),
            StorageLimits::default(),
        ));
        let host = Arc::new(HostApiImpl::new(
            &manager,
            Default::default(),
            storage.clone(),
            None,
        ));
        let source = Arc::new(PackageModuleSource {
            data_dir: dir.clone(),
            max_package_bytes: 8 * 1024 * 1024,
        });
        let wasm = Arc::new(WasmInvoker::new(WasmConfig::default(), source, host).unwrap());
        manager.attach_wasm(storage, wasm);
        (manager, dir)
    }

    async fn install_wasm(m: &PluginManager, id: &str, result: &str, spin: bool, optional: bool) {
        let cap = if optional {
            format!("  - events:intercept:{BEFORE_NODE_CLASSIFY}: {{ optional: true }}\n")
        } else {
            format!("  - events:intercept:{BEFORE_NODE_CLASSIFY}\n")
        };
        let yaml = format!(
            "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {{ kind: wasm, entry: plugin.wasm, limits: {{ timeout_ms: 5000 }} }}\n\
             capabilities:\n{cap}events:\n  intercept:\n    - {{ type: {BEFORE_NODE_CLASSIFY} }}\n"
        );
        let wasm = wasm_guest(result, spin);
        let pkg = build_signed_package(
            &[("plugin.yaml", yaml.as_bytes()), ("plugin.wasm", &wasm)],
            id,
            "1.0.0",
            2,
            SEED,
        );
        m.install(&pkg, "admin", None).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_wasm_interceptor_is_cut_by_the_event_limits_and_needs_its_approval() {
        let (m, dir) = wasm_env().await;
        let approve = |caps: &[&str]| ApproveCapabilitiesRequest {
            version: None,
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
        };
        let cap = format!("{CAPABILITY_PREFIX}{BEFORE_NODE_CLASSIFY}");

        // A guest that never returns, although its manifest asks for 5 s:
        // the event's limits apply and the event fails open.
        install_wasm(&m, "w-spin", r#"{"ok":{"decision":"allow"}}"#, true, false).await;
        m.approve_capabilities("w-spin", &approve(&[&cap]), "admin")
            .await
            .unwrap();
        m.enable("w-spin").await.unwrap();
        let started = Instant::now();
        let out = m
            .intercept(
                BEFORE_NODE_CLASSIFY,
                "n1",
                InterceptActor::system(),
                payload("db"),
            )
            .await
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(1_000));
        assert_eq!(out.degraded.len(), 1, "{:?}", out.degraded);
        // Cut by the interceptor fuel ceiling or the 50 ms deadline,
        // whichever comes first; either way a failure and fail open.
        let s = m
            .interceptor_runtime()
            .stats(BEFORE_NODE_CLASSIFY, "w-spin");
        assert_eq!((s.failures, s.failed_open), (1, 1));
        let err = s.last_error.unwrap();
        assert!(
            err.contains("fuel exhausted") || err.contains("timed out"),
            "{err}"
        );
        m.disable("w-spin").await.unwrap();

        // An optional capability that is not approved keeps it out of play.
        install_wasm(
            &m,
            "w-veto",
            r#"{"ok":{"decision":"veto","reason":"wasm says no"}}"#,
            false,
            true,
        )
        .await;
        m.enable("w-veto").await.unwrap();
        assert!(m
            .intercept(
                BEFORE_NODE_CLASSIFY,
                "n1",
                InterceptActor::system(),
                payload("db")
            )
            .await
            .is_ok());
        assert_eq!(
            m.interceptor_runtime()
                .stats(BEFORE_NODE_CLASSIFY, "w-veto")
                .skipped,
            1
        );
        // Approved: it runs and vetoes, without a reload.
        m.approve_capabilities("w-veto", &approve(&[&cap]), "admin")
            .await
            .unwrap();
        let err = m
            .intercept(
                BEFORE_NODE_CLASSIFY,
                "n1",
                InterceptActor::system(),
                payload("db"),
            )
            .await
            .unwrap_err();
        assert_eq!(
            err,
            InterceptRefusal::Vetoed {
                event: BEFORE_NODE_CLASSIFY.into(),
                plugin_id: "w-veto".into(),
                reason: "wasm says no".into(),
            }
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_fourth_interceptor_is_refused_at_enable_with_a_clear_error() {
        let e = env(&["a1", "a2", "a3", "a4", "b1"]).await;
        let m = &e.manager;
        for (i, id) in ["a1", "a2", "a3"].iter().enumerate() {
            add(m, id, BEFORE_NODE_CLASSIFY, i as i32, "allow").await;
        }
        install(m, "a4", &manifest("a4", BEFORE_NODE_CLASSIFY, 0, "allow")).await;
        match m.enable("a4").await.unwrap_err() {
            PluginError::Conflict { code, message } => {
                assert_eq!(code, "interceptor_limit");
                assert!(message.contains("a1, a2, a3"), "{message}");
                assert!(message.contains(BEFORE_NODE_CLASSIFY));
            }
            other => panic!("{other:?}"),
        }
        // Another event has its own budget.
        add(m, "b1", BEFORE_GROUP_CHANGE, 0, "allow").await;
        // A plugin isolated by the breaker keeps its place.
        PluginRepository::new(m.pool())
            .set_state("a1", PluginState::Error, Some(CIRCUIT_REASON))
            .await
            .unwrap();
        m.reload().await.unwrap();
        assert!(m.enable("a4").await.is_err());
        // Disabling one frees the place.
        m.disable("a2").await.unwrap();
        m.enable("a4").await.unwrap();
    }
}
