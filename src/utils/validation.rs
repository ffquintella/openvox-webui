//! Input validation utilities

use regex::Regex;
use once_cell::sync::Lazy;

/// Regex for validating certificate names
static CERTNAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9._-]*$").unwrap()
});

/// Regex for validating group names
static GROUP_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
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

#[cfg(test)]
mod tests {
    use super::*;

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
