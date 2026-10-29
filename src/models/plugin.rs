//! Plugin platform models: manifest (`plugin.yaml`), package signature
//! envelope, trusted keys and the admin API payloads.
//!
//! See docs/architecture/PLUGIN_PLATFORM.md sections 4, 7 and 8.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Major version of the plugin API this host implements.
pub const PLUGIN_API_VERSION: u32 = 2;
/// Oldest plugin API major the host still accepts (N and N-1; there is no v1).
pub const PLUGIN_API_MIN_VERSION: u32 = 2;

// ============================================================================
// Manifest (plugin.yaml)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    #[serde(default)]
    pub min_webui_version: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    pub runtime: RuntimeSpec,
    #[serde(default)]
    pub capabilities: Vec<CapabilityDecl>,
    /// Event subscriptions and publications (acted on from M21).
    #[serde(default)]
    pub events: Option<Value>,
    #[serde(default)]
    pub contributes: Contributes,
    /// JSON Schema for the plugin settings (acted on from M20).
    #[serde(default)]
    pub config_schema: Option<Value>,
    #[serde(default)]
    pub migrations: Option<MigrationsSpec>,
    #[serde(default)]
    pub lifecycle: Option<LifecycleSpec>,
    #[serde(default)]
    pub telemetry: Option<TelemetrySpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Wasm,
    Remote,
    Builtin,
}

impl RuntimeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeKind::Wasm => "wasm",
            RuntimeKind::Remote => "remote",
            RuntimeKind::Builtin => "builtin",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSpec {
    pub kind: RuntimeKind,
    /// `wasm`: path of the module inside the package. `builtin`: key of the
    /// compiled-in plugin. Unused for `remote`.
    #[serde(default)]
    pub entry: Option<String>,
    /// `remote`: HTTPS base URL of the plugin service.
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub limits: Option<RuntimeLimits>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLimits {
    #[serde(default)]
    pub memory_mb: Option<u64>,
    #[serde(default)]
    pub fuel_per_call: Option<u64>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Handler calls per minute the plugin asks for; the host ceiling still applies
    #[serde(default)]
    pub invocations_per_minute: Option<u32>,
}

/// A capability as written in the manifest: a bare name or a one-entry map
/// (`http:egress: { hosts: [...] }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CapabilityDecl {
    Name(String),
    Scoped(BTreeMap<String, Value>),
}

/// A capability after normalisation, with its approval-screen description.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Capability {
    pub name: String,
    pub scope: Option<Value>,
    /// The plugin works in a degraded mode without it (`optional: true`).
    pub optional: bool,
}

impl Capability {
    /// Same capability and scope, ignoring whether it is optional.
    pub fn same_grant(&self, other: &Capability) -> bool {
        self.name == other.name && self.scope == other.scope
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contributes {
    #[serde(default)]
    pub slots: Vec<SlotContribution>,
    #[serde(default)]
    pub menus: Vec<MenuEntry>,
    /// Plugin API routes (acted on from M20).
    #[serde(default)]
    pub api_routes: Vec<Value>,
    /// Scheduled jobs (acted on from M21).
    #[serde(default)]
    pub jobs: Vec<Value>,
    /// Webhooks (acted on from M21).
    #[serde(default)]
    pub webhooks: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderKind {
    Declarative,
    Iframe,
    Module,
}

impl RenderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RenderKind::Declarative => "declarative",
            RenderKind::Iframe => "iframe",
            RenderKind::Module => "module",
        }
    }
}

/// Permission needed to see a contribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    pub resource: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotContribution {
    pub slot: String,
    pub id: String,
    pub render: RenderKind,
    /// `iframe`: HTML entry inside the package.
    #[serde(default)]
    pub src: Option<String>,
    /// `declarative`: JSON page description inside the package.
    #[serde(default)]
    pub spec: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// `page` slot only: route under `/p/<plugin-id>/`.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub requires: Option<Requires>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MenuEntry {
    Item(MenuItem),
    Op(MenuOp),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuItem {
    pub id: String,
    /// `main` | `admin` | `user-menu`
    pub section: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Always under `/p/<plugin-id>/`.
    pub href: String,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub requires: Option<Requires>,
}

