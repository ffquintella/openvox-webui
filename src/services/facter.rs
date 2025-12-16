//! Facter generation service

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::{ClassificationResult, FactDefinition, FactTemplate, FactValueSource};

/// Service for generating external facts based on classification
pub struct FacterService {
    templates: Vec<FactTemplate>,
}

impl FacterService {
    /// Create a new facter service
    pub fn new(templates: Vec<FactTemplate>) -> Self {
        Self { templates }
    }

    /// Generate facts for a node based on its classification
    pub fn generate_facts(
        &self,
        classification: &ClassificationResult,
        existing_facts: &serde_json::Value,
        template_name: &str,
    ) -> Result<GeneratedFacts> {
        let template = self
            .templates
            .iter()
            .find(|t| t.name == template_name)
            .context(format!("Template '{}' not found", template_name))?;

        let mut facts: HashMap<String, serde_json::Value> = HashMap::new();

        for fact_def in &template.facts {
            let value = self.resolve_fact_value(
                &fact_def.value,
                classification,
                existing_facts,
            )?;
            facts.insert(fact_def.name.clone(), value);
        }

        Ok(GeneratedFacts {
            certname: classification.certname.clone(),
            template: template_name.to_string(),
            facts,
        })
    }

    /// Resolve a fact value from its source
    fn resolve_fact_value(
        &self,
        source: &FactValueSource,
        classification: &ClassificationResult,
        existing_facts: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match source {
            FactValueSource::Static(value) => Ok(value.clone()),

            FactValueSource::FromClassification(key) => {
                // Get value from classification result
                match key.as_str() {
                    "environment" => Ok(serde_json::json!(classification.environment)),
                    "classes" => Ok(serde_json::json!(classification.classes)),
                    "groups" => {
                        let group_names: Vec<&str> =
                            classification.groups.iter().map(|g| g.name.as_str()).collect();
                        Ok(serde_json::json!(group_names))
                    }
                    _ => {
                        // Try to get from parameters
                        classification
                            .parameters
                            .get(key)
                            .cloned()
                            .ok_or_else(|| anyhow::anyhow!("Classification key '{}' not found", key))
                    }
                }
            }

            FactValueSource::FromFact(path) => {
                get_fact_by_path(existing_facts, path)
                    .ok_or_else(|| anyhow::anyhow!("Fact '{}' not found", path))
            }

            FactValueSource::Template(template) => {
                // Simple template substitution
                let rendered = self.render_template(template, classification, existing_facts)?;
                Ok(serde_json::Value::String(rendered))
            }
        }
    }

    /// Render a simple template string
    fn render_template(
        &self,
        template: &str,
        classification: &ClassificationResult,
        facts: &serde_json::Value,
    ) -> Result<String> {
        let mut result = template.to_string();

        // Replace classification variables
        result = result.replace("{{certname}}", &classification.certname);

        if let Some(env) = &classification.environment {
            result = result.replace("{{environment}}", env);
        }

        // Replace fact variables: {{fact:path}}
        let fact_regex = regex::Regex::new(r"\{\{fact:([^}]+)\}\}")?;
        let replaced = fact_regex.replace_all(&result, |caps: &regex::Captures| {
            let path = &caps[1];
            get_fact_by_path(facts, path)
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{{{{fact:{}}}}}", path))
        });

        Ok(replaced.to_string())
    }

    /// Export generated facts in various formats
    pub fn export_facts(facts: &GeneratedFacts, format: ExportFormat) -> Result<String> {
        match format {
            ExportFormat::Json => {
                serde_json::to_string_pretty(&facts.facts).context("Failed to serialize to JSON")
            }
            ExportFormat::Yaml => {
                serde_yaml::to_string(&facts.facts).context("Failed to serialize to YAML")
            }
            ExportFormat::Shell => {
                let mut output = String::new();
                for (key, value) in &facts.facts {
                    let value_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => value.to_string(),
                    };
                    output.push_str(&format!("export FACTER_{}=\"{}\"\n", key.to_uppercase(), value_str));
                }
                Ok(output)
            }
        }
    }
}

