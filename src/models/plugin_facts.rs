//! Plugin facts (M25, T89): the `contributes.facts` manifest section, stored
//! values, the administrator's opt-in per fact and the API payloads.
//!
//! See docs/architecture/PLUGIN_PLATFORM.md sections 12.2, 12.4 and 17.6.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{PluginState, Requires};

/// Largest serialized value of one plugin fact.
pub const MAX_PLUGIN_FACT_VALUE_BYTES: usize = 16 * 1024;
/// Most facts one plugin may hold for one node (and declare).
pub const MAX_PLUGIN_FACTS_PER_NODE: usize = 200;
/// Freshness used when the manifest does not say.
pub const DEFAULT_PLUGIN_FACT_TTL_S: u64 = 3600;
pub const DEFAULT_PLUGIN_FACT_MAX_STALE_S: u64 = 86_400;

// ============================================================================
// Manifest: contributes.facts
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactMode {
    /// The plugin writes values with the host API `facts.put`.
    #[default]
    Push,
    /// The host calls the plugin's `handler` on demand (`resolve`).
    Pull,
}

impl FactMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            FactMode::Push => "push",
            FactMode::Pull => "pull",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginFactType {
    String,
    Integer,
    Float,
    Bool,
    List,
    Object,
}

impl PluginFactType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginFactType::String => "string",
            PluginFactType::Integer => "integer",
            PluginFactType::Float => "float",
            PluginFactType::Bool => "bool",
            PluginFactType::List => "list",
            PluginFactType::Object => "object",
        }
    }

    /// Type name used by the PuppetDB fact-paths catalog.
    pub fn catalog_type(&self) -> &'static str {
        match self {
            PluginFactType::String => "string",
            PluginFactType::Integer => "integer",
            PluginFactType::Float => "float",
            PluginFactType::Bool => "boolean",
            PluginFactType::List => "array",
            PluginFactType::Object => "map",
        }
    }

    pub fn matches(&self, v: &Value) -> bool {
        match self {
            PluginFactType::String => v.is_string(),
            PluginFactType::Integer => v.is_i64() || v.is_u64(),
            PluginFactType::Float => v.is_number(),
            PluginFactType::Bool => v.is_boolean(),
            PluginFactType::List => v.is_array(),
            PluginFactType::Object => v.is_object(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsContribution {
    /// Unique prefix of every fact of the plugin (`<namespace>.<name>`).
    pub namespace: String,
    #[serde(default)]
    pub mode: FactMode,
    /// Default freshness of the facts (pull: cache lifetime).
    #[serde(default)]
    pub ttl_s: Option<u64>,
    /// Data older than this counts as missing for classification and export.
    #[serde(default)]
    pub max_stale_s: Option<u64>,
    #[serde(default)]
    pub items: Vec<FactDecl>,
    /// Pull mode only: the handler called with `{certname, names}`.
    #[serde(default)]
    pub handler: Option<String>,
}

impl FactsContribution {
    pub fn ttl_for(&self, item: &FactDecl) -> u64 {
        item.ttl_s
            .or(self.ttl_s)
            .unwrap_or(DEFAULT_PLUGIN_FACT_TTL_S)
            .max(1)
    }

    pub fn max_stale(&self) -> u64 {
        self.max_stale_s
            .unwrap_or(DEFAULT_PLUGIN_FACT_MAX_STALE_S)
            .max(1)
    }

    pub fn full_name(&self, item: &FactDecl) -> String {
        format!("{}.{}", self.namespace, item.name)
    }

    /// The declaration of a full (`ns.name`) or local (`name`) fact name.
    pub fn find(&self, name: &str) -> Option<&FactDecl> {
        let local = match name.split_once('.') {
            Some((ns, rest)) if ns == self.namespace => rest,
            Some(_) => return None,
            None => name,
        };
        self.items.iter().find(|i| i.name == local)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactDecl {
    /// Local name; the fact is published as `<namespace>.<name>`.
    pub name: String,
    #[serde(rename = "type")]
    pub fact_type: PluginFactType,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "enum")]
    pub enum_values: Option<Vec<Value>>,
    #[serde(default)]
    pub ttl_s: Option<u64>,
    /// `none` (default) or `pii`; `secret` is refused by validation.
    #[serde(default = "default_sensitivity")]
    pub sensitivity: String,
    /// The plugin offers the fact for classification rules (the
    /// administrator still has to enable it).
    #[serde(default)]
    pub usable_in_rules: bool,
    /// The plugin offers the fact for export to agents (needs the
    /// `facts:export:external` capability and the administrator's opt-in).
    #[serde(default)]
    pub exportable: bool,
    /// Permission needed to see the fact in the merged views.
    #[serde(default)]
    pub requires: Option<Requires>,
}

fn default_sensitivity() -> String {
    "none".to_string()
}

impl FactDecl {
    pub fn is_pii(&self) -> bool {
        self.sensitivity == "pii"
    }
}

// ============================================================================
// Stored values and settings
// ============================================================================

/// One stored value (`plugin_facts`).
#[derive(Debug, Clone, Serialize)]
pub struct PluginFactRecord {
    pub plugin_id: String,
    pub certname: String,
    pub name: String,
    pub value: Value,
    pub computed_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Administrator opt-in for one fact (`plugin_fact_settings`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct PluginFactSetting {
    pub usable_in_rules: bool,
    pub exportable: bool,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

/// Last write or resolution outcome of a provider.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FactProviderStatus {
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub failures: u64,
    pub rejected: u64,
}

/// A plugin fact handed to classification, with its age.
#[derive(Debug, Clone, Serialize)]
pub struct ClassificationPluginFact {
    pub plugin_id: String,
    pub name: String,
    pub value: Value,
    pub computed_at: DateTime<Utc>,
    /// Older than this counts as missing.
    pub max_stale_s: u64,
}

// ============================================================================
// API payloads
// ============================================================================

/// One declared fact with the administrator's settings and its usage.
#[derive(Debug, Clone, Serialize)]
pub struct PluginFactInfo {
    pub name: String,
    pub local_name: String,
    #[serde(rename = "type")]
    pub fact_type: PluginFactType,
    pub description: Option<String>,
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<Value>>,
    pub ttl_s: u64,
    pub max_stale_s: u64,
    pub sensitivity: String,
    pub offers_rules: bool,
    pub offers_export: bool,
    pub usable_in_rules: bool,
    pub exportable: bool,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub requires: Option<Requires>,
    /// Nodes with a stored value.
    pub nodes: u64,
    /// Of those, nodes whose value is older than `max_stale_s`.
    pub stale_nodes: u64,
    /// The name collides with a fact reported by agents; the agent's wins.
    pub conflict: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportCapabilityState {
    Approved,
    NotApproved,
    NotDeclared,
    DeniedByPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginFactCatalog {
    pub plugin_id: String,
    pub plugin_name: String,
    pub state: PluginState,
    pub active_version: Option<String>,
    pub namespace: String,
    pub mode: FactMode,
    pub handler: Option<String>,
    pub ttl_s: u64,
    pub max_stale_s: u64,
    pub export_capability: ExportCapabilityState,
    pub facts: Vec<PluginFactInfo>,
    pub provider: FactProviderStatus,
    /// False when conflicts could not be checked against PuppetDB.
    pub puppetdb_checked: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateFactSettingsRequest {
    #[serde(default)]
    pub usable_in_rules: Option<bool>,
    #[serde(default)]
    pub exportable: Option<bool>,
    /// Enabling `exportable` needs the full fact name typed again.
    #[serde(default)]
    pub confirm: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginFactPreviewRow {
    pub certname: String,
    pub value: Value,
    pub computed_at: DateTime<Utc>,
    pub stale: bool,
    /// Size of the value as exported (JSON bytes).
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginFactPreview {
    pub plugin_id: String,
    pub name: String,
    pub sensitivity: String,
    pub exportable: bool,
    pub total: u64,
    pub limit: u32,
    pub rows: Vec<PluginFactPreviewRow>,
}

/// A fact in a merged (PuppetDB ∪ plugin) view. PuppetDB facts keep their
/// original shape: the extra fields are only present on plugin facts.
#[derive(Debug, Clone, Serialize)]
pub struct MergedFact {
    #[serde(flatten)]
    pub fact: super::Fact,
    /// `plugin:<id>`; absent for PuppetDB facts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
}

impl From<super::Fact> for MergedFact {
    fn from(fact: super::Fact) -> Self {
        Self {
            fact,
            source: None,
            computed_at: None,
            stale: None,
        }
    }
}

pub fn plugin_source(plugin_id: &str) -> String {
    format!("plugin:{}", plugin_id)
}
