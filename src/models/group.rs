//! Node group data model

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::default_organization_uuid;

/// Represents a node classification group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeGroup {
    /// Unique identifier
    pub id: Uuid,

    /// Organization/tenant identifier
    #[serde(default = "default_organization_uuid")]
    pub organization_id: Uuid,

    /// Group name
    pub name: String,

    /// Group description
    pub description: Option<String>,

    /// Parent group ID (for hierarchy)
    pub parent_id: Option<Uuid>,

    /// Environment this group applies to
    pub environment: Option<String>,

    /// When true, this group assigns its environment to matching nodes
    /// instead of filtering by the node's current environment.
    /// This allows environment-defining groups (e.g., "Production Servers")
    /// to set a node's environment based on classification rules.
    #[serde(default)]
    pub is_environment_group: bool,

    /// When true, groups with no rules will match all nodes (that pass environment filtering).
    /// When false (default), groups with no rules will match no nodes unless inherited from parent.
    #[serde(default)]
    pub match_all_nodes: bool,

    /// Whether to match all rules (AND) or any rule (OR)
    pub rule_match_type: RuleMatchType,

    /// Classes to apply to nodes in this group (Puppet Enterprise format)
    /// Each class name maps to its parameters: {"ntp": {"servers": ["ntp1.example.com"]}, "apache": {}}
    pub classes: serde_json::Value,

    /// Variables that become external facts (key => value)
    pub variables: serde_json::Value,

    /// Classification rules
    pub rules: Vec<ClassificationRule>,

    /// Pinned (static) nodes
    pub pinned_nodes: Vec<String>,

    /// Priority among siblings (M26): higher wins on conflicting values,
    /// ties keep the stored order (name).
    #[serde(default = "default_group_priority")]
    pub priority: i32,

    /// Composite expression (M26), canonical JSON. When the group also has
    /// flat rules, both must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<serde_json::Value>,

    /// Per-group behavior policies (M26); the default reproduces the
    /// behavior from before policies existed.
    #[serde(default)]
    pub behavior: GroupBehavior,
}

/// Default group priority.
pub const DEFAULT_GROUP_PRIORITY: i32 = 100;
/// Accepted priority range.
pub const GROUP_PRIORITY_RANGE: std::ops::RangeInclusive<i32> = -10_000..=10_000;

fn default_group_priority() -> i32 {
    DEFAULT_GROUP_PRIORITY
}

/// How a group relates to its ancestors (M26).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Inheritance {
    /// The group is only evaluated when its parent matched and inherits all
    /// classes and variables from its ancestors.
    #[default]
    Inherit,
    /// The group is evaluated even if its parent did not match; it inherits
    /// only from the ancestors that did match.
    Independent,
    /// Cuts class and variable inheritance from every ancestor.
    Block,
    /// Inherits only the listed class names and variable keys.
    Select { keys: Vec<String> },
}

/// How a group's rules are combined (M26); overrides `rule_match_type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum MatchMode {
    All,
    Any,
    /// At least `k` rules must match.
    NOf {
        k: usize,
    },
}

/// How the group's classes and variables merge into what was inherited (M26).
///
/// Serialized as a string: `deep`, `replace`, `union`, `fail_on_conflict` or
/// `plugin:<plugin-id>:<strategy>` (M27).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MergeStrategy {
    /// Deep merge of class parameters, the group's value wins (today's behavior).
    #[default]
    Deep,
    /// A class the group declares replaces the inherited entry as a whole.
    Replace,
    /// Like `deep`, but arrays are united without duplicates.
    Union,
    /// A different value for an existing key is an error; the first value is kept.
    FailOnConflict,
    /// A pure merge strategy contributed by a plugin, applied per top-level key.
    /// When it is unavailable the key is merged with `deep` and the result is
    /// marked degraded.
    Plugin(super::ExtRef),
}

impl MergeStrategy {
    pub fn as_string(&self) -> String {
        match self {
            MergeStrategy::Deep => "deep".into(),
            MergeStrategy::Replace => "replace".into(),
            MergeStrategy::Union => "union".into(),
            MergeStrategy::FailOnConflict => "fail_on_conflict".into(),
            MergeStrategy::Plugin(r) => r.full(),
        }
    }
}

