//! Plugin second factors (H-1): the `AuthFactor` contract served by plugin
//! handlers of kind `factor`. A factor is reachable only while its plugin is
//! enabled with an approved `auth:factor`; every call passes the governor and
//! the handler timeout; records are grouped by the qualified id
//! `<plugin-id>:<factor-id>`; any failure is `FactorError::Unavailable`.
//! The JSON wire format is in PLUGIN_PLATFORM.md section 17.15.

use std::collections::HashSet;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{json, Map, Value};

use super::invoke::{
    invoke_with_timeout, HandlerCall, HandlerKind, HookIndex, InvokeError,
    DEFAULT_HANDLER_TIMEOUT_MS,
};
use super::manager::PluginManager;
use crate::models::{AuthFactorDecl, PluginState, RuntimeKind, RuntimeSpec};
use crate::services::auth_pipeline::factors::{
    now_unix, AuthFactor, ChallengeContext, ChallengeStart, EnrollContext, EnrollMode,
    EnrollRecord, EnrollStart, FactorDescriptor, FactorError, FactorOutcome, FactorRecord,
};

pub const FACTOR_CAPABILITY: &str = "auth:factor";
pub const BEGIN_ENROLL: &str = "begin_enroll";
pub const COMPLETE_ENROLL: &str = "complete_enroll";
pub const BEGIN_CHALLENGE: &str = "begin_challenge";
pub const VERIFY: &str = "verify";
/// Separates the plugin id from the factor id. Neither may contain it, and no
/// built-in factor id does, so qualified ids never collide.
pub const QUALIFIER: char = ':';
/// Message of every `Unavailable` the host raises itself. It reaches
/// unauthenticated clients, so it names nothing internal.
pub const UNAVAILABLE: &str = "factor_unavailable";
/// Code used when a plugin's error code is not a safe identifier.
const FALLBACK_CODE: &str = "plugin_error";
const MAX_CODE_LEN: usize = 64;
const MAX_LABEL_CHARS: usize = 64;
const MAX_FLOW_STATE_BYTES: usize = 64 * 1024;

pub fn qualified_id(plugin_id: &str, factor_id: &str) -> String {
    format!("{plugin_id}{QUALIFIER}{factor_id}")
}

/// One factor of an enabled plugin version.
#[derive(Debug, Clone)]
pub struct PluginFactorEntry {
    pub plugin_id: String,
    pub version: String,
    pub runtime: RuntimeSpec,
    pub decl: AuthFactorDecl,
}

impl PluginFactorEntry {
    pub fn qualified_id(&self) -> String {
        qualified_id(&self.plugin_id, &self.decl.id)
    }

    pub fn descriptor(&self) -> FactorDescriptor {
        FactorDescriptor {
            id: self.qualified_id(),
            kind: self.decl.kind.clone(),
            label: self.decl.label.clone(),
            amr: self.decl.amr.clone(),
            enrollment: self.decl.enrollment.clone(),
            fields: self.decl.fields.clone(),
        }
    }

    pub fn timeout_ms(&self) -> u64 {
        self.runtime
            .limits
            .as_ref()
            .and_then(|l| l.timeout_ms)
            .unwrap_or(DEFAULT_HANDLER_TIMEOUT_MS)
    }
}

/// The factors of the enabled plugins. `PluginManager::reload` swaps the
/// whole set, so a factor is registered when its plugin is enabled and
/// unregistered when it is disabled, uninstalled, isolated by the circuit
/// breaker or fails its integrity check.
#[derive(Debug, Default)]
pub struct PluginFactorRegistry {
    entries: RwLock<Arc<Vec<PluginFactorEntry>>>,
}

impl PluginFactorRegistry {
    pub fn snapshot(&self) -> Arc<Vec<PluginFactorEntry>> {
        self.entries
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Qualified ids of the registered factors.
    pub fn ids(&self) -> Vec<String> {
        self.snapshot().iter().map(|e| e.qualified_id()).collect()
    }

    pub fn is_registered(&self, plugin_id: &str, version: &str, factor_id: &str) -> bool {
        self.snapshot()
            .iter()
            .any(|e| e.plugin_id == plugin_id && e.version == version && e.decl.id == factor_id)
    }

    /// Replaces the set from the hooks of the enabled plugins. A poisoned
    /// lock is recovered so that unregistering always happens.
    pub fn rebuild(&self, hooks: &HookIndex) {
        let entries = hooks
            .plugins()
            .flat_map(|p| {
                p.auth_factors.iter().map(move |d| PluginFactorEntry {
                    plugin_id: p.plugin_id.clone(),
                    version: p.version.clone(),
                    runtime: p.runtime.clone(),
                    decl: d.clone(),
                })
            })
            .collect();
        *self.entries.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(entries);
    }
}

impl PluginManager {
    /// Factors of the enabled plugins, ready for an `AuthPipeline`.
    pub fn auth_factors(self: &Arc<Self>) -> Vec<Box<dyn AuthFactor>> {
        self.auth_factor_registry()
            .snapshot()
            .iter()
            .map(|e| Box::new(PluginFactor::new(self.clone(), e.clone())) as Box<dyn AuthFactor>)
            .collect()
    }

    /// Checked before every call, because a pipeline may hold a factor across
    /// a disable, a revoked approval or a policy change. Builtin plugins are
    /// first-party code and need no approval, as elsewhere.
    pub(super) async fn factor_admitted(&self, e: &PluginFactorEntry) -> bool {
        if !self
            .auth_factor_registry()
            .is_registered(&e.plugin_id, &e.version, &e.decl.id)
            || self.policy().denies(FACTOR_CAPABILITY)
        {
            return false;
        }
        let Ok(Some(rec)) = self.repo().get_plugin(&e.plugin_id).await else {
            return false;
        };
        if rec.state != PluginState::Enabled || rec.active_version.as_deref() != Some(&e.version) {
            return false;
        }
        if e.runtime.kind == RuntimeKind::Builtin {
            return true;
        }
        self.repo()
            .list_grants(&e.plugin_id)
            .await
            .map(|g| {
                g.iter()
                    .any(|g| g.capability == FACTOR_CAPABILITY && g.scope.is_none())
            })
            .unwrap_or(false)
    }
}

/// The `AuthFactor` of one plugin factor.
pub struct PluginFactor {
    manager: Arc<PluginManager>,
    entry: PluginFactorEntry,
}

impl PluginFactor {
    pub fn new(manager: Arc<PluginManager>, entry: PluginFactorEntry) -> Self {
        Self { manager, entry }
    }

    fn unavailable(&self, handler: &str, why: &str) -> FactorError {
        // Never the payloads: they carry factor records and user responses.
        tracing::warn!(
            plugin = %self.entry.plugin_id,
            version = %self.entry.version,
            factor = %self.entry.decl.id,
            handler,
            "plugin factor unavailable: {}",
            why
        );
        FactorError::Unavailable(UNAVAILABLE.into())
    }

