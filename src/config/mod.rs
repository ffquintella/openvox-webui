//! Configuration management
//!
//! This module provides YAML-based configuration management with support for:
//! - Environment variable overrides
//! - Multiple configuration file locations
//! - Default values for all settings

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub puppetdb: Option<PuppetDbConfig>,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub cache: CacheConfig,
}

/// Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_workers")]
    pub workers: usize,
    #[serde(default)]
    pub request_timeout_secs: Option<u64>,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_workers() -> usize {
    num_cpus::get()
}

/// PuppetDB connection configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PuppetDbConfig {
    pub url: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_ssl_verify")]
    pub ssl_verify: bool,
    pub ssl_cert: Option<PathBuf>,
    pub ssl_key: Option<PathBuf>,
    pub ssl_ca: Option<PathBuf>,
}

fn default_timeout() -> u64 {
    30
}

fn default_ssl_verify() -> bool {
    true
}

/// Authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    #[serde(default = "default_token_expiry")]
    pub token_expiry_hours: u64,
    #[serde(default = "default_refresh_expiry")]
    pub refresh_token_expiry_days: u64,
    #[serde(default = "default_bcrypt_cost")]
    pub bcrypt_cost: u32,
    #[serde(default = "default_password_min_length")]
    pub password_min_length: usize,
}

fn default_token_expiry() -> u64 {
    24
}

fn default_refresh_expiry() -> u64 {
    7
}

fn default_bcrypt_cost() -> u32 {
    12
}

fn default_password_min_length() -> usize {
    8
}

/// Database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
}

fn default_max_connections() -> u32 {
    10
}

fn default_min_connections() -> u32 {
    1
}

fn default_connect_timeout() -> u64 {
    30
}

fn default_idle_timeout() -> u64 {
    600
}

/// Logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: LogFormat,
    #[serde(default)]
    pub file: Option<PathBuf>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> LogFormat {
    LogFormat::Pretty
}

/// Cache configuration for PuppetDB data
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    /// Enable/disable caching
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
    /// TTL for node cache in seconds
    #[serde(default = "default_node_ttl")]
    pub node_ttl_secs: u64,
    /// TTL for fact cache in seconds
    #[serde(default = "default_fact_ttl")]
    pub fact_ttl_secs: u64,
    /// TTL for report cache in seconds
    #[serde(default = "default_report_ttl")]
    pub report_ttl_secs: u64,
    /// TTL for resource cache in seconds
    #[serde(default = "default_resource_ttl")]
    pub resource_ttl_secs: u64,
    /// TTL for catalog cache in seconds
    #[serde(default = "default_catalog_ttl")]
    pub catalog_ttl_secs: u64,
    /// Maximum number of entries per cache type
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    /// Background sync interval in seconds (0 to disable)
    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,
}

fn default_cache_enabled() -> bool {
    true
}

fn default_node_ttl() -> u64 {
    300 // 5 minutes
}

fn default_fact_ttl() -> u64 {
    300 // 5 minutes
}

fn default_report_ttl() -> u64 {
    60 // 1 minute (reports change frequently)
}

fn default_resource_ttl() -> u64 {
    600 // 10 minutes
}

fn default_catalog_ttl() -> u64 {
    600 // 10 minutes
}

fn default_max_entries() -> usize {
    10000
}