/// Change to a built-in menu item. Cosmetic only: RBAC still guards the route.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuOp {
    /// Stable id such as `core:updates`.
    pub target: String,
    /// `hide` | `rename`
    pub op: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationsSpec {
    pub dir: String,
    #[serde(default)]
    pub target: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleSpec {
    #[serde(default)]
    pub on_install: Option<String>,
    #[serde(default)]
    pub on_upgrade: Option<String>,
    #[serde(default)]
    pub on_uninstall: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetrySpec {
    #[serde(default)]
    pub declared_pii_fields: Vec<String>,
}

// ============================================================================
// Package signature (plugin.sig)
// ============================================================================

/// Signed header: binds the signature to one plugin id, version and key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignatureHeader {
    pub id: String,
    pub version: String,
    pub api_version: u32,
    /// Hex SHA-256 of the signer's raw ed25519 public key.
    pub key_id: String,
    pub signed_at: String,
}

/// Contents of `plugin.sig`. The public key travels with the package so an
/// unknown signer can be shown to the administrator; trust is decided only by
/// the trusted key store, never by the package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureEnvelope {
    pub header: SignatureHeader,
    /// Base64 raw 32-byte ed25519 public key.
    pub public_key: String,
    /// Base64 ed25519 signature.
    pub signature: String,
}

// ============================================================================
// Persisted state
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    Installed,
    Configured,
    Enabled,
    Disabled,
    Error,
    UpdatePending,
}

