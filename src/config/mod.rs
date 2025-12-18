//! Configuration management
//!
//! This module provides YAML-based configuration management with support for:
//! - Environment variable overrides
//! - Multiple configuration file locations
//! - Default values for all settings
//! - Dashboard layout preferences
//! - RBAC configuration
//! - Node group definitions (loaded from separate file)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub puppetdb: Option<PuppetDbConfig>,
    #[serde(default)]
    pub puppet_ca: Option<PuppetCAConfig>,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub dashboard: DashboardConfig,
    #[serde(default)]
    pub rbac: RbacConfig,
    /// Path to groups configuration file (optional, groups can also be in database)
    #[serde(default)]
    pub groups_config_path: Option<PathBuf>,
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
    5051
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

/// Puppet CA connection configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PuppetCAConfig {
    pub url: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_ssl_verify")]
    pub ssl_verify: bool,
    pub ssl_cert: Option<PathBuf>,
    pub ssl_key: Option<PathBuf>,
    pub ssl_ca: Option<PathBuf>,
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

/// Dashboard layout and display preferences
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashboardConfig {
    /// Default time range for charts (1h, 6h, 12h, 24h, 7d, 30d)
    #[serde(default = "default_time_range")]
    pub default_time_range: String,
    /// Auto-refresh interval in seconds (0 to disable)
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,
    /// Number of nodes per page in listings
    #[serde(default = "default_nodes_per_page")]
    pub nodes_per_page: usize,
    /// Number of reports per page in listings
    #[serde(default = "default_reports_per_page")]
    pub reports_per_page: usize,
    /// Show nodes that haven't reported recently
    #[serde(default = "default_show_inactive")]
    pub show_inactive_nodes: bool,
    /// Hours after which a node is considered inactive
    #[serde(default = "default_inactive_threshold")]
    pub inactive_threshold_hours: u64,
    /// UI theme (light, dark, system)
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Dashboard widgets configuration
    #[serde(default)]
    pub widgets: Vec<WidgetConfig>,
}

fn default_time_range() -> String {
    "24h".to_string()
}

fn default_refresh_interval() -> u64 {
    60
}

fn default_nodes_per_page() -> usize {
    50
}

fn default_reports_per_page() -> usize {
    25
}

fn default_show_inactive() -> bool {
    true
}

fn default_inactive_threshold() -> u64 {
    24
}

fn default_theme() -> String {
    "light".to_string()
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            default_time_range: default_time_range(),
            refresh_interval_secs: default_refresh_interval(),
            nodes_per_page: default_nodes_per_page(),
            reports_per_page: default_reports_per_page(),
            show_inactive_nodes: default_show_inactive(),
            inactive_threshold_hours: default_inactive_threshold(),
            theme: default_theme(),
            widgets: Vec::new(),
        }
    }
}

/// Dashboard widget configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WidgetConfig {
    /// Unique widget identifier
    pub id: String,
    /// Widget type
    #[serde(rename = "type")]
    pub widget_type: WidgetType,
    /// Widget display title
    #[serde(default)]
    pub title: Option<String>,
    /// Whether widget is visible
    #[serde(default = "default_widget_enabled")]
    pub enabled: bool,
    /// Widget position on grid
    #[serde(default)]
    pub position: Option<WidgetPosition>,
    /// Widget-specific configuration
    #[serde(default)]
    pub config: serde_json::Value,
}

fn default_widget_enabled() -> bool {
    true
}

/// Widget position on dashboard grid
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WidgetPosition {
    pub row: usize,
    pub col: usize,
    #[serde(default = "default_widget_width")]
    pub width: usize,
    #[serde(default = "default_widget_height")]
    pub height: usize,
}

fn default_widget_width() -> usize {
    6
}

fn default_widget_height() -> usize {
    2
}

/// Widget types available for dashboard
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
    NodeStatus,
    ReportTimeline,
    FactDistribution,
    GroupMembership,
    ActivityHeatmap,
    InfrastructureTopology,
    QuickSearch,
    RecentActivity,
}

