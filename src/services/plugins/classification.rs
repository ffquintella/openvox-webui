//! Classification extensions for plugins (M27): pure functions, operators and
//! merge strategies on the hot path, membership providers feeding dynamic
//! groups behind a churn guard, and template-generated groups.
//! See docs/architecture/PLUGIN_PLATFORM.md sections 13 and 17.8.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::events::EventBus;
use super::facts::valid_certname;
use super::invoke::{invoke_with_timeout, HandlerKind, HookIndex, PluginHooks};
use super::manager::PluginManager;
use crate::config::PluginClassificationConfig;
use crate::db::plugin_classification_repository::MembershipState;
use crate::db::repository::GroupRepository;
use crate::db::PluginClassificationRepository;
use crate::models::*;
use crate::services::classification::ClassificationService;
use crate::services::classification_expr::{ClassificationExtensions, Expr, PluginUseKind};
use crate::services::puppetdb::PuppetDbClient;

/// Audit resource type of everything in this module.
pub const AUDIT_PLUGIN_CLASSIFICATION: &str = "plugin_classification";

const MAX_CACHE_VALUE_BYTES: usize = 8 * 1024;
const MEMBERS_CACHE_TTL: Duration = Duration::from_secs(5);
const ALERT_REPEAT: Duration = Duration::from_secs(300);
const MAX_LISTED_CERTNAMES: usize = 100;
const MAX_DISTINCT_FACT_VALUES: usize = 5_000;

#[derive(Debug, thiserror::Error)]
pub enum ClassificationError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    /// Not allowed in the current state (busy, not approved, plugin disabled).
    #[error("{0}")]
    Conflict(String),
    /// The plugin failed, timed out or its runtime is unavailable.
    #[error("{0}")]
    Failed(String),
}

type MembersSnapshot = Arc<HashMap<Uuid, Vec<(String, DateTime<Utc>)>>>;

/// In-memory state of the classification extensions: the cache of pure call
/// results, the refresh bulkhead and the alert throttle.
pub struct ClassificationRuntime {
    cfg: PluginClassificationConfig,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
    refreshing: Mutex<HashSet<Uuid>>,
    alerted: Mutex<HashMap<String, Instant>>,
    members: Mutex<Option<(Instant, MembersSnapshot)>>,
}

impl ClassificationRuntime {
    pub fn new(cfg: PluginClassificationConfig) -> Self {
        Self {
            cfg,
            cache: Mutex::new(HashMap::new()),
            refreshing: Mutex::new(HashSet::new()),
            alerted: Mutex::new(HashMap::new()),
            members: Mutex::new(None),
        }
    }

    pub fn config(&self) -> &PluginClassificationConfig {
        &self.cfg
    }

    fn cache_get(&self, key: &str) -> Option<Value> {
        let ttl = Duration::from_secs(self.cfg.cache_ttl_secs.max(1));
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .get(key)
            .filter(|(at, _)| at.elapsed() < ttl)
            .map(|(_, v)| v.clone())
    }