fn default_sync_interval() -> u64 {
    0 // Disabled by default
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_cache_enabled(),
            node_ttl_secs: default_node_ttl(),
            fact_ttl_secs: default_fact_ttl(),
            report_ttl_secs: default_report_ttl(),
            resource_ttl_secs: default_resource_ttl(),
            catalog_ttl_secs: default_catalog_ttl(),
            max_entries: default_max_entries(),
            sync_interval_secs: default_sync_interval(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
    Compact,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: default_host(),
                port: default_port(),
                workers: default_workers(),
                request_timeout_secs: None,
            },
            puppetdb: None,
            auth: AuthConfig {
                jwt_secret: "change-me-in-production-minimum-32-characters-long".to_string(),
                token_expiry_hours: default_token_expiry(),
                refresh_token_expiry_days: default_refresh_expiry(),
                bcrypt_cost: default_bcrypt_cost(),
                password_min_length: default_password_min_length(),
            },
            database: DatabaseConfig {
                url: "sqlite://./data/openvox.db".to_string(),
                max_connections: default_max_connections(),
                min_connections: default_min_connections(),
                connect_timeout_secs: default_connect_timeout(),
                idle_timeout_secs: default_idle_timeout(),
            },
            logging: LoggingConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration from file and environment variables
    ///
    /// Configuration is loaded in the following order (later overrides earlier):
    /// 1. Default values
    /// 2. Configuration file (YAML)
    /// 3. Environment variables (prefixed with OPENVOX_)
    pub fn load() -> Result<Self> {
        // Try to load .env file if it exists
        let _ = dotenvy::dotenv();

        // Check for config path override from environment
        let config_path = std::env::var("OPENVOX_CONFIG")
            .map(PathBuf::from)
            .ok()
            .or_else(|| Self::find_config_file());

        let mut config = if let Some(path) = config_path {
            if path.exists() {
                let contents = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read config file: {:?}", path))?;
                serde_yaml::from_str(&contents)
                    .with_context(|| format!("Failed to parse config file: {:?}", path))?
            } else {
                AppConfig::default()
            }
        } else {
            AppConfig::default()
        };

        // Apply environment variable overrides
        config.apply_env_overrides();

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Find the configuration file in standard locations
    fn find_config_file() -> Option<PathBuf> {
        let paths = [
            // Current directory
            PathBuf::from("config.yaml"),
            PathBuf::from("config/config.yaml"),
            // System config directory
            PathBuf::from("/etc/openvox-webui/config.yaml"),
            // User config directory
            dirs::config_dir()
                .map(|p| p.join("openvox-webui/config.yaml"))
                .unwrap_or_default(),
        ];

        paths.into_iter().find(|p| p.exists())
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Server overrides
        if let Ok(host) = std::env::var("OPENVOX_HOST") {
            self.server.host = host;
        }
        if let Ok(port) = std::env::var("OPENVOX_PORT") {
            if let Ok(p) = port.parse() {
                self.server.port = p;
            }
        }

        // Database overrides
        if let Ok(url) = std::env::var("DATABASE_URL") {
            self.database.url = url;
        }

        // Auth overrides
        if let Ok(secret) = std::env::var("JWT_SECRET") {
            self.auth.jwt_secret = secret;
        }

        // Logging overrides
        if let Ok(level) = std::env::var("RUST_LOG") {
            self.logging.level = level;
        }
        if let Ok(format) = std::env::var("OPENVOX_LOG_FORMAT") {
            self.logging.format = match format.to_lowercase().as_str() {
                "json" => LogFormat::Json,
                "compact" => LogFormat::Compact,
                _ => LogFormat::Pretty,
            };
        }

        // PuppetDB overrides
        if let Ok(url) = std::env::var("PUPPETDB_URL") {
            let puppetdb = self.puppetdb.get_or_insert_with(|| PuppetDbConfig {
                url: url.clone(),
                timeout_secs: default_timeout(),
                ssl_verify: default_ssl_verify(),
                ssl_cert: None,
                ssl_key: None,
                ssl_ca: None,
            });
            puppetdb.url = url;
        }
    }

    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Validate JWT secret length
        if self.auth.jwt_secret.len() < 32 {
            anyhow::bail!("JWT secret must be at least 32 characters long");
        }

        // Validate port
        if self.server.port == 0 {
            anyhow::bail!("Server port cannot be 0");
        }

        // Validate database URL
        if self.database.url.is_empty() {
            anyhow::bail!("Database URL cannot be empty");
        }

        Ok(())
    }

    /// Create a default configuration file
    pub fn create_default_config(path: &PathBuf) -> Result<()> {
        let config = AppConfig::default();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let yaml = serde_yaml::to_string(&config)?;
        std::fs::write(path, yaml)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert!(config.puppetdb.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.server.port, config.server.port);
        assert_eq!(parsed.database.max_connections, config.database.max_connections);
    }

    #[test]
    fn test_log_format_parsing() {
        let yaml = r#"
server:
  host: "0.0.0.0"
  port: 8080
auth:
  jwt_secret: "test-secret-that-is-at-least-32-characters-long"
database:
  url: "sqlite://test.db"
logging:
  level: "debug"
  format: "json"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.logging.format, LogFormat::Json);
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    fn test_validation_jwt_secret_length() {
        let mut config = AppConfig::default();
        config.auth.jwt_secret = "short".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_valid_config() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_puppetdb_optional() {
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 3000
auth:
  jwt_secret: "test-secret-that-is-at-least-32-characters-long"
database:
  url: "sqlite://test.db"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.puppetdb.is_none());
    }
}
