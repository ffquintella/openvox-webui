//! Classification data model

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result of classifying a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// Certificate name of the classified node
    pub certname: String,

    /// Groups the node belongs to
    pub groups: Vec<GroupMatch>,

    /// Combined classes from all groups
    pub classes: Vec<String>,

    /// Combined parameters from all groups
    pub parameters: serde_json::Value,

    /// Environment (from highest priority group)
    pub environment: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classification_result() {
        let result = ClassificationResult {
            certname: "node1.example.com".to_string(),
            groups: vec![],
            classes: vec!["profile::base".to_string()],
            parameters: serde_json::json!({}),
            environment: Some("production".to_string()),
        };

        assert_eq!(result.certname, "node1.example.com");
        assert_eq!(result.classes.len(), 1);
    }

    #[test]
    fn test_match_type_serialization() {
        let match_type = MatchType::Rules;
        let json = serde_json::to_string(&match_type).unwrap();
        assert_eq!(json, "\"rules\"");
    }
}
