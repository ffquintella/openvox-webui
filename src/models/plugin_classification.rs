//! Classification extensions for plugins (M27): the `contributes.classification`
//! manifest section, dynamic and generated groups, held membership updates and
//! the API payloads.
//!
//! See docs/architecture/PLUGIN_PLATFORM.md section 13 and 17.8.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Prefix of every plugin-provided name in expressions and group policies:
/// `plugin:<plugin-id>:<id>`.
pub const PLUGIN_REF_PREFIX: &str = "plugin:";

/// Default share of members one membership update may add or remove.
pub const DEFAULT_CHURN_LIMIT_PCT: u32 = 20;
/// Default `max_stale_s` of a membership provider.
pub const DEFAULT_MEMBERSHIP_MAX_STALE_S: u64 = 3600;
/// Default most members one dynamic group may hold.
pub const DEFAULT_MAX_MEMBERS: u32 = 10_000;
/// Default most groups one template may generate.
pub const DEFAULT_MAX_GROUPS: u32 = 100;

/// A reference to something a plugin contributes (a function, an operator or
/// a merge strategy), written `plugin:<plugin-id>:<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtRef {
    pub plugin_id: String,
    pub id: String,
}

impl ExtRef {
    pub fn parse(s: &str) -> Option<ExtRef> {
        let rest = s.strip_prefix(PLUGIN_REF_PREFIX)?;
        let (plugin_id, id) = rest.split_once(':')?;
        if plugin_id.is_empty() || id.is_empty() || id.contains(':') {
            return None;
        }
        Some(ExtRef {
            plugin_id: plugin_id.to_string(),
            id: id.to_string(),
        })
    }

    pub fn full(&self) -> String {
        format!("{}{}:{}", PLUGIN_REF_PREFIX, self.plugin_id, self.id)
    }
}

impl std::fmt::Display for ExtRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.full())
    }
}

// ============================================================================
// Manifest: contributes.classification
// ============================================================================

/// Type of an argument or of a result of a plugin function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    String,
    Number,
    Bool,
    List,
    /// Any JSON value.
    Any,
}

impl ValueType {
    pub fn accepts(&self, v: &Value) -> bool {
        match self {
            ValueType::String => v.is_string(),
            ValueType::Number => v.is_number(),
            ValueType::Bool => v.is_boolean(),
            ValueType::List => v.is_array(),
            ValueType::Any => true,
        }
    }
}

/// A pure function usable in expressions: `plugin:<plugin-id>:<id>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionDecl {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub args: Vec<ValueType>,
    pub returns: ValueType,
    pub handler: String,
}

/// A pure comparison operator usable in expressions as `{"op": "plugin:<plugin-id>:<id>"}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorDecl {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Types of the left and right operand.
    pub arg_types: [ValueType; 2],
    pub handler: String,
}

/// What happens to the members of a dynamic group when the provider stops
/// refreshing them for longer than `max_stale_s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnStale {
    /// Keep the last members until `max_stale_s`, then drop them with an alert.
    #[default]
    Keep,
    /// Drop the members as soon as they are past `max_stale_s`.
    Drop,
    /// Keep the last members whatever their age.
    Freeze,
}

impl OnStale {
    pub fn as_str(&self) -> &'static str {
        match self {
            OnStale::Keep => "keep",
            OnStale::Drop => "drop",
            OnStale::Freeze => "freeze",
        }
    }

    pub fn parse(s: &str) -> OnStale {
        match s {
            "drop" => OnStale::Drop,
            "freeze" => OnStale::Freeze,
            _ => OnStale::Keep,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshDecl {
    pub cron: String,
}

/// Feeds a dynamic group. Runs on a schedule and writes the result to the
/// host; the classifier only reads the stored members.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipProviderDecl {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub refresh: Option<RefreshDecl>,
    #[serde(default)]
    pub max_stale_s: Option<u64>,
    #[serde(default)]
    pub on_stale: Option<OnStale>,
    #[serde(default)]
    pub churn_limit_pct: Option<u32>,
    #[serde(default)]
    pub max_members: Option<u32>,
    #[serde(default)]
    pub params_schema: Option<Value>,
    pub handler: String,
}

impl MembershipProviderDecl {
    pub fn max_stale(&self) -> u64 {
        self.max_stale_s
            .unwrap_or(DEFAULT_MEMBERSHIP_MAX_STALE_S)
            .max(1)
    }

    pub fn on_stale(&self) -> OnStale {
        self.on_stale.unwrap_or_default()
    }

    pub fn churn_limit(&self) -> u32 {
        self.churn_limit_pct
            .unwrap_or(DEFAULT_CHURN_LIMIT_PCT)
            .min(100)
    }

    pub fn max_members(&self) -> u32 {
        self.max_members.unwrap_or(DEFAULT_MAX_MEMBERS).max(1)
    }
}

/// "One group for each value of fact X". The administrator instantiates it
/// under a parent group and approves it; the host then creates, updates and
/// removes the generated groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTemplateDecl {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// `fact('<path>')`
    pub group_by: String,
    #[serde(default)]
    pub max_groups: Option<u32>,
    #[serde(default)]
    pub requires_approval: Option<bool>,
}