impl PluginState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginState::Installed => "installed",
            PluginState::Configured => "configured",
            PluginState::Enabled => "enabled",
            PluginState::Disabled => "disabled",
            PluginState::Error => "error",
            PluginState::UpdatePending => "update_pending",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "configured" => PluginState::Configured,
            "enabled" => PluginState::Enabled,
            "disabled" => PluginState::Disabled,
            "error" => PluginState::Error,
            "update_pending" => PluginState::UpdatePending,
            _ => PluginState::Installed,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginRecord {
    pub id: String,
    pub active_version: Option<String>,
    /// Newer version waiting for the capabilities it adds to be approved.
    pub pending_version: Option<String>,
    pub schema_version: u32,
    pub state: PluginState,
    pub state_reason: Option<String>,
    pub installed_by: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginVersionRecord {
    pub plugin_id: String,
    pub version: String,
    pub api_version: u32,
    pub package_sha256: String,
    pub signed_by_key_id: String,
    pub signed_at: String,
    pub revoked: bool,
    pub added_at: DateTime<Utc>,
    #[serde(skip)]
    pub manifest_json: String,
    #[serde(skip)]
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyOrigin {
    Builtin,
    Admin,
}

impl KeyOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyOrigin::Builtin => "builtin",
            KeyOrigin::Admin => "admin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrustedKeyRecord {
    pub key_id: String,
    pub public_key: Vec<u8>,
    pub label: String,
    pub origin: KeyOrigin,
    pub allowed_plugin_ids: Option<Vec<String>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub added_by: Option<String>,
    pub added_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
}

// ============================================================================
// API payloads
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    Valid,
    /// Signature is mathematically valid but the signer is not trusted yet.
    UntrustedKey,
    Invalid,
    KeyRevoked,
    KeyExpired,
    KeyScope,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignatureReport {
    pub status: SignatureStatus,
    pub key_id: String,
    pub key_label: Option<String>,
    pub key_origin: Option<KeyOrigin>,
    pub signed_at: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityInfo {
    pub name: String,
    pub scope: Option<Value>,
    pub optional: bool,
    pub risk: RiskLevel,
    pub description: String,
}

/// A declared capability with the administrator's decision.
#[derive(Debug, Clone, Serialize)]
pub struct CapabilityApproval {
    #[serde(flatten)]
    pub info: CapabilityInfo,
    pub granted: bool,
    /// Denied by `plugins.policy` and so impossible to approve.
    pub blocked_by_policy: bool,
}

/// One approved capability (`plugin_grants`).
#[derive(Debug, Clone, Serialize)]
pub struct PluginGrant {
    pub plugin_id: String,
    pub capability: String,
    pub scope: Option<Value>,
    pub granted_by: Option<String>,
    pub granted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApproveCapabilitiesRequest {
    /// Version the approval is for; defaults to the pending version if there
    /// is one, otherwise the active one.
    #[serde(default)]
    pub version: Option<String>,
    /// Capabilities to approve; every other declared one is not granted.
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub api_version: u32,
    pub license: Option<String>,
    pub runtime_kind: RuntimeKind,
    pub min_webui_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightResult {
    pub manifest: ManifestSummary,
    pub package_sha256: String,
    pub signature: SignatureReport,
    pub capabilities: Vec<CapabilityInfo>,
    pub declared_pii_fields: Vec<String>,
    pub installed_version: Option<String>,
    /// Blocking problems other than the signature (unsupported runtime, id
    /// already installed at a newer version, ...).
    pub issues: Vec<String>,
    /// True when the package can be installed now, or after trusting its key.
    pub can_install: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub active_version: Option<String>,
    pub state: PluginState,
    pub state_reason: Option<String>,
    pub pending_version: Option<String>,
    pub runtime_kind: Option<RuntimeKind>,
    pub signer_key_id: Option<String>,
    pub signer_label: Option<String>,
    pub installed_by: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginDetail {
    #[serde(flatten)]
    pub summary: PluginSummary,
    pub manifest: Option<PluginManifest>,
    pub capabilities: Vec<CapabilityInfo>,
    /// Approval state of the capabilities of the pending version if there
    /// is one, otherwise of the active version.
    pub approvals: Vec<CapabilityApproval>,
    pub versions: Vec<PluginVersionRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustedKeyView {
    pub key_id: String,
    pub fingerprint: String,
    pub label: String,
    pub origin: KeyOrigin,
    pub allowed_plugin_ids: Option<Vec<String>>,
    pub expires_at: Option<DateTime<Utc>>,
    /// `active` | `revoked` | `expired`
    pub state: String,
    pub expires_soon: bool,
    pub added_by: Option<String>,
    pub added_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
    pub dependent_plugins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddTrustedKeyRequest {
    /// PEM (SPKI), `ssh-ed25519 ...` line or base64 of the raw 32 bytes.
    pub public_key: String,
    pub label: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub allowed_plugin_ids: Option<Vec<String>>,
    /// Current password of the acting user (step-up re-authentication).
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTrustedKeyRequest {
    #[serde(default)]
    pub label: Option<String>,
    /// `Some(None)` clears the expiry.
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub expires_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub allowed_plugin_ids: Option<Option<Vec<String>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevokeTrustedKeyRequest {
    pub reason: String,
    /// Current password of the acting user (step-up re-authentication).
    pub password: String,
}

fn deserialize_double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(d)?))
}

// ============================================================================
// UI slot registry output
// ============================================================================

/// One contribution in the permission-filtered registry served to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct UiContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub slot: String,
    pub id: String,
    pub render: RenderKind,
    pub title: Option<String>,
    pub path: Option<String>,
    pub order: i32,
    /// Menu contributions only.
    pub label: Option<String>,
    pub icon: Option<String>,
    pub href: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiMenuOp {
    pub plugin_id: String,
    pub target: String,
    pub op: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UiSlots {
    /// Contributions grouped by slot name, ordered by `order` then plugin id.
    pub slots: BTreeMap<String, Vec<UiContribution>>,
    pub menu_ops: Vec<UiMenuOp>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiContributionDetail {
    #[serde(flatten)]
    pub contribution: UiContribution,
    /// Declarative page description, passed through as JSON.
    pub spec: Option<Value>,
}