impl Serialize for MergeStrategy {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_string())
    }
}

impl<'de> Deserialize<'de> for MergeStrategy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        match v.as_str() {
            "deep" => Ok(MergeStrategy::Deep),
            "replace" => Ok(MergeStrategy::Replace),
            "union" => Ok(MergeStrategy::Union),
            "fail_on_conflict" => Ok(MergeStrategy::FailOnConflict),
            other => super::ExtRef::parse(other)
                .map(MergeStrategy::Plugin)
                .ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        "unknown merge strategy '{other}': use deep, replace, union, fail_on_conflict or plugin:<plugin-id>:<strategy>"
                    ))
                }),
        }
    }
}

/// A group created and kept up to date by a group template (M27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedGroupInfo {
    /// The template instance that owns the group.
    pub instance_id: Uuid,
    /// The value of the template's fact this group stands for.
    pub key: String,
}

/// Per-group behavior policies (M26). Every field defaults to the engine's
/// behavior before policies existed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GroupBehavior {
    pub inheritance: Inheritance,
    /// Mutually exclusive siblings: among matching siblings of the same parent
    /// and set, only the one with the highest priority stays.
    pub exclusive_set: Option<String>,
    pub match_mode: Option<MatchMode>,
    pub merge: MergeStrategy,
    /// The group does not match before this instant.
    pub active_from: Option<chrono::DateTime<chrono::Utc>>,
    /// The group does not match from this instant on.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `static` (rules, expression, pinned), `dynamic` (members from a plugin
    /// membership provider) or `generated` (kept by a group template) (M27).
    #[serde(skip_serializing_if = "is_static_kind")]
    pub kind: super::GroupKind,
    /// Dynamic groups: the provider that feeds the group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub membership: Option<super::DynamicGroupConfig>,
    /// The plugin that owns the group; set by the host, never by the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_plugin_id: Option<String>,
    /// Generated groups: the template instance and fact value; set by the host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated: Option<GeneratedGroupInfo>,
}

fn is_static_kind(k: &super::GroupKind) -> bool {
    *k == super::GroupKind::Static
}

impl GroupBehavior {
    /// True when nothing deviates from the defaults.
    pub fn is_default(&self) -> bool {
        *self == GroupBehavior::default()
    }

    /// Reject values that cannot work.
    pub fn validate(&self) -> Result<(), String> {
        if let Inheritance::Select { keys } = &self.inheritance {
            if keys.is_empty() || keys.iter().any(|k| k.trim().is_empty()) {
                return Err("inheritance 'select' needs a non-empty list of keys".into());
            }
        }
        if let Some(MatchMode::NOf { k }) = &self.match_mode {
            if *k == 0 {
                return Err("match_mode 'n_of' needs k >= 1".into());
            }
        }
        if let Some(set) = &self.exclusive_set {
            if set.trim().is_empty() || set.len() > 64 {
                return Err("exclusive_set must be 1 to 64 characters".into());
            }
        }
        if let (Some(from), Some(to)) = (self.active_from, self.expires_at) {
            if to <= from {
                return Err("expires_at must be after active_from".into());
            }
        }
        match (self.kind, &self.membership) {
            (super::GroupKind::Dynamic, None) => {
                return Err("a dynamic group needs a membership provider".into())
            }
            (super::GroupKind::Dynamic, Some(m)) => {
                if m.plugin_id.is_empty() || m.provider_id.is_empty() {
                    return Err("membership needs plugin_id and provider_id".into());
                }
            }
            (_, Some(_)) => {
                return Err("only a dynamic group can have a membership provider".into())
            }
            _ => {}
        }
        if (self.kind == super::GroupKind::Generated) != self.generated.is_some() {
            return Err("generated groups are created by group templates only".into());
        }
        Ok(())
    }
}

