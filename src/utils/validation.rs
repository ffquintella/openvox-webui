//! Input validation utilities

use regex::Regex;
use once_cell::sync::Lazy;

use crate::models::{FactDefinition, FactTemplate, FactValueSource};

/// Regex for validating certificate names
static CERTNAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9._-]*$").unwrap()
});

/// Regex for validating group names
static GROUP_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$").unwrap()
});

/// Regex for validating fact names (Puppet/Facter compatible)
static FACT_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-z][a-z0-9_]*$").unwrap()
});

/// Regex for validating template names
static TEMPLATE_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$").unwrap()
});

/// Validate a certificate name
pub fn validate_certname(certname: &str) -> bool {
    !certname.is_empty() && certname.len() <= 255 && CERTNAME_REGEX.is_match(certname)
}

/// Validate a group name
pub fn validate_group_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && GROUP_NAME_REGEX.is_match(name)
}

/// Validate a fact path
pub fn validate_fact_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 255 {
        return false;
    }

    // Fact paths are dot-separated identifiers
    path.split('.').all(|part| {
        !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Validation error for fact templates
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Validation result type
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Validate a fact template name
pub fn validate_template_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && TEMPLATE_NAME_REGEX.is_match(name)
}

/// Validate a fact name (lowercase, underscores, starts with letter)
pub fn validate_fact_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && FACT_NAME_REGEX.is_match(name)
}

/// Validate a fact value source
pub fn validate_fact_value_source(source: &FactValueSource) -> Result<(), String> {
    match source {
        FactValueSource::Static(value) => {
            // Static values can be any JSON, but check for reasonable size
            let serialized = serde_json::to_string(value).unwrap_or_default();
            if serialized.len() > 10_000 {
                return Err("Static value exceeds maximum size (10KB)".to_string());
            }
            Ok(())
        }
        FactValueSource::FromClassification(key) => {
            if key.is_empty() {
                return Err("Classification key cannot be empty".to_string());
            }
            if key.len() > 100 {
                return Err("Classification key exceeds maximum length (100)".to_string());
            }
            // Valid classification keys
            let valid_keys = ["environment", "classes", "groups", "certname"];
            if !valid_keys.contains(&key.as_str()) && !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(format!("Invalid classification key: '{}'. Use: environment, classes, groups, certname, or a parameter key", key));
            }
            Ok(())
        }
        FactValueSource::FromFact(path) => {
            if !validate_fact_path(path) {
                return Err(format!("Invalid fact path: '{}'. Use dot-separated identifiers (e.g., 'os.family')", path));
            }
            Ok(())
        }
        FactValueSource::Template(template) => {
            if template.is_empty() {
                return Err("Template string cannot be empty".to_string());
            }
            if template.len() > 1000 {
                return Err("Template string exceeds maximum length (1000)".to_string());
            }
            // Validate template syntax (check for unclosed braces)
            let open_count = template.matches("{{").count();
            let close_count = template.matches("}}").count();
            if open_count != close_count {
                return Err("Template has mismatched braces".to_string());
            }
            Ok(())
        }
    }
}

/// Validate a fact definition
pub fn validate_fact_definition(def: &FactDefinition) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if !validate_fact_name(&def.name) {
        errors.push(ValidationError::new(
            format!("facts.{}.name", def.name),
            "Fact name must start with a lowercase letter and contain only lowercase letters, numbers, and underscores",
        ));
    }

    if let Err(e) = validate_fact_value_source(&def.value) {
        errors.push(ValidationError::new(
            format!("facts.{}.value", def.name),
            e,
        ));
    }

    errors
}

