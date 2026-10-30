//! Handler invocation and the index of event, job and webhook hooks (M21).
//!
//! The dispatcher, the job scheduler and the inbound webhook route never run
//! plugin code themselves: they build a [`HandlerCall`] and give it to a
//! [`HandlerInvoker`]. Built-in plugins are served by the manager; the WASM
//! runtime (M20) plugs in through `PluginManager::with_invoker`.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use cron::Schedule;
use serde_json::Value;

use super::filter::{pattern_matches, Filter};
use super::manifest::parse_cron;
use super::package::PluginPackage;
use crate::models::{
    AnnotationDecl, ClassificationContribution, DatasetDecl, ExportFormatDecl, FactsContribution,
    JobDecl, ReportTypeDecl, RuntimeSpec, WebhookDecl, DEFAULT_EVENT_HANDLER,
    DEFAULT_INTERCEPT_HANDLER,
};

pub const DEFAULT_HANDLER_TIMEOUT_MS: u64 = 5_000;
pub const MAX_HANDLER_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    Event,
    Job,
    Webhook,
    /// install, upgrade, enable, disable and uninstall hooks (M20)
    Lifecycle,
    /// pull-mode fact resolution (M25)
    Fact,
    /// report generation and export rendering (M25)
    Report,
    /// pure functions, operators and merge strategies on the classification
    /// hot path, and membership provider refreshes (M27)
    Classification,
    /// synchronous veto/alter hooks on interceptable events (M31)
    Interceptor,
}