/// Get a fact value by dot-notation path
fn get_fact_by_path(facts: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = facts;

    for part in parts {
        current = current.get(part)?;
    }

    Some(current.clone())
}

/// Generated facts result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFacts {
    pub certname: String,
    pub template: String,
    pub facts: HashMap<String, serde_json::Value>,
}

/// Export format for generated facts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Yaml,
    Shell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{GroupMatch, MatchType};
    use uuid::Uuid;

    fn sample_classification() -> ClassificationResult {
        ClassificationResult {
            certname: "node1.example.com".to_string(),
            groups: vec![GroupMatch {
                id: Uuid::new_v4(),
                name: "webservers".to_string(),
                match_type: MatchType::Rules,
                matched_rules: vec![],
            }],
            classes: vec!["profile::webserver".to_string()],
            parameters: serde_json::json!({"http_port": 8080}),
            environment: Some("production".to_string()),
        }
    }

    #[test]
    fn test_generate_facts_static() {
        let template = FactTemplate {
            name: "basic".to_string(),
            description: Some("Basic facts".to_string()),
            facts: vec![FactDefinition {
                name: "custom_role".to_string(),
                value: FactValueSource::Static(serde_json::json!("webserver")),
            }],
        };

        let service = FacterService::new(vec![template]);
        let classification = sample_classification();
        let facts = serde_json::json!({});

        let result = service.generate_facts(&classification, &facts, "basic").unwrap();

        assert_eq!(result.facts.get("custom_role"), Some(&serde_json::json!("webserver")));
    }

    #[test]
    fn test_generate_facts_from_classification() {
        let template = FactTemplate {
            name: "classification".to_string(),
            description: None,
            facts: vec![
                FactDefinition {
                    name: "node_environment".to_string(),
                    value: FactValueSource::FromClassification("environment".to_string()),
                },
                FactDefinition {
                    name: "node_groups".to_string(),
                    value: FactValueSource::FromClassification("groups".to_string()),
                },
            ],
        };

        let service = FacterService::new(vec![template]);
        let classification = sample_classification();
        let facts = serde_json::json!({});

        let result = service
            .generate_facts(&classification, &facts, "classification")
            .unwrap();

        assert_eq!(
            result.facts.get("node_environment"),
            Some(&serde_json::json!("production"))
        );
    }

    #[test]
    fn test_generate_facts_from_existing_fact() {
        let template = FactTemplate {
            name: "derived".to_string(),
            description: None,
            facts: vec![FactDefinition {
                name: "os_family".to_string(),
                value: FactValueSource::FromFact("os.family".to_string()),
            }],
        };

        let service = FacterService::new(vec![template]);
        let classification = sample_classification();
        let facts = serde_json::json!({
            "os": {
                "family": "RedHat"
            }
        });

        let result = service.generate_facts(&classification, &facts, "derived").unwrap();

        assert_eq!(result.facts.get("os_family"), Some(&serde_json::json!("RedHat")));
    }

    #[test]
    fn test_export_facts_json() {
        let facts = GeneratedFacts {
            certname: "node1".to_string(),
            template: "basic".to_string(),
            facts: [("key".to_string(), serde_json::json!("value"))]
                .into_iter()
                .collect(),
        };

        let output = FacterService::export_facts(&facts, ExportFormat::Json).unwrap();
        assert!(output.contains("\"key\""));
        assert!(output.contains("\"value\""));
    }

    #[test]
    fn test_export_facts_shell() {
        let facts = GeneratedFacts {
            certname: "node1".to_string(),
            template: "basic".to_string(),
            facts: [("my_fact".to_string(), serde_json::json!("my_value"))]
                .into_iter()
                .collect(),
        };

        let output = FacterService::export_facts(&facts, ExportFormat::Shell).unwrap();
        assert!(output.contains("export FACTER_MY_FACT=\"my_value\""));
    }
}
