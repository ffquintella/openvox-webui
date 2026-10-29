//! Classification data model

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result of classifying a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// Certificate name of the classified node
    pub certname: String,

    /// Organization ID that owns this node's classification
    /// This is determined by which organization's groups matched the node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<Uuid>,

    /// Groups the node belongs to
    pub groups: Vec<GroupMatch>,

    /// Combined classes from all groups (Puppet Enterprise format)
    /// Each class name maps to its parameters: {"ntp": {"servers": ["ntp1.example.com"]}, "apache": {}}
    pub classes: serde_json::Value,

    /// Combined variables from all groups (exported as external facts)
    pub variables: serde_json::Value,

    /// Environment (from highest priority group)
    pub environment: Option<String>,

    /// Error if node matches groups from multiple organizations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_error: Option<String>,
}

/// A group that a node matches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMatch {
    /// Group ID
    pub id: Uuid,

    /// Group name
    pub name: String,

    /// How the node matched (rules or pinned)
    pub match_type: MatchType,

    /// Rules that matched (if applicable)
    pub matched_rules: Vec<Uuid>,
}

/// How a node matched a group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchType {
    /// Matched via classification rules
    Rules,
    /// Pinned (statically assigned)
    Pinned,
    /// Inherited from parent group
    Inherited,
}

/// Request to classify a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyRequest {
    /// Certificate name of the node
    pub certname: String,

    /// Facts to use for classification (optional, will fetch from PuppetDB if not provided)
    pub facts: Option<serde_json::Value>,
}

/// Classification rule evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluation {
    /// Rule ID
    pub rule_id: Uuid,

    /// Whether the rule matched
    pub matched: bool,

    /// The fact value that was evaluated
    pub fact_value: Option<serde_json::Value>,

    /// Error if evaluation failed
    pub error: Option<String>,
}

/// One evaluated step of an expression, with the values it saw (M26).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExprTrace {
    /// `all`, `any`, `not`, `compare` or `fn`
    pub kind: String,
    pub label: String,
    pub matched: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub children: Vec<ExprTrace>,
}

/// What happened to a group during classification (M26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupOutcome {
    Matched,
    NotMatched,
    EnvironmentMismatch,
    /// Outside its `active_from` / `expires_at` window.
    Inactive,
    /// A sibling of the same `exclusive_set` with a higher priority matched.
    Excluded,
    /// The parent did not match, so the group was never evaluated.
    NotEvaluated,
}

/// A flat rule evaluated against the node's facts (M26).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleExplanation {
    pub rule_id: Uuid,
    pub fact_path: String,
    pub operator: crate::models::RuleOperator,
    pub expected: serde_json::Value,
    pub actual: Option<serde_json::Value>,
    pub matched: bool,
}

/// Why a group did or did not match (M26).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupExplanation {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub priority: i32,
    pub outcome: GroupOutcome,
    pub match_type: Option<MatchType>,
    pub reason: String,
    pub rules: Vec<RuleExplanation>,
    pub expression: Option<ExprTrace>,
}

/// The group a value came from (M26).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub group_id: Uuid,
    pub group_name: String,
}

/// Where each class, variable and the environment came from. In the lists the
/// last entry is the group whose value won (M26).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Provenance {
    pub classes: std::collections::BTreeMap<String, Vec<Origin>>,
    pub variables: std::collections::BTreeMap<String, Vec<Origin>>,
    pub environment: Option<Origin>,
}

/// Explanation of a classification (M26).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationExplain {
    pub evaluated_at: chrono::DateTime<chrono::Utc>,
    pub elapsed_us: u64,
    pub groups: Vec<GroupExplanation>,
    pub provenance: Provenance,
    pub merge_errors: Vec<String>,
    /// Plugin extensions that were unavailable and what that changed (M27).
    #[serde(default)]
    pub degraded: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classification_result() {
        let result = ClassificationResult {
            certname: "node1.example.com".to_string(),
            organization_id: None,
            groups: vec![],
            classes: serde_json::json!({"profile::base": {}}),
            variables: serde_json::json!({}),
            environment: Some("production".to_string()),
            conflict_error: None,
        };

        assert_eq!(result.certname, "node1.example.com");
        assert!(result
            .classes
            .as_object()
            .unwrap()
            .contains_key("profile::base"));
    }

    #[test]
    fn test_match_type_serialization() {
        let match_type = MatchType::Rules;
        let json = serde_json::to_string(&match_type).unwrap();
        assert_eq!(json, "\"rules\"");
    }
}