impl HandlerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HandlerKind::Event => "event",
            HandlerKind::Job => "job",
            HandlerKind::Webhook => "webhook",
            HandlerKind::Lifecycle => "lifecycle",
            HandlerKind::Fact => "fact",
            HandlerKind::Report => "report",
            HandlerKind::Classification => "classification",
            HandlerKind::Interceptor => "interceptor",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HandlerCall {
    pub plugin_id: String,
    pub version: String,
    pub runtime: RuntimeSpec,
    pub handler: String,
    pub kind: HandlerKind,
    pub input: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum InvokeError {
    #[error("runtime unavailable: {0}")]
    Unavailable(String),
    #[error("handler failed: {0}")]
    Failed(String),
    #[error("handler timed out after {0} ms")]
    Timeout(u64),
}

#[async_trait]
pub trait HandlerInvoker: Send + Sync {
    async fn invoke(&self, call: HandlerCall) -> Result<Value, InvokeError>;
}

/// Runs the call under the plugin's timeout (capped by the host ceiling).
pub async fn invoke_with_timeout(
    invoker: &dyn HandlerInvoker,
    call: HandlerCall,
    timeout_ms: u64,
) -> Result<Value, InvokeError> {
    let ms = timeout_ms.clamp(1, MAX_HANDLER_TIMEOUT_MS);
    match tokio::time::timeout(Duration::from_millis(ms), invoker.invoke(call)).await {
        Ok(r) => r,
        Err(_) => Err(InvokeError::Timeout(ms)),
    }
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub pattern: String,
    pub priority: i32,
    pub filter: Option<Filter>,
    pub handler: String,
}

/// One interceptor of an enabled plugin (M31).
#[derive(Debug, Clone)]
pub struct Interceptor {
    pub event_type: String,
    pub priority: i32,
    pub filter: Option<Filter>,
    /// The filter as written, for the administrator's listing.
    pub filter_src: Option<String>,
    pub handler: String,
}

#[derive(Debug, Clone)]
pub struct PluginJob {
    pub decl: JobDecl,
    pub schedule: Schedule,
}

/// What one enabled plugin declared, parsed once when the plugin is loaded.
#[derive(Debug, Clone)]
pub struct PluginHooks {
    pub plugin_id: String,
    pub version: String,
    pub runtime: RuntimeSpec,
    pub subscriptions: Vec<Subscription>,
    pub publish: Vec<String>,
    /// Interceptors on interceptable events, at most one per event (M31).
    pub interceptors: Vec<Interceptor>,
    pub jobs: Vec<PluginJob>,
    pub webhooks: Vec<WebhookDecl>,
    /// Facts the plugin provides (M25).
    pub facts: Option<FactsContribution>,
    /// Report types, export formats, annotation keys and datasets (M25).
    pub report_types: Vec<ReportTypeDecl>,
    pub export_formats: Vec<ExportFormatDecl>,
    pub report_annotations: Vec<AnnotationDecl>,
    pub datasets: Vec<DatasetDecl>,
    /// Classification functions, operators, providers, templates and merge
    /// strategies (M27).
    pub classification: Option<ClassificationContribution>,
}

impl PluginHooks {
    /// Parses the declarations of a verified package. Manifest validation
    /// already rejected malformed ones, so failures here skip the entry.
    pub fn from_package(pkg: &PluginPackage) -> Self {
        let m = &pkg.manifest;
        let events = m.events_decl().unwrap_or_default();
        let subscriptions = events
            .subscribe
            .iter()
            .map(|s| Subscription {
                pattern: s.event_type.clone(),
                priority: s.priority,
                filter: s.filter.as_deref().and_then(|f| Filter::parse(f).ok()),
                handler: s
                    .handler
                    .clone()
                    .unwrap_or_else(|| DEFAULT_EVENT_HANDLER.to_string()),
            })
            .collect();
        let interceptors = events
            .intercept
            .iter()
            .filter(|i| super::interceptors::find(&i.event_type).is_some())
            .map(|i| Interceptor {
                event_type: i.event_type.clone(),
                priority: i.priority,
                filter: i.filter.as_deref().and_then(|f| Filter::parse(f).ok()),
                filter_src: i.filter.clone(),
                handler: i
                    .handler
                    .clone()
                    .unwrap_or_else(|| DEFAULT_INTERCEPT_HANDLER.to_string()),
            })
            .collect();
        let jobs = m
            .job_decls()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|decl| {
                parse_cron(&decl.cron)
                    .ok()
                    .map(|schedule| PluginJob { decl, schedule })
            })
            .collect();
        Self {
            plugin_id: m.id.clone(),
            version: m.version.clone(),
            runtime: m.runtime.clone(),
            subscriptions,
            publish: events.publish,
            interceptors,
            jobs,
            webhooks: m.webhook_decls().unwrap_or_default(),
            facts: m.contributes.facts.clone(),
            report_types: m.contributes.report_types.clone(),
            export_formats: m.contributes.export_formats.clone(),
            report_annotations: m.contributes.report_annotations.clone(),
            datasets: m.contributes.datasets.clone(),
            classification: m.contributes.classification.clone(),
        }
    }

    pub fn timeout_ms(&self) -> u64 {
        self.runtime
            .limits
            .as_ref()
            .and_then(|l| l.timeout_ms)
            .unwrap_or(DEFAULT_HANDLER_TIMEOUT_MS)
    }

    pub fn call(&self, kind: HandlerKind, handler: &str, input: Value) -> HandlerCall {
        HandlerCall {
            plugin_id: self.plugin_id.clone(),
            version: self.version.clone(),
            runtime: self.runtime.clone(),
            handler: handler.to_string(),
            kind,
            input,
        }
    }

    /// The matching subscription with the lowest priority number, if any.
    pub fn subscription_for(&self, event_type: &str, payload: &Value) -> Option<&Subscription> {
        self.subscriptions
            .iter()
            .filter(|s| pattern_matches(&s.pattern, event_type))
            .filter(|s| s.filter.as_ref().is_none_or(|f| f.matches(payload)))
            .min_by_key(|s| s.priority)
    }
}

/// Hooks of every enabled plugin, rebuilt on each reload.
#[derive(Debug, Clone, Default)]
pub struct HookIndex {
    plugins: BTreeMap<String, PluginHooks>,
}

impl HookIndex {
    pub fn add(&mut self, h: PluginHooks) {
        self.plugins.insert(h.plugin_id.clone(), h);
    }

    pub fn get(&self, plugin_id: &str) -> Option<&PluginHooks> {
        self.plugins.get(plugin_id)
    }

    pub fn plugins(&self) -> impl Iterator<Item = &PluginHooks> {
        self.plugins.values()
    }

    /// Every interceptor of `event_type` in run order: ascending priority,
    /// then plugin id. The caller applies the per-event maximum.
    pub fn interceptors_for(&self, event_type: &str) -> Vec<(&PluginHooks, &Interceptor)> {
        let mut out: Vec<(&PluginHooks, &Interceptor)> = self
            .plugins
            .values()
            .flat_map(|p| {
                p.interceptors
                    .iter()
                    .filter(|i| i.event_type == event_type)
                    .map(move |i| (p, i))
            })
            .collect();
        out.sort_by(|a, b| (a.1.priority, &a.0.plugin_id).cmp(&(b.1.priority, &b.0.plugin_id)));
        out
    }
}
