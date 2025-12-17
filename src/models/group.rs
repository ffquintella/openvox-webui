//! Node group data model

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a node classification group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeGroup {
    /// Unique identifier
    pub id: Uuid,

    /// Group name
    pub name: String,

    /// Group description
    pub description: Option<String>,

    /// Parent group ID (for hierarchy)
    pub parent_id: Option<Uuid>,

    /// Environment this group applies to
    pub environment: Option<String>,

    /// Whether to match all rules (AND) or any rule (OR)
    pub rule_match_type: RuleMatchType,

    /// Classes to apply to nodes in this group
    pub classes: Vec<String>,

    /// Parameters for the classes
    pub parameters: serde_json::Value,

    /// Classification rules
    pub rules: Vec<ClassificationRule>,

    /// Pinned (static) nodes
    pub pinned_nodes: Vec<String>,
}

impl Default for NodeGroup {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            description: None,
            parent_id: None,
            environment: None,
            rule_match_type: RuleMatchType::All,
            classes: vec![],
            parameters: serde_json::json!({}),
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
    pub rule_match_type: Option<RuleMatchType>,
    pub classes: Option<Vec<String>>,
    pub parameters: Option<serde_json::Value>,
}

/// Request to update an existing node group
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub environment: Option<String>,
    pub rule_match_type: Option<RuleMatchType>,
    pub classes: Option<Vec<String>>,
    pub parameters: Option<serde_json::Value>,
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
