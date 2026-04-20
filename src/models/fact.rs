//! Fact data model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::default_organization_uuid;

/// Represents a fact from a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    /// Certificate name of the node
    pub certname: String,

    /// Fact name (path for structured facts, e.g., "os.family")
    pub name: String,

    /// Fact value
    pub value: serde_json::Value,

    /// Environment
    pub environment: Option<String>,
}

/// Fact with full path information for structured facts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactPath {
    /// Root fact name
    pub name: String,

    /// Full path within the fact (for structured facts)
    pub path: Vec<String>,

    /// Value at this path
    pub value: serde_json::Value,

    /// Value type
    pub value_type: FactValueType,
}

/// Type of a fact value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactValueType {
    String,
    Integer,
    Float,
    Boolean,
    Array,
    Map,
    Null,
}

impl From<&serde_json::Value> for FactValueType {
    fn from(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => FactValueType::Null,
            serde_json::Value::Bool(_) => FactValueType::Boolean,
            serde_json::Value::Number(n) => {
                if n.is_f64() {
                    FactValueType::Float
                } else {
                    FactValueType::Integer
                }
            }
            serde_json::Value::String(_) => FactValueType::String,
            serde_json::Value::Array(_) => FactValueType::Array,
            serde_json::Value::Object(_) => FactValueType::Map,
        }
    }
}

/// Fact set for a node (all facts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactSet {
    /// Certificate name of the node
    pub certname: String,

    /// Timestamp when facts were collected
    pub timestamp: DateTime<Utc>,

    /// Environment
    pub environment: Option<String>,

    /// All facts as a JSON object
    pub facts: serde_json::Value,
}

/// Fact template for generating external facts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactTemplate {
    /// Unique identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Organization/tenant identifier
    #[serde(default = "default_organization_uuid")]
    pub organization_id: Uuid,

    /// Template name
    pub name: String,

    /// Description
    pub description: Option<String>,

    /// Facts to generate
    pub facts: Vec<FactDefinition>,
}

/// Request to create a fact template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFactTemplateRequest {
    pub name: String,
    pub description: Option<String>,
    pub facts: Vec<FactDefinition>,
}

/// Request to update a fact template
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateFactTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub facts: Option<Vec<FactDefinition>>,
}

/// Request to generate facts for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateFactsRequest {
    /// Node certname
    pub certname: String,
    /// Template name to use
    pub template: String,
    /// Optional existing facts (if not provided, will be fetched from PuppetDB)
    pub existing_facts: Option<serde_json::Value>,
}

/// Export format for facts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Json,
    Yaml,
    Shell,
}

/// Definition of a fact to generate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactDefinition {
    /// Fact name
    pub name: String,

    /// Static value or template expression
    pub value: FactValueSource,
}

/// Source of a fact value
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum FactValueSource {
    /// Static literal value
    Static(serde_json::Value),

    /// Value from classification
    FromClassification(String),

    /// Value from another fact
    FromFact(String),

    /// Computed from template
    Template(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fact_value_type_from_json() {
        assert_eq!(
            FactValueType::from(&serde_json::json!("hello")),
            FactValueType::String
        );
        assert_eq!(
            FactValueType::from(&serde_json::json!(42)),
            FactValueType::Integer
        );
        assert_eq!(
            FactValueType::from(&serde_json::json!(std::f64::consts::PI)),
            FactValueType::Float
        );
        assert_eq!(
            FactValueType::from(&serde_json::json!(true)),
            FactValueType::Boolean
        );
        assert_eq!(
            FactValueType::from(&serde_json::json!([1, 2, 3])),
            FactValueType::Array
        );
        assert_eq!(
            FactValueType::from(&serde_json::json!({"key": "value"})),
            FactValueType::Map
        );
    }

    #[test]
    fn test_fact_definition_serialization() {
        let def = FactDefinition {
            name: "custom_fact".to_string(),
            value: FactValueSource::Static(serde_json::json!("static_value")),
        };

        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("custom_fact"));
    }
}