    fn reject(&self, handler: &str, r: Reject) -> FactorError {
        match r {
            Reject::Factor(e) => e,
            Reject::Malformed(why) => self.unavailable(handler, why),
        }
    }

    async fn call(&self, handler: &'static str, input: Value) -> Result<Value, FactorError> {
        let e = &self.entry;
        if !self.manager.factor_admitted(e).await {
            return Err(self.unavailable(
                handler,
                "the plugin is not enabled or auth:factor is not granted",
            ));
        }
        let call = HandlerCall {
            plugin_id: e.plugin_id.clone(),
            version: e.version.clone(),
            runtime: e.runtime.clone(),
            handler: handler.to_string(),
            kind: HandlerKind::Factor,
            input,
            actor: None,
        };
        let timeout = e.timeout_ms();
        let started = Instant::now();
        let result = invoke_with_timeout(self.manager.as_ref(), call, timeout).await;
        match result {
            // A late answer is discarded: builtin handlers cannot be
            // preempted and the WASM wait has a grace period.
            Ok(_) if started.elapsed() > Duration::from_millis(timeout) => {
                Err(self.unavailable(handler, "answered after the handler timeout"))
            }
            Ok(v) => split_error(v).map_err(|r| self.reject(handler, r)),
            Err(err) => Err(self.unavailable(handler, failure_class(&err))),
        }
    }

    /// `{factor, user_id, username, existing: [{id, data}], pending, now_unix, response?}`
    fn enroll_input(&self, ctx: &EnrollContext, response: Option<Value>) -> Value {
        let mut v = json!({
            "factor": self.entry.decl.id,
            "user_id": ctx.user_id,
            "username": ctx.username,
            "existing": records_json(&ctx.existing),
            "pending": ctx.pending.as_ref().map(record_json),
            "now_unix": now_unix(),
        });
        if let Some(r) = response {
            v["response"] = r;
        }
        v
    }

    /// `{factor, user_id, username, records: [{id, data}], flow_state, now_unix, response?}`
    fn challenge_input(&self, ctx: &ChallengeContext, response: Option<Value>) -> Value {
        let mut v = json!({
            "factor": self.entry.decl.id,
            "user_id": ctx.user_id,
            "username": ctx.username,
            "records": records_json(&ctx.records),
            "flow_state": ctx.flow_state,
            "now_unix": ctx.now_unix,
        });
        if let Some(r) = response {
            v["response"] = r;
        }
        v
    }
}

#[async_trait]
impl AuthFactor for PluginFactor {
    fn descriptor(&self) -> FactorDescriptor {
        self.entry.descriptor()
    }

    async fn begin_enroll(&self, ctx: &EnrollContext) -> Result<EnrollStart, FactorError> {
        let out = self
            .call(BEGIN_ENROLL, self.enroll_input(ctx, None))
            .await?;
        parse_enroll_start(out).map_err(|r| self.reject(BEGIN_ENROLL, r))
    }

    async fn complete_enroll(
        &self,
        ctx: &EnrollContext,
        resp: Value,
    ) -> Result<EnrollRecord, FactorError> {
        let out = self
            .call(COMPLETE_ENROLL, self.enroll_input(ctx, Some(resp)))
            .await?;
        parse_enroll_record(out).map_err(|r| self.reject(COMPLETE_ENROLL, r))
    }

    async fn begin_challenge(&self, ctx: &ChallengeContext) -> Result<ChallengeStart, FactorError> {
        let out = self
            .call(BEGIN_CHALLENGE, self.challenge_input(ctx, None))
            .await?;
        parse_challenge_start(out).map_err(|r| self.reject(BEGIN_CHALLENGE, r))
    }

    async fn verify(
        &self,
        ctx: &ChallengeContext,
        resp: Value,
    ) -> Result<FactorOutcome, FactorError> {
        let out = self
            .call(VERIFY, self.challenge_input(ctx, Some(resp)))
            .await?;
        parse_outcome(out, &self.entry.decl.amr, &ctx.records).map_err(|r| self.reject(VERIFY, r))
    }
}

// ---- wire format ---------------------------------------------------------------

fn record_json(r: &FactorRecord) -> Value {
    json!({ "id": r.id, "data": r.data })
}

fn records_json(records: &[FactorRecord]) -> Value {
    Value::Array(records.iter().map(record_json).collect())
}

fn failure_class(e: &InvokeError) -> &'static str {
    match e {
        InvokeError::Unavailable(_) => "refused (quota, circuit breaker, busy or no runtime)",
        InvokeError::Timeout(_) => "timed out",
        InvokeError::Failed(_) => "the handler failed (trap, limit or error result)",
    }
}

/// Why an answer is not accepted.
#[derive(Debug)]
pub enum Reject {
    /// The plugin answered with an error of the contract.
    Factor(FactorError),
    /// The answer does not follow the contract.
    Malformed(&'static str),
}

/// The object with exactly the `required` keys and some of the `optional` ones.
fn object(v: Value, required: &[&str], optional: &[&str]) -> Result<Map<String, Value>, Reject> {
    let Value::Object(map) = v else {
        return Err(Reject::Malformed("the answer is not an object"));
    };
    if map
        .keys()
        .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(Reject::Malformed("the answer has unknown fields"));
    }
    if required.iter().any(|k| !map.contains_key(*k)) {
        return Err(Reject::Malformed("the answer misses a required field"));
    }
    Ok(map)
}

fn safe_code(code: &str) -> String {
    let ok = !code.is_empty()
        && code.len() <= MAX_CODE_LEN
        && code.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'.' | b'-')
        });
    if ok {
        code.to_string()
    } else {
        FALLBACK_CODE.to_string()
    }
}

/// `{"error": {"kind", "code"}}` becomes the matching `FactorError`
/// (`unavailable`, `invalid`, `internal`); any other value is passed on to
/// the handler's parser.
pub fn split_error(v: Value) -> Result<Value, Reject> {
    let is_error = matches!(&v, Value::Object(m) if m.contains_key("error"));
    if !is_error {
        return Ok(v);
    }
    let mut outer = object(v, &["error"], &[])?;
    let inner = object(
        outer.remove("error").unwrap_or(Value::Null),
        &["kind", "code"],
        &[],
    )?;
    let (Some(kind), Some(code)) = (
        inner.get("kind").and_then(Value::as_str),
        inner.get("code").and_then(Value::as_str),
    ) else {
        return Err(Reject::Malformed("the error kind and code must be strings"));
    };
    let code = safe_code(code);
    Err(Reject::Factor(match kind {
        "unavailable" => FactorError::Unavailable(code),
        "invalid" => FactorError::Invalid(code),
        "internal" => FactorError::Internal(code),
        _ => return Err(Reject::Malformed("unknown error kind")),
    }))
}

