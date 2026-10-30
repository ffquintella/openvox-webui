//! Plugin facts (M25, T89): push and pull providers, the merged PuppetDB ∪
//! plugin view, the opt-in use in classification rules and the opt-in export
//! to agents. A plugin writes only its own declared facts, never overrides a
//! fact reported by an agent, and a failing provider only makes its facts
//! missing or stale. See docs/architecture/PLUGIN_PLATFORM.md §12.2 and §17.6.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use super::events::EventBus;
use super::interceptors::{
    apply_fact_changes, InterceptActor, InterceptRefusal, BEFORE_NODE_CLASSIFY,
};
use super::invoke::{invoke_with_timeout, HandlerKind, PluginHooks};
use super::manager::{PluginError, PluginManager, AUDIT_PLUGINS};
use super::manifest::capabilities;
use crate::config::PluginFactsConfig;
use crate::db::plugin_facts_repository::{FactWrite, StoredClassification};
use crate::db::PluginFactsRepository;
use crate::models::*;
use crate::services::classification::build_classification_facts_with_plugins;

/// Audit resource type of classification changes and exports caused by
/// plugin facts (the resource id is the certname).
pub const AUDIT_PLUGIN_FACTS: &str = "plugin_facts";

const MAX_VALUE_DEPTH: usize = 32;
const MAX_CACHE_ENTRIES: usize = 100_000;

/// In-memory state of the fact providers: pull bulkheads, the time a node
/// was last resolved and the failure backoff.
pub struct FactsRuntime {
    cfg: PluginFactsConfig,
    bulkheads: Mutex<HashMap<String, Arc<Semaphore>>>,
    resolved_until: Mutex<HashMap<(String, String), DateTime<Utc>>>,
    retry_at: Mutex<HashMap<(String, String), DateTime<Utc>>>,
}

type NodeKey = (String, String);

fn key(plugin_id: &str, certname: &str) -> NodeKey {
    (plugin_id.to_string(), certname.to_string())
}

impl FactsRuntime {
    pub fn new(cfg: PluginFactsConfig) -> Self {
        Self {
            cfg,
            bulkheads: Mutex::new(HashMap::new()),
            resolved_until: Mutex::new(HashMap::new()),
            retry_at: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &PluginFactsConfig {
        &self.cfg
    }

    fn pull_timeout_ms(&self) -> u64 {
        self.cfg.pull_timeout_ms.clamp(100, 10_000)
    }

    fn bulkhead(&self, plugin_id: &str) -> Arc<Semaphore> {
        let mut map = self.bulkheads.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(plugin_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.cfg.pull_concurrency.max(1) as usize)))
            .clone()
    }

    fn fresh(&self, plugin_id: &str, certname: &str, now: DateTime<Utc>) -> bool {
        let map = self
            .resolved_until
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.get(&key(plugin_id, certname)).is_some_and(|t| *t > now)
    }

    fn mark_resolved(&self, plugin_id: &str, certname: &str, until: DateTime<Utc>) {
        let mut map = self
            .resolved_until
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_CACHE_ENTRIES {
            map.clear();
        }
        map.insert(key(plugin_id, certname), until);
        self.retry_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key(plugin_id, certname));
    }

    fn backing_off(&self, plugin_id: &str, certname: &str, now: DateTime<Utc>) -> bool {
        let map = self.retry_at.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&key(plugin_id, certname)).is_some_and(|t| *t > now)
    }

    fn back_off(&self, plugin_id: &str, certname: &str, now: DateTime<Utc>) {
        let mut map = self.retry_at.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_CACHE_ENTRIES {
            map.clear();
        }
        let secs = self.cfg.pull_failure_backoff_secs.min(86_400) as i64;
        map.insert(key(plugin_id, certname), now + Duration::seconds(secs));
    }

    /// Drops the in-memory state of one plugin (uninstall).
    pub fn forget(&self, plugin_id: &str) {
        self.bulkheads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plugin_id);
        self.resolved_until
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(p, _), _| p != plugin_id);
        self.retry_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(p, _), _| p != plugin_id);
    }

    fn prune(&self, now: DateTime<Utc>) {
        self.resolved_until
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, t| *t > now);
        self.retry_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, t| *t > now);
    }
}

// ============================================================================
// Validation
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum FactError {
    /// The values do not fit the declared schema or the limits.
    #[error("{}", .0.join("; "))]
    Invalid(Vec<String>),
    /// The plugin is not an enabled fact provider.
    #[error("{0}")]
    NotProvider(String),
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

/// A certname as PuppetDB knows it: printable, no separators, bounded.
pub fn valid_certname(c: &str) -> bool {
    !c.is_empty()
        && c.len() <= 255
        && c.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
}

fn check_plain_json(v: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_VALUE_DEPTH {
        return Err(format!("is nested deeper than {} levels", MAX_VALUE_DEPTH));
    }
    match v {
        Value::String(s) if s.chars().any(char::is_control) => {
            Err("contains control characters".to_string())
        }
        Value::Array(a) => a.iter().try_for_each(|x| check_plain_json(x, depth + 1)),
        Value::Object(o) => o.iter().try_for_each(|(k, x)| {
            if k.chars().any(char::is_control) {
                Err("has a key with control characters".to_string())
            } else {
                check_plain_json(x, depth + 1)
            }
        }),
        _ => Ok(()),
    }
}

/// Checks one value against its declaration and the platform limits: type,
/// optional enum, 16 KB serialized, plain JSON without control characters.
pub fn validate_value(decl: &FactDecl, value: &Value) -> Result<(), String> {
    if !decl.fact_type.matches(value) {
        return Err(format!("must be a {}", decl.fact_type.as_str()));
    }
    if let Some(values) = &decl.enum_values {
        let ok = values.iter().any(|e| {
            e == value || matches!((e.as_f64(), value.as_f64()), (Some(a), Some(b)) if a == b)
        });
        if !ok {
            return Err("is not one of the declared enum values".to_string());
        }
    }
    let size = serde_json::to_vec(value)
        .map(|b| b.len())
        .unwrap_or(usize::MAX);
    if size > MAX_PLUGIN_FACT_VALUE_BYTES {
        return Err(format!(
            "is larger than {} bytes",
            MAX_PLUGIN_FACT_VALUE_BYTES
        ));
    }
    check_plain_json(value, 0)
}

/// Whether a plugin fact name collides with a fact reported by agents: the
/// same name, or a top-level fact named like the namespace.
pub fn collides(full_name: &str, agent_names: &HashSet<String>) -> bool {
    let ns = full_name.split('.').next().unwrap_or_default();
    agent_names.contains(full_name) || agent_names.contains(ns)
}

/// Full names and top-level names of agent facts, for [`collides`].
pub fn agent_names_of<'a>(names: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    let mut out = HashSet::new();
    for n in names {
        out.insert(n.to_string());
        if let Some((top, _)) = n.split_once('.') {
            out.insert(top.to_string());
        }
    }
    out
}

fn is_stale(computed_at: DateTime<Utc>, max_stale_s: u64, now: DateTime<Utc>) -> bool {
    now.signed_duration_since(computed_at) > secs(max_stale_s)
}

fn secs(s: u64) -> Duration {
    Duration::seconds(s.min(i64::MAX as u64 / 2000) as i64)
}

/// A value of the merged node view.
#[derive(Debug, Clone)]
pub struct NodePluginFact {
    pub plugin_id: String,
    pub certname: String,
    pub name: String,
    pub value: Value,
    pub computed_at: DateTime<Utc>,
    pub stale: bool,
    pub requires: Option<Requires>,
}

impl NodePluginFact {
    pub fn to_merged(&self) -> MergedFact {
        MergedFact {
            fact: Fact {
                certname: self.certname.clone(),
                name: self.name.clone(),
                value: self.value.clone(),
                environment: None,
            },
            source: Some(plugin_source(&self.plugin_id)),
            computed_at: Some(self.computed_at),
            stale: Some(self.stale),
        }
    }
}