impl Default for NodeGroup {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id: default_organization_uuid(),
            name: String::new(),
            description: None,
            parent_id: None,
            environment: None,
            is_environment_group: false,
            match_all_nodes: false,
            rule_match_type: RuleMatchType::All,
            classes: serde_json::json!({}),
            variables: serde_json::json!({}),
            rules: vec![],
            pinned_nodes: vec![],
            priority: DEFAULT_GROUP_PRIORITY,
            expression: None,
            behavior: GroupBehavior::default(),
        }
    }
}

/// How rules should be matched
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RuleMatchType {
    /// All rules must match (AND)
    #[default]
    All,
    /// Any rule must match (OR)
    Any,
}

/// Request to create a new node group
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub environment: Option<String>,
    /// When true, this group assigns its environment to matching nodes
    /// instead of filtering by the node's current environment
    pub is_environment_group: Option<bool>,
    /// When true, groups with no rules will match all nodes from parent (or all nodes if root)
    pub match_all_nodes: Option<bool>,
    pub rule_match_type: Option<RuleMatchType>,
    /// Classes in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
    pub classes: Option<serde_json::Value>,
    pub variables: Option<serde_json::Value>,
    /// Priority among siblings (M26)
    pub priority: Option<i32>,
    /// Composite expression (M26); on update an empty object clears it
    pub expression: Option<serde_json::Value>,
    /// Behavior policies (M26)
    pub behavior: Option<GroupBehavior>,
}

/// Request to update an existing node group
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub environment: Option<String>,
    /// When true, this group assigns its environment to matching nodes
    /// instead of filtering by the node's current environment
    pub is_environment_group: Option<bool>,
    /// When true, groups with no rules will match all nodes from parent (or all nodes if root)
    pub match_all_nodes: Option<bool>,
    pub rule_match_type: Option<RuleMatchType>,
    /// Classes in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
    pub classes: Option<serde_json::Value>,
    pub variables: Option<serde_json::Value>,
    /// Priority among siblings (M26)
    pub priority: Option<i32>,
    /// Composite expression (M26); on update an empty object clears it
    pub expression: Option<serde_json::Value>,
    /// Behavior policies (M26)
    pub behavior: Option<GroupBehavior>,
}

/// Request to create a classification rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRuleRequest {
    pub fact_path: String,
    pub operator: RuleOperator,
    pub value: serde_json::Value,
}

/// Request to add a pinned node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddPinnedNodeRequest {
    pub certname: String,
}

/// Classification rule for matching nodes to groups
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClassificationRule {
    /// Unique identifier
    pub id: Uuid,

    /// Fact path (e.g., "os.family", "networking.ip")
    pub fact_path: String,

    /// Comparison operator
    pub operator: RuleOperator,

    /// Value to compare against
    pub value: serde_json::Value,
}

/// Rule comparison operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RuleOperator {
    /// Equals
    #[default]
    #[serde(rename = "=")]
    Equals,

    /// Not equals
    #[serde(rename = "!=")]
    NotEquals,

    /// Regex match
    #[serde(rename = "~")]
    Regex,

    /// Not regex match
    #[serde(rename = "!~")]
    NotRegex,

    /// Greater than
    #[serde(rename = ">")]
    GreaterThan,

    /// Greater than or equal
    #[serde(rename = ">=")]
    GreaterThanOrEqual,

    /// Less than
    #[serde(rename = "<")]
    LessThan,

    /// Less than or equal
    #[serde(rename = "<=")]
    LessThanOrEqual,

    /// Value is in list
    #[serde(rename = "in")]
    In,

    /// Value is not in list
    #[serde(rename = "not_in")]
    NotIn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_group_default() {
        let group = NodeGroup::default();
        assert!(group.name.is_empty());
        assert_eq!(group.rule_match_type, RuleMatchType::All);
    }

    #[test]
    fn test_rule_operator_serialization() {
        let rule = ClassificationRule {
            id: Uuid::new_v4(),
            fact_path: "os.family".to_string(),
            operator: RuleOperator::Equals,
            value: serde_json::json!("RedHat"),
        };

        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"=\""));
    }
}