/// `begin_enroll`: `{client, mode: {pending: v} | {immediate: v}}`.
pub fn parse_enroll_start(v: Value) -> Result<EnrollStart, Reject> {
    const ONE_MODE: &str = "mode must be one of pending or immediate";
    let mut m = object(v, &["client", "mode"], &[])?;
    let mode = object(
        m.remove("mode").unwrap_or(Value::Null),
        &[],
        &["pending", "immediate"],
    )?;
    if mode.len() != 1 {
        return Err(Reject::Malformed(ONE_MODE));
    }
    let mode = match mode.into_iter().next() {
        Some((k, v)) if k == "pending" => EnrollMode::Pending(v),
        Some((_, v)) => EnrollMode::Immediate(v),
        None => return Err(Reject::Malformed(ONE_MODE)),
    };
    Ok(EnrollStart {
        client: m.remove("client").unwrap_or(Value::Null),
        mode,
    })
}

/// `complete_enroll`: `{data, label?}`.
pub fn parse_enroll_record(v: Value) -> Result<EnrollRecord, Reject> {
    let mut m = object(v, &["data"], &["label"])?;
    let label = match m.remove("label") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            if s.chars().count() > MAX_LABEL_CHARS || s.chars().any(char::is_control) {
                return Err(Reject::Factor(FactorError::Invalid("invalid_label".into())));
            }
            Some(s)
        }
        Some(_) => return Err(Reject::Malformed("label must be a string")),
    };
    Ok(EnrollRecord {
        data: m.remove("data").unwrap_or(Value::Null),
        label,
    })
}

/// `begin_challenge`: `{client, flow_state?}`.
pub fn parse_challenge_start(v: Value) -> Result<ChallengeStart, Reject> {
    let mut m = object(v, &["client"], &["flow_state"])?;
    let flow_state = match m.remove("flow_state") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.len() <= MAX_FLOW_STATE_BYTES => Some(s),
        Some(Value::String(_)) => return Err(Reject::Malformed("flow_state is too large")),
        Some(_) => return Err(Reject::Malformed("flow_state must be a string")),
    };
    Ok(ChallengeStart {
        client: m.remove("client").unwrap_or(Value::Null),
        flow_state,
    })
}

/// `verify`: `{outcome: "verified", amr, updates?: [[id, data]]}`,
/// `{outcome: "failed", retryable}` or `{outcome: "pending"}`. `Verified`
/// needs the declared `amr` and updates only of records the plugin was
/// given, each at most once: anything else is malformed, never a pass.
pub fn parse_outcome(
    v: Value,
    amr: &str,
    records: &[FactorRecord],
) -> Result<FactorOutcome, Reject> {
    let outcome = match &v {
        Value::Object(m) => m.get("outcome").and_then(Value::as_str).map(str::to_string),
        _ => None,
    };
    match outcome.as_deref() {
        Some("verified") => {
            let mut m = object(v, &["outcome", "amr"], &["updates"])?;
            if m.get("amr").and_then(Value::as_str) != Some(amr) {
                return Err(Reject::Malformed("amr is not the factor's declared amr"));
            }
            let updates = parse_updates(m.remove("updates"), records)?;
            Ok(FactorOutcome::Verified {
                amr: amr.to_string(),
                updates,
            })
        }
        Some("failed") => {
            let m = object(v, &["outcome", "retryable"], &[])?;
            match m.get("retryable") {
                Some(Value::Bool(retryable)) => Ok(FactorOutcome::Failed {
                    retryable: *retryable,
                }),
                _ => Err(Reject::Malformed("retryable must be a boolean")),
            }
        }
        Some("pending") => {
            object(v, &["outcome"], &[])?;
            Ok(FactorOutcome::Pending)
        }
        _ => Err(Reject::Malformed(
            "outcome must be verified, failed or pending",
        )),
    }
}