/// Plugin facts an administrator enabled for rules, for one node.
#[derive(Debug, Clone, Default)]
pub struct UsableFacts {
    pub facts: Vec<ClassificationPluginFact>,
    /// Providers that failed to resolve or whose data is past `max_stale_s`.
    pub unavailable: BTreeSet<String>,
    /// At least one usable fact exists, so classification must look at them.
    pub relevant: bool,
}

/// Usable plugin facts of every node (no pull resolution), for fleet-wide
/// views such as the nodes of a group.
#[derive(Debug, Clone, Default)]
pub struct UsableFactsSnapshot {
    by_node: HashMap<String, Vec<ClassificationPluginFact>>,
}

impl UsableFactsSnapshot {
    pub fn for_node(&self, certname: &str) -> &[ClassificationPluginFact] {
        self.by_node
            .get(certname)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.by_node.is_empty()
    }
}

/// What makes classifications comparable: groups, environment, classes,
/// variables and a cross-organization conflict.
pub fn classification_signature(r: &ClassificationResult) -> String {
    let mut groups: Vec<String> = r.groups.iter().map(|g| g.id.to_string()).collect();
    groups.sort();
    json!({
        "groups": groups,
        "environment": r.environment,
        "classes": r.classes,
        "variables": r.variables,
        "conflict": r.conflict_error,
    })
    .to_string()
}

fn short(r: &ClassificationResult) -> Value {
    json!({
        "groups": r.groups.iter().map(|g| &g.name).collect::<Vec<_>>(),
        "environment": r.environment,
    })
}

type Provider = (PluginHooks, FactsContribution);

// ============================================================================
// Manager operations
// ============================================================================

