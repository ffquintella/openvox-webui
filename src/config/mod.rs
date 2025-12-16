//! Configuration management

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub puppetdb: PuppetDbConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
}

/// Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// PuppetDB connection configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PuppetDbConfig {
    pub url: String,
    pub timeout_secs: u64,
    pub ssl_verify: bool,
    pub ssl_cert: Option<PathBuf>,
    pub ssl_key: Option<PathBuf>,
    pub ssl_ca: Option<PathBuf>,
}

/// Authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub token_expiry_hours: u64,
    pub bcrypt_cost: u32,
}

/// Database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            puppetdb: PuppetDbConfig {
                url: "http://localhost:8081".to_string(),
                timeout_secs: 30,
                ssl_verify: true,
                ssl_cert: None,
                ssl_key: None,
                ssl_ca: None,
            },
            auth: AuthConfig {
                jwt_secret: "change-me-in-production".to_string(),
                token_expiry_hours: 24,
                bcrypt_cost: 12,
            },
            database: DatabaseConfig {
                url: "sqlite://openvox.db".to_string(),
                max_connections: 5,
            },
        }
    }
}

impl AppConfig {
    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let config_path = Self::find_config_file()?;
        let contents = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
        let config: AppConfig = serde_yaml::from_str(&contents)
            .with_context(|| "Failed to parse config file")?;
        Ok(config)
    }

    /// Find the configuration file
    fn find_config_file() -> Result<PathBuf> {
        let paths = [
            PathBuf::from("config/config.yaml"),
            PathBuf::from("config.yaml"),
            dirs::config_dir()
                .map(|p| p.join("openvox-webui/config.yaml"))
                .unwrap_or_default(),
        ];

        for path in &paths {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        // If no config file exists, create default
        let default_path = PathBuf::from("config/config.yaml");
        let default_config = AppConfig::default();

        if let Some(parent) = default_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let yaml = serde_yaml::to_string(&default_config)?;
        std::fs::write(&default_path, yaml)?;

        Ok(default_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "127.0.0.1");
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.server.port, config.server.port);
    }
}