fn parse_updates(
    v: Option<Value>,
    records: &[FactorRecord],
) -> Result<Vec<(String, Value)>, Reject> {
    const PAIR: &str = "an update must be [record id, data]";
    let list = match v {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(a)) => a,
        Some(_) => return Err(Reject::Malformed("updates must be a list")),
    };
    let given: HashSet<&str> = records.iter().map(|r| r.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut updates = Vec::new();
    for u in list {
        let Value::Array(mut pair) = u else {
            return Err(Reject::Malformed(PAIR));
        };
        let (Some(data), Some(Value::String(id)), None) = (pair.pop(), pair.pop(), pair.pop())
        else {
            return Err(Reject::Malformed(PAIR));
        };
        if !given.contains(id.as_str()) {
            return Err(Reject::Malformed(
                "an update names a record the factor was not given",
            ));
        }
        if !seen.insert(id.clone()) {
            return Err(Reject::Malformed("a record is updated twice"));
        }
        updates.push((id, data));
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::models::{ApproveCapabilitiesRequest, User};
    use crate::services::auth_pipeline::factors::generate_recovery_codes;
    use crate::services::auth_pipeline::policy::{self, PolicyInput};
    use crate::services::auth_pipeline::{
        AuthPipeline, ClientInfo, LoginOutcome, PipelineError, PrimaryIdentity, VerifyOutcome,
    };
    use crate::services::plugins::host::{HostApiImpl, PackageModuleSource};
    use crate::services::plugins::manager::BuiltinPlugin;
    use crate::services::plugins::package::build_signed_package;
    use crate::services::plugins::storage::{PluginStorage, StorageLimits};
    use crate::services::plugins::wasm::{WasmConfig, WasmInvoker};
    use crate::services::AuthService;
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;
    use std::sync::Mutex;
    use uuid::Uuid;

    const SEED: [u8; 32] = [11; 32];

    // ---- a tiny factor guest in WAT ------------------------------------------

    #[derive(Clone, Copy)]
    enum Out<'a> {
        /// The whole guest output, e.g. `{"ok": {...}}`.
        Json(&'a str),
        /// `{"ok":{"client":<the request>,"flow_state":null}}`.
        Echo,
        Trap,
        Spin,
    }

    #[derive(Clone, Copy)]
    struct Guest<'a> {
        enroll: Out<'a>,
        complete: Out<'a>,
        challenge: Out<'a>,
        verify: Out<'a>,
    }

    impl Default for Guest<'_> {
        fn default() -> Self {
            Self {
                enroll: Out::Json(
                    r#"{"ok":{"client":{"publicKey":{}},"mode":{"pending":{"s":1}}}}"#,
                ),
                complete: Out::Json(r#"{"ok":{"data":{"credential":"c1"},"label":"Key"}}"#),
                challenge: Out::Json(r#"{"ok":{"client":{"publicKey":{}},"flow_state":"fs"}}"#),
                verify: Out::Json(r#"{"ok":{"outcome":"verified","amr":"hwk"}}"#),
            }
        }
    }

    const ECHO_PREFIX: &str = r#"{"ok":{"client":"#;
    const ECHO_SUFFIX: &str = r#","flow_state":null}}"#;
    const EMPTY_OK: &str = r#"{"ok":{}}"#;

    fn wat_str(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Dispatches on the request bytes: `{"kind":"factor","handler":"` puts
    /// the handler name at offset 28; other kinds (lifecycle) answer `{}`.
    fn factor_guest(g: &Guest) -> Vec<u8> {
        let (pre, suf) = (ECHO_PREFIX.len(), ECHO_SUFFIX.len());
        let mut data = String::new();
        let mut funcs = String::new();
        for (i, out) in [g.enroll, g.complete, g.challenge, g.verify]
            .iter()
            .enumerate()
        {
            let off = 1024 + i * 8192;
            let body = match out {
                Out::Json(s) => {
                    data += &format!("  (data (i32.const {off}) \"{}\")\n", wat_str(s));
                    format!(
                        "(i64.or (i64.shl (i64.const {off}) (i64.const 32)) (i64.const {}))",
                        s.len()
                    )
                }
                Out::Trap => "(unreachable)".to_string(),
                Out::Spin => "(loop $spin (br $spin)) (unreachable)".to_string(),
                Out::Echo => format!(
                    "(local.set $out (call $alloc (i32.add (local.get $l) (i32.const {n})))) \
                     (memory.copy (local.get $out) (i32.const 40960) (i32.const {pre})) \
                     (memory.copy (i32.add (local.get $out) (i32.const {pre})) (local.get $p) (local.get $l)) \
                     (memory.copy (i32.add (i32.add (local.get $out) (i32.const {pre})) (local.get $l)) (i32.const 41000) (i32.const {suf})) \
                     (i64.or (i64.shl (i64.extend_i32_u (local.get $out)) (i64.const 32)) \
                             (i64.extend_i32_u (i32.add (local.get $l) (i32.const {n}))))",
                    n = pre + suf
                ),
            };
            funcs += &format!(
                "  (func $emit{i} (param $p i32) (param $l i32) (result i64) (local $out i32) {body})\n"
            );
        }
        let wat = format!(
            r#"(module
  (import "openvox" "host_call" (func $hc (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 4)
  (global $bump (mut i32) (i32.const 65536))
{data}  (data (i32.const 40960) "{prefix}")
  (data (i32.const 41000) "{suffix}")
  (data (i32.const 41100) "{empty}")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $r i32)
    (local.set $r (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $r))
{funcs}  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    (if (i32.ne (i32.load8_u (i32.add (local.get $p) (i32.const 13))) (i32.const 111))
      (then (return (i64.or (i64.shl (i64.const 41100) (i64.const 32)) (i64.const {empty_len})))))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 28))) (i32.const 118))
      (then (return (call $emit3 (local.get $p) (local.get $l)))))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 28))) (i32.const 99))
      (then (return (call $emit1 (local.get $p) (local.get $l)))))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 34))) (i32.const 101))
      (then (return (call $emit0 (local.get $p) (local.get $l)))))
    (call $emit2 (local.get $p) (local.get $l))))"#,
            prefix = wat_str(ECHO_PREFIX),
            suffix = wat_str(ECHO_SUFFIX),
            empty = wat_str(EMPTY_OK),
            empty_len = EMPTY_OK.len(),
        );
        wat::parse_str(wat).unwrap()
    }

    fn factor_yaml(id: &str, runtime: &str) -> String {
        format!(
            "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {runtime}\ncapabilities:\n  - auth:factor\n\
             contributes:\n  auth_factors:\n    - {{ id: key, kind: webauthn, label: Security key, amr: hwk, enrollment: host-managed, fields: [credential] }}\n"
        )
    }

    // ---- a capturing builtin factor ------------------------------------------

    #[derive(Default)]
    struct Capture {
        calls: Mutex<Vec<(HandlerKind, String, Value)>>,
    }

    impl BuiltinPlugin for Capture {
        fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
            self.calls
                .lock()
                .unwrap()
                .push((call.kind, call.handler.clone(), call.input.clone()));
            Ok(match call.handler.as_str() {
                BEGIN_ENROLL => {
                    json!({"client": {"options": 1}, "mode": {"pending": {"state": "p"}}})
                }
                COMPLETE_ENROLL => {
                    json!({"data": {"credential": "c1", "n": 0}, "label": "My key"})
                }
                BEGIN_CHALLENGE => json!({"client": {"options": 2}, "flow_state": "fs-1"}),
                VERIFY => json!({
                    "outcome": "verified",
                    "amr": "hwk",
                    "updates": [[call.input["records"][0]["id"].clone(), {"credential": "c1", "n": 1}]],
                }),
                _ => json!({}),
            })
        }
    }

    // ---- environment ---------------------------------------------------------

    struct Env {
        manager: Arc<PluginManager>,
        pool: SqlitePool,
        config: AppConfig,
        user: User,
    }

    async fn env(builtins: Vec<(&str, Arc<dyn BuiltinPlugin>)>) -> Env {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ovx_factor_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let mut m = PluginManager::new(pool.clone(), dir.clone(), 8)
            .with_builtin_keys(vec![("T".into(), pk)]);
        for (entry, p) in builtins {
            m = m.with_builtin_runtime(entry, p);
        }
        let manager = Arc::new(m);
        manager.reload().await.unwrap();
        let storage = Arc::new(PluginStorage::new(
            pool.clone(),
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
            data_dir: dir,
            max_package_bytes: 8 * 1024 * 1024,
        });
        let wasm = Arc::new(WasmInvoker::new(WasmConfig::default(), source, host).unwrap());
        manager.attach_wasm(storage, wasm);
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role) \
             VALUES (?, 'alice', 'alice@example.com', 'x', 'viewer')",
        )
        .bind(id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        let user = AuthService::new(pool.clone())
            .get_user_by_id(&id)
            .await
            .unwrap()
            .unwrap();
        policy::replace(
            &pool,
            user.organization_id,
            &[PolicyInput {
                name: "everyone".into(),
                enabled: true,
                priority: 100,
                conditions: Default::default(),
                required_acr: 2,
            }],
        )
        .await
        .unwrap();
        Env {
            manager,
            pool,
            config: AppConfig::default(),
            user,
        }
    }

    fn approve(caps: &[&str]) -> ApproveCapabilitiesRequest {
        ApproveCapabilitiesRequest {
            version: None,
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
        }
    }

    impl Env {
        fn pipeline(&self) -> AuthPipeline<'_> {
            AuthPipeline::new(&self.pool, &self.config)
                .with_plugin_factors(self.manager.auth_factors())
        }

        async fn install_wasm(&self, id: &str, limits: &str, g: &Guest<'_>) {
            let y = factor_yaml(
                id,
                &format!("{{ kind: wasm, entry: plugin.wasm, limits: {{ {limits} }} }}"),
            );
            let wasm = factor_guest(g);
            let pkg = build_signed_package(
                &[("plugin.yaml", y.as_bytes()), ("plugin.wasm", &wasm)],
                id,
                "1.0.0",
                2,
                SEED,
            );
            self.manager.install(&pkg, "admin", None).await.unwrap();
            self.manager
                .approve_capabilities(id, &approve(&[FACTOR_CAPABILITY]), "admin")
                .await
                .unwrap();
            self.manager.enable(id).await.unwrap();
        }

        async fn enroll(&self, kind: &str, data: Value) -> String {
            self.pipeline()
                .store()
                .insert(self.user.id, kind, "k", &data, true)
                .await
                .unwrap()
                .id
        }

        async fn login(&self) -> LoginOutcome {
            self.pipeline()
                .start_login(
                    PrimaryIdentity {
                        user: self.user.clone(),
                        provider: "local".into(),
                        amr: vec!["pwd".into()],
                    },
                    &ClientInfo::default(),
                )
                .await
                .unwrap()
        }

        async fn challenge(&self) -> (String, Vec<String>) {
            match self.login().await {
                LoginOutcome::Challenge(o) => (
                    o.challenge_id,
                    o.factors.into_iter().map(|f| f.id).collect(),
                ),
                LoginOutcome::Denied { code, .. } => panic!("denied: {code}"),
                LoginOutcome::Session(_) => panic!("a session without a factor"),
            }
        }

        async fn sessions(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM auth_sessions WHERE user_id = ?")
                .bind(self.user.id.to_string())
                .fetch_one(&self.pool)
                .await
                .unwrap()
        }

        fn ctx(&self, records: Vec<FactorRecord>) -> ChallengeContext {
            ChallengeContext {
                user_id: self.user.id,
                username: self.user.username.clone(),
                records,
                flow_state: None,
                now_unix: now_unix(),
            }
        }
    }

    fn denied_code(o: LoginOutcome) -> &'static str {
        match o {
            LoginOutcome::Denied { code, .. } => code,
            LoginOutcome::Challenge(c) => panic!("challenge offered: {:?}", c.factors),
            LoginOutcome::Session(_) => panic!("session issued"),
        }
    }

    fn is_unavailable<T>(r: &Result<T, FactorError>) -> bool {
        matches!(r, Err(FactorError::Unavailable(m)) if m == UNAVAILABLE)
    }

    // ---- registry ------------------------------------------------------------

    #[tokio::test]
    async fn plugin_factors_follow_the_enabled_set() {
        let e = env(vec![]).await;
        let ids = || e.manager.auth_factor_registry().ids();
        assert!(ids().is_empty());
        e.install_wasm("fa", "timeout_ms: 5000", &Guest::default())
            .await;
        assert_eq!(ids(), vec!["fa:key".to_string()]);
        let d = e.pipeline().descriptors();
        let f = d.iter().find(|d| d.id == "fa:key").expect("offered");
        assert_eq!(
            (f.kind.as_str(), f.amr.as_str(), f.label.as_str()),
            ("webauthn", "hwk", "Security key")
        );
        assert_eq!(f.fields, vec!["credential".to_string()]);
        assert!(d.iter().any(|d| d.id == "totp"), "built-ins stay");

        e.manager.disable("fa").await.unwrap();
        assert!(ids().is_empty());
        assert!(!e.pipeline().descriptors().iter().any(|d| d.id == "fa:key"));
        e.manager.enable("fa").await.unwrap();
        assert_eq!(ids(), vec!["fa:key".to_string()]);
        e.manager.uninstall("fa", true).await.unwrap();
        assert!(ids().is_empty());

        // A failed integrity check unregisters the factor too.
        e.install_wasm("fc", "timeout_ms: 5000", &Guest::default())
            .await;
        assert_eq!(ids(), vec!["fc:key".to_string()]);
        std::fs::write(e.manager.package_path("fc", "1.0.0"), b"tampered").unwrap();
        assert!(e.manager.verify("fc").await.is_err());
        assert!(ids().is_empty());
    }

    struct Stub(&'static str);

    #[async_trait]
    impl AuthFactor for Stub {
        fn descriptor(&self) -> FactorDescriptor {
            FactorDescriptor {
                id: self.0.into(),
                kind: "otp".into(),
                label: "Stub".into(),
                amr: "otp".into(),
                enrollment: "host-managed".into(),
                fields: vec![],
            }
        }
        async fn begin_enroll(&self, _: &EnrollContext) -> Result<EnrollStart, FactorError> {
            Err(FactorError::Internal("stub".into()))
        }
        async fn complete_enroll(
            &self,
            _: &EnrollContext,
            _: Value,
        ) -> Result<EnrollRecord, FactorError> {
            Err(FactorError::Internal("stub".into()))
        }
        async fn begin_challenge(
            &self,
            _: &ChallengeContext,
        ) -> Result<ChallengeStart, FactorError> {
            Err(FactorError::Internal("stub".into()))
        }
        async fn verify(
            &self,
            _: &ChallengeContext,
            _: Value,
        ) -> Result<FactorOutcome, FactorError> {
            Ok(FactorOutcome::Verified {
                amr: "otp".into(),
                updates: vec![],
            })
        }
    }

    #[tokio::test]
    async fn plugin_factors_cannot_stand_in_for_built_ins_or_each_other() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let mut config = AppConfig::default();
        config.auth.mfa.webauthn = Some(crate::services::auth_pipeline::WebAuthnConfig {
            rp_id: "puppet.example.com".into(),
            rp_name: "OpenVox".into(),
            origin: "https://puppet.example.com".into(),
        });
        let p = AuthPipeline::new(&pool, &config).with_plugin_factors(vec![
            Box::new(Stub("totp")),
            Box::new(Stub("webauthn")),
            Box::new(Stub("pa:key")),
            Box::new(Stub("pa:key")),
            Box::new(Stub("pb:webauthn")),
        ]);
        let ids: Vec<String> = p.descriptors().into_iter().map(|d| d.id).collect();
        assert_eq!(
            ids,
            ["totp", "recovery", "webauthn", "pa:key", "pb:webauthn"]
        );
        // The built-ins keep their own implementation.
        assert_eq!(
            p.factor("totp").unwrap().descriptor().label,
            "Authenticator app"
        );
        assert_eq!(
            p.factor("webauthn").unwrap().descriptor().label,
            "Security key or passkey"
        );
    }

    // ---- login through a WASM factor -----------------------------------------

    #[tokio::test]
    async fn a_verified_answer_issues_a_new_session_with_the_factor_amr() {
        let e = env(vec![]).await;
        e.install_wasm("fa", "timeout_ms: 5000", &Guest::default())
            .await;
        e.enroll("fa:key", json!({"credential": "c1"})).await;
        let (id, factors) = e.challenge().await;
        assert_eq!(factors, vec!["fa:key".to_string()]);
        let p = e.pipeline();
        let c = ClientInfo::default();
        let client = p.begin_challenge(&id, "fa:key", &c).await.unwrap();
        assert_eq!(client, json!({"publicKey": {}}));
        let r = p
            .verify_challenge(&id, Some("fa:key"), json!({"credential": {}}), &c)
            .await
            .unwrap();
        assert!(matches!(r, VerifyOutcome::Session(_)));
        let (amr, acr): (String, i64) = sqlx::query_as(
            "SELECT amr, acr FROM auth_sessions WHERE user_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(e.user.id.to_string())
        .fetch_one(&e.pool)
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&amr).unwrap(),
            json!(["pwd", "hwk"])
        );
        assert_eq!(acr, 2);
        // Single use: the consumed challenge cannot be verified again.
        assert!(matches!(
            p.verify_challenge(&id, Some("fa:key"), json!({}), &c).await,
            Err(PipelineError::InvalidChallenge)
        ));
    }

    #[tokio::test]
    async fn a_failing_or_malformed_factor_never_passes_a_challenge() {
        let e = env(vec![]).await;
        let cases: [(&str, &str, Out); 6] = [
            ("ftrap", "timeout_ms: 5000", Out::Trap),
            (
                "fspin",
                "timeout_ms: 300, fuel_per_call: 5000000000",
                Out::Spin,
            ),
            ("fabi", "timeout_ms: 5000", Out::Json(r#"{"error":"boom"}"#)),
            (
                "famr",
                "timeout_ms: 5000",
                Out::Json(r#"{"ok":{"outcome":"verified","amr":"otp"}}"#),
            ),
            (
                "fupd",
                "timeout_ms: 5000",
                Out::Json(r#"{"ok":{"outcome":"verified","amr":"hwk","updates":[["nope",{}]]}}"#),
            ),
            (
                "fextra",
                "timeout_ms: 5000",
                Out::Json(r#"{"ok":{"outcome":"verified","amr":"hwk","extra":1}}"#),
            ),
        ];
        for (id, limits, verify) in cases {
            let g = Guest {
                verify,
                ..Guest::default()
            };
            e.install_wasm(id, limits, &g).await;
            e.enroll(&format!("{id}:key"), json!({"credential": id}))
                .await;
        }
        let c = ClientInfo::default();
        for (id, _, _) in cases {
            let factor = format!("{id}:key");
            let (challenge, factors) = e.challenge().await;
            assert!(factors.contains(&factor), "{factors:?}");
            let p = e.pipeline();
            p.begin_challenge(&challenge, &factor, &c).await.unwrap();
            let r = p
                .verify_challenge(&challenge, Some(&factor), json!({"credential": {}}), &c)
                .await;
            assert!(
                matches!(&r, Err(PipelineError::Unavailable(m)) if m == UNAVAILABLE),
                "{id}: {:?}",
                r.err()
            );
        }
        assert_eq!(e.sessions().await, 0, "no failure may issue a session");
    }

    #[tokio::test]
    async fn plugin_answers_map_one_to_one_onto_the_pipeline() {
        let e = env(vec![]).await;
        for (id, verify) in [
            (
                "finv",
                r#"{"ok":{"error":{"kind":"invalid","code":"bad_assertion"}}}"#,
            ),
            ("fno", r#"{"ok":{"outcome":"failed","retryable":false}}"#),
            ("fwait", r#"{"ok":{"outcome":"pending"}}"#),
        ] {
            let g = Guest {
                verify: Out::Json(verify),
                ..Guest::default()
            };
            e.install_wasm(id, "timeout_ms: 5000", &g).await;
            e.enroll(&format!("{id}:key"), json!({})).await;
        }
        let c = ClientInfo::default();
        let p = e.pipeline();
        // invalid: a retryable failure that keeps the challenge.
        let (id, _) = e.challenge().await;
        p.begin_challenge(&id, "finv:key", &c).await.unwrap();
        let r = p
            .verify_challenge(&id, Some("finv:key"), json!({}), &c)
            .await
            .unwrap();
        assert!(matches!(r, VerifyOutcome::Failed { attempts_left: 4 }));
        // failed and not retryable: the challenge is burnt.
        let (id, _) = e.challenge().await;
        p.begin_challenge(&id, "fno:key", &c).await.unwrap();
        let r = p
            .verify_challenge(&id, Some("fno:key"), json!({}), &c)
            .await
            .unwrap();
        assert!(matches!(r, VerifyOutcome::Failed { .. }));
        assert!(matches!(
            p.verify_challenge(&id, Some("fno:key"), json!({}), &c)
                .await,
            Err(PipelineError::InvalidChallenge)
        ));
        // pending: no session, the attempt is given back.
        let (id, _) = e.challenge().await;
        p.begin_challenge(&id, "fwait:key", &c).await.unwrap();
        let r = p
            .verify_challenge(&id, Some("fwait:key"), json!({}), &c)
            .await
            .unwrap();
        assert!(matches!(r, VerifyOutcome::Pending));
        assert_eq!(e.sessions().await, 0);
    }

    // ---- governor ------------------------------------------------------------

    #[tokio::test]
    async fn the_quota_and_the_circuit_breaker_make_a_factor_unavailable() {
        let e = env(vec![]).await;
        // Lifecycle hooks are exempt from the quota; factor calls are not.
        e.install_wasm(
            "fq",
            "timeout_ms: 5000, invocations_per_minute: 2",
            &Guest::default(),
        )
        .await;
        let rec = e.enroll("fq:key", json!({})).await;
        let f = e.manager.auth_factors();
        let ctx = e.ctx(vec![FactorRecord {
            id: rec,
            data: json!({}),
        }]);
        assert!(f[0].begin_challenge(&ctx).await.is_ok());
        assert!(f[0].verify(&ctx, json!({})).await.is_ok());
        assert!(is_unavailable(&f[0].verify(&ctx, json!({})).await));

        // Five traps open the breaker: the plugin is isolated and its factor
        // leaves the registry, so a login that needs it is refused.
        e.manager.disable("fq").await.unwrap();
        let g = Guest {
            verify: Out::Trap,
            ..Guest::default()
        };
        e.install_wasm("fz", "timeout_ms: 5000", &g).await;
        let rec = e.enroll("fz:key", json!({})).await;
        let f = e.manager.auth_factors();
        let ctx = e.ctx(vec![FactorRecord {
            id: rec,
            data: json!({}),
        }]);
        for _ in 0..5 {
            assert!(is_unavailable(&f[0].verify(&ctx, json!({})).await));
        }
        assert!(e.manager.auth_factor_registry().ids().is_empty());
        assert_eq!(
            e.manager.get("fz").await.unwrap().summary.state,
            PluginState::Error
        );
        assert!(is_unavailable(&f[0].verify(&ctx, json!({})).await));
        assert_eq!(denied_code(e.login().await), "factor_unavailable");
    }

    // ---- availability --------------------------------------------------------

    #[tokio::test]
    async fn a_disabled_or_unapproved_plugin_factor_is_neither_offered_nor_usable() {
        let e = env(vec![]).await;
        e.install_wasm("fa", "timeout_ms: 5000", &Guest::default())
            .await;
        let rec = e.enroll("fa:key", json!({})).await;
        let (_, factors) = e.challenge().await;
        assert_eq!(factors, vec!["fa:key".to_string()]);
        let held = e.manager.auth_factors();
        let ctx = e.ctx(vec![FactorRecord {
            id: rec,
            data: json!({}),
        }]);
        assert!(held[0].begin_challenge(&ctx).await.is_ok());

        // Approval withdrawn while enabled: every call is refused at once.
        e.manager
            .approve_capabilities("fa", &approve(&[]), "admin")
            .await
            .unwrap();
        assert!(is_unavailable(&held[0].begin_challenge(&ctx).await));
        e.manager
            .approve_capabilities("fa", &approve(&[FACTOR_CAPABILITY]), "admin")
            .await
            .unwrap();
        assert!(held[0].begin_challenge(&ctx).await.is_ok());

        // Disabled: not offered, not usable, and a factor held across the
        // disable is refused.
        e.manager.disable("fa").await.unwrap();
        assert!(is_unavailable(&held[0].begin_challenge(&ctx).await));
        assert_eq!(denied_code(e.login().await), "factor_unavailable");
        let u = e.user.clone();
        assert!(matches!(
            e.pipeline()
                .start_step_up(&u, "no-session", &ClientInfo::default())
                .await,
            Err(PipelineError::Unavailable(_))
        ));
        assert!(matches!(
            e.pipeline().begin_enroll(&u, "fa:key").await,
            Err(PipelineError::Invalid(_))
        ));

        // With recovery codes the user is offered those only.
        let (_, stored) = generate_recovery_codes().unwrap();
        e.pipeline()
            .store()
            .replace_recovery(e.user.id, &stored)
            .await
            .unwrap();
        let (_, factors) = e.challenge().await;
        assert_eq!(factors, vec!["recovery".to_string()]);
        assert_eq!(e.sessions().await, 0);
    }

    // ---- isolation -----------------------------------------------------------

    #[tokio::test]
    async fn a_plugin_only_receives_records_of_its_own_factor() {
        let e = env(vec![]).await;
        let echo = Guest {
            challenge: Out::Echo,
            ..Guest::default()
        };
        e.install_wasm("pa", "timeout_ms: 5000", &echo).await;
        e.install_wasm("pb", "timeout_ms: 5000", &echo).await;
        let a1 = e.enroll("pa:key", json!({"n": "a1"})).await;
        let a2 = e.enroll("pa:key", json!({"n": "a2"})).await;
        let b1 = e.enroll("pb:key", json!({"n": "b1"})).await;
        let w1 = e.enroll("webauthn", json!({"passkey": {}})).await;
        let t1 = e.enroll("totp", json!({"secret": "x"})).await;
        let c = ClientInfo::default();
        for (factor, mine, others) in [
            ("pa:key", vec![&a1, &a2], vec![&b1, &w1, &t1]),
            ("pb:key", vec![&b1], vec![&a1, &a2, &w1, &t1]),
        ] {
            let (id, offered) = e.challenge().await;
            // The built-in WebAuthn factor is not configured: not offered.
            assert_eq!(offered, vec!["totp", "pa:key", "pb:key"]);
            let echoed = e.pipeline().begin_challenge(&id, factor, &c).await.unwrap();
            assert_eq!(echoed["kind"], "factor");
            assert_eq!(echoed["handler"], BEGIN_CHALLENGE);
            let input = &echoed["input"];
            assert_eq!(input["factor"], "key");
            assert_eq!(input["user_id"], e.user.id.to_string());
            assert_eq!(input["username"], "alice");
            assert!(input["flow_state"].is_null());
            assert!(input["now_unix"].as_u64().unwrap() > 0);
            let got: HashSet<String> = input["records"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_string())
                .collect();
            let want: HashSet<String> = mine.iter().map(|s| s.to_string()).collect();
            assert_eq!(got, want, "{factor}");
            let text = echoed.to_string();
            for other in others {
                assert!(!text.contains(other.as_str()), "{factor} saw {other}");
            }
        }
    }

    #[tokio::test]
    async fn inputs_and_results_use_the_m29_field_names() {
        let cap = Arc::new(Capture::default());
        let e = env(vec![("cap", cap.clone() as Arc<dyn BuiltinPlugin>)]).await;
        let y = factor_yaml("cap", "{ kind: builtin, entry: cap }");
        let pkg = build_signed_package(&[("plugin.yaml", y.as_bytes())], "cap", "1.0.0", 2, SEED);
        e.manager.install(&pkg, "admin", None).await.unwrap();
        e.manager.enable("cap").await.unwrap();
        let other = e.enroll("other:key", json!({"n": "o"})).await;
        let web = e.enroll("webauthn", json!({"passkey": {}})).await;
        let p = e.pipeline();
        let u = e.user.clone();

        let client = p.begin_enroll(&u, "cap:key").await.unwrap();
        assert_eq!(client, json!({"options": 1}));
        let resp = json!({"credential": {"id": "x"}, "label": "My key"});
        p.complete_enroll(&u, "cap:key", resp.clone())
            .await
            .unwrap();
        let listed = p.store().list(u.id).await.unwrap();
        let mine = listed.iter().find(|f| f.kind == "cap:key").unwrap();
        assert!(mine.verified);
        assert_eq!(mine.label, "My key");

        let (id, _) = e.challenge().await;
        let c = ClientInfo::default();
        assert_eq!(
            p.begin_challenge(&id, "cap:key", &c).await.unwrap(),
            json!({"options": 2})
        );
        let r = p
            .verify_challenge(&id, Some("cap:key"), json!({"assertion": 1}), &c)
            .await
            .unwrap();
        assert!(matches!(r, VerifyOutcome::Session(_)));
        // The update was written with compare-and-set.
        let rec = p.store().records(u.id, "cap:key", true).await.unwrap();
        assert_eq!(rec[0].data, json!({"credential": "c1", "n": 1}));

        let calls = cap.calls.lock().unwrap().clone();
        let handlers: Vec<&str> = calls.iter().map(|c| c.1.as_str()).collect();
        assert_eq!(
            handlers,
            [BEGIN_ENROLL, COMPLETE_ENROLL, BEGIN_CHALLENGE, VERIFY]
        );
        assert!(calls.iter().all(|c| c.0 == HandlerKind::Factor));
        let keys = |v: &Value| -> Vec<String> {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        let (be, ce, bc, ve) = (&calls[0].2, &calls[1].2, &calls[2].2, &calls[3].2);
        assert_eq!(
            keys(be),
            ["existing", "factor", "now_unix", "pending", "user_id", "username"]
        );
        assert_eq!(be["existing"], json!([]));
        assert!(be["pending"].is_null());
        assert_eq!(be["user_id"], u.id.to_string());
        assert_eq!(
            keys(ce),
            ["existing", "factor", "now_unix", "pending", "response", "user_id", "username"]
        );
        assert_eq!(ce["pending"]["id"], json!(rec[0].id));
        assert_eq!(ce["pending"]["data"], json!({"state": "p"}));
        assert_eq!(ce["response"], resp);
        assert_eq!(
            keys(bc),
            [
                "factor",
                "flow_state",
                "now_unix",
                "records",
                "user_id",
                "username"
            ]
        );
        assert_eq!(
            bc["records"],
            json!([{"id": rec[0].id, "data": {"credential": "c1", "n": 0}}])
        );
        assert!(bc["flow_state"].is_null());
        // The flow state came back from the host, sealed in between.
        assert_eq!(ve["flow_state"], "fs-1");
        assert_eq!(ve["response"], json!({"assertion": 1}));
        let text = serde_json::to_string(&calls.iter().map(|c| &c.2).collect::<Vec<_>>()).unwrap();
        assert!(!text.contains(&other) && !text.contains(&web));
    }

    // ---- wire format ---------------------------------------------------------

    fn rec(id: &str) -> FactorRecord {
        FactorRecord {
            id: id.into(),
            data: json!({}),
        }
    }

    fn malformed<T>(r: Result<T, Reject>) -> bool {
        matches!(r, Err(Reject::Malformed(_)))
    }

    #[test]
    fn plugin_errors_map_one_to_one_onto_factor_errors() {
        let err =
            |kind: &str, code: &str| split_error(json!({"error": {"kind": kind, "code": code}}));
        assert!(matches!(err("unavailable", "no_usable_credential"),
            Err(Reject::Factor(FactorError::Unavailable(c))) if c == "no_usable_credential"));
        assert!(matches!(err("invalid", "bad.response-1"),
            Err(Reject::Factor(FactorError::Invalid(c))) if c == "bad.response-1"));
        assert!(matches!(err("internal", "x"),
            Err(Reject::Factor(FactorError::Internal(c))) if c == "x"));
        // Unsafe codes never reach a client.
        assert!(matches!(err("invalid", "<script>"),
            Err(Reject::Factor(FactorError::Invalid(c))) if c == "plugin_error"));
        assert!(malformed(err("fatal", "x")));
        assert!(malformed(split_error(json!({"error": "boom"}))));
        assert!(malformed(split_error(
            json!({"error": {"kind": "invalid"}})
        )));
        assert!(malformed(split_error(
            json!({"error": {"kind": "invalid", "code": "x"}, "outcome": "verified"})
        )));
        assert_eq!(
            split_error(json!({"client": 1})).unwrap(),
            json!({"client": 1})
        );
    }

    #[test]
    fn enroll_and_challenge_answers_are_strict() {
        let s =
            parse_enroll_start(json!({"client": {"a": 1}, "mode": {"pending": {"s": 1}}})).unwrap();
        assert_eq!(s.client, json!({"a": 1}));
        assert!(matches!(s.mode, EnrollMode::Pending(v) if v == json!({"s": 1})));
        let s = parse_enroll_start(json!({"client": null, "mode": {"immediate": [1]}})).unwrap();
        assert!(matches!(s.mode, EnrollMode::Immediate(v) if v == json!([1])));
        assert!(malformed(parse_enroll_start(
            json!({"client": 1, "mode": {"pending": 1, "immediate": 2}})
        )));
        assert!(malformed(parse_enroll_start(
            json!({"client": 1, "mode": {}})
        )));
        assert!(malformed(parse_enroll_start(
            json!({"client": 1, "mode": "pending"})
        )));
        assert!(malformed(parse_enroll_start(
            json!({"mode": {"pending": 1}})
        )));
        assert!(malformed(parse_enroll_start(
            json!({"client": 1, "mode": {"pending": 1}, "x": 1})
        )));

        let r = parse_enroll_record(json!({"data": {"c": 1}, "label": "Key"})).unwrap();
        assert_eq!((r.data, r.label.as_deref()), (json!({"c": 1}), Some("Key")));
        assert!(parse_enroll_record(json!({"data": 1}))
            .unwrap()
            .label
            .is_none());
        assert!(matches!(
            parse_enroll_record(json!({"data": 1, "label": "x".repeat(65)})),
            Err(Reject::Factor(FactorError::Invalid(c))) if c == "invalid_label"
        ));
        assert!(matches!(
            parse_enroll_record(json!({"data": 1, "label": "a\nb"})),
            Err(Reject::Factor(FactorError::Invalid(_)))
        ));
        assert!(malformed(parse_enroll_record(
            json!({"data": 1, "label": 3})
        )));
        assert!(malformed(parse_enroll_record(json!({"label": "x"}))));

        let c = parse_challenge_start(json!({"client": {"o": 1}, "flow_state": "s"})).unwrap();
        assert_eq!(c.flow_state.as_deref(), Some("s"));
        assert!(parse_challenge_start(json!({"client": 1}))
            .unwrap()
            .flow_state
            .is_none());
        assert!(malformed(parse_challenge_start(
            json!({"client": 1, "flow_state": 7})
        )));
        assert!(malformed(parse_challenge_start(
            json!({"client": 1, "flow_state": "x".repeat(MAX_FLOW_STATE_BYTES + 1)})
        )));
        assert!(malformed(parse_challenge_start(json!([1]))));
    }

    #[test]
    fn verify_answers_never_turn_an_error_into_a_pass() {
        let records = [rec("r1"), rec("r2")];
        let ok = parse_outcome(
            json!({"outcome": "verified", "amr": "hwk", "updates": [["r2", {"n": 1}]]}),
            "hwk",
            &records,
        )
        .unwrap();
        assert!(matches!(ok, FactorOutcome::Verified { amr, updates }
            if amr == "hwk" && updates == vec![("r2".to_string(), json!({"n": 1}))]));
        assert!(matches!(
            parse_outcome(
                json!({"outcome": "failed", "retryable": true}),
                "hwk",
                &records
            ),
            Ok(FactorOutcome::Failed { retryable: true })
        ));
        assert!(matches!(
            parse_outcome(json!({"outcome": "pending"}), "hwk", &records),
            Ok(FactorOutcome::Pending)
        ));
        for bad in [
            json!({"outcome": "verified", "amr": "otp"}),
            json!({"outcome": "verified"}),
            json!({"outcome": "verified", "amr": "hwk", "updates": [["r3", {}]]}),
            json!({"outcome": "verified", "amr": "hwk", "updates": [["r1", {}], ["r1", {}]]}),
            json!({"outcome": "verified", "amr": "hwk", "updates": [["r1"]]}),
            json!({"outcome": "verified", "amr": "hwk", "updates": [["r1", {}, 1]]}),
            json!({"outcome": "verified", "amr": "hwk", "updates": [[1, {}]]}),
            json!({"outcome": "verified", "amr": "hwk", "updates": {"r1": {}}}),
            json!({"outcome": "verified", "amr": "hwk", "session": "admin"}),
            json!({"outcome": "failed"}),
            json!({"outcome": "failed", "retryable": "yes"}),
            json!({"outcome": "pending", "amr": "hwk"}),
            json!({"outcome": "Verified", "amr": "hwk"}),
            json!({"amr": "hwk"}),
            json!(true),
            json!(null),
        ] {
            assert!(
                malformed(parse_outcome(bad.clone(), "hwk", &records)),
                "{bad}"
            );
        }
    }
}