/// RBAC configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RbacConfig {
    /// Default role for new users
    #[serde(default = "default_role")]
    pub default_role: String,
    /// Session timeout in minutes
    #[serde(default = "default_session_timeout")]
    pub session_timeout_minutes: u64,
    /// Maximum failed login attempts before lockout
    #[serde(default = "default_max_failed_logins")]
    pub max_failed_logins: u32,
    /// Account lockout duration in minutes
    #[serde(default = "default_lockout_duration")]
    pub lockout_duration_minutes: u64,
    /// Custom role definitions (in addition to built-in roles)
    #[serde(default)]
    pub roles: Vec<RoleDefinition>,
}

fn default_role() -> String {
    "viewer".to_string()
}

fn default_session_timeout() -> u64 {
    480 // 8 hours
}

fn default_max_failed_logins() -> u32 {
    5
}

fn default_lockout_duration() -> u64 {
    30
}

impl Default for RbacConfig {
    fn default() -> Self {
        Self {
            default_role: default_role(),
            session_timeout_minutes: default_session_timeout(),
            max_failed_logins: default_max_failed_logins(),
            lockout_duration_minutes: default_lockout_duration(),
            roles: Vec::new(),
        }
    }
}

/// Role definition for YAML-based RBAC configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoleDefinition {
    /// Role name (lowercase, alphanumeric with underscores)
    pub name: String,
    /// Human-readable role name
    #[serde(default)]
    pub display_name: Option<String>,
    /// Role description
    #[serde(default)]
    pub description: Option<String>,
    /// Whether this is a system role (cannot be deleted)
    #[serde(default)]
    pub is_system: bool,
    /// Role permissions
    pub permissions: Vec<PermissionDefinition>,
}

/// Permission definition for YAML-based RBAC configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PermissionDefinition {
    /// Resource type
    pub resource: String,
    /// Action type
    pub action: String,
    /// Permission scope
    #[serde(default = "default_permission_scope")]
    pub scope: String,
    /// Scope value (e.g., environment name, group ID)
    #[serde(default)]
    pub scope_value: Option<String>,
}

fn default_permission_scope() -> String {
    "all".to_string()
}

/// Node groups configuration (loaded from separate file)
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GroupsConfig {
    /// List of node group definitions
    #[serde(default)]
    pub groups: Vec<NodeGroupDefinition>,
}

/// Node group definition for YAML configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeGroupDefinition {
    /// Unique group identifier (UUID format)
    pub id: String,
    /// Group name
    pub name: String,
    /// Group description
    #[serde(default)]
    pub description: Option<String>,
    /// Parent group ID
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Puppet environment for this group
    #[serde(default)]
    pub environment: Option<String>,
    /// How rules should be matched (all or any)
    #[serde(default = "default_rule_match_type")]
    pub rule_match_type: String,
    /// Puppet classes to apply
    #[serde(default)]
    pub classes: Vec<String>,
    /// Class parameters
    #[serde(default)]
    pub parameters: serde_json::Value,
    /// Classification rules
    #[serde(default)]
    pub rules: Vec<ClassificationRuleDefinition>,
    /// Pinned nodes
    #[serde(default)]
    pub pinned_nodes: Vec<String>,
}

fn default_rule_match_type() -> String {
    "all".to_string()
}

/// Classification rule definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClassificationRuleDefinition {
    /// Fact path to match (dot notation)
    pub fact_path: String,
    /// Comparison operator
    pub operator: String,
    /// Value to compare against
    pub value: serde_json::Value,
}