impl PluginManager {
    fn facts_repo(&self) -> PluginFactsRepository<'_> {
        PluginFactsRepository::new(self.pool())
    }

    /// Enabled plugins that declare facts.
    pub fn fact_providers(&self) -> Vec<PluginHooks> {
        self.hooks()
            .plugins()
            .filter(|h| h.facts.is_some())
            .cloned()
            .collect()
    }

    fn providers(&self) -> Vec<Provider> {
        self.fact_providers()
            .into_iter()
            .filter_map(|h| h.facts.clone().map(|f| (h, f)))
            .collect()
    }

    fn provider(&self, plugin_id: &str) -> Option<Provider> {
        let h = self.hooks().get(plugin_id)?.clone();
        let f = h.facts.clone()?;
        Some((h, f))
    }

    // ---- writes -----------------------------------------------------------

    /// Stores facts of one node for a plugin (host API `facts.put` and pull
    /// results). Keys are full or local names of declared facts and `null`
    /// removes a fact. One bad value refuses the whole batch; a stored batch
    /// publishes one `plugin.fact.updated` event.
    pub async fn put_plugin_facts(
        &self,
        plugin_id: &str,
        certname: &str,
        facts: &Map<String, Value>,
        mode: FactMode,
    ) -> Result<usize, FactError> {
        let (_, contrib) = self.provider(plugin_id).ok_or_else(|| {
            FactError::NotProvider(format!(
                "plugin '{}' is not an enabled fact provider",
                plugin_id
            ))
        })?;
        let now = Utc::now();
        let mut problems = Vec::new();
        if !valid_certname(certname) {
            problems.push(format!("certname '{}' is invalid", certname));
        }
        if facts.len() > MAX_PLUGIN_FACTS_PER_NODE {
            problems.push(format!(
                "a batch holds at most {} facts",
                MAX_PLUGIN_FACTS_PER_NODE
            ));
        }
        let mut writes: Vec<(String, &Value, Option<DateTime<Utc>>)> = Vec::new();
        let mut delete: Vec<String> = Vec::new();
        for (k, value) in facts.iter().take(MAX_PLUGIN_FACTS_PER_NODE + 1) {
            if let Some((ns, _)) = k.split_once('.') {
                if ns != contrib.namespace {
                    problems.push(format!(
                        "'{}' is outside the plugin namespace '{}'",
                        k, contrib.namespace
                    ));
                    continue;
                }
            }
            let Some(decl) = contrib.find(k) else {
                problems.push(format!("'{}' is not a declared fact", k));
                continue;
            };
            let full = contrib.full_name(decl);
            if value.is_null() {
                delete.push(full);
                continue;
            }
            if let Err(e) = validate_value(decl, value) {
                problems.push(format!("'{}' {}", full, e));
                continue;
            }
            let expires = now + secs(contrib.ttl_for(decl));
            writes.push((full, value, Some(expires)));
        }
        if problems.is_empty() {
            let mut held: BTreeSet<String> = self
                .facts_repo()
                .names_for_node(plugin_id, certname)
                .await?
                .into_iter()
                .collect();
            held.extend(writes.iter().map(|(n, _, _)| n.clone()));
            for d in &delete {
                held.remove(d);
            }
            if held.len() > MAX_PLUGIN_FACTS_PER_NODE {
                problems.push(format!(
                    "a plugin may hold at most {} facts per node",
                    MAX_PLUGIN_FACTS_PER_NODE
                ));
            }
        }
        if !problems.is_empty() {
            let _ = self
                .facts_repo()
                .record_failure(plugin_id, &problems.join("; "), true, now)
                .await;
            return Err(FactError::Invalid(problems));
        }
        let batch: Vec<FactWrite<'_>> = writes
            .iter()
            .map(|(name, value, expires_at)| FactWrite {
                name,
                value,
                expires_at: *expires_at,
            })
            .collect();
        self.facts_repo()
            .write_batch(plugin_id, certname, &batch, &delete, now)
            .await?;
        let _ = self.facts_repo().record_success(plugin_id, now).await;
        if !writes.is_empty() || !delete.is_empty() {
            let names: Vec<&String> = writes.iter().map(|(n, _, _)| n).collect();
            EventBus::new(self.pool().clone())
                .emit(
                    "plugin.fact.updated",
                    json!({
                        "plugin_id": plugin_id,
                        "certname": certname,
                        "names": names,
                        "removed": delete,
                        "computed_at": now,
                        "mode": mode.as_str(),
                    }),
                    EventActor::plugin(plugin_id),
                )
                .await;
        }
        Ok(writes.len())
    }

    // ---- pull resolution --------------------------------------------------

    /// Calls the `resolve` handler of a pull provider for one node unless its
    /// cached values are fresh, under the pull timeout and the per-plugin
    /// bulkhead. A failure backs off and keeps the cached values.
    async fn resolve_pull(
        &self,
        hooks: &PluginHooks,
        contrib: &FactsContribution,
        certname: &str,
    ) -> Result<(), String> {
        let plugin_id = hooks.plugin_id.as_str();
        let now = Utc::now();
        let rt = self.facts_runtime();
        if rt.fresh(plugin_id, certname, now) {
            return Ok(());
        }
        // After a restart the in-memory marks are gone: unexpired rows count.
        let rows = self
            .facts_repo()
            .node_facts(certname)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(until) = rows
            .iter()
            .filter(|r| r.plugin_id == plugin_id)
            .filter_map(|r| r.expires_at)
            .filter(|t| *t > now)
            .min()
        {
            rt.mark_resolved(plugin_id, certname, until);
            return Ok(());
        }
        if rt.backing_off(plugin_id, certname, now) {
            return Err("the provider failed recently; using cached values".to_string());
        }
        let Some(handler) = contrib.handler.clone() else {
            return Err("no resolve handler".to_string());
        };
        let Ok(_permit) = rt.bulkhead(plugin_id).try_acquire_owned() else {
            return Err("too many concurrent resolve calls; using cached values".to_string());
        };
        let names: Vec<String> = contrib.items.iter().map(|i| contrib.full_name(i)).collect();
        let call = hooks.call(
            HandlerKind::Fact,
            &handler,
            json!({"certname": certname, "names": names}),
        );
        let outcome = match invoke_with_timeout(self, call, rt.pull_timeout_ms()).await {
            Ok(v) => match v.get("facts").and_then(Value::as_object) {
                Some(map) => self
                    .put_plugin_facts(plugin_id, certname, map, FactMode::Pull)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                None => {
                    let msg = "resolve must return {\"facts\": {...}}".to_string();
                    let _ = self
                        .facts_repo()
                        .record_failure(plugin_id, &msg, true, now)
                        .await;
                    Err(msg)
                }
            },
            Err(e) => {
                let msg = e.to_string();
                let _ = self
                    .facts_repo()
                    .record_failure(plugin_id, &msg, false, now)
                    .await;
                Err(msg)
            }
        };
        match &outcome {
            Ok(()) => {
                let ttl = contrib.ttl_s.unwrap_or(DEFAULT_PLUGIN_FACT_TTL_S).max(1);
                rt.mark_resolved(plugin_id, certname, now + secs(ttl));
            }
            Err(e) => {
                tracing::warn!(
                    plugin = plugin_id,
                    "fact resolution for {} failed: {}",
                    certname,
                    e
                );
                rt.back_off(plugin_id, certname, now);
            }
        }
        outcome
    }

    /// Resolves the pull providers among `providers` concurrently; returns
    /// the ids of those that failed.
    async fn resolve_providers(&self, providers: &[Provider], certname: &str) -> BTreeSet<String> {
        let calls =
            providers
                .iter()
                .filter(|(_, c)| c.mode == FactMode::Pull)
                .map(|(h, c)| async move {
                    (h.plugin_id.clone(), self.resolve_pull(h, c, certname).await)
                });
        futures::future::join_all(calls)
            .await
            .into_iter()
            .filter_map(|(id, r)| r.err().map(|_| id))
            .collect()
    }

    // ---- merged views -----------------------------------------------------

    /// Plugin facts of one node for the merged view (pull providers are
    /// resolved first when `resolve` is set). Rows of facts that are no longer
    /// declared and of disabled plugins are left out.
    pub async fn node_plugin_facts(&self, certname: &str, resolve: bool) -> Vec<NodePluginFact> {
        let providers = self.providers();
        if providers.is_empty() || !valid_certname(certname) {
            return Vec::new();
        }
        if resolve {
            self.resolve_providers(&providers, certname).await;
        }
        let rows = match self.facts_repo().node_facts(certname).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("could not read plugin facts of {}: {}", certname, e);
                return Vec::new();
            }
        };
        let now = Utc::now();
        rows.into_iter()
            .filter_map(|r| {
                let (_, contrib) = providers.iter().find(|(h, _)| h.plugin_id == r.plugin_id)?;
                let decl = contrib.find(&r.name)?;
                Some(NodePluginFact {
                    stale: is_stale(r.computed_at, contrib.max_stale(), now),
                    plugin_id: r.plugin_id,
                    certname: r.certname,
                    name: r.name,
                    value: r.value,
                    computed_at: r.computed_at,
                    requires: decl.requires.clone(),
                })
            })
            .collect()
    }

    /// Declared facts of the enabled providers: `(plugin_id, full name, decl)`.
    pub fn declared_plugin_facts(&self) -> Vec<(String, String, FactDecl)> {
        let mut out = Vec::new();
        for (h, f) in self.providers() {
            for i in &f.items {
                out.push((h.plugin_id.clone(), f.full_name(i), i.clone()));
            }
        }
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out
    }

    /// Stored values of one enabled provider (`GET /facts?source=plugin:<id>`).
    pub async fn query_plugin_facts(
        &self,
        plugin_id: &str,
        name: Option<&str>,
        certname: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<NodePluginFact>, PluginError> {
        let Some((_, contrib)) = self.provider(plugin_id) else {
            return Ok(Vec::new());
        };
        let rows = self
            .facts_repo()
            .query(plugin_id, name, certname, limit, offset)
            .await?;
        let now = Utc::now();
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let decl = contrib.find(&r.name)?;
                Some(NodePluginFact {
                    stale: is_stale(r.computed_at, contrib.max_stale(), now),
                    plugin_id: r.plugin_id,
                    certname: r.certname,
                    name: r.name,
                    value: r.value,
                    computed_at: r.computed_at,
                    requires: decl.requires.clone(),
                })
            })
            .collect())
    }

    // ---- classification ---------------------------------------------------

    /// Facts offered for rules and enabled by an administrator, for one node
    /// (pull providers are resolved first when `resolve` is set).
    pub async fn usable_facts_for_node(&self, certname: &str, resolve: bool) -> UsableFacts {
        let providers = self.providers();
        if providers.is_empty() || !valid_certname(certname) {
            return UsableFacts::default();
        }
        let settings = match self.facts_repo().enabled_settings().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("could not read plugin fact settings: {}", e);
                return UsableFacts::default();
            }
        };
        let mut usable: HashMap<NodeKey, u64> = HashMap::new();
        let mut used = Vec::new();
        for (h, f) in providers {
            let mut any = false;
            for i in f.items.iter().filter(|i| i.usable_in_rules) {
                let k = key(&h.plugin_id, &f.full_name(i));
                if settings.get(&k).is_some_and(|s| s.usable_in_rules) {
                    usable.insert(k, f.max_stale());
                    any = true;
                }
            }
            if any {
                used.push((h, f));
            }
        }
        if usable.is_empty() {
            return UsableFacts::default();
        }
        let mut unavailable = if resolve {
            self.resolve_providers(&used, certname).await
        } else {
            BTreeSet::new()
        };
        let rows = self
            .facts_repo()
            .node_facts(certname)
            .await
            .unwrap_or_default();
        let now = Utc::now();
        let mut facts = Vec::new();
        for r in rows {
            let Some(max_stale) = usable.get(&key(&r.plugin_id, &r.name)).copied() else {
                continue;
            };
            if is_stale(r.computed_at, max_stale, now) {
                unavailable.insert(r.plugin_id.clone());
            }
            facts.push(ClassificationPluginFact {
                plugin_id: r.plugin_id,
                name: r.name,
                value: r.value,
                computed_at: r.computed_at,
                max_stale_s: max_stale,
            });
        }
        UsableFacts {
            facts,
            unavailable,
            relevant: true,
        }
    }

    /// Usable plugin facts of every node from the stored values (no pull
    /// resolution), for fleet-wide views.
    pub async fn usable_facts_snapshot(&self) -> UsableFactsSnapshot {
        let providers = self.providers();
        if providers.is_empty() {
            return UsableFactsSnapshot::default();
        }
        let Ok(settings) = self.facts_repo().enabled_settings().await else {
            return UsableFactsSnapshot::default();
        };
        let mut snap = UsableFactsSnapshot::default();
        for (h, f) in providers {
            for i in f.items.iter().filter(|i| i.usable_in_rules) {
                let name = f.full_name(i);
                let enabled = settings
                    .get(&key(&h.plugin_id, &name))
                    .is_some_and(|s| s.usable_in_rules);
                if !enabled {
                    continue;
                }
                let rows = self
                    .facts_repo()
                    .values_of(&h.plugin_id, &name, None)
                    .await
                    .unwrap_or_default();
                for r in rows {
                    snap.by_node
                        .entry(r.certname)
                        .or_default()
                        .push(ClassificationPluginFact {
                            plugin_id: r.plugin_id,
                            name: r.name,
                            value: r.value,
                            computed_at: r.computed_at,
                            max_stale_s: f.max_stale(),
                        });
                }
            }
        }
        snap
    }

    /// Classifies a node with its usable plugin facts; without any it is the
    /// plain PuppetDB classification and reads no table. An unavailable
    /// provider with `freeze` keeps the last classification computed with
    /// plugin facts. Results are stored per `(certname, scope)` and a change
    /// that plugin facts influenced is audited with the responsible facts.
    /// The `before.node.classify` interceptors (M31) see the facts first and
    /// may veto the classification or change top-level facts.
    #[allow(clippy::too_many_arguments)]
    pub async fn classify_with_plugin_facts<F>(
        &self,
        certname: &str,
        scope: &str,
        facts: Vec<Fact>,
        catalog_environment: Option<&str>,
        groups: &[NodeGroup],
        freeze: bool,
        classify: F,
    ) -> Result<ClassificationResult, InterceptRefusal>
    where
        F: Fn(&Value) -> ClassificationResult,
    {
        let now = Utc::now();
        let usable = self.usable_facts_for_node(certname, true).await;
        let mut base_json = build_classification_facts_with_plugins(
            facts.clone(),
            certname,
            catalog_environment,
            &[],
            now,
        );
        let mut merged_json = usable.relevant.then(|| {
            build_classification_facts_with_plugins(
                facts,
                certname,
                catalog_environment,
                &usable.facts,
                now,
            )
        });
        if self.has_interceptors(BEFORE_NODE_CLASSIFY) {
            let payload = json!({
                "certname": certname,
                "scope": scope,
                "environment": catalog_environment,
                "facts": merged_json.as_ref().unwrap_or(&base_json),
            });
            let outcome = self
                .intercept(
                    BEFORE_NODE_CLASSIFY,
                    certname,
                    InterceptActor::system(),
                    payload,
                )
                .await?;
            if let Some(Value::Object(changed)) = outcome.changes.get("facts") {
                apply_fact_changes(&mut base_json, changed);
                if let Some(m) = merged_json.as_mut() {
                    apply_fact_changes(m, changed);
                }
            }
        }
        let Some(merged_json) = merged_json else {
            return Ok(classify(&base_json));
        };
        let base = classify(&base_json);
        let merged = classify(&merged_json);
        let repo = self.facts_repo();
        let stored = repo
            .get_classification(certname, scope)
            .await
            .ok()
            .flatten();

        if !usable.unavailable.is_empty() {
            if let (true, Some(s)) = (freeze, &stored) {
                if let Ok(frozen) = serde_json::from_str::<ClassificationResult>(&s.result_json) {
                    tracing::warn!(
                        "plugin fact provider(s) {:?} unavailable for {}: keeping its last classification",
                        usable.unavailable,
                        certname
                    );
                    return Ok(frozen);
                }
            }
            // A degraded result is served but not stored, so a later freeze
            // keeps the last classification computed with complete data.
            return Ok(merged);
        }

        let sig = classification_signature(&merged);
        let influenced = sig != classification_signature(&base);
        if stored.as_ref().is_some_and(|s| s.signature == sig) {
            return Ok(merged);
        }
        let previous: Option<ClassificationResult> = stored
            .as_ref()
            .and_then(|s| serde_json::from_str(&s.result_json).ok());
        let was_influenced = stored.as_ref().is_some_and(|s| s.influenced);
        if influenced || was_influenced {
            let before = previous.as_ref().unwrap_or(&base);
            let responsible = responsible_facts(before, &merged, groups, &usable.facts, now);
            self.audit(
                default_organization_uuid(),
                None,
                AUDIT_PLUGIN_FACTS,
                "plugin.fact.classification_changed",
                certname,
                json!({
                    "certname": certname,
                    "scope": scope,
                    "before": short(before),
                    "after": short(&merged),
                    "facts": responsible,
                }),
            )
            .await;
        }
        let facts_json = serde_json::to_string(&usable.facts).unwrap_or_else(|_| "[]".into());
        if let Ok(result_json) = serde_json::to_string(&merged) {
            let row = StoredClassification {
                result_json,
                signature: sig,
                influenced,
                facts_json,
                updated_at: now,
            };
            if let Err(e) = repo.put_classification(certname, scope, &row).await {
                tracing::warn!("could not store the classification of {}: {}", certname, e);
            }
        }
        Ok(merged)
    }

    // ---- export to agents -------------------------------------------------

    /// Exportable facts of one node as sorted `(name, value)`: enabled by an
    /// administrator, `facts:export:external` effective now, not stale, no
    /// agent conflict, capped by `plugins.facts.max_export_bytes`. A change
    /// of the exported set is audited with names only, never values.
    pub async fn exportable_facts_for_node(
        &self,
        certname: &str,
        agent_names: &HashSet<String>,
    ) -> Vec<(String, Value)> {
        let providers = self.providers();
        if providers.is_empty() || !valid_certname(certname) {
            return Vec::new();
        }
        let Ok(settings) = self.facts_repo().enabled_settings().await else {
            return Vec::new();
        };
        let mut wanted: HashMap<NodeKey, (FactDecl, u64)> = HashMap::new();
        let mut used = Vec::new();
        for (h, f) in providers {
            let mut any = false;
            for i in f.items.iter().filter(|i| i.exportable) {
                let k = key(&h.plugin_id, &f.full_name(i));
                if settings.get(&k).is_some_and(|s| s.exportable) {
                    wanted.insert(k, (i.clone(), f.max_stale()));
                    any = true;
                }
            }
            if any && self.export_capability(&h.plugin_id).await == ExportCapabilityState::Approved
            {
                used.push((h, f));
            }
        }
        wanted.retain(|(p, _), _| used.iter().any(|(h, _)| &h.plugin_id == p));
        if wanted.is_empty() {
            self.record_export(certname, &[], 0).await;
            return Vec::new();
        }
        self.resolve_providers(&used, certname).await;
        let rows = self
            .facts_repo()
            .node_facts(certname)
            .await
            .unwrap_or_default();
        let now = Utc::now();
        let mut out: BTreeMap<String, Value> = BTreeMap::new();
        for r in rows {
            let Some((decl, max_stale)) = wanted.get(&key(&r.plugin_id, &r.name)) else {
                continue;
            };
            if is_stale(r.computed_at, *max_stale, now)
                || collides(&r.name, agent_names)
                || validate_value(decl, &r.value).is_err()
            {
                continue;
            }
            out.insert(r.name, r.value);
        }
        let max = self.facts_runtime().config().max_export_bytes;
        let mut total = 0usize;
        let mut list = Vec::new();
        for (name, value) in out {
            let size = name.len() + serde_json::to_vec(&value).map(|b| b.len()).unwrap_or(0);
            if total + size > max {
                tracing::warn!(
                    "plugin fact {} left out of the export of {}: over {} bytes",
                    name,
                    certname,
                    max
                );
                continue;
            }
            total += size;
            list.push((name, value));
        }
        self.record_export(certname, &list, total).await;
        list
    }

    async fn record_export(&self, certname: &str, list: &[(String, Value)], bytes: usize) {
        let digest = hex::encode(Sha256::digest(serde_json::to_vec(list).unwrap_or_default()));
        let repo = self.facts_repo();
        let previous = repo.export_digest(certname).await.ok().flatten();
        if previous.as_deref() == Some(digest.as_str()) || (previous.is_none() && list.is_empty()) {
            return;
        }
        let names: Vec<String> = list.iter().map(|(n, _)| n.clone()).collect();
        self.audit(
            default_organization_uuid(),
            None,
            AUDIT_PLUGIN_FACTS,
            "plugin.fact.exported",
            certname,
            json!({"certname": certname, "names": names, "bytes": bytes, "digest": digest}),
        )
        .await;
        if let Err(e) = repo.put_export(certname, &digest, &names).await {
            tracing::warn!("could not record the fact export of {}: {}", certname, e);
        }
    }

    /// Whether `facts:export:external` is declared and usable for a plugin.
    pub async fn export_capability(&self, plugin_id: &str) -> ExportCapabilityState {
        let Ok(Some(rec)) = self.repo().get_plugin(plugin_id).await else {
            return ExportCapabilityState::NotDeclared;
        };
        let Some(v) = rec.active_version else {
            return ExportCapabilityState::NotDeclared;
        };
        let Ok(m) = self.manifest_of(plugin_id, &v).await else {
            return ExportCapabilityState::NotDeclared;
        };
        self.export_capability_of(plugin_id, &m).await
    }

    async fn export_capability_of(
        &self,
        plugin_id: &str,
        m: &PluginManifest,
    ) -> ExportCapabilityState {
        const CAP: &str = "facts:export:external";
        if !capabilities(m).iter().any(|c| c.name == CAP) {
            return ExportCapabilityState::NotDeclared;
        }
        if self.policy().denies(CAP) {
            return ExportCapabilityState::DeniedByPolicy;
        }
        // Built-in plugins are first-party code and are not gated (M20).
        if m.runtime.kind == RuntimeKind::Builtin
            || self
                .effective_capabilities(plugin_id)
                .await
                .iter()
                .any(|c| c.name == CAP)
        {
            ExportCapabilityState::Approved
        } else {
            ExportCapabilityState::NotApproved
        }
    }

    // ---- administration ---------------------------------------------------

    /// Fact catalog of every installed plugin that declares facts;
    /// `agent_names` (from PuppetDB) marks conflicts when known.
    pub async fn fact_catalogs(
        &self,
        agent_names: Option<&HashSet<String>>,
    ) -> Result<Vec<PluginFactCatalog>, PluginError> {
        let mut out = Vec::new();
        for rec in self.repo().list_plugins().await? {
            if let Some(c) = self.catalog_of(rec, agent_names).await? {
                out.push(c);
            }
        }
        Ok(out)
    }

    pub async fn fact_catalog(
        &self,
        id: &str,
        agent_names: Option<&HashSet<String>>,
    ) -> Result<PluginFactCatalog, PluginError> {
        let rec = self.require(id).await?;
        self.catalog_of(rec, agent_names)
            .await?
            .ok_or_else(|| PluginError::NotFound(format!("Plugin '{}' declares no facts", id)))
    }

    async fn active_facts(
        &self,
        rec: &PluginRecord,
    ) -> Result<Option<(PluginManifest, FactsContribution)>, PluginError> {
        let Some(v) = &rec.active_version else {
            return Ok(None);
        };
        let m = self.manifest_of(&rec.id, v).await?;
        Ok(m.contributes.facts.clone().map(|f| (m, f)))
    }

    async fn declared_fact(
        &self,
        id: &str,
        name: &str,
    ) -> Result<(PluginManifest, FactsContribution, FactDecl), PluginError> {
        let rec = self.require(id).await?;
        let (m, f) = self
            .active_facts(&rec)
            .await?
            .ok_or_else(|| PluginError::NotFound(format!("Plugin '{}' declares no facts", id)))?;
        let decl = f
            .find(name)
            .filter(|_| name.contains('.'))
            .cloned()
            .ok_or_else(|| {
                PluginError::NotFound(format!("'{}' is not a fact of '{}'", name, id))
            })?;
        Ok((m, f, decl))
    }

    async fn catalog_of(
        &self,
        rec: PluginRecord,
        agent_names: Option<&HashSet<String>>,
    ) -> Result<Option<PluginFactCatalog>, PluginError> {
        let Some((m, f)) = self.active_facts(&rec).await? else {
            return Ok(None);
        };
        let repo = self.facts_repo();
        let settings = repo.settings(&rec.id).await?;
        let stats = repo
            .stats(&rec.id, Utc::now() - secs(f.max_stale()))
            .await?;
        let facts = f
            .items
            .iter()
            .map(|i| {
                let name = f.full_name(i);
                fact_info(&f, i, settings.get(&name), stats.get(&name), agent_names)
            })
            .collect();
        Ok(Some(PluginFactCatalog {
            plugin_name: m.name.clone(),
            state: rec.state,
            active_version: rec.active_version.clone(),
            namespace: f.namespace.clone(),
            mode: f.mode,
            handler: f.handler.clone(),
            ttl_s: f.ttl_s.unwrap_or(DEFAULT_PLUGIN_FACT_TTL_S),
            max_stale_s: f.max_stale(),
            export_capability: self.export_capability_of(&rec.id, &m).await,
            facts,
            provider: repo.provider_status(&rec.id).await?,
            puppetdb_checked: agent_names.is_some(),
            plugin_id: rec.id,
        }))
    }

    /// Changes the administrator's opt-in for one fact. Enabling needs the
    /// manifest to offer the use; enabling export also needs an effective
    /// `facts:export:external` and the full fact name in `confirm`. Audited.
    pub async fn update_fact_setting(
        &self,
        id: &str,
        name: &str,
        req: &UpdateFactSettingsRequest,
        actor: &str,
    ) -> Result<PluginFactInfo, PluginError> {
        let (m, f, decl) = self.declared_fact(id, name).await?;
        let full = f.full_name(&decl);
        let repo = self.facts_repo();
        let before = repo.settings(id).await?.remove(&full).unwrap_or_default();
        let mut after = before.clone();
        let refuse = |code: &'static str, message: String| PluginError::Unprocessable {
            code,
            message,
            details: None,
        };
        if let Some(on) = req.usable_in_rules {
            if on && !decl.usable_in_rules {
                return Err(refuse(
                    "not_offered",
                    format!(
                        "The plugin does not offer '{}' for classification rules",
                        full
                    ),
                ));
            }
            after.usable_in_rules = on;
        }
        if let Some(on) = req.exportable {
            if on && !before.exportable {
                if !decl.exportable {
                    return Err(refuse(
                        "not_offered",
                        format!("The plugin does not offer '{}' for export to agents", full),
                    ));
                }
                if self.export_capability_of(id, &m).await != ExportCapabilityState::Approved {
                    return Err(refuse(
                        "export_capability_missing",
                        "The capability facts:export:external is not approved for this plugin"
                            .to_string(),
                    ));
                }
                if req.confirm.as_deref() != Some(full.as_str()) {
                    return Err(refuse(
                        "confirmation_required",
                        format!("Type the fact name '{}' to confirm the export", full),
                    ));
                }
            }
            after.exportable = on;
        }
        let enabling = (after.usable_in_rules && !before.usable_in_rules)
            || (after.exportable && !before.exportable);
        if enabling {
            after.approved_by = Some(actor.to_string());
            after.approved_at = Some(Utc::now());
        } else if !after.usable_in_rules && !after.exportable {
            after.approved_by = None;
            after.approved_at = None;
        }
        repo.put_setting(id, &full, &after).await?;
        if before.usable_in_rules != after.usable_in_rules || before.exportable != after.exportable
        {
            self.audit(
                default_organization_uuid(),
                None,
                AUDIT_PLUGINS,
                "plugin.fact.settings_changed",
                id,
                json!({
                    "fact": full,
                    "sensitivity": decl.sensitivity,
                    "usable_in_rules": {"from": before.usable_in_rules, "to": after.usable_in_rules},
                    "exportable": {"from": before.exportable, "to": after.exportable},
                    "by": actor,
                }),
            )
            .await;
        }
        let stats = repo.stats(id, Utc::now() - secs(f.max_stale())).await?;
        Ok(fact_info(&f, &decl, Some(&after), stats.get(&full), None))
    }

    /// Values of one fact per node, shown before an administrator enables
    /// its export.
    pub async fn fact_preview(
        &self,
        id: &str,
        name: &str,
        limit: u32,
    ) -> Result<PluginFactPreview, PluginError> {
        let (_, f, decl) = self.declared_fact(id, name).await?;
        let full = f.full_name(&decl);
        let limit = limit.clamp(1, 500);
        let repo = self.facts_repo();
        let rows = repo.values_of(id, &full, Some(limit)).await?;
        let now = Utc::now();
        let setting = repo.settings(id).await?.remove(&full).unwrap_or_default();
        Ok(PluginFactPreview {
            plugin_id: id.to_string(),
            name: full.clone(),
            sensitivity: decl.sensitivity.clone(),
            exportable: setting.exportable,
            total: repo.count_values(id, &full).await?,
            limit,
            rows: rows
                .into_iter()
                .map(|r| PluginFactPreviewRow {
                    bytes: serde_json::to_vec(&r.value).map(|b| b.len()).unwrap_or(0),
                    stale: is_stale(r.computed_at, f.max_stale(), now),
                    certname: r.certname,
                    value: r.value,
                    computed_at: r.computed_at,
                })
                .collect(),
        })
    }

    /// Removes a plugin's fact settings and status, and its values with `purge`.
    pub(super) async fn forget_plugin_facts(&self, id: &str, purge: bool) {
        self.facts_runtime().forget(id);
        if let Err(e) = self.facts_repo().delete_plugin(id, purge).await {
            tracing::warn!("could not remove the facts of {}: {}", id, e);
        }
    }

    /// Fact namespaces of the active versions of the other installed
    /// plugins: `namespace -> plugin id`.
    pub(super) async fn fact_namespaces_except(&self, id: &str) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Ok(list) = self.repo().list_plugins().await else {
            return out;
        };
        for rec in list.into_iter().filter(|r| r.id != id) {
            if let Ok(Some((_, f))) = self.active_facts(&rec).await {
                out.insert(f.namespace, rec.id);
            }
        }
        out
    }

    // ---- stale sweep ------------------------------------------------------

    /// Publishes `plugin.fact.stale` once per (plugin, node) for values that
    /// became older than `max_stale_s`, and prunes the in-memory caches.
    pub async fn sweep_stale_facts(&self) {
        let now = Utc::now();
        self.facts_runtime().prune(now);
        let bus = EventBus::new(self.pool().clone());
        for (h, f) in self.providers() {
            let before = now - secs(f.max_stale());
            let rows = match self
                .facts_repo()
                .newly_stale(&h.plugin_id, before, 5000)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("stale fact sweep of {} failed: {}", h.plugin_id, e);
                    continue;
                }
            };
            let mut by_node: BTreeMap<String, Vec<PluginFactRecord>> = BTreeMap::new();
            for r in rows {
                by_node.entry(r.certname.clone()).or_default().push(r);
            }
            for (certname, list) in by_node {
                let names: Vec<String> = list.iter().map(|r| r.name.clone()).collect();
                let oldest = list.iter().map(|r| r.computed_at).min();
                bus.emit(
                    "plugin.fact.stale",
                    json!({
                        "plugin_id": h.plugin_id,
                        "certname": certname,
                        "names": names,
                        "computed_at": oldest,
                        "max_stale_s": f.max_stale(),
                    }),
                    EventActor::system(),
                )
                .await;
                if let Err(e) = self
                    .facts_repo()
                    .mark_stale_notified(&h.plugin_id, &certname, &names)
                    .await
                {
                    tracing::warn!("could not mark stale facts of {}: {}", certname, e);
                }
            }
        }
    }
}