impl GroupTemplateDecl {
    /// The fact path in `group_by`, if it is well formed.
    pub fn fact_path(&self) -> Option<String> {
        parse_group_by(&self.group_by)
    }

    pub fn max_groups(&self) -> u32 {
        self.max_groups.unwrap_or(DEFAULT_MAX_GROUPS).max(1)
    }
}

/// `fact('os.family')` or `fact("os.family")` -> `os.family`.
pub fn parse_group_by(s: &str) -> Option<String> {
    let inner = s.trim().strip_prefix("fact(")?.strip_suffix(')')?.trim();
    let path = inner
        .strip_prefix('\'')
        .and_then(|i| i.strip_suffix('\''))
        .or_else(|| inner.strip_prefix('"').and_then(|i| i.strip_suffix('"')))?;
    let ok = !path.is_empty()
        && path.len() <= 200
        && path.split('.').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        });
    ok.then(|| path.to_string())
}

/// A pure per-key merge strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeStrategyDecl {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    pub handler: String,
}

/// `contributes.classification`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationContribution {
    #[serde(default)]
    pub functions: Vec<FunctionDecl>,
    #[serde(default)]
    pub operators: Vec<OperatorDecl>,
    #[serde(default)]
    pub membership_providers: Vec<MembershipProviderDecl>,
    #[serde(default)]
    pub group_templates: Vec<GroupTemplateDecl>,
    #[serde(default)]
    pub merge_strategies: Vec<MergeStrategyDecl>,
    /// Accepted by the schema but refused by validation until they exist.
    #[serde(default)]
    pub group_behaviors: Vec<Value>,
    #[serde(default)]
    pub transformers: Vec<Value>,
}

impl ClassificationContribution {
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
            && self.operators.is_empty()
            && self.membership_providers.is_empty()
            && self.group_templates.is_empty()
            && self.merge_strategies.is_empty()
            && self.group_behaviors.is_empty()
            && self.transformers.is_empty()
    }
}

// ============================================================================
// Group kinds, members and held updates
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// Rules, expression and pinned nodes (the only kind before M27).
    #[default]
    Static,
    /// Members come from a plugin membership provider.
    Dynamic,
    /// Created, updated and removed by a group template.
    Generated,
}

impl GroupKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupKind::Static => "static",
            GroupKind::Dynamic => "dynamic",
            GroupKind::Generated => "generated",
        }
    }

    pub fn parse(s: &str) -> GroupKind {
        match s {
            "dynamic" => GroupKind::Dynamic,
            "generated" => GroupKind::Generated,
            _ => GroupKind::Static,
        }
    }
}

/// How a dynamic group is fed: set on the group, bound to one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicGroupConfig {
    pub plugin_id: String,
    pub provider_id: String,
    #[serde(default)]
    pub params: Value,
}

/// What the classifier knows about the members of one dynamic group.
#[derive(Debug, Clone, Default)]
pub struct DynamicMembers {
    pub certnames: std::collections::HashSet<String>,
    /// The provider data is older than its `max_stale_s`.
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipProviderStatus {
    pub group_id: Uuid,
    pub plugin_id: String,
    pub provider_id: String,
    pub last_refresh_at: Option<DateTime<Utc>>,
    pub last_status: String,
    pub last_error: Option<String>,
    pub members: u64,
    pub stale: bool,
    pub held: bool,
}

/// A membership update held by the churn guard until an administrator approves it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldMembership {
    pub group_id: Uuid,
    pub plugin_id: String,
    pub provider_id: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub current: u64,
    pub proposed: u64,
    pub churn_pct: f64,
    pub limit_pct: u32,
    pub created_at: DateTime<Utc>,
}

/// A group template instantiated by an administrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupTemplateInstance {
    pub id: Uuid,
    pub plugin_id: String,
    pub template_id: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub classes: Value,
    pub variables: Value,
    pub approved: bool,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
    pub generated_groups: u32,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_ref_round_trip() {
        let r = ExtRef::parse("plugin:acme-cmdb:semver_ok").unwrap();
        assert_eq!(r.plugin_id, "acme-cmdb");
        assert_eq!(r.id, "semver_ok");
        assert_eq!(r.full(), "plugin:acme-cmdb:semver_ok");
        assert!(ExtRef::parse("lower").is_none());
        assert!(ExtRef::parse("plugin:acme").is_none());
        assert!(ExtRef::parse("plugin:a:b:c").is_none());
    }

    #[test]
    fn group_by_forms() {
        assert_eq!(parse_group_by("fact('datacenter')").unwrap(), "datacenter");
        assert_eq!(parse_group_by("fact(\"os.family\")").unwrap(), "os.family");
        assert!(parse_group_by("fact('')").is_none());
        assert!(parse_group_by("fact('a b')").is_none());
        assert!(parse_group_by("datacenter").is_none());
    }
}