    fn cache_put(&self, key: String, value: Value) {
        if value.to_string().len() > MAX_CACHE_VALUE_BYTES {
            return;
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= self.cfg.cache_entries.max(1) {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), value));
    }

    /// Drops cached results of a plugin (upgrade, disable, uninstall).
    pub fn forget(&self, plugin_id: &str) {
        let prefix = format!("{plugin_id}\u{0}");
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|k, _| !k.starts_with(&prefix));
    }

    /// True once per `ALERT_REPEAT` for a key, so a failing plugin does not
    /// flood the event bus.
    fn should_alert(&self, key: &str) -> bool {
        let mut alerted = self.alerted.lock().unwrap_or_else(|e| e.into_inner());
        alerted.retain(|_, at| at.elapsed() < ALERT_REPEAT);
        if alerted.contains_key(key) {
            return false;
        }
        alerted.insert(key.to_string(), Instant::now());
        true
    }

    fn invalidate_members(&self) {
        *self.members.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

// ============================================================================
// The pure extensions used on the hot path
// ============================================================================

/// The plugin vocabulary of one classification run. Implements the sync
/// [`ClassificationExtensions`] by running the plugin's pure handler on a
/// blocking section of the current multi-thread runtime; on any other runtime
/// the calls are unavailable instead of blocking it.
pub struct PluginExtensions {
    manager: Arc<PluginManager>,
    hooks: Arc<HookIndex>,
    handle: tokio::runtime::Handle,
    spent: Mutex<Duration>,
    degraded: Mutex<BTreeSet<String>>,
}

impl PluginExtensions {
    fn new(manager: Arc<PluginManager>, handle: tokio::runtime::Handle) -> Self {
        let hooks = manager.hooks();
        Self {
            manager,
            hooks,
            handle,
            spent: Mutex::new(Duration::ZERO),
            degraded: Mutex::new(BTreeSet::new()),
        }
    }

    fn note(&self, reason: String) {
        self.degraded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(reason);
    }

    fn plugin(&self, ext: &ExtRef) -> Result<(&PluginHooks, &ClassificationContribution), String> {
        let hooks = self
            .hooks
            .get(&ext.plugin_id)
            .ok_or_else(|| format!("plugin '{}' is not enabled", ext.plugin_id))?;
        let c = hooks.classification.as_ref().ok_or_else(|| {
            format!(
                "plugin '{}' has no classification extensions",
                ext.plugin_id
            )
        })?;
        Ok((hooks, c))
    }

    /// One bounded, cached call of a pure handler. `Err` means unavailable.
    fn run(
        &self,
        hooks: &PluginHooks,
        handler: &str,
        what: &str,
        input: Value,
    ) -> Result<Value, String> {
        let rt = self.manager.classification_runtime();
        let cfg = rt.config();
        let key = format!(
            "{}\u{0}{}\u{0}{}\u{0}{}",
            hooks.plugin_id, hooks.version, handler, input
        );
        if let Some(v) = rt.cache_get(&key) {
            return Ok(v);
        }
        let budget = Duration::from_millis(cfg.node_budget_ms.max(1));
        if *self.spent.lock().unwrap_or_else(|e| e.into_inner()) >= budget {
            let e = format!(
                "{what}: the {} ms budget per node is spent",
                cfg.node_budget_ms
            );
            self.note(e.clone());
            return Err(e);
        }
        if !matches!(
            self.handle.runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread
        ) {
            let e = format!("{what}: no multi-thread runtime to run the plugin on");
            self.note(e.clone());
            return Err(e);
        }
        let mut call = hooks.call(HandlerKind::Classification, handler, input);
        let limits = call.runtime.limits.get_or_insert_with(Default::default);
        limits.fuel_per_call = Some(
            limits
                .fuel_per_call
                .map_or(cfg.fuel_per_call, |f| f.min(cfg.fuel_per_call))
                .max(1),
        );
        limits.timeout_ms = Some(cfg.call_timeout_ms.max(1));
        let (manager, timeout) = (self.manager.clone(), cfg.call_timeout_ms.max(1));
        let started = Instant::now();
        let outcome = tokio::task::block_in_place(|| {
            self.handle
                .block_on(async move { invoke_with_timeout(manager.as_ref(), call, timeout).await })
        });
        *self.spent.lock().unwrap_or_else(|e| e.into_inner()) += started.elapsed();
        match outcome {
            Ok(v) => {
                rt.cache_put(key, v.clone());
                Ok(v)
            }
            Err(e) => {
                let e = format!("{what}: {e}");
                self.note(e.clone());
                Err(e)
            }
        }
    }
}

impl ClassificationExtensions for PluginExtensions {
    fn call_function(&self, ext: &ExtRef, args: &[Value]) -> Result<Option<Value>, String> {
        let (hooks, c) = self
            .plugin(ext)
            .inspect_err(|e| self.note(format!("{ext}: {e}")))?;
        let decl = c.functions.iter().find(|f| f.id == ext.id).ok_or_else(|| {
            let e = format!("{ext} is not provided by the plugin");
            self.note(e.clone());
            e
        })?;
        if args.len() != decl.args.len() {
            return Err(format!(
                "{ext} takes {} arguments, got {}",
                decl.args.len(),
                args.len()
            ));
        }
        // A fact of another type than declared is data, not a failure: the
        // function does not apply and the comparison does not match.
        if args.iter().zip(&decl.args).any(|(v, t)| !t.accepts(v)) {
            return Ok(None);
        }
        let out = self.run(
            hooks,
            &decl.handler,
            &ext.full(),
            json!({"kind": "function", "id": decl.id, "args": args}),
        )?;
        let value = out.get("value").cloned().unwrap_or(Value::Null);
        if !decl.returns.accepts(&value) {
            let e = format!("{ext} returned a value of the wrong type");
            self.note(e.clone());
            return Err(e);
        }
        Ok(Some(value))
    }

    fn call_operator(&self, ext: &ExtRef, left: &Value, right: &Value) -> Result<bool, String> {
        let (hooks, c) = self
            .plugin(ext)
            .inspect_err(|e| self.note(format!("{ext}: {e}")))?;
        let decl = c.operators.iter().find(|o| o.id == ext.id).ok_or_else(|| {
            let e = format!("{ext} is not provided by the plugin");
            self.note(e.clone());
            e
        })?;
        if !decl.arg_types[0].accepts(left) || !decl.arg_types[1].accepts(right) {
            return Ok(false);
        }
        let out = self.run(
            hooks,
            &decl.handler,
            &ext.full(),
            json!({"kind": "operator", "id": decl.id, "left": left, "right": right}),
        )?;
        match out.get("result").and_then(Value::as_bool) {
            Some(b) => Ok(b),
            None => {
                let e = format!("{ext} must answer {{\"result\": true|false}}");
                self.note(e.clone());
                Err(e)
            }
        }
    }

    fn merge(
        &self,
        ext: &ExtRef,
        key: &str,
        current: &Value,
        incoming: &Value,
    ) -> Result<Value, String> {
        let unavailable = |e: String| {
            self.note(format!("{ext}: {e}; merged with deep"));
            e
        };
        let (hooks, c) = self.plugin(ext).map_err(unavailable)?;
        let decl = c
            .merge_strategies
            .iter()
            .find(|s| s.id == ext.id)
            .ok_or_else(|| unavailable("the strategy is not provided by the plugin".into()))?;
        let out = self.run(
            hooks,
            &decl.handler,
            &ext.full(),
            json!({"kind": "merge", "id": decl.id, "key": key, "current": current, "incoming": incoming}),
        )?;
        out.get("value")
            .cloned()
            .ok_or_else(|| unavailable("the strategy must answer {\"value\": ...}".into()))
    }

    fn begin_node(&self) {
        *self.spent.lock().unwrap_or_else(|e| e.into_inner()) = Duration::ZERO;
    }

    fn degraded(&self) -> Vec<String> {
        self.degraded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

impl Drop for PluginExtensions {
    /// Publishes `classification.degraded` for what was unavailable, at most
    /// once per reason every few minutes.
    fn drop(&mut self) {
        let reasons: Vec<String> =
            std::mem::take(&mut *self.degraded.lock().unwrap_or_else(|e| e.into_inner()))
                .into_iter()
                .collect();
        if reasons.is_empty() {
            return;
        }
        let rt = self.manager.classification_runtime().clone();
        let fresh: Vec<String> = reasons.into_iter().filter(|r| rt.should_alert(r)).collect();
        if fresh.is_empty() {
            return;
        }
        let pool = self.manager.pool().clone();
        self.handle.spawn(async move {
            EventBus::new(pool)
                .emit(
                    "classification.degraded",
                    json!({"reasons": fresh}),
                    EventActor::system(),
                )
                .await;
        });
    }
}

// ============================================================================
// Catalog and validation
// ============================================================================

/// One function, operator or merge strategy of the catalog served by the API.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub description: Option<String>,
    pub args: Vec<ValueType>,
    pub returns: Option<ValueType>,
}

/// What enabled plugins offer to classification.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExtensionCatalog {
    pub functions: Vec<CatalogEntry>,
    pub operators: Vec<CatalogEntry>,
    pub merge_strategies: Vec<CatalogEntry>,
}

/// Everything one plugin contributes to classification, for the API.
#[derive(Debug, Clone, Serialize)]
pub struct PluginClassificationView {
    pub plugin_id: String,
    pub version: String,
    pub functions: Vec<FunctionDecl>,
    pub operators: Vec<OperatorDecl>,
    pub merge_strategies: Vec<MergeStrategyDecl>,
    pub providers: Vec<ProviderView>,
    pub templates: Vec<TemplateView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub decl: MembershipProviderDecl,
    pub groups: Vec<MembershipProviderStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateView {
    #[serde(flatten)]
    pub decl: GroupTemplateDecl,
    pub instances: Vec<GroupTemplateInstance>,
}

fn catalog_of(h: &PluginHooks) -> ExtensionCatalog {
    let mut out = ExtensionCatalog::default();
    let Some(c) = &h.classification else {
        return out;
    };
    let entry = |id: &str, d: &Option<String>, args: Vec<ValueType>, returns| CatalogEntry {
        id: ExtRef {
            plugin_id: h.plugin_id.clone(),
            id: id.to_string(),
        }
        .full(),
        plugin_id: h.plugin_id.clone(),
        plugin_version: h.version.clone(),
        description: d.clone(),
        args,
        returns,
    };
    out.functions = c
        .functions
        .iter()
        .map(|f| entry(&f.id, &f.description, f.args.clone(), Some(f.returns)))
        .collect();
    out.operators = c
        .operators
        .iter()
        .map(|o| {
            entry(
                &o.id,
                &o.description,
                o.arg_types.to_vec(),
                Some(ValueType::Bool),
            )
        })
        .collect();
    out.merge_strategies = c
        .merge_strategies
        .iter()
        .map(|s| entry(&s.id, &s.description, vec![], None))
        .collect();
    out
}

/// Validates the plugin references of a group expression, merge strategy and
/// membership provider against the enabled plugins. Unknown ones are refused
/// at save time; one that disappears later is only unavailable (never matches).
pub fn validate_group_extensions(
    hooks: &HookIndex,
    expression: Option<&Value>,
    behavior: &GroupBehavior,
) -> Result<(), String> {
    let find = |r: &ExtRef| {
        hooks
            .get(&r.plugin_id)
            .and_then(|h| h.classification.as_ref())
    };
    if let Some(e) = expression {
        let parsed = Expr::parse(e)?;
        for u in parsed.plugin_uses() {
            let c = find(&u.ext).ok_or_else(|| {
                format!(
                    "{} needs the plugin '{}', which is not enabled",
                    u.ext, u.ext.plugin_id
                )
            })?;
            match u.kind {
                PluginUseKind::Function => {
                    let f = c
                        .functions
                        .iter()
                        .find(|f| f.id == u.ext.id)
                        .ok_or_else(|| format!("{} is not a function of the plugin", u.ext))?;
                    if f.args.len() != u.args {
                        return Err(format!(
                            "{} takes {} arguments, got {}",
                            u.ext,
                            f.args.len(),
                            u.args
                        ));
                    }
                    if u.as_condition && f.returns != ValueType::Bool {
                        return Err(format!(
                            "{} returns a value; compare it with an operator",
                            u.ext
                        ));
                    }
                }
                PluginUseKind::Operator => {
                    if !c.operators.iter().any(|o| o.id == u.ext.id) {
                        return Err(format!("{} is not an operator of the plugin", u.ext));
                    }
                }
            }
        }
    }
    if let MergeStrategy::Plugin(r) = &behavior.merge {
        let known = find(r).is_some_and(|c| c.merge_strategies.iter().any(|s| s.id == r.id));
        if !known {
            return Err(format!(
                "merge strategy {r} is not provided by an enabled plugin"
            ));
        }
    }
    if behavior.kind == GroupKind::Dynamic {
        let m = behavior
            .membership
            .as_ref()
            .ok_or("a dynamic group needs a membership provider")?;
        let known = hooks
            .get(&m.plugin_id)
            .and_then(|h| h.classification.as_ref())
            .is_some_and(|c| c.membership_providers.iter().any(|p| p.id == m.provider_id));
        if !known {
            return Err(format!(
                "membership provider {}/{} is not provided by an enabled plugin",
                m.plugin_id, m.provider_id
            ));
        }
    }
    Ok(())
}

/// Replaces `{{key}}` in every string of a JSON template.
fn substitute(v: &Value, key: &str) -> Value {
    match v {
        Value::String(s) => Value::String(s.replace("{{key}}", key)),
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute(x, key)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.replace("{{key}}", key), substitute(x, key)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A value of the template's fact usable in a group name: scalars of
/// letters, digits, `.`, `_`, `-` and spaces, at most 100 characters.
fn scalar_key(v: &Value) -> Option<String> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return None,
    };
    let ok = !s.is_empty()
        && s.chars().count() <= 100
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '));
    ok.then_some(s)
}

// ============================================================================
// Manager API
// ============================================================================

/// What `apply_membership` did.
#[derive(Debug, Clone, Serialize)]
pub struct MembershipOutcome {
    pub applied: bool,
    pub held: bool,
    pub added: usize,
    pub removed: usize,
    pub members: usize,
    pub churn_pct: f64,
}

/// A change proposed by a provider.
pub enum MembershipChange {
    Replace(Vec<String>),
    Patch {
        add: Vec<String>,
        remove: Vec<String>,
    },
}

/// What one reconciliation of a template did.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateRun {
    pub created: usize,
    pub updated: usize,
    pub removed: usize,
    pub groups: usize,
    pub truncated: bool,
}

impl PluginManager {
    fn class_repo(&self) -> PluginClassificationRepository<'_> {
        PluginClassificationRepository::new(self.pool())
    }

    /// Enabled plugins that declare classification extensions.
    pub fn classification_plugins(&self) -> Vec<PluginHooks> {
        self.hooks()
            .plugins()
            .filter(|h| h.classification.as_ref().is_some_and(|c| !c.is_empty()))
            .cloned()
            .collect()
    }

    /// Functions, operators and merge strategies of the enabled plugins.
    pub fn extension_catalog(&self) -> ExtensionCatalog {
        let mut cat = ExtensionCatalog::default();
        for h in self.classification_plugins() {
            let c = catalog_of(&h);
            cat.functions.extend(c.functions);
            cat.operators.extend(c.operators);
            cat.merge_strategies.extend(c.merge_strategies);
        }
        cat
    }

    /// Plugin references and dynamic groups of a proposed group, checked
    /// against the enabled plugins.
    pub fn validate_group_extensions(
        &self,
        expression: Option<&Value>,
        behavior: &GroupBehavior,
    ) -> Result<(), String> {
        validate_group_extensions(&self.hooks(), expression, behavior)
    }

    /// A classification service with the plugin vocabulary and the members of
    /// the dynamic groups. Without plugin extensions and dynamic groups this
    /// is the plain service and reads nothing.
    pub async fn classification_service(
        self: &Arc<Self>,
        groups: Vec<NodeGroup>,
    ) -> ClassificationService {
        let members = if groups.iter().any(|g| g.behavior.kind == GroupKind::Dynamic) {
            self.dynamic_members(&groups).await
        } else {
            HashMap::new()
        };
        let has_ext = !self.classification_plugins().is_empty()
            && groups.iter().any(|g| {
                g.expression.is_some() || matches!(g.behavior.merge, MergeStrategy::Plugin(_))
            });
        let svc = ClassificationService::new(groups).with_members(members);
        match (has_ext, tokio::runtime::Handle::try_current()) {
            (true, Ok(handle)) => {
                svc.with_extensions(Arc::new(PluginExtensions::new(self.clone(), handle)))
            }
            _ => svc,
        }
    }

    /// The materialized members of the dynamic groups among `groups`, with
    /// each provider's stale policy applied.
    pub async fn dynamic_members(&self, groups: &[NodeGroup]) -> HashMap<Uuid, DynamicMembers> {
        let snapshot = self.members_snapshot().await;
        let states: HashMap<Uuid, MembershipState> = self
            .class_repo()
            .states()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.group_id, s))
            .collect();
        let hooks = self.hooks();
        let now = Utc::now();
        let mut out = HashMap::new();
        for g in groups
            .iter()
            .filter(|g| g.behavior.kind == GroupKind::Dynamic)
        {
            let Some(cfg) = &g.behavior.membership else {
                continue;
            };
            let decl = hooks
                .get(&cfg.plugin_id)
                .and_then(|h| h.classification.as_ref())
                .and_then(|c| {
                    c.membership_providers
                        .iter()
                        .find(|p| p.id == cfg.provider_id)
                });
            let (max_stale, policy) = decl
                .map(|d| (d.max_stale(), d.on_stale()))
                .unwrap_or((DEFAULT_MEMBERSHIP_MAX_STALE_S, OnStale::Keep));
            let rows = snapshot.get(&g.id).map(Vec::as_slice).unwrap_or(&[]);
            let newest = rows.iter().map(|(_, at)| *at).max();
            let stale = newest.is_some_and(|t| (now - t).num_seconds() > max_stale as i64);
            let failed = states.get(&g.id).is_some_and(|s| s.last_status == "failed");
            let keep = match policy {
                OnStale::Freeze => true,
                OnStale::Keep => !stale,
                OnStale::Drop => !stale && !failed,
            };
            out.insert(
                g.id,
                DynamicMembers {
                    certnames: if keep {
                        rows.iter().map(|(c, _)| c.clone()).collect()
                    } else {
                        HashSet::new()
                    },
                    stale,
                },
            );
        }
        out
    }

    async fn members_snapshot(&self) -> MembersSnapshot {
        let rt = self.classification_runtime();
        if let Some((at, s)) = rt
            .members
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            if at.elapsed() < MEMBERS_CACHE_TTL {
                return s.clone();
            }
        }
        let fresh: MembersSnapshot =
            Arc::new(self.class_repo().all_members().await.unwrap_or_default());
        *rt.members.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Instant::now(), fresh.clone()));
        fresh
    }

    /// Everything one plugin contributes, with provider status and template instances.
    pub async fn plugin_classification_view(
        &self,
        plugin_id: &str,
    ) -> Option<PluginClassificationView> {
        let hooks = self.hooks();
        let h = hooks.get(plugin_id)?;
        let c = h.classification.clone()?;
        let groups = self.dynamic_groups().await;
        let states: HashMap<Uuid, MembershipState> = self
            .class_repo()
            .states()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.group_id, s))
            .collect();
        let held: HashSet<Uuid> = self
            .class_repo()
            .all_held()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|h| h.group_id)
            .collect();
        let snapshot = self.members_snapshot().await;
        let now = Utc::now();
        let providers = c
            .membership_providers
            .iter()
            .map(|p| ProviderView {
                decl: p.clone(),
                groups: groups
                    .iter()
                    .filter(|g| {
                        g.behavior
                            .membership
                            .as_ref()
                            .is_some_and(|m| m.plugin_id == plugin_id && m.provider_id == p.id)
                    })
                    .map(|g| {
                        let st = states.get(&g.id);
                        let rows = snapshot.get(&g.id).map(Vec::as_slice).unwrap_or(&[]);
                        let newest = rows.iter().map(|(_, at)| *at).max();
                        MembershipProviderStatus {
                            group_id: g.id,
                            plugin_id: plugin_id.to_string(),
                            provider_id: p.id.clone(),
                            last_refresh_at: st.and_then(|s| s.last_refresh_at),
                            last_status: st.map_or("never".to_string(), |s| s.last_status.clone()),
                            last_error: st.and_then(|s| s.last_error.clone()),
                            members: rows.len() as u64,
                            stale: newest
                                .is_some_and(|t| (now - t).num_seconds() > p.max_stale() as i64),
                            held: held.contains(&g.id),
                        }
                    })
                    .collect(),
            })
            .collect();
        let instances = self.template_instances(Some(plugin_id)).await;
        let templates = c
            .group_templates
            .iter()
            .map(|t| TemplateView {
                decl: t.clone(),
                instances: instances
                    .iter()
                    .filter(|i| i.template_id == t.id)
                    .cloned()
                    .collect(),
            })
            .collect();
        Some(PluginClassificationView {
            plugin_id: plugin_id.to_string(),
            version: h.version.clone(),
            functions: c.functions,
            operators: c.operators,
            merge_strategies: c.merge_strategies,
            providers,
            templates,
        })
    }

    async fn all_groups(&self) -> Vec<NodeGroup> {
        GroupRepository::new(self.pool())
            .get_all_across_organizations()
            .await
            .unwrap_or_default()
    }

    async fn dynamic_groups(&self) -> Vec<NodeGroup> {
        self.all_groups()
            .await
            .into_iter()
            .filter(|g| g.behavior.kind == GroupKind::Dynamic)
            .collect()
    }

    async fn find_group(&self, id: Uuid) -> Result<NodeGroup, ClassificationError> {
        self.all_groups()
            .await
            .into_iter()
            .find(|g| g.id == id)
            .ok_or_else(|| ClassificationError::NotFound("group not found".into()))
    }

    /// Members of a group with their source, for `GET /groups/{id}/members`.
    pub async fn group_members(
        &self,
        group: &NodeGroup,
    ) -> Vec<(String, &'static str, Option<DateTime<Utc>>)> {
        let mut out: Vec<(String, &'static str, Option<DateTime<Utc>>)> = group
            .pinned_nodes
            .iter()
            .map(|c| (c.clone(), "pinned", None))
            .collect();
        if group.behavior.kind == GroupKind::Dynamic {
            let rows = self
                .class_repo()
                .members(group.id)
                .await
                .unwrap_or_default();
            out.extend(rows.into_iter().map(|(c, at)| (c, "dynamic", Some(at))));
        }
        out
    }

    /// Deletes what is stored for a deleted group.
    pub async fn forget_group_membership(&self, group_id: Uuid) {
        let _ = self.class_repo().forget_group(group_id).await;
        self.classification_runtime().invalidate_members();
    }

    pub async fn held_membership(&self, group_id: Uuid) -> Option<HeldMembership> {
        self.class_repo().held(group_id).await.ok().flatten()
    }

    pub async fn all_held_memberships(&self) -> Vec<HeldMembership> {
        self.class_repo().all_held().await.unwrap_or_default()
    }

    fn provider_decl(
        &self,
        plugin_id: &str,
        provider_id: &str,
    ) -> Result<(PluginHooks, MembershipProviderDecl), ClassificationError> {
        let hooks = self.hooks();
        let h = hooks.get(plugin_id).ok_or_else(|| {
            ClassificationError::Conflict(format!("plugin '{plugin_id}' is not enabled"))
        })?;
        let decl = h
            .classification
            .as_ref()
            .and_then(|c| c.membership_providers.iter().find(|p| p.id == provider_id))
            .cloned()
            .ok_or_else(|| {
                ClassificationError::Conflict(format!(
                    "plugin '{plugin_id}' has no membership provider '{provider_id}'"
                ))
            })?;
        Ok((h.clone(), decl))
    }

    // ---- membership: refresh, apply, churn guard --------------------------

    /// Runs a provider's refresh handler for one dynamic group and applies
    /// what it returns through the churn guard.
    pub async fn refresh_membership(
        &self,
        group_id: Uuid,
    ) -> Result<MembershipOutcome, ClassificationError> {
        let group = self.find_group(group_id).await?;
        let cfg = group
            .behavior
            .membership
            .clone()
            .filter(|_| group.behavior.kind == GroupKind::Dynamic)
            .ok_or_else(|| ClassificationError::Invalid("the group is not dynamic".into()))?;
        let (hooks, decl) = self.provider_decl(&cfg.plugin_id, &cfg.provider_id)?;
        {
            let mut busy = self
                .classification_runtime()
                .refreshing
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if !busy.insert(group_id) {
                return Err(ClassificationError::Conflict(
                    "a refresh of this group is already running".into(),
                ));
            }
        }
        let result = self.refresh_inner(&group, &cfg, &hooks, &decl).await;
        self.classification_runtime()
            .refreshing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&group_id);
        result
    }

    async fn record_membership(
        &self,
        group: &NodeGroup,
        cfg: &DynamicGroupConfig,
        status: &str,
        error: Option<&str>,
    ) {
        let _ = self
            .class_repo()
            .record_attempt(
                group.id,
                &cfg.plugin_id,
                &cfg.provider_id,
                status,
                error,
                Utc::now(),
            )
            .await;
    }

    async fn refresh_inner(
        &self,
        group: &NodeGroup,
        cfg: &DynamicGroupConfig,
        hooks: &PluginHooks,
        decl: &MembershipProviderDecl,
    ) -> Result<MembershipOutcome, ClassificationError> {
        let known = self
            .class_repo()
            .members(group.id)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let call = hooks.call(
            HandlerKind::Classification,
            &decl.handler,
            json!({
                "kind": "membership",
                "provider": decl.id,
                "group_id": group.id,
                "group_name": group.name,
                "params": cfg.params,
                "known_members": known,
            }),
        );
        let timeout = self.classification_runtime().config().refresh_timeout_ms;
        let reply = match invoke_with_timeout(self, call, timeout).await {
            Ok(v) => v,
            Err(e) => {
                let msg = e.to_string();
                self.record_membership(group, cfg, "failed", Some(&msg))
                    .await;
                return Err(ClassificationError::Failed(msg));
            }
        };
        let list = |k: &str| -> Result<Option<Vec<String>>, ClassificationError> {
            let bad = || ClassificationError::Invalid(format!("'{k}' must be a list of certnames"));
            match reply.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|v| v.as_str().map(str::to_string).ok_or_else(bad))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Some),
                Some(_) => Err(bad()),
            }
        };
        let parsed =
            (|| Ok::<_, ClassificationError>((list("members")?, list("add")?, list("remove")?)))();
        let change = match parsed {
            Ok((Some(members), None, None)) => MembershipChange::Replace(members),
            Ok((None, add, remove)) if add.is_some() || remove.is_some() => {
                MembershipChange::Patch {
                    add: add.unwrap_or_default(),
                    remove: remove.unwrap_or_default(),
                }
            }
            Ok((None, None, None)) => {
                // Nothing to apply (the plugin may have pushed with the host API)
                self.record_membership(group, cfg, "ok", None).await;
                return Ok(MembershipOutcome {
                    applied: false,
                    held: false,
                    added: 0,
                    removed: 0,
                    members: known,
                    churn_pct: 0.0,
                });
            }
            Ok(_) => {
                let msg = "answer either 'members' or 'add'/'remove', not both".to_string();
                self.record_membership(group, cfg, "failed", Some(&msg))
                    .await;
                return Err(ClassificationError::Invalid(msg));
            }
            Err(e) => {
                self.record_membership(group, cfg, "failed", Some(&e.to_string()))
                    .await;
                return Err(e);
            }
        };
        let outcome = self.apply_membership(group, cfg, decl, change, false).await;
        if let Err(e) = &outcome {
            self.record_membership(group, cfg, "failed", Some(&e.to_string()))
                .await;
        }
        outcome
    }

    /// Applies a change from the plugin (`membership.replace|add|remove` of
    /// the host API), for a group bound to one of the plugin's providers.
    pub async fn push_membership(
        &self,
        plugin_id: &str,
        group_id: Uuid,
        change: MembershipChange,
    ) -> Result<MembershipOutcome, ClassificationError> {
        let group = self.find_group(group_id).await?;
        let cfg = group
            .behavior
            .membership
            .clone()
            .filter(|m| group.behavior.kind == GroupKind::Dynamic && m.plugin_id == plugin_id)
            .ok_or_else(|| {
                ClassificationError::Conflict(
                    "the group is not a dynamic group fed by this plugin".into(),
                )
            })?;
        let (_, decl) = self.provider_decl(plugin_id, &cfg.provider_id)?;
        self.apply_membership(&group, &cfg, &decl, change, false)
            .await
    }

    async fn apply_membership(
        &self,
        group: &NodeGroup,
        cfg: &DynamicGroupConfig,
        decl: &MembershipProviderDecl,
        change: MembershipChange,
        bypass_guard: bool,
    ) -> Result<MembershipOutcome, ClassificationError> {
        let now = Utc::now();
        let check = |list: &[String]| -> Result<BTreeSet<String>, ClassificationError> {
            if let Some(bad) = list.iter().find(|c| !valid_certname(c)) {
                return Err(ClassificationError::Invalid(format!(
                    "'{}' is not a valid certname",
                    bad.chars().take(40).collect::<String>()
                )));
            }
            Ok(list.iter().cloned().collect())
        };
        let repo = self.class_repo();
        let current: BTreeSet<String> = repo
            .members(group.id)
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        let (add, remove): (Vec<String>, Vec<String>) = match change {
            MembershipChange::Replace(list) => {
                let wanted = check(&list)?;
                (
                    wanted.difference(&current).cloned().collect(),
                    current.difference(&wanted).cloned().collect(),
                )
            }
            MembershipChange::Patch { add, remove } => {
                let (add, remove) = (check(&add)?, check(&remove)?);
                (
                    add.difference(&current).cloned().collect(),
                    remove.intersection(&current).cloned().collect(),
                )
            }
        };
        let proposed = current.len() + add.len() - remove.len();
        if proposed > decl.max_members() as usize {
            return Err(ClassificationError::Invalid(format!(
                "the update would give the group {proposed} members; the limit is {}",
                decl.max_members()
            )));
        }
        let changed = add.len() + remove.len();
        let churn_pct = if current.is_empty() {
            0.0
        } else {
            changed as f64 / current.len() as f64 * 100.0
        };
        if !bypass_guard && !current.is_empty() && churn_pct > decl.churn_limit() as f64 {
            let held = HeldMembership {
                group_id: group.id,
                plugin_id: cfg.plugin_id.clone(),
                provider_id: cfg.provider_id.clone(),
                added: add.clone(),
                removed: remove.clone(),
                current: current.len() as u64,
                proposed: proposed as u64,
                churn_pct,
                limit_pct: decl.churn_limit(),
                created_at: now,
            };
            repo.put_held(&held)
                .await
                .map_err(|e| ClassificationError::Failed(e.to_string()))?;
            self.record_membership(group, cfg, "held", Some("held by the churn guard"))
                .await;
            let details = json!({
                "group_id": group.id,
                "group": group.name,
                "plugin_id": cfg.plugin_id,
                "provider": cfg.provider_id,
                "added": add.len(),
                "removed": remove.len(),
                "current": current.len(),
                "churn_pct": (churn_pct * 10.0).round() / 10.0,
                "limit_pct": decl.churn_limit(),
            });
            EventBus::new(self.pool().clone())
                .emit(
                    "membership.held",
                    details.clone(),
                    EventActor::plugin(&cfg.plugin_id),
                )
                .await;
            self.audit(
                group.organization_id,
                None,
                AUDIT_PLUGIN_CLASSIFICATION,
                "plugin.membership.held",
                &group.id.to_string(),
                details,
            )
            .await;
            return Ok(MembershipOutcome {
                applied: false,
                held: true,
                added: add.len(),
                removed: remove.len(),
                members: current.len(),
                churn_pct,
            });
        }
        self.commit_membership(group, cfg, &add, &remove, proposed, now, None)
            .await?;
        Ok(MembershipOutcome {
            applied: true,
            held: false,
            added: add.len(),
            removed: remove.len(),
            members: proposed,
            churn_pct,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_membership(
        &self,
        group: &NodeGroup,
        cfg: &DynamicGroupConfig,
        add: &[String],
        remove: &[String],
        members: usize,
        now: DateTime<Utc>,
        approved_by: Option<&str>,
    ) -> Result<(), ClassificationError> {
        let repo = self.class_repo();
        repo.apply(group.id, add, remove, true, now)
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?;
        let _ = repo.delete_held(group.id).await;
        self.record_membership(group, cfg, "ok", None).await;
        self.classification_runtime().invalidate_members();
        if add.is_empty() && remove.is_empty() {
            return Ok(());
        }
        let list = |l: &[String]| {
            l.iter()
                .take(MAX_LISTED_CERTNAMES)
                .cloned()
                .collect::<Vec<_>>()
        };
        let details = json!({
            "group_id": group.id,
            "group": group.name,
            "plugin_id": cfg.plugin_id,
            "provider": cfg.provider_id,
            "added": add.len(),
            "removed": remove.len(),
            "members": members,
            "added_certnames": list(add),
            "removed_certnames": list(remove),
            "approved_by": approved_by,
        });
        EventBus::new(self.pool().clone())
            .emit(
                "group.membership.changed",
                details.clone(),
                EventActor::plugin(&cfg.plugin_id),
            )
            .await;
        self.audit(
            group.organization_id,
            None,
            AUDIT_PLUGIN_CLASSIFICATION,
            if approved_by.is_some() {
                "plugin.membership.approved"
            } else {
                "plugin.membership.changed"
            },
            &group.id.to_string(),
            details,
        )
        .await;
        Ok(())
    }

    /// Applies an update held by the churn guard.
    pub async fn approve_held_membership(
        &self,
        group_id: Uuid,
        approved_by: &str,
    ) -> Result<MembershipOutcome, ClassificationError> {
        let held = self
            .class_repo()
            .held(group_id)
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?
            .ok_or_else(|| ClassificationError::NotFound("no held update for the group".into()))?;
        let group = self.find_group(group_id).await?;
        let cfg = group
            .behavior
            .membership
            .clone()
            .ok_or_else(|| ClassificationError::Conflict("the group is not dynamic".into()))?;
        self.commit_membership(
            &group,
            &cfg,
            &held.added,
            &held.removed,
            held.proposed as usize,
            Utc::now(),
            Some(approved_by),
        )
        .await?;
        Ok(MembershipOutcome {
            applied: true,
            held: false,
            added: held.added.len(),
            removed: held.removed.len(),
            members: held.proposed as usize,
            churn_pct: held.churn_pct,
        })
    }

    /// Discards an update held by the churn guard.
    pub async fn reject_held_membership(
        &self,
        group_id: Uuid,
        rejected_by: &str,
    ) -> Result<(), ClassificationError> {
        let group = self.find_group(group_id).await?;
        let removed = self
            .class_repo()
            .delete_held(group_id)
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?;
        if !removed {
            return Err(ClassificationError::NotFound(
                "no held update for the group".into(),
            ));
        }
        self.audit(
            group.organization_id,
            None,
            AUDIT_PLUGIN_CLASSIFICATION,
            "plugin.membership.rejected",
            &group_id.to_string(),
            json!({"group_id": group_id, "group": group.name, "rejected_by": rejected_by}),
        )
        .await;
        Ok(())
    }

    /// Refreshes every dynamic group whose provider schedule is due and
    /// alerts once about members that went stale.
    pub async fn run_due_refreshes(&self) {
        let now = Utc::now();
        let hooks = self.hooks();
        let states: HashMap<Uuid, MembershipState> = self
            .class_repo()
            .states()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.group_id, s))
            .collect();
        for g in self.dynamic_groups().await {
            let Some(cfg) = &g.behavior.membership else {
                continue;
            };
            let Some(decl) = hooks
                .get(&cfg.plugin_id)
                .and_then(|h| h.classification.as_ref())
                .and_then(|c| {
                    c.membership_providers
                        .iter()
                        .find(|p| p.id == cfg.provider_id)
                })
            else {
                continue;
            };
            let st = states.get(&g.id);
            self.alert_if_stale(&g, cfg, decl, st, now).await;
            let Some(cron) = decl
                .refresh
                .as_ref()
                .and_then(|r| super::manifest::parse_cron(&r.cron).ok())
            else {
                continue;
            };
            let due = match st.and_then(|s| s.last_attempt_at) {
                None => true,
                Some(last) => cron.after(&last).next().is_some_and(|n| n <= now),
            };
            if !due {
                continue;
            }
            match self.refresh_membership(g.id).await {
                Ok(o) if o.held => tracing::warn!(
                    "membership update of group {} held by the churn guard ({:.0}%)",
                    g.name,
                    o.churn_pct
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!("membership refresh of group {} failed: {}", g.name, e),
            }
        }
    }

    async fn alert_if_stale(
        &self,
        group: &NodeGroup,
        cfg: &DynamicGroupConfig,
        decl: &MembershipProviderDecl,
        state: Option<&MembershipState>,
        now: DateTime<Utc>,
    ) {
        if state.is_some_and(|s| s.stale_alerted) {
            return;
        }
        let rows = self
            .class_repo()
            .members(group.id)
            .await
            .unwrap_or_default();
        let Some(newest) = rows.iter().map(|(_, at)| *at).max() else {
            return;
        };
        if (now - newest).num_seconds() <= decl.max_stale() as i64 {
            return;
        }
        let _ = self.class_repo().mark_stale_alerted(group.id).await;
        let policy = decl.on_stale();
        let what = match policy {
            OnStale::Freeze => "kept as they are (freeze)",
            _ => "dropped",
        };
        EventBus::new(self.pool().clone())
            .emit(
                "classification.degraded",
                json!({
                    "reasons": [format!(
                        "members of group '{}' from {}/{} are stale and {}",
                        group.name, cfg.plugin_id, cfg.provider_id, what
                    )],
                    "group_id": group.id,
                    "on_stale": policy.as_str(),
                }),
                EventActor::plugin(&cfg.plugin_id),
            )
            .await;
    }

    // ---- generated groups -------------------------------------------------

    pub async fn template_instances(&self, plugin_id: Option<&str>) -> Vec<GroupTemplateInstance> {
        let groups = self.all_groups().await;
        self.class_repo()
            .instances()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|i| plugin_id.is_none_or(|p| i.plugin_id == p))
            .map(|mut i| {
                i.generated_groups = groups
                    .iter()
                    .filter(|g| {
                        g.behavior
                            .generated
                            .as_ref()
                            .is_some_and(|x| x.instance_id == i.id)
                    })
                    .count() as u32;
                i
            })
            .collect()
    }

    fn template_decl(
        &self,
        plugin_id: &str,
        template_id: &str,
    ) -> Result<GroupTemplateDecl, ClassificationError> {
        self.hooks()
            .get(plugin_id)
            .and_then(|h| h.classification.as_ref())
            .and_then(|c| c.group_templates.iter().find(|t| t.id == template_id))
            .cloned()
            .ok_or_else(|| {
                ClassificationError::Conflict(format!(
                    "plugin '{plugin_id}' has no enabled group template '{template_id}'"
                ))
            })
    }

    /// Puts a group template to use under `parent_id`. It generates nothing
    /// until an administrator approves it, unless the template says it does
    /// not need approval.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_template_instance(
        &self,
        plugin_id: &str,
        template_id: &str,
        name: &str,
        parent_id: Option<Uuid>,
        classes: Value,
        variables: Value,
        created_by: &str,
    ) -> Result<GroupTemplateInstance, ClassificationError> {
        let decl = self.template_decl(plugin_id, template_id)?;
        let name = name.trim();
        if name.is_empty()
            || name.chars().count() > 80
            || name.chars().any(|c| c.is_control() || c == '<' || c == '>')
        {
            return Err(ClassificationError::Invalid(
                "name must be 1 to 80 characters without markup".into(),
            ));
        }
        for (what, v) in [("classes", &classes), ("variables", &variables)] {
            if !v.is_object() {
                return Err(ClassificationError::Invalid(format!(
                    "{what} must be an object"
                )));
            }
        }
        if let Some(p) = parent_id {
            self.find_group(p).await.map_err(|_| {
                ClassificationError::Invalid("the parent group does not exist".into())
            })?;
        }
        let now = Utc::now();
        let auto = !decl.requires_approval.unwrap_or(true);
        let instance = GroupTemplateInstance {
            id: Uuid::new_v4(),
            plugin_id: plugin_id.to_string(),
            template_id: template_id.to_string(),
            name: name.to_string(),
            parent_id,
            classes,
            variables,
            approved: auto,
            approved_by: auto.then(|| "template".to_string()),
            approved_at: auto.then_some(now),
            enabled: true,
            last_run_at: None,
            last_status: None,
            generated_groups: 0,
            created_at: now,
        };
        self.class_repo()
            .insert_instance(&instance)
            .await
            .map_err(|e| {
                ClassificationError::Conflict(format!("could not create the instance: {e}"))
            })?;
        self.audit(
            default_organization_uuid(),
            None,
            AUDIT_PLUGIN_CLASSIFICATION,
            "plugin.template.created",
            &instance.id.to_string(),
            json!({
                "plugin_id": plugin_id,
                "template": template_id,
                "name": name,
                "parent_id": parent_id,
                "created_by": created_by,
                "needs_approval": !auto,
            }),
        )
        .await;
        Ok(instance)
    }

    pub async fn approve_template_instance(
        &self,
        id: Uuid,
        by: &str,
    ) -> Result<(), ClassificationError> {
        let inst = self
            .class_repo()
            .instance(id)
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?
            .ok_or_else(|| ClassificationError::NotFound("template instance not found".into()))?;
        self.class_repo()
            .approve_instance(id, by, Utc::now())
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?;
        self.audit(
            default_organization_uuid(),
            None,
            AUDIT_PLUGIN_CLASSIFICATION,
            "plugin.template.approved",
            &id.to_string(),
            json!({"plugin_id": inst.plugin_id, "template": inst.template_id, "name": inst.name, "approved_by": by}),
        )
        .await;
        Ok(())
    }

    pub async fn set_template_instance_enabled(
        &self,
        id: Uuid,
        enabled: bool,
    ) -> Result<(), ClassificationError> {
        match self.class_repo().set_instance_enabled(id, enabled).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(ClassificationError::NotFound(
                "template instance not found".into(),
            )),
            Err(e) => Err(ClassificationError::Failed(e.to_string())),
        }
    }

    /// Deletes an instance and the groups it generated.
    pub async fn delete_template_instance(
        &self,
        id: Uuid,
        by: &str,
    ) -> Result<usize, ClassificationError> {
        let inst = self
            .class_repo()
            .instance(id)
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?
            .ok_or_else(|| ClassificationError::NotFound("template instance not found".into()))?;
        let removed = self.remove_generated_groups(Some(id), None).await;
        let _ = self.class_repo().delete_instance(id).await;
        self.audit(
            default_organization_uuid(),
            None,
            AUDIT_PLUGIN_CLASSIFICATION,
            "plugin.template.deleted",
            &id.to_string(),
            json!({"plugin_id": inst.plugin_id, "name": inst.name, "removed_groups": removed, "deleted_by": by}),
        )
        .await;
        Ok(removed)
    }

    async fn remove_generated_groups(&self, instance: Option<Uuid>, plugin: Option<&str>) -> usize {
        let repo = GroupRepository::new(self.pool());
        let mut removed = 0;
        for g in self.all_groups().await.iter().filter(|g| {
            instance.is_some_and(|i| {
                g.behavior
                    .generated
                    .as_ref()
                    .is_some_and(|x| x.instance_id == i)
            }) || plugin.is_some_and(|p| g.behavior.owner_plugin_id.as_deref() == Some(p))
        }) {
            if repo.delete(g.organization_id, g.id).await.unwrap_or(false) {
                removed += 1;
                EventBus::new(self.pool().clone())
                    .emit(
                        "group.deleted",
                        json!({"group_id": g.id, "group": g.name, "generated": true}),
                        EventActor::system(),
                    )
                    .await;
            }
        }
        removed
    }

    /// The distinct values of a fact path across the fleet: PuppetDB plus the
    /// non-expired values of plugin facts.
    async fn distinct_fact_values(
        &self,
        puppetdb: Option<&PuppetDbClient>,
        path: &str,
    ) -> Result<BTreeSet<String>, ClassificationError> {
        let mut out = BTreeSet::new();
        let mut parts = path.split('.');
        let top = parts.next().unwrap_or(path);
        let rest: Vec<&str> = parts.collect();
        if let Some(db) = puppetdb {
            let facts = db
                .query_facts(Some(top))
                .await
                .map_err(|e| ClassificationError::Failed(format!("PuppetDB: {e}")))?;
            for f in facts {
                let mut v = &f.value;
                let mut found = true;
                for p in &rest {
                    match v.get(*p) {
                        Some(n) => v = n,
                        None => {
                            found = false;
                            break;
                        }
                    }
                }
                if let Some(k) = found.then(|| scalar_key(v)).flatten() {
                    out.insert(k);
                }
            }
        }
        if let Ok(rows) = crate::db::PluginFactsRepository::new(self.pool())
            .distinct_values(path, Utc::now())
            .await
        {
            out.extend(rows.iter().filter_map(scalar_key));
        }
        if out.len() > MAX_DISTINCT_FACT_VALUES {
            return Err(ClassificationError::Invalid(format!(
                "the fact '{path}' has more than {MAX_DISTINCT_FACT_VALUES} distinct values"
            )));
        }
        Ok(out)
    }

    /// Creates, updates and removes the groups of one approved template
    /// instance so that there is one group per value of the template's fact.
    pub async fn reconcile_template(
        &self,
        id: Uuid,
        puppetdb: Option<&PuppetDbClient>,
    ) -> Result<TemplateRun, ClassificationError> {
        let inst = self
            .class_repo()
            .instance(id)
            .await
            .map_err(|e| ClassificationError::Failed(e.to_string()))?
            .ok_or_else(|| ClassificationError::NotFound("template instance not found".into()))?;
        if !inst.approved {
            return Err(ClassificationError::Conflict(
                "the template instance is waiting for approval".into(),
            ));
        }
        if !inst.enabled {
            return Err(ClassificationError::Conflict(
                "the template instance is disabled".into(),
            ));
        }
        let decl = self.template_decl(&inst.plugin_id, &inst.template_id)?;
        let path = decl.fact_path().ok_or_else(|| {
            ClassificationError::Invalid("the template's group_by is invalid".into())
        })?;
        let now = Utc::now();
        let values = match self.distinct_fact_values(puppetdb, &path).await {
            Ok(v) => v,
            Err(e) => {
                let _ = self
                    .class_repo()
                    .record_instance_run(id, &format!("failed: {e}"), now)
                    .await;
                return Err(e);
            }
        };
        let max = decl.max_groups() as usize;
        let truncated = values.len() > max;
        let wanted: Vec<String> = values.into_iter().take(max).collect();

        let repo = GroupRepository::new(self.pool());
        let existing = self.all_groups().await;
        let mine: HashMap<String, NodeGroup> = existing
            .iter()
            .filter_map(|g| {
                let x = g.behavior.generated.as_ref()?;
                (x.instance_id == id).then(|| (x.key.clone(), g.clone()))
            })
            .collect();
        let taken: HashSet<&str> = existing.iter().map(|g| g.name.as_str()).collect();
        let org = inst
            .parent_id
            .and_then(|p| existing.iter().find(|g| g.id == p))
            .map_or_else(default_organization_uuid, |g| g.organization_id);

        let (mut created, mut updated, mut removed) = (0, 0, 0);
        let bus = EventBus::new(self.pool().clone());
        for key in &wanted {
            let classes = substitute(&inst.classes, key);
            let variables = substitute(&inst.variables, key);
            let behavior = GroupBehavior {
                kind: GroupKind::Generated,
                owner_plugin_id: Some(inst.plugin_id.clone()),
                generated: Some(GeneratedGroupInfo {
                    instance_id: id,
                    key: key.clone(),
                }),
                ..Default::default()
            };
            match mine.get(key) {
                Some(g) => {
                    let changed = g.classes != classes
                        || g.variables != variables
                        || g.parent_id != inst.parent_id;
                    if changed {
                        let req = UpdateGroupRequest {
                            classes: Some(classes),
                            variables: Some(variables),
                            parent_id: inst.parent_id,
                            behavior: Some(behavior),
                            ..Default::default()
                        };
                        if repo.update(g.organization_id, g.id, &req).await.is_ok() {
                            updated += 1;
                            bus.emit(
                                "group.updated",
                                json!({"group_id": g.id, "group": g.name, "generated": true}),
                                EventActor::plugin(&inst.plugin_id),
                            )
                            .await;
                        }
                    }
                }
                None => {
                    let name = format!("{}: {}", inst.name, key);
                    if taken.contains(name.as_str()) {
                        tracing::warn!("generated group '{}' skipped: the name is taken", name);
                        continue;
                    }
                    let req = CreateGroupRequest {
                        name: name.clone(),
                        description: Some(format!(
                            "Generated by {} / {} for {} = {}",
                            inst.plugin_id, inst.template_id, path, key
                        )),
                        parent_id: inst.parent_id,
                        classes: Some(classes),
                        variables: Some(variables),
                        behavior: Some(behavior),
                        ..Default::default()
                    };
                    match repo.create(org, &req).await {
                        Ok(g) => {
                            let rule = CreateRuleRequest {
                                fact_path: path.clone(),
                                operator: RuleOperator::Equals,
                                value: Value::String(key.clone()),
                            };
                            if let Err(e) = repo.add_rule(g.id, &rule).await {
                                tracing::warn!(
                                    "could not add the rule of generated group {}: {}",
                                    name,
                                    e
                                );
                            }
                            created += 1;
                            bus.emit(
                                "group.created",
                                json!({"group_id": g.id, "group": name, "generated": true}),
                                EventActor::plugin(&inst.plugin_id),
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::warn!("could not create generated group {}: {}", name, e)
                        }
                    }
                }
            }
        }
        let wanted_set: HashSet<&String> = wanted.iter().collect();
        for (key, g) in &mine {
            if wanted_set.contains(key) {
                continue;
            }
            if repo.delete(g.organization_id, g.id).await.unwrap_or(false) {
                removed += 1;
                bus.emit(
                    "group.deleted",
                    json!({"group_id": g.id, "group": g.name, "generated": true}),
                    EventActor::plugin(&inst.plugin_id),
                )
                .await;
            }
        }
        let status = if truncated {
            format!("ok (limited to {max} groups)")
        } else {
            "ok".to_string()
        };
        let _ = self
            .class_repo()
            .record_instance_run(id, &status, now)
            .await;
        if created + updated + removed > 0 {
            self.audit(
                org,
                None,
                AUDIT_PLUGIN_CLASSIFICATION,
                "plugin.template.reconciled",
                &id.to_string(),
                json!({
                    "plugin_id": inst.plugin_id,
                    "template": inst.template_id,
                    "created": created,
                    "updated": updated,
                    "removed": removed,
                    "fact": path,
                    "truncated": truncated,
                }),
            )
            .await;
        }
        if truncated {
            bus.emit(
                "classification.degraded",
                json!({"reasons": [format!(
                    "template '{}' has more than {} values of '{}'; the extra ones get no group",
                    inst.name, max, path
                )]}),
                EventActor::plugin(&inst.plugin_id),
            )
            .await;
        }
        Ok(TemplateRun {
            created,
            updated,
            removed,
            groups: wanted.len(),
            truncated,
        })
    }

    /// Reconciles every approved, enabled template instance.
    pub async fn reconcile_all_templates(&self, puppetdb: Option<&PuppetDbClient>) {
        let instances = self.class_repo().instances().await.unwrap_or_default();
        for i in instances.iter().filter(|i| i.approved && i.enabled) {
            if let Err(e) = self.reconcile_template(i.id, puppetdb).await {
                tracing::warn!("group template '{}' could not be reconciled: {}", i.name, e);
            }
        }
    }

    // ---- lifecycle ---------------------------------------------------------

    /// Uninstall: generated groups and instances always go (the host owns
    /// them); dynamic groups stay, with their members only unless `purge`,
    /// and report their provider as unavailable.
    pub(super) async fn forget_plugin_classification(&self, plugin_id: &str, purge: bool) {
        self.classification_runtime().forget(plugin_id);
        let removed = self.remove_generated_groups(None, Some(plugin_id)).await;
        let _ = self.class_repo().delete_instances_of(plugin_id).await;
        let mut purged = 0;
        if purge {
            for g in self.dynamic_groups().await {
                if g.behavior
                    .membership
                    .as_ref()
                    .is_some_and(|m| m.plugin_id == plugin_id)
                {
                    let _ = self.class_repo().forget_group(g.id).await;
                    purged += 1;
                }
            }
            self.classification_runtime().invalidate_members();
        }
        if removed + purged > 0 {
            self.audit(
                default_organization_uuid(),
                None,
                AUDIT_PLUGIN_CLASSIFICATION,
                "plugin.classification.removed",
                plugin_id,
                json!({"plugin_id": plugin_id, "generated_groups": removed, "purged_member_lists": purged}),
            )
            .await;
        }
    }

    /// Groups whose expression, merge strategy or provider needs a plugin
    /// that is not enabled; they never match or fall back, so the admin is
    /// warned instead of the failure being silent.
    pub async fn unavailable_group_extensions(&self) -> BTreeMap<String, Vec<String>> {
        let hooks = self.hooks();
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for g in self.all_groups().await {
            if let Err(e) = validate_group_extensions(&hooks, g.expression.as_ref(), &g.behavior) {
                if e.contains("plugin") || e.contains("not provided") {
                    out.entry(g.name.clone()).or_default().push(e);
                }
            }
        }
        out
    }
}