fn fact_info(
    f: &FactsContribution,
    i: &FactDecl,
    setting: Option<&PluginFactSetting>,
    stats: Option<&(u64, u64)>,
    agent_names: Option<&HashSet<String>>,
) -> PluginFactInfo {
    let name = f.full_name(i);
    let s = setting.cloned().unwrap_or_default();
    let (nodes, stale_nodes) = stats.copied().unwrap_or((0, 0));
    PluginFactInfo {
        conflict: agent_names.is_some_and(|a| collides(&name, a)),
        name,
        local_name: i.name.clone(),
        fact_type: i.fact_type,
        description: i.description.clone(),
        enum_values: i.enum_values.clone(),
        ttl_s: f.ttl_for(i),
        max_stale_s: f.max_stale(),
        sensitivity: i.sensitivity.clone(),
        offers_rules: i.usable_in_rules,
        offers_export: i.exportable,
        usable_in_rules: s.usable_in_rules,
        exportable: s.exportable,
        approved_by: s.approved_by,
        approved_at: s.approved_at,
        requires: i.requires.clone(),
        nodes,
        stale_nodes,
    }
}

/// Plugin facts referenced by the rules of the groups whose membership
/// differs between two classifications; all usable facts when none is.
fn responsible_facts(
    before: &ClassificationResult,
    after: &ClassificationResult,
    groups: &[NodeGroup],
    facts: &[ClassificationPluginFact],
    now: DateTime<Utc>,
) -> Vec<Value> {
    let ids_before: HashSet<_> = before.groups.iter().map(|g| g.id).collect();
    let ids_after: HashSet<_> = after.groups.iter().map(|g| g.id).collect();
    let changed: HashSet<_> = ids_before.symmetric_difference(&ids_after).collect();
    let paths: Vec<&str> = groups
        .iter()
        .filter(|g| changed.contains(&g.id))
        .flat_map(|g| g.rules.iter().map(|r| r.fact_path.as_str()))
        .collect();
    let referenced = |name: &str| {
        paths
            .iter()
            .any(|p| *p == name || p.starts_with(&format!("{}.", name)))
    };
    let pick: Vec<&ClassificationPluginFact> = if facts.iter().any(|f| referenced(&f.name)) {
        facts.iter().filter(|f| referenced(&f.name)).collect()
    } else {
        facts.iter().collect()
    };
    pick.into_iter()
        .map(|f| {
            json!({
                "name": f.name,
                "source": plugin_source(&f.plugin_id),
                "value": f.value,
                "computed_at": f.computed_at,
                "stale": is_stale(f.computed_at, f.max_stale_s, now),
            })
        })
        .collect()
}

