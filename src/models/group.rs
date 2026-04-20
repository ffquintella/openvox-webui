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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