/// Validate a complete fact template
pub fn validate_fact_template(template: &FactTemplate) -> ValidationResult {
    let mut errors = Vec::new();

    // Validate name
    if !validate_template_name(&template.name) {
        errors.push(ValidationError::new(
            "name",
            "Template name must start with a letter and contain only letters, numbers, underscores, and hyphens",
        ));
    }

    // Validate description (optional, but check length)
    if let Some(ref desc) = template.description {
        if desc.len() > 500 {
            errors.push(ValidationError::new(
                "description",
                "Description exceeds maximum length (500 characters)",
            ));
        }
    }

    // Validate facts
    if template.facts.is_empty() {
        errors.push(ValidationError::new(
            "facts",
            "Template must define at least one fact",
        ));
    }

    if template.facts.len() > 100 {
        errors.push(ValidationError::new(
            "facts",
            "Template exceeds maximum number of facts (100)",
        ));
    }

    // Validate each fact definition
    let mut seen_names = std::collections::HashSet::new();
    for def in &template.facts {
        // Check for duplicate fact names
        if !seen_names.insert(&def.name) {
            errors.push(ValidationError::new(
                format!("facts.{}", def.name),
                "Duplicate fact name in template",
            ));
        }

        // Validate the fact definition
        errors.extend(validate_fact_definition(def));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Format validation errors as a single string
pub fn format_validation_errors(errors: &[ValidationError]) -> String {
    errors
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fact template validation tests
    #[test]
    fn test_validate_template_name_valid() {
        assert!(validate_template_name("webserver_facts"));
        assert!(validate_template_name("BaseTemplate"));
        assert!(validate_template_name("prod-facts"));
        assert!(validate_template_name("t1"));
    }

    #[test]
    fn test_validate_template_name_invalid() {
        assert!(!validate_template_name("")); // Empty
        assert!(!validate_template_name("1template")); // Starts with number
        assert!(!validate_template_name("has spaces")); // Contains space
        assert!(!validate_template_name("has.dots")); // Contains dot
    }

    #[test]
    fn test_validate_fact_name_valid() {
        assert!(validate_fact_name("custom_role"));
        assert!(validate_fact_name("os_family"));
        assert!(validate_fact_name("myapp123"));
    }

    #[test]
    fn test_validate_fact_name_invalid() {
        assert!(!validate_fact_name("")); // Empty
        assert!(!validate_fact_name("CustomRole")); // Uppercase
        assert!(!validate_fact_name("1fact")); // Starts with number
        assert!(!validate_fact_name("has-hyphen")); // Contains hyphen
        assert!(!validate_fact_name("has.dot")); // Contains dot
    }

    #[test]
    fn test_validate_fact_value_source_static() {
        let source = FactValueSource::Static(serde_json::json!("test"));
        assert!(validate_fact_value_source(&source).is_ok());

        let source = FactValueSource::Static(serde_json::json!({"nested": {"value": 123}}));
        assert!(validate_fact_value_source(&source).is_ok());
    }

    #[test]
    fn test_validate_fact_value_source_from_classification() {
        let source = FactValueSource::FromClassification("environment".to_string());
        assert!(validate_fact_value_source(&source).is_ok());

        let source = FactValueSource::FromClassification("groups".to_string());
        assert!(validate_fact_value_source(&source).is_ok());

        let source = FactValueSource::FromClassification("custom_param".to_string());
        assert!(validate_fact_value_source(&source).is_ok());

        let source = FactValueSource::FromClassification("".to_string());
        assert!(validate_fact_value_source(&source).is_err());
    }

    #[test]
    fn test_validate_fact_value_source_from_fact() {
        let source = FactValueSource::FromFact("os.family".to_string());
        assert!(validate_fact_value_source(&source).is_ok());

        let source = FactValueSource::FromFact("networking.interfaces.eth0".to_string());
        assert!(validate_fact_value_source(&source).is_ok());

        let source = FactValueSource::FromFact("".to_string());
        assert!(validate_fact_value_source(&source).is_err());

        let source = FactValueSource::FromFact(".invalid".to_string());
        assert!(validate_fact_value_source(&source).is_err());
    }

    #[test]
    fn test_validate_fact_value_source_template() {
        let source = FactValueSource::Template("{{certname}}-{{environment}}".to_string());
        assert!(validate_fact_value_source(&source).is_ok());

        let source = FactValueSource::Template("".to_string());
        assert!(validate_fact_value_source(&source).is_err());

        let source = FactValueSource::Template("{{unclosed".to_string());
        assert!(validate_fact_value_source(&source).is_err());
    }

    #[test]
    fn test_validate_fact_template_valid() {
        let template = FactTemplate {
            id: None,
            name: "valid_template".to_string(),
            description: Some("A valid template".to_string()),
            facts: vec![FactDefinition {
                name: "custom_fact".to_string(),
                value: FactValueSource::Static(serde_json::json!("value")),
            }],
        };
        assert!(validate_fact_template(&template).is_ok());
    }

    #[test]
    fn test_validate_fact_template_empty_facts() {
        let template = FactTemplate {
            id: None,
            name: "empty".to_string(),
            description: None,
            facts: vec![],
        };
        let result = validate_fact_template(&template);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field == "facts"));
    }

    #[test]
    fn test_validate_fact_template_duplicate_fact_names() {
        let template = FactTemplate {
            id: None,
            name: "duplicates".to_string(),
            description: None,
            facts: vec![
                FactDefinition {
                    name: "same_name".to_string(),
                    value: FactValueSource::Static(serde_json::json!("value1")),
                },
                FactDefinition {
                    name: "same_name".to_string(),
                    value: FactValueSource::Static(serde_json::json!("value2")),
                },
            ],
        };
        let result = validate_fact_template(&template);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("Duplicate")));
    }

    #[test]
    fn test_validate_fact_template_invalid_fact_name() {
        let template = FactTemplate {
            id: None,
            name: "invalid_fact".to_string(),
            description: None,
            facts: vec![FactDefinition {
                name: "InvalidName".to_string(), // Uppercase not allowed
                value: FactValueSource::Static(serde_json::json!("value")),
            }],
        };
        let result = validate_fact_template(&template);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_validation_errors() {
        let errors = vec![
            ValidationError::new("name", "Invalid name"),
            ValidationError::new("facts", "No facts defined"),
        ];
        let formatted = format_validation_errors(&errors);
        assert!(formatted.contains("name: Invalid name"));
        assert!(formatted.contains("facts: No facts defined"));
    }

    // Original tests
    #[test]
    fn test_validate_certname_valid() {
        assert!(validate_certname("node1.example.com"));
        assert!(validate_certname("web-server-01"));
        assert!(validate_certname("db_primary"));
    }

    #[test]
    fn test_validate_certname_invalid() {
        assert!(!validate_certname(""));
        assert!(!validate_certname("123node")); // Can't start with number
        assert!(!validate_certname("-invalid")); // Can't start with hyphen
    }

    #[test]
    fn test_validate_group_name_valid() {
        assert!(validate_group_name("webservers"));
        assert!(validate_group_name("prod-db"));
        assert!(validate_group_name("All_Nodes"));
    }

    #[test]
    fn test_validate_group_name_invalid() {
        assert!(!validate_group_name(""));
        assert!(!validate_group_name("has spaces"));
        assert!(!validate_group_name("has.dots"));
    }

    #[test]
    fn test_validate_fact_path_valid() {
        assert!(validate_fact_path("os"));
        assert!(validate_fact_path("os.family"));
        assert!(validate_fact_path("networking.interfaces.eth0"));
    }

    #[test]
    fn test_validate_fact_path_invalid() {
        assert!(!validate_fact_path(""));
        assert!(!validate_fact_path(".invalid"));
        assert!(!validate_fact_path("invalid."));
        assert!(!validate_fact_path("has spaces.here"));
    }
}