/// Publishes `plugin.fact.stale` periodically (`plugins.facts.sweep_secs`).
pub fn start_fact_sweeper(manager: Arc<PluginManager>) {
    let every = manager.facts_runtime().config().sweep_secs.max(30);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(every));
        tick.tick().await;
        loop {
            tick.tick().await;
            manager.sweep_stale_facts().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ClassificationRule, RuleMatchType, RuleOperator};
    use crate::services::classification::ClassificationService;
    use crate::services::plugins::invoke::{HandlerCall, HandlerInvoker, InvokeError};
    use crate::services::plugins::package::build_signed_package;
    use crate::services::plugins::BuiltinPlugin;
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;
    use uuid::Uuid;

    const SEED: [u8; 32] = [11; 32];

    fn manifest(id: &str, kind: &str, mode: &str) -> String {
        let runtime = match kind {
            "builtin" => format!("{{ kind: builtin, entry: {id} }}"),
            _ => "{ kind: remote, endpoint: \"https://cmdb.example.com\" }".to_string(),
        };
        let handler = if mode == "pull" {
            "\n    handler: resolve"
        } else {
            ""
        };
        format!(
            "api_version: 2\nid: {id}\nname: CMDB\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {runtime}\ncapabilities:\n  - facts:provide\n  - facts:export:external\n\
             contributes:\n  facts:\n    namespace: acme_cmdb\n    mode: {mode}{handler}\n\
             \x20   ttl_s: 60\n    max_stale_s: 3600\n    items:\n\
             \x20     - {{ name: owner, type: string, sensitivity: pii, exportable: true }}\n\
             \x20     - {{ name: tier, type: integer, enum: [1, 2, 3], usable_in_rules: true }}\n\
             \x20     - {{ name: tags, type: list }}\n"
        )
    }

    fn package(id: &str, kind: &str, mode: &str) -> Vec<u8> {
        let y = manifest(id, kind, mode);
        build_signed_package(&[("plugin.yaml", y.as_bytes())], id, "1.0.0", 2, SEED)
    }

    /// Built-in provider whose `resolve` answers with `reply`.
    struct Cmdb {
        reply: Mutex<Result<Value, String>>,
        calls: Mutex<u32>,
    }

    impl BuiltinPlugin for Cmdb {
        fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
            *self.calls.lock().unwrap() += 1;
            assert_eq!(call.kind, HandlerKind::Fact);
            self.reply
                .lock()
                .unwrap()
                .clone()
                .map_err(|e| anyhow::anyhow!(e))
        }
    }

    /// Answers every call after `delay`, to prove the pull timeout.
    struct Slow;

    #[async_trait::async_trait]
    impl HandlerInvoker for Slow {
        async fn invoke(&self, _call: HandlerCall) -> Result<Value, InvokeError> {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(json!({"facts": {"tier": 1}}))
        }
    }

    struct Env {
        manager: Arc<PluginManager>,
        cmdb: Arc<Cmdb>,
        _dir: std::path::PathBuf,
    }

    async fn env(id: &str, kind: &str, mode: &str, cfg: PluginFactsConfig) -> Env {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ovx_facts_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let cmdb = Arc::new(Cmdb {
            reply: Mutex::new(Ok(json!({"facts": {"tier": 2, "acme_cmdb.owner": "ops"}}))),
            calls: Mutex::new(0),
        });
        let manager = Arc::new(
            PluginManager::new(pool, dir.clone(), 8)
                .with_builtin_keys(vec![("OpenVox".into(), pk)])
                .with_builtin_runtime(id, cmdb.clone())
                .with_invoker(Arc::new(Slow))
                .with_facts(cfg),
        );
        manager.reload().await.unwrap();
        manager
            .install(&package(id, kind, mode), "admin", None)
            .await
            .unwrap();
        if kind == "builtin" {
            manager.enable(id).await.unwrap();
        } else {
            // The remote runtime cannot be enabled yet; force it into play.
            manager
                .repo()
                .set_state(id, PluginState::Enabled, None)
                .await
                .unwrap();
            manager.reload().await.unwrap();
        }
        Env {
            manager,
            cmdb,
            _dir: dir,
        }
    }

    async fn count(m: &PluginManager, sql: &'static str, arg: &str) -> i64 {
        sqlx::query_scalar(sql)
            .bind(arg)
            .fetch_one(m.pool())
            .await
            .unwrap()
    }

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn group(env: &str, tier: i64) -> NodeGroup {
        NodeGroup {
            name: format!("tier{tier}"),
            environment: Some(env.to_string()),
            is_environment_group: true,
            rule_match_type: RuleMatchType::All,
            rules: vec![ClassificationRule {
                id: Uuid::new_v4(),
                fact_path: "acme_cmdb.tier".into(),
                operator: RuleOperator::Equals,
                value: json!(tier),
            }],
            ..Default::default()
        }
    }

    async fn classify(
        m: &PluginManager,
        groups: &[NodeGroup],
        freeze: bool,
    ) -> ClassificationResult {
        let svc = ClassificationService::new(groups.to_vec());
        m.classify_with_plugin_facts("n1", "all", Vec::new(), None, groups, freeze, |f| {
            svc.classify("n1", f)
        })
        .await
        .unwrap()
    }

    #[test]
    fn values_are_checked_against_type_enum_size_and_control_characters() {
        let decl = |t: PluginFactType| FactDecl {
            name: "x".into(),
            fact_type: t,
            description: None,
            enum_values: None,
            ttl_s: None,
            sensitivity: "none".into(),
            usable_in_rules: false,
            exportable: false,
            requires: None,
        };
        assert!(validate_value(&decl(PluginFactType::String), &json!("ok")).is_ok());
        assert!(validate_value(&decl(PluginFactType::String), &json!(1)).is_err());
        assert!(validate_value(&decl(PluginFactType::Integer), &json!(3)).is_ok());
        assert!(validate_value(&decl(PluginFactType::Integer), &json!(3.5)).is_err());
        assert!(validate_value(&decl(PluginFactType::Float), &json!(3)).is_ok());
        assert!(validate_value(&decl(PluginFactType::Bool), &json!(true)).is_ok());
        assert!(validate_value(&decl(PluginFactType::List), &json!([1, "a"])).is_ok());
        assert!(validate_value(&decl(PluginFactType::Object), &json!([])).is_err());
        let mut e = decl(PluginFactType::Integer);
        e.enum_values = Some(vec![json!(1), json!(2)]);
        assert!(validate_value(&e, &json!(2)).is_ok());
        assert!(validate_value(&e, &json!(5)).is_err());
        let big = "a".repeat(MAX_PLUGIN_FACT_VALUE_BYTES);
        assert!(validate_value(&decl(PluginFactType::String), &json!(big)).is_err());
        assert!(validate_value(&decl(PluginFactType::String), &json!("a\nb")).is_err());
        assert!(validate_value(&decl(PluginFactType::Object), &json!({"k\u{7}": 1})).is_err());
        assert!(validate_value(&decl(PluginFactType::List), &json!(["\u{1b}[31m"])).is_err());
        assert!(!valid_certname("a/b") && !valid_certname("") && valid_certname("n1.example.com"));
        // A structured agent fact `acme` takes the whole namespace.
        let agent = agent_names_of(["os.family", "kernel", "acme.owner"]);
        assert!(collides("acme.tier", &agent) && collides("os.x", &agent));
        assert!(!collides("cmdb.tier", &agent));
    }

    #[tokio::test]
    async fn push_writes_only_declared_facts_of_the_own_namespace() {
        let e = env("cmdb", "builtin", "push", Default::default()).await;
        let m = &e.manager;
        for bad in [
            json!({"other_ns.owner": "x"}),
            json!({"unknown": 1}),
            json!({"tier": 9}),
            json!({"tier": 1, "owner": 5}),
            json!({"owner": "line\nbreak"}),
        ] {
            let err = m
                .put_plugin_facts("cmdb", "n1", &map(bad.clone()), FactMode::Push)
                .await
                .err()
                .unwrap_or_else(|| panic!("{bad} was accepted"));
            assert!(matches!(err, FactError::Invalid(_)), "{bad}: {err}");
        }
        // A refused batch stores nothing.
        assert!(m.node_plugin_facts("n1", false).await.is_empty());
        let n = m
            .put_plugin_facts(
                "cmdb",
                "n1",
                &map(json!({"tier": 1, "acme_cmdb.owner": "ops", "tags": ["a"]})),
                FactMode::Push,
            )
            .await
            .unwrap();
        assert_eq!(n, 3);
        let facts = m.node_plugin_facts("n1", false).await;
        assert_eq!(facts.len(), 3);
        assert!(facts.iter().all(|f| !f.stale && f.plugin_id == "cmdb"));
        // One event per batch, not per fact.
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM event_outbox WHERE type = ?",
                "plugin.fact.updated"
            )
            .await,
            1
        );
        // null removes a fact.
        m.put_plugin_facts("cmdb", "n1", &map(json!({"tags": null})), FactMode::Push)
            .await
            .unwrap();
        assert_eq!(m.node_plugin_facts("n1", false).await.len(), 2);
        let status = m.fact_catalog("cmdb", None).await.unwrap().provider;
        assert_eq!(status.rejected, 5);
        assert!(status.last_success_at.is_some());
        // A plugin that is not a provider cannot write.
        assert!(matches!(
            m.put_plugin_facts("nobody", "n1", &map(json!({"tier": 1})), FactMode::Push)
                .await,
            Err(FactError::NotProvider(_))
        ));
    }

    #[tokio::test]
    async fn settings_need_the_offer_the_capability_and_a_confirmation() {
        let e = env("cmdb", "builtin", "push", Default::default()).await;
        let m = &e.manager;
        let req = |rules: Option<bool>, export: Option<bool>, confirm: Option<&str>| {
            UpdateFactSettingsRequest {
                usable_in_rules: rules,
                exportable: export,
                confirm: confirm.map(str::to_string),
            }
        };
        let code = |r: Result<PluginFactInfo, PluginError>| match r {
            Err(PluginError::Unprocessable { code, .. }) => code,
            other => panic!("unexpected {:?}", other.map(|i| i.name)),
        };
        // Defaults are off.
        let cat = m.fact_catalog("cmdb", None).await.unwrap();
        assert!(cat
            .facts
            .iter()
            .all(|f| !f.usable_in_rules && !f.exportable));
        assert_eq!(cat.export_capability, ExportCapabilityState::Approved);
        assert_eq!(
            code(
                m.update_fact_setting("cmdb", "acme_cmdb.owner", &req(Some(true), None, None), "a")
                    .await
            ),
            "not_offered"
        );
        assert_eq!(
            code(
                m.update_fact_setting("cmdb", "acme_cmdb.tier", &req(None, Some(true), None), "a")
                    .await
            ),
            "not_offered"
        );
        assert_eq!(
            code(
                m.update_fact_setting(
                    "cmdb",
                    "acme_cmdb.owner",
                    &req(None, Some(true), Some("owner")),
                    "a"
                )
                .await
            ),
            "confirmation_required"
        );
        let info = m
            .update_fact_setting(
                "cmdb",
                "acme_cmdb.owner",
                &req(None, Some(true), Some("acme_cmdb.owner")),
                "alice",
            )
            .await
            .unwrap();
        assert!(info.exportable && info.approved_by.as_deref() == Some("alice"));
        let info = m
            .update_fact_setting(
                "cmdb",
                "acme_cmdb.tier",
                &req(Some(true), None, None),
                "alice",
            )
            .await
            .unwrap();
        assert!(info.usable_in_rules);
        assert!(matches!(
            m.update_fact_setting("cmdb", "tier", &req(Some(true), None, None), "a")
                .await,
            Err(PluginError::NotFound(_))
        ));
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM audit_log WHERE action = ?",
                "plugin.fact.settings_changed"
            )
            .await,
            2
        );
    }

    #[tokio::test]
    async fn rules_only_see_enabled_fresh_facts_and_changes_are_audited() {
        let e = env("cmdb", "builtin", "push", Default::default()).await;
        let m = &e.manager;
        let groups = vec![group("production", 1)];
        m.put_plugin_facts("cmdb", "n1", &map(json!({"tier": 1})), FactMode::Push)
            .await
            .unwrap();
        // Not enabled for rules: no effect and nothing stored.
        assert!(classify(m, &groups, false).await.groups.is_empty());
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM plugin_fact_classifications WHERE certname = ?",
                "n1"
            )
            .await,
            0
        );
        m.update_fact_setting(
            "cmdb",
            "acme_cmdb.tier",
            &UpdateFactSettingsRequest {
                usable_in_rules: Some(true),
                ..Default::default()
            },
            "alice",
        )
        .await
        .unwrap();
        let r = classify(m, &groups, false).await;
        assert_eq!(r.environment.as_deref(), Some("production"));
        let audit: String = sqlx::query_scalar(
            "SELECT details FROM audit_log WHERE action = 'plugin.fact.classification_changed'",
        )
        .fetch_one(m.pool())
        .await
        .unwrap();
        assert!(
            audit.contains("acme_cmdb.tier") && audit.contains("plugin:cmdb"),
            "{audit}"
        );
        // The same result again is not audited twice.
        classify(m, &groups, false).await;
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM audit_log WHERE action = ?",
                "plugin.fact.classification_changed"
            )
            .await,
            1
        );

        // Older than max_stale_s: missing, unless the last classification is frozen.
        sqlx::query("UPDATE plugin_facts SET computed_at = '2000-01-01T00:00:00.000Z'")
            .execute(m.pool())
            .await
            .unwrap();
        assert_eq!(
            classify(m, &groups, true).await.environment.as_deref(),
            Some("production")
        );
        assert!(classify(m, &groups, false).await.groups.is_empty());
        // The stale value is still visible in the node view, flagged.
        assert!(m.node_plugin_facts("n1", false).await[0].stale);
    }

    #[tokio::test]
    async fn pull_resolves_on_demand_caches_and_degrades_on_failure() {
        let e = env("cmdb", "builtin", "pull", Default::default()).await;
        let m = &e.manager;
        let facts = m.node_plugin_facts("n1", true).await;
        assert_eq!(facts.len(), 2);
        // Cached for ttl_s: no second call.
        m.node_plugin_facts("n1", true).await;
        assert_eq!(*e.cmdb.calls.lock().unwrap(), 1);
        // A failing provider leaves the node view working (cached values only).
        *e.cmdb.reply.lock().unwrap() = Err("cmdb is down".into());
        assert!(m.node_plugin_facts("n2", true).await.is_empty());
        assert_eq!(*e.cmdb.calls.lock().unwrap(), 2);
        // ... and backs off instead of calling again at once.
        m.node_plugin_facts("n2", true).await;
        assert_eq!(*e.cmdb.calls.lock().unwrap(), 2);
        let status = m.fact_catalog("cmdb", None).await.unwrap().provider;
        assert_eq!(status.failures, 1);
        assert!(status.last_error.unwrap().contains("cmdb is down"));
        // A reply that does not fit the schema is rejected, not stored.
        *e.cmdb.reply.lock().unwrap() = Ok(json!({"facts": {"tier": "high"}}));
        assert!(m.node_plugin_facts("n3", true).await.is_empty());
    }

    #[tokio::test]
    async fn a_slow_pull_provider_times_out_without_failing_the_request() {
        let cfg = PluginFactsConfig {
            pull_timeout_ms: 150,
            ..Default::default()
        };
        let e = env("slowcmdb", "remote", "pull", cfg).await;
        let started = std::time::Instant::now();
        let facts = e.manager.node_plugin_facts("n1", true).await;
        assert!(facts.is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        let usable = e.manager.usable_facts_for_node("n1", true).await;
        assert!(!usable.relevant);
        let status = e
            .manager
            .fact_catalog("slowcmdb", None)
            .await
            .unwrap()
            .provider;
        assert!(status.last_error.unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn export_needs_the_opt_in_and_skips_conflicts_and_stale_values() {
        let e = env("cmdb", "builtin", "push", Default::default()).await;
        let m = &e.manager;
        m.put_plugin_facts(
            "cmdb",
            "n1",
            &map(json!({"owner": "ops", "tier": 1})),
            FactMode::Push,
        )
        .await
        .unwrap();
        let none = HashSet::new();
        assert!(m.exportable_facts_for_node("n1", &none).await.is_empty());
        m.update_fact_setting(
            "cmdb",
            "acme_cmdb.owner",
            &UpdateFactSettingsRequest {
                exportable: Some(true),
                confirm: Some("acme_cmdb.owner".into()),
                ..Default::default()
            },
            "alice",
        )
        .await
        .unwrap();
        let preview = m.fact_preview("cmdb", "acme_cmdb.owner", 50).await.unwrap();
        assert_eq!(preview.total, 1);
        assert_eq!(preview.rows[0].value, json!("ops"));
        assert_eq!(preview.sensitivity, "pii");
        let out = m.exportable_facts_for_node("n1", &none).await;
        assert_eq!(out, vec![("acme_cmdb.owner".to_string(), json!("ops"))]);
        // Exported again unchanged: audited once.
        m.exportable_facts_for_node("n1", &none).await;
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM audit_log WHERE action = ?",
                "plugin.fact.exported"
            )
            .await,
            1
        );
        // The agent's fact wins.
        let agent = agent_names_of(["acme_cmdb.owner"]);
        assert!(m.exportable_facts_for_node("n1", &agent).await.is_empty());
        // Stale values are not exported.
        sqlx::query("UPDATE plugin_facts SET computed_at = '2000-01-01T00:00:00.000Z'")
            .execute(m.pool())
            .await
            .unwrap();
        assert!(m.exportable_facts_for_node("n1", &none).await.is_empty());
        m.sweep_stale_facts().await;
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM event_outbox WHERE type = ?",
                "plugin.fact.stale"
            )
            .await,
            1
        );
        m.sweep_stale_facts().await;
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM event_outbox WHERE type = ?",
                "plugin.fact.stale"
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn namespaces_are_unique_and_uninstall_removes_settings_and_purges_values() {
        let e = env("cmdb", "builtin", "push", Default::default()).await;
        let m = &e.manager;
        let other = manifest("cmdb-two", "remote", "push");
        let pkg = build_signed_package(
            &[("plugin.yaml", other.as_bytes())],
            "cmdb-two",
            "1.0.0",
            2,
            SEED,
        );
        let pre = m.preflight(&pkg).await.unwrap();
        assert!(
            pre.issues.iter().any(|i| i.contains("acme_cmdb")),
            "{:?}",
            pre.issues
        );
        assert!(pre
            .declared_pii_fields
            .iter()
            .any(|f| f.contains("acme_cmdb.owner")));
        assert!(m.install(&pkg, "admin", None).await.is_err());

        m.put_plugin_facts("cmdb", "n1", &map(json!({"tier": 1})), FactMode::Push)
            .await
            .unwrap();
        m.update_fact_setting(
            "cmdb",
            "acme_cmdb.tier",
            &UpdateFactSettingsRequest {
                usable_in_rules: Some(true),
                ..Default::default()
            },
            "alice",
        )
        .await
        .unwrap();
        m.uninstall("cmdb", false).await.unwrap();
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM plugin_fact_settings WHERE plugin_id = ?",
                "cmdb"
            )
            .await,
            0
        );
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM plugin_facts WHERE plugin_id = ?",
                "cmdb"
            )
            .await,
            1
        );
        // Uninstalled: out of every merged view at once.
        assert!(m.node_plugin_facts("n1", false).await.is_empty());
        m.install(&package("cmdb", "builtin", "push"), "admin", None)
            .await
            .unwrap();
        m.uninstall("cmdb", true).await.unwrap();
        assert_eq!(
            count(
                m,
                "SELECT COUNT(*) FROM plugin_facts WHERE plugin_id = ?",
                "cmdb"
            )
            .await,
            0
        );
    }
}