impl GroupsConfig {
    /// Load groups configuration from file
    pub fn load(path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read groups config file: {:?}", path))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse groups config file: {:?}", path))
    }

    /// Find groups configuration file in standard locations
    pub fn find_config_file() -> Option<PathBuf> {
        let paths = [
            PathBuf::from("groups.yaml"),
            PathBuf::from("config/groups.yaml"),
            PathBuf::from("/etc/openvox-webui/groups.yaml"),
            dirs::config_dir()
                .map(|p| p.join("openvox-webui/groups.yaml"))
                .unwrap_or_default(),
        ];

        paths.into_iter().find(|p| p.exists())
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
            puppet_ca: None,
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
            dashboard: DashboardConfig::default(),
            rbac: RbacConfig::default(),
            groups_config_path: None,
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
            .or_else(Self::find_config_file);

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

        // PuppetDB SSL certificate overrides
        if let Ok(cert) = std::env::var("PUPPETDB_SSL_CERT") {
            if let Some(ref mut puppetdb) = self.puppetdb {
                puppetdb.ssl_cert = Some(PathBuf::from(cert));
            }
        }
        if let Ok(key) = std::env::var("PUPPETDB_SSL_KEY") {
            if let Some(ref mut puppetdb) = self.puppetdb {
                puppetdb.ssl_key = Some(PathBuf::from(key));
            }
        }
        if let Ok(ca) = std::env::var("PUPPETDB_SSL_CA") {
            if let Some(ref mut puppetdb) = self.puppetdb {
                puppetdb.ssl_ca = Some(PathBuf::from(ca));
            }
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
        assert_eq!(config.server.port, 5051);
        assert_eq!(config.server.host, "127.0.0.1");
        assert!(config.puppetdb.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.server.port, config.server.port);
        assert_eq!(
            parsed.database.max_connections,
            config.database.max_connections
        );
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

    #[test]
    fn test_dashboard_config_defaults() {
        let config = DashboardConfig::default();
        assert_eq!(config.default_time_range, "24h");
        assert_eq!(config.refresh_interval_secs, 60);
        assert_eq!(config.nodes_per_page, 50);
        assert_eq!(config.reports_per_page, 25);
        assert!(config.show_inactive_nodes);
        assert_eq!(config.inactive_threshold_hours, 24);
        assert_eq!(config.theme, "light");
        assert!(config.widgets.is_empty());
    }

    #[test]
    fn test_dashboard_config_parsing() {
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 3000
auth:
  jwt_secret: "test-secret-that-is-at-least-32-characters-long"
database:
  url: "sqlite://test.db"
dashboard:
  default_time_range: "7d"
  refresh_interval_secs: 120
  nodes_per_page: 100
  theme: "dark"
  widgets:
    - id: "status-widget"
      type: "node_status"
      title: "Node Status"
      enabled: true
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.dashboard.default_time_range, "7d");
        assert_eq!(config.dashboard.refresh_interval_secs, 120);
        assert_eq!(config.dashboard.nodes_per_page, 100);
        assert_eq!(config.dashboard.theme, "dark");
        assert_eq!(config.dashboard.widgets.len(), 1);
        assert_eq!(config.dashboard.widgets[0].id, "status-widget");
        assert_eq!(
            config.dashboard.widgets[0].widget_type,
            WidgetType::NodeStatus
        );
    }

    #[test]
    fn test_rbac_config_defaults() {
        let config = RbacConfig::default();
        assert_eq!(config.default_role, "viewer");
        assert_eq!(config.session_timeout_minutes, 480);
        assert_eq!(config.max_failed_logins, 5);
        assert_eq!(config.lockout_duration_minutes, 30);
        assert!(config.roles.is_empty());
    }

    #[test]
    fn test_rbac_config_parsing() {
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 3000
auth:
  jwt_secret: "test-secret-that-is-at-least-32-characters-long"
database:
  url: "sqlite://test.db"
rbac:
  default_role: "operator"
  session_timeout_minutes: 240
  max_failed_logins: 3
  lockout_duration_minutes: 60
  roles:
    - name: "developer"
      display_name: "Developer"
      description: "Developer role"
      permissions:
        - resource: "nodes"
          action: "read"
          scope: "all"
        - resource: "facter_templates"
          action: "admin"
          scope: "all"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.rbac.default_role, "operator");
        assert_eq!(config.rbac.session_timeout_minutes, 240);
        assert_eq!(config.rbac.max_failed_logins, 3);
        assert_eq!(config.rbac.roles.len(), 1);
        assert_eq!(config.rbac.roles[0].name, "developer");
        assert_eq!(config.rbac.roles[0].permissions.len(), 2);
        assert_eq!(config.rbac.roles[0].permissions[0].resource, "nodes");
        assert_eq!(config.rbac.roles[0].permissions[0].action, "read");
    }

    #[test]
    fn test_groups_config_parsing() {
        let yaml = r#"
groups:
  - id: "00000000-0000-0000-0000-000000000001"
    name: "All Nodes"
    description: "Root group"
    rule_match_type: "all"
    classes:
      - "profile::base"
    parameters:
      monitoring: true
    rules: []
  - id: "00000000-0000-0000-0000-000000000002"
    name: "Production"
    parent_id: "00000000-0000-0000-0000-000000000001"
    environment: "production"
    rule_match_type: "all"
    classes:
      - "profile::production"
    rules:
      - fact_path: "trusted.extensions.pp_environment"
        operator: "="
        value: "production"
"#;
        let groups_config: GroupsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(groups_config.groups.len(), 2);
        assert_eq!(groups_config.groups[0].name, "All Nodes");
        assert!(groups_config.groups[0].parent_id.is_none());
        assert_eq!(groups_config.groups[1].name, "Production");
        assert_eq!(
            groups_config.groups[1].parent_id,
            Some("00000000-0000-0000-0000-000000000001".to_string())
        );
        assert_eq!(groups_config.groups[1].rules.len(), 1);
        assert_eq!(
            groups_config.groups[1].rules[0].fact_path,
            "trusted.extensions.pp_environment"
        );
        assert_eq!(groups_config.groups[1].rules[0].operator, "=");
    }

    #[test]
    fn test_widget_types_parsing() {
        let yaml = r#"
widgets:
  - id: "w1"
    type: "node_status"
  - id: "w2"
    type: "report_timeline"
  - id: "w3"
    type: "fact_distribution"
  - id: "w4"
    type: "group_membership"
  - id: "w5"
    type: "activity_heatmap"
  - id: "w6"
    type: "infrastructure_topology"
  - id: "w7"
    type: "quick_search"
  - id: "w8"
    type: "recent_activity"
"#;
        #[derive(Deserialize)]
        struct TestWidgets {
            widgets: Vec<WidgetConfig>,
        }
        let parsed: TestWidgets = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.widgets.len(), 8);
        assert_eq!(parsed.widgets[0].widget_type, WidgetType::NodeStatus);
        assert_eq!(parsed.widgets[1].widget_type, WidgetType::ReportTimeline);
        assert_eq!(parsed.widgets[2].widget_type, WidgetType::FactDistribution);
        assert_eq!(parsed.widgets[3].widget_type, WidgetType::GroupMembership);
        assert_eq!(parsed.widgets[4].widget_type, WidgetType::ActivityHeatmap);
        assert_eq!(
            parsed.widgets[5].widget_type,
            WidgetType::InfrastructureTopology
        );
        assert_eq!(parsed.widgets[6].widget_type, WidgetType::QuickSearch);
        assert_eq!(parsed.widgets[7].widget_type, WidgetType::RecentActivity);
    }

    #[test]
    fn test_full_config_serialization() {
        let config = AppConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();

        // Verify all sections are preserved
        assert_eq!(parsed.server.port, config.server.port);
        assert_eq!(parsed.database.url, config.database.url);
        assert_eq!(parsed.auth.jwt_secret, config.auth.jwt_secret);
        assert_eq!(parsed.logging.level, config.logging.level);
        assert_eq!(parsed.cache.enabled, config.cache.enabled);
        assert_eq!(
            parsed.dashboard.default_time_range,
            config.dashboard.default_time_range
        );
        assert_eq!(parsed.rbac.default_role, config.rbac.default_role);
    }
}
