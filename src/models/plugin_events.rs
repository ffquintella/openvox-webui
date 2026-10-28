//! Plugin platform events, jobs and webhooks (M21).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::PluginManifest;

pub const EVENT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_EVENT_HANDLER: &str = "on_event";

// ---- manifest declarations ---------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsDecl {
    #[serde(default)]
    pub subscribe: Vec<SubscriptionDecl>,
    #[serde(default)]
    pub publish: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionDecl {
    /// Exact event type or a `prefix.*` pattern.
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    /// Expression over the payload, evaluated by the host before the plugin runs.
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub handler: Option<String>,
}

fn default_priority() -> i32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobDecl {
    pub id: String,
    /// Standard five-field cron expression, evaluated in UTC.
    pub cron: String,
    pub handler: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookDecl {
    pub id: String,
    pub auth: String,
    pub handler: String,
}

impl PluginManifest {
    pub fn events_decl(&self) -> Result<EventsDecl, String> {
        match &self.events {
            None => Ok(EventsDecl::default()),
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("events: {}", e)),
        }
    }

    pub fn job_decls(&self) -> Result<Vec<JobDecl>, String> {
        self.contributes
            .jobs
            .iter()
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| format!("jobs: {}", e)))
            .collect()
    }

    pub fn webhook_decls(&self) -> Result<Vec<WebhookDecl>, String> {
        self.contributes
            .webhooks
            .iter()
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| format!("webhooks: {}", e)))
            .collect()
    }
}

// ---- envelope ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventActor {
    /// `user` | `system` | `plugin`
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
}

impl EventActor {
    pub fn system() -> Self {
        Self {
            kind: "system".into(),
            id: None,
        }
    }
    pub fn user(id: impl Into<String>) -> Self {
        Self {
            kind: "user".into(),
            id: Some(id.into()),
        }
    }
    pub fn plugin(id: impl Into<String>) -> Self {
        Self {
            kind: "plugin".into(),
            id: Some(id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub schema_version: u32,
    pub occurred_at: DateTime<Utc>,
    pub actor: EventActor,
    pub payload: Value,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

// ---- admin views ------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EventDeliveryView {
    pub id: i64,
    pub event_id: String,
    pub event_type: String,
    pub target_kind: String,
    pub target_id: String,
    pub handler: Option<String>,
    pub status: String,
    pub attempts: u32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct EventStats {
    pub pending_events: u64,
    pub pending_deliveries: u64,
    pub in_flight: u64,
    pub dead_letters: u64,
    pub delivered: u64,
    /// Age in seconds of the oldest undelivered event or delivery (outbox lag).
    pub oldest_pending_age_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginJobView {
    pub plugin_id: String,
    pub job_id: String,
    pub cron: String,
    pub handler: String,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub run_count: u64,
    pub failure_count: u64,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginJobRunView {
    pub id: i64,
    pub plugin_id: String,
    pub job_id: String,
    pub trigger: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginWebhookView {
    pub plugin_id: String,
    pub hook_id: String,
    pub auth: String,
    pub handler: String,
    /// Path to configure on the sender, under the API prefix.
    pub path: String,
    pub secret_configured: bool,
    pub rotated_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookSecretResponse {
    pub plugin_id: String,
    pub hook_id: String,
    /// Shown once; only the host keeps it afterwards.
    pub secret: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutboundWebhookView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub event_types: Vec<String>,
    pub filter: Option<String>,
    pub enabled: bool,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutboundWebhookCreated {
    #[serde(flatten)]
    pub webhook: OutboundWebhookView,
    /// Shown once.
    pub secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOutboundWebhookRequest {
    pub name: String,
    pub url: String,
    pub event_types: Vec<String>,
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateOutboundWebhookRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub event_types: Option<Vec<String>>,
    /// `null` clears the filter.
    #[serde(default, deserialize_with = "double_option")]
    pub filter: Option<Option<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}