pub fn start_classification_scheduler(
    manager: Arc<PluginManager>,
    puppetdb: Option<Arc<PuppetDbClient>>,
) {
    let cfg = manager.classification_runtime().config().clone();
    if cfg.refresh_tick_secs > 0 {
        let m = manager.clone();
        let every = cfg.refresh_tick_secs.max(10);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(every));
            tick.tick().await;
            loop {
                tick.tick().await;
                m.run_due_refreshes().await;
            }
        });
    }
    if cfg.template_secs > 0 {
        let every = cfg.template_secs.max(30);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(every));
            tick.tick().await;
            loop {
                tick.tick().await;
                manager.reconcile_all_templates(puppetdb.as_deref()).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::plugins::invoke::HandlerCall;
    use crate::services::plugins::package::build_signed_package;
    use crate::services::plugins::BuiltinPlugin;
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;

    const SEED: [u8; 32] = [13; 32];
    const ID: &str = "acme-cls";

    const MANIFEST: &str = "api_version: 2\nid: acme-cls\nname: Classification\nversion: 1.0.0\n\
publisher: ACME\nruntime: { kind: builtin, entry: acme-cls }\ncapabilities:\n\
\x20 - classification:operators\n  - classification:membership\n  - classification:templates\n\
\x20 - classification:behaviors\ncontributes:\n  classification:\n    functions:\n\
\x20     - { id: is_prod, args: [string], returns: bool, handler: fn_is_prod }\n\
\x20     - { id: shout, args: [string], returns: string, handler: fn_shout }\n\
\x20   operators:\n      - { id: starts, arg_types: [string, string], handler: op_starts }\n\
\x20   merge_strategies:\n      - { id: concat, handler: merge_concat }\n\
\x20   membership_providers:\n      - id: critical\n        label: Critical assets\n\
\x20       refresh: { cron: \"*/15 * * * *\" }\n        max_stale_s: 3600\n\
\x20       churn_limit_pct: 20\n        max_members: 50\n        handler: members\n\
\x20   group_templates:\n      - { id: by-dc, group_by: \"fact('datacenter')\", max_groups: 2 }\n";

    /// Pure handlers and a provider whose members the test sets.
    #[derive(Default)]
    struct Cls {
        members: Mutex<Value>,
        calls: Mutex<u32>,
    }

    impl BuiltinPlugin for Cls {
        fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
            assert_eq!(call.kind, HandlerKind::Classification);
            *self.calls.lock().unwrap() += 1;
            let i = &call.input;
            Ok(match call.handler.as_str() {
                "fn_is_prod" => {
                    json!({"value": i["args"][0].as_str().is_some_and(|s| s.starts_with("prod"))})
                }
                "fn_shout" => json!({"value": i["args"][0].as_str().unwrap_or("").to_uppercase()}),
                "op_starts" => json!({"result": i["left"].as_str().zip(i["right"].as_str())
                    .is_some_and(|(l, r)| l.starts_with(r))}),
                "merge_concat" => json!({"value": [i["current"], i["incoming"]]}),
                "members" => self.members.lock().unwrap().clone(),
                other => anyhow::bail!("unknown handler {other}"),
            })
        }
    }

    struct Env {
        manager: Arc<PluginManager>,
        plugin: Arc<Cls>,
        _dir: std::path::PathBuf,
    }

    async fn env() -> Env {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ovx_cls_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let plugin = Arc::new(Cls::default());
        let manager = Arc::new(
            PluginManager::new(pool, dir.clone(), 8)
                .with_builtin_keys(vec![("OpenVox".into(), pk)])
                .with_builtin_runtime(ID, plugin.clone()),
        );
        manager.reload().await.unwrap();
        let pkg = build_signed_package(
            &[("plugin.yaml", MANIFEST.as_bytes())],
            ID,
            "1.0.0",
            2,
            SEED,
        );
        manager.install(&pkg, "admin", None).await.unwrap();
        manager.enable(ID).await.unwrap();
        Env {
            manager,
            plugin,
            _dir: dir,
        }
    }

    fn group(name: &str) -> CreateGroupRequest {
        CreateGroupRequest {
            name: name.into(),
            match_all_nodes: Some(true),
            ..Default::default()
        }
    }

    async fn make(m: &PluginManager, req: CreateGroupRequest) -> NodeGroup {
        GroupRepository::new(m.pool())
            .create(default_organization_uuid(), &req)
            .await
            .unwrap()
    }

    async fn groups(m: &PluginManager) -> Vec<NodeGroup> {
        GroupRepository::new(m.pool())
            .get_all(default_organization_uuid())
            .await
            .unwrap()
    }

    fn dynamic(m: &PluginManager, name: &str) -> CreateGroupRequest {
        let _ = m;
        CreateGroupRequest {
            behavior: Some(GroupBehavior {
                kind: GroupKind::Dynamic,
                membership: Some(DynamicGroupConfig {
                    plugin_id: ID.into(),
                    provider_id: "critical".into(),
                    params: json!({}),
                }),
                ..Default::default()
            }),
            ..group(name)
        }
    }

    fn members(names: &[&str]) -> Value {
        json!({"members": names})
    }

    fn names(prefix: &str, n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("{prefix}{i:02}.example.com"))
            .collect()
    }

    fn matched(m: &ClassificationResult, name: &str) -> bool {
        m.groups.iter().any(|g| g.name == name)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn plugin_function_operator_and_merge_strategy_run_on_the_hot_path() {
        let e = env().await;
        let mut expr = group("prod-web");
        expr.match_all_nodes = Some(false);
        expr.expression = Some(json!({"all": [
            {"fn": "plugin:acme-cls:is_prod", "args": [{"fact": "role"}]},
            {"op": "plugin:acme-cls:starts", "left": {"fact": "certname"}, "right": "web"},
            {"op": "=", "left": {"fn": "plugin:acme-cls:shout", "args": [{"fact": "role"}]}, "right": "PROD-WEB"},
        ]}));
        assert!(e
            .manager
            .validate_group_extensions(expr.expression.as_ref(), &Default::default())
            .is_ok());
        make(&e.manager, expr).await;
        for (n, v) in [("a", json!({"x": 1})), ("b", json!({"x": 2}))] {
            make(
                &e.manager,
                CreateGroupRequest {
                    classes: Some(json!({"app": v})),
                    behavior: Some(GroupBehavior {
                        merge: MergeStrategy::Plugin(
                            ExtRef::parse("plugin:acme-cls:concat").unwrap(),
                        ),
                        ..Default::default()
                    }),
                    priority: Some(if n == "a" { 1 } else { 2 }),
                    ..group(n)
                },
            )
            .await;
        }
        let svc = e
            .manager
            .classification_service(groups(&e.manager).await)
            .await;
        let facts = json!({"certname": "web01", "role": "prod-web"});
        let (r, why) = svc.classify_explained("web01", &facts);
        assert!(matched(&r, "prod-web"));
        assert_eq!(r.classes["app"], json!([{"x": 1}, {"x": 2}]));
        assert!(why.degraded.is_empty(), "{:?}", why.degraded);

        let other = json!({"certname": "db01", "role": "prod-web"});
        assert!(!matched(&svc.classify("db01", &other), "prod-web"));
        let wrong_type = json!({"certname": "web02", "role": 7});
        assert!(!matched(&svc.classify("web02", &wrong_type), "prod-web"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn results_are_cached_so_the_plugin_runs_once_per_input() {
        let e = env().await;
        let mut g = group("prod");
        g.match_all_nodes = Some(false);
        g.expression = Some(json!({"fn": "plugin:acme-cls:is_prod", "args": [{"fact": "role"}]}));
        make(&e.manager, g).await;
        let svc = e
            .manager
            .classification_service(groups(&e.manager).await)
            .await;
        let facts = json!({"role": "prod-1"});
        for _ in 0..5 {
            assert!(matched(&svc.classify("n", &facts), "prod"));
        }
        assert_eq!(*e.plugin.calls.lock().unwrap(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_missing_plugin_makes_the_rule_unavailable_and_never_matches() {
        let e = env().await;
        let mut g = group("prod");
        g.match_all_nodes = Some(false);
        g.expression = Some(json!({"fn": "plugin:acme-cls:is_prod", "args": [{"fact": "role"}]}));
        make(&e.manager, g).await;
        make(
            &e.manager,
            CreateGroupRequest {
                classes: Some(json!({"app": {"x": 1}})),
                ..group("fallback")
            },
        )
        .await;
        e.manager.disable(ID).await.unwrap();
        let svc = e
            .manager
            .classification_service(groups(&e.manager).await)
            .await;
        let r = svc.classify("n", &json!({"role": "prod-1"}));
        assert!(!matched(&r, "prod"));
        assert!(matched(&r, "fallback"), "other groups still classify");
        let warned = e.manager.unavailable_group_extensions().await;
        assert!(warned.contains_key("prod"));
        // Saving it again is refused while the plugin is off
        assert!(e
            .manager
            .validate_group_extensions(
                Some(&json!({"fn": "plugin:acme-cls:is_prod", "args": [{"fact": "role"}]})),
                &Default::default()
            )
            .is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn failing_merge_strategy_falls_back_to_deep_and_is_reported() {
        let e = env().await;
        for (n, v, p) in [("a", json!({"x": 1}), 1), ("b", json!({"y": 2}), 2)] {
            make(
                &e.manager,
                CreateGroupRequest {
                    classes: Some(json!({"app": v})),
                    behavior: Some(GroupBehavior {
                        // the plugin has no strategy with this id
                        merge: MergeStrategy::Plugin(
                            ExtRef::parse("plugin:acme-cls:nope").unwrap(),
                        ),
                        ..Default::default()
                    }),
                    priority: Some(p),
                    ..group(n)
                },
            )
            .await;
        }
        let svc = e
            .manager
            .classification_service(groups(&e.manager).await)
            .await;
        let (r, why) = svc.classify_explained("n", &json!({}));
        assert_eq!(r.classes["app"], json!({"x": 1, "y": 2}));
        assert!(why.degraded.iter().any(|d| d.contains("merged with deep")));
        assert!(r.conflict_error.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dynamic_group_membership_feeds_classification_and_churn_guard_holds_big_changes() {
        let e = env().await;
        let g = make(&e.manager, dynamic(&e.manager, "critical")).await;
        let all = names("n", 10);
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();

        // The first load is not limited by the guard
        *e.plugin.members.lock().unwrap() = members(&refs);
        let o = e.manager.refresh_membership(g.id).await.unwrap();
        assert!(o.applied && !o.held);
        assert_eq!((o.added, o.removed, o.members), (10, 0, 10));
        let svc = e
            .manager
            .classification_service(groups(&e.manager).await)
            .await;
        assert!(matched(
            &svc.classify("n00.example.com", &json!({})),
            "critical"
        ));
        assert!(!matched(
            &svc.classify("other.example.com", &json!({})),
            "critical"
        ));

        // 3 changes on 10 members is 30%: over the 20% limit, held and not applied
        let mut next = refs.clone();
        next.drain(0..2);
        next.push("new.example.com");
        *e.plugin.members.lock().unwrap() = members(&next);
        let o = e.manager.refresh_membership(g.id).await.unwrap();
        assert!(o.held && !o.applied);
        let held = e.manager.held_membership(g.id).await.unwrap();
        assert_eq!((held.added.len(), held.removed.len()), (1, 2));
        let svc = e
            .manager
            .classification_service(groups(&e.manager).await)
            .await;
        assert!(matched(
            &svc.classify("n00.example.com", &json!({})),
            "critical"
        ));
        assert!(!matched(
            &svc.classify("new.example.com", &json!({})),
            "critical"
        ));

        // An administrator approves it
        let o = e
            .manager
            .approve_held_membership(g.id, "admin")
            .await
            .unwrap();
        assert!(o.applied);
        assert!(e.manager.held_membership(g.id).await.is_none());
        e.manager.classification_runtime().invalidate_members();
        let svc = e
            .manager
            .classification_service(groups(&e.manager).await)
            .await;
        assert!(!matched(
            &svc.classify("n00.example.com", &json!({})),
            "critical"
        ));
        assert!(matched(
            &svc.classify("new.example.com", &json!({})),
            "critical"
        ));

        // A change within the limit applies at once; rejecting discards a held one
        let mut small = next.clone();
        small.push("extra.example.com");
        *e.plugin.members.lock().unwrap() = members(&small);
        assert!(e.manager.refresh_membership(g.id).await.unwrap().applied);
        *e.plugin.members.lock().unwrap() = members(&["only.example.com"]);
        assert!(e.manager.refresh_membership(g.id).await.unwrap().held);
        e.manager
            .reject_held_membership(g.id, "admin")
            .await
            .unwrap();
        assert!(e.manager.held_membership(g.id).await.is_none());
        let audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE resource_type = ? AND action LIKE 'plugin.membership.%'",
        )
        .bind(AUDIT_PLUGIN_CLASSIFICATION)
        .fetch_one(e.manager.pool())
        .await
        .unwrap();
        assert!(
            audits >= 4,
            "held, approved, changed and rejected are audited"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn membership_updates_are_validated_and_limited() {
        let e = env().await;
        let g = make(&e.manager, dynamic(&e.manager, "critical")).await;
        *e.plugin.members.lock().unwrap() = members(&["bad name"]);
        assert!(matches!(
            e.manager.refresh_membership(g.id).await,
            Err(ClassificationError::Invalid(_))
        ));
        let many = names("m", 51);
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        *e.plugin.members.lock().unwrap() = members(&refs);
        let err = e.manager.refresh_membership(g.id).await.unwrap_err();
        assert!(err.to_string().contains("limit"), "{err}");
        // Mixed answers are refused
        *e.plugin.members.lock().unwrap() = json!({"members": ["a"], "add": ["b"]});
        assert!(e.manager.refresh_membership(g.id).await.is_err());
        // Another plugin's group is not writable through the host API path
        let other = make(&e.manager, group("static")).await;
        assert!(e
            .manager
            .push_membership(ID, other.id, MembershipChange::Replace(vec![]))
            .await
            .is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stale_members_follow_the_provider_policy() {
        let e = env().await;
        let g = make(&e.manager, dynamic(&e.manager, "critical")).await;
        *e.plugin.members.lock().unwrap() = members(&["a.example.com"]);
        e.manager.refresh_membership(g.id).await.unwrap();
        let old = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        sqlx::query("UPDATE plugin_group_members SET computed_at = ?")
            .bind(old)
            .execute(e.manager.pool())
            .await
            .unwrap();
        e.manager.classification_runtime().invalidate_members();
        // keep (default): past max_stale_s the members are dropped
        let m = e.manager.dynamic_members(&groups(&e.manager).await).await;
        let dm = m.get(&g.id).unwrap();
        assert!(dm.stale && dm.certnames.is_empty());
        // The scheduler alerts once
        e.manager.run_due_refreshes().await;
        let alerts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_outbox WHERE type = 'classification.degraded'",
        )
        .fetch_one(e.manager.pool())
        .await
        .unwrap();
        assert!(alerts >= 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn generated_groups_need_approval_follow_the_fact_and_respect_the_limit() {
        let e = env().await;
        let parent = make(&e.manager, group("datacenters")).await;
        let inst = e
            .manager
            .create_template_instance(
                ID,
                "by-dc",
                "dc",
                Some(parent.id),
                json!({"profile::dc": {"site": "{{key}}"}}),
                json!({"datacenter": "{{key}}"}),
                "admin",
            )
            .await
            .unwrap();
        assert!(!inst.approved);
        assert!(matches!(
            e.manager.reconcile_template(inst.id, None).await,
            Err(ClassificationError::Conflict(_))
        ));
        e.manager
            .approve_template_instance(inst.id, "admin")
            .await
            .unwrap();

        let put = |cert: &'static str, v: &'static str| {
            let pool = e.manager.pool().clone();
            async move {
                sqlx::query(
                    "INSERT OR REPLACE INTO plugin_facts (plugin_id, certname, name, value_json, computed_at) \
                     VALUES ('src', ?, 'datacenter', ?, ?)",
                )
                .bind(cert)
                .bind(serde_json::to_string(&json!(v)).unwrap())
                .bind(Utc::now().to_rfc3339())
                .execute(&pool)
                .await
                .unwrap();
            }
        };
        put("a", "dc1").await;
        put("b", "dc2").await;
        let run = e.manager.reconcile_template(inst.id, None).await.unwrap();
        assert_eq!((run.created, run.removed, run.groups), (2, 0, 2));
        let all = groups(&e.manager).await;
        let g1 = all.iter().find(|g| g.name == "dc: dc1").unwrap();
        assert_eq!(g1.parent_id, Some(parent.id));
        assert_eq!(g1.classes["profile::dc"]["site"], json!("dc1"));
        assert_eq!(g1.behavior.owner_plugin_id.as_deref(), Some(ID));
        assert_eq!(g1.behavior.kind, GroupKind::Generated);
        let svc = e.manager.classification_service(all.clone()).await;
        let r = svc.classify("a", &json!({"datacenter": "dc1"}));
        assert!(matched(&r, "dc: dc1") && !matched(&r, "dc: dc2"));

        // A value that disappears removes its group; a third one over the limit is left out
        sqlx::query("DELETE FROM plugin_facts WHERE certname = 'b'")
            .execute(e.manager.pool())
            .await
            .unwrap();
        put("c", "dc3").await;
        put("d", "dc4").await;
        let run = e.manager.reconcile_template(inst.id, None).await.unwrap();
        assert_eq!((run.created, run.removed), (1, 1));
        assert!(run.truncated, "max_groups is 2");
        assert_eq!(
            groups(&e.manager)
                .await
                .iter()
                .filter(|g| g.behavior.generated.is_some())
                .count(),
            2
        );

        // Deleting the instance removes what it generated
        let removed = e
            .manager
            .delete_template_instance(inst.id, "admin")
            .await
            .unwrap();
        assert_eq!(removed, 2);
        assert!(groups(&e.manager)
            .await
            .iter()
            .all(|g| g.behavior.generated.is_none()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn uninstall_removes_generated_groups_and_purge_removes_member_lists() {
        let e = env().await;
        let g = make(&e.manager, dynamic(&e.manager, "critical")).await;
        *e.plugin.members.lock().unwrap() = members(&["a.example.com"]);
        e.manager.refresh_membership(g.id).await.unwrap();
        let inst = e
            .manager
            .create_template_instance(ID, "by-dc", "dc", None, json!({}), json!({}), "admin")
            .await
            .unwrap();
        e.manager.uninstall(ID, false).await.unwrap();
        assert!(e.manager.template_instances(None).await.is_empty());
        assert!(e
            .manager
            .class_repo()
            .instance(inst.id)
            .await
            .unwrap()
            .is_none());
        // The dynamic group stays (user data) and keeps its last members
        assert_eq!(e.manager.class_repo().members(g.id).await.unwrap().len(), 1);
        e.manager.forget_plugin_classification(ID, true).await;
        assert!(e
            .manager
            .class_repo()
            .members(g.id)
            .await
            .unwrap()
            .is_empty());
    }
}
