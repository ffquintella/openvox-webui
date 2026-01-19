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
    /// Code Deploy configuration (Git-based environment management)
    #[serde(default)]
    pub code_deploy: Option<CodeDeployYamlConfig>,
    /// SAML 2.0 SSO configuration
    #[serde(default)]
    pub saml: Option<SamlConfig>,
    /// Server backup configuration
    #[serde(default)]
    pub backup: Option<BackupConfig>,
    /// Node removal tracking configuration (for nodes with revoked/missing certificates)
    #[serde(default)]
    pub node_removal: Option<NodeRemovalConfig>,
    /// Node bootstrap configuration (for adding new nodes)
    #[serde(default)]
    pub node_bootstrap: Option<NodeBootstrapConfig>,
    /// Classification endpoint configuration
    #[serde(default)]
    pub classification: Option<ClassificationConfig>,
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
    /// TLS/HTTPS configuration (if not set, server runs HTTP)
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    /// Path to static files directory (frontend build output)
    #[serde(default = "default_static_dir")]
    pub static_dir: Option<PathBuf>,
    /// Whether to serve the frontend SPA (enables fallback to index.html)
    #[serde(default = "default_serve_frontend")]
    pub serve_frontend: bool,
}

/// TLS/HTTPS configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    /// Path to TLS certificate file (PEM format)
    pub cert_file: PathBuf,
    /// Path to TLS private key file (PEM format)
    pub key_file: PathBuf,
    /// Minimum TLS version (1.2 or 1.3, defaults to 1.2)
    #[serde(default = "default_min_tls_version")]
    pub min_version: String,
    /// TLS cipher suites (if empty, uses secure defaults)
    #[serde(default)]
    pub ciphers: Vec<String>,
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

fn default_static_dir() -> Option<PathBuf> {
    // Default to looking for frontend/dist in current directory
    let path = PathBuf::from("frontend/dist");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn default_serve_frontend() -> bool {
    true
}

fn default_min_tls_version() -> String {
    "1.3".to_string()
}

/// PuppetDB SSL configuration (nested format from Puppet module)
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PuppetDbSslConfig {
    /// Path to client certificate (cert_path in Puppet config)
    #[serde(alias = "cert_path")]
    pub cert_path: Option<PathBuf>,
    /// Path to client private key (key_path in Puppet config)
    #[serde(alias = "key_path")]
    pub key_path: Option<PathBuf>,
    /// Path to CA certificate (ca_path in Puppet config)
    #[serde(alias = "ca_path")]
    pub ca_path: Option<PathBuf>,
    /// Verify SSL certificates
    #[serde(default = "default_ssl_verify")]
    pub verify: bool,
}

/// PuppetDB connection configuration
/// Supports both flat format (ssl_cert, ssl_key, ssl_ca) and nested format (ssl.cert_path, etc.)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PuppetDbConfig {
    pub url: String,
    /// Timeout in seconds (supports both timeout_secs and timeout field names)
    #[serde(default = "default_timeout", alias = "timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_ssl_verify")]
    pub ssl_verify: bool,
    /// Flat format: ssl_cert path
    pub ssl_cert: Option<PathBuf>,
    /// Flat format: ssl_key path
    pub ssl_key: Option<PathBuf>,
    /// Flat format: ssl_ca path
    pub ssl_ca: Option<PathBuf>,
    /// Nested format: ssl configuration block (from Puppet module)
    #[serde(default)]
    pub ssl: Option<PuppetDbSslConfig>,
}

impl PuppetDbConfig {
    /// Get the effective SSL cert path (checks nested config first, then flat)
    pub fn effective_ssl_cert(&self) -> Option<&PathBuf> {
        self.ssl
            .as_ref()
            .and_then(|s| s.cert_path.as_ref())
            .or(self.ssl_cert.as_ref())
    }

    /// Get the effective SSL key path (checks nested config first, then flat)
    pub fn effective_ssl_key(&self) -> Option<&PathBuf> {
        self.ssl
            .as_ref()
            .and_then(|s| s.key_path.as_ref())
            .or(self.ssl_key.as_ref())
    }

    /// Get the effective SSL CA path (checks nested config first, then flat)
    pub fn effective_ssl_ca(&self) -> Option<&PathBuf> {
        self.ssl
            .as_ref()
            .and_then(|s| s.ca_path.as_ref())
            .or(self.ssl_ca.as_ref())
    }

    /// Get the effective SSL verify setting (checks nested config first, then flat)
    pub fn effective_ssl_verify(&self) -> bool {
        self.ssl.as_ref().map(|s| s.verify).unwrap_or(self.ssl_verify)
    }
}

fn default_timeout() -> u64 {
    30
}

fn default_ssl_verify() -> bool {
    true
}

/// Puppet CA SSL configuration (nested format from Puppet module)
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PuppetCASslConfig {
    /// Path to client certificate (cert_path in Puppet config)
    #[serde(alias = "cert_path")]
    pub cert_path: Option<PathBuf>,
    /// Path to client private key (key_path in Puppet config)
    #[serde(alias = "key_path")]
    pub key_path: Option<PathBuf>,
    /// Path to CA certificate (ca_path in Puppet config)
    #[serde(alias = "ca_path")]
    pub ca_path: Option<PathBuf>,
    /// Verify SSL certificates
    #[serde(default = "default_ssl_verify")]
    pub verify: bool,
}

/// Puppet CA connection configuration
/// Supports both flat format (ssl_cert, ssl_key, ssl_ca) and nested format (ssl.cert_path, etc.)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PuppetCAConfig {
    pub url: String,
    /// Timeout in seconds (supports both timeout_secs and timeout field names)
    #[serde(default = "default_timeout", alias = "timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_ssl_verify")]
    pub ssl_verify: bool,
    /// Flat format: ssl_cert path
    pub ssl_cert: Option<PathBuf>,
    /// Flat format: ssl_key path
    pub ssl_key: Option<PathBuf>,
    /// Flat format: ssl_ca path
    pub ssl_ca: Option<PathBuf>,
    /// Nested format: ssl configuration block (from Puppet module)
    #[serde(default)]
    pub ssl: Option<PuppetCASslConfig>,
}

impl PuppetCAConfig {
    /// Get the effective SSL cert path (checks nested config first, then flat)
    pub fn effective_ssl_cert(&self) -> Option<&PathBuf> {
        self.ssl
            .as_ref()
            .and_then(|s| s.cert_path.as_ref())
            .or(self.ssl_cert.as_ref())
    }

    /// Get the effective SSL key path (checks nested config first, then flat)
    pub fn effective_ssl_key(&self) -> Option<&PathBuf> {
        self.ssl
            .as_ref()
            .and_then(|s| s.key_path.as_ref())
            .or(self.ssl_key.as_ref())
    }

    /// Get the effective SSL CA path (checks nested config first, then flat)
    pub fn effective_ssl_ca(&self) -> Option<&PathBuf> {
        self.ssl
            .as_ref()
            .and_then(|s| s.ca_path.as_ref())
            .or(self.ssl_ca.as_ref())
    }

    /// Get the effective SSL verify setting (checks nested config first, then flat)
    pub fn effective_ssl_verify(&self) -> bool {
        self.ssl.as_ref().map(|s| s.verify).unwrap_or(self.ssl_verify)
    }
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
    /// Log output target (console or file)
    #[serde(default = "default_log_target")]
    pub target: LogTarget,
    /// Directory for log files (used when target is "file")
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    /// Log file name prefix (default: "openvox-webui")
    #[serde(default = "default_log_prefix")]
    pub log_prefix: String,
    /// Enable daily log rotation (default: true for production)
    #[serde(default = "default_log_rotation")]
    pub daily_rotation: bool,
    /// Maximum number of log files to keep (0 = unlimited)
    #[serde(default = "default_max_log_files")]
    pub max_log_files: usize,
    /// Deprecated: use log_dir instead
    #[serde(default)]
    pub file: Option<PathBuf>,
}

/// Log output target
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogTarget {
    /// Log to console (stdout/stderr) - default for development
    #[default]
    Console,
    /// Log to file with optional rotation - recommended for production
    File,
    /// Log to both console and file
    Both,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> LogFormat {
    LogFormat::Pretty
}

fn default_log_target() -> LogTarget {
    LogTarget::Console
}

fn default_log_dir() -> PathBuf {
    PathBuf::from("/var/log/openvox/webui")
}

fn default_log_prefix() -> String {
    "openvox-webui".to_string()
}

fn default_log_rotation() -> bool {
    true
}

fn default_max_log_files() -> usize {
    30 // Keep 30 days of logs by default
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

/// Code Deploy configuration (YAML format)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodeDeployYamlConfig {
    /// Whether the code deploy feature is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Base directory for cloned repositories
    #[serde(default = "default_repos_base_dir")]
    pub repos_base_dir: PathBuf,
    /// Directory for SSH keys
    #[serde(default = "default_ssh_keys_dir")]
    pub ssh_keys_dir: PathBuf,
    /// Path to r10k binary
    #[serde(default = "default_r10k_binary_path")]
    pub r10k_binary_path: PathBuf,
    /// Path to r10k configuration file
    #[serde(default = "default_r10k_config_path")]
    pub r10k_config_path: PathBuf,
    /// Base directory for Puppet environments
    #[serde(default = "default_environments_basedir")]
    pub environments_basedir: PathBuf,
    /// r10k cache directory
    #[serde(default = "default_r10k_cachedir")]
    pub r10k_cachedir: PathBuf,
    /// Encryption key for SSH private keys (should come from secure storage)
    #[serde(default)]
    pub encryption_key: String,
    /// Base URL for webhook URLs (e.g., https://openvox.example.com)
    #[serde(default)]
    pub webhook_base_url: Option<String>,
    /// Retain deployment history for this many days
    #[serde(default = "default_retain_history_days")]
    pub retain_history_days: u32,
}

fn default_repos_base_dir() -> PathBuf {
    PathBuf::from("/var/lib/openvox-webui/repos")
}

fn default_ssh_keys_dir() -> PathBuf {
    PathBuf::from("/etc/openvox-webui/ssh-keys")
}

fn default_r10k_binary_path() -> PathBuf {
    PathBuf::from("/opt/puppetlabs/puppet/bin/r10k")
}

fn default_r10k_config_path() -> PathBuf {
    PathBuf::from("/etc/puppetlabs/r10k/r10k.yaml")
}

fn default_environments_basedir() -> PathBuf {
    PathBuf::from("/etc/puppetlabs/code/environments")
}

fn default_r10k_cachedir() -> PathBuf {
    PathBuf::from("/opt/puppetlabs/puppet/cache/r10k")
}

fn default_retain_history_days() -> u32 {
    90
}

impl Default for CodeDeployYamlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            repos_base_dir: default_repos_base_dir(),
            ssh_keys_dir: default_ssh_keys_dir(),
            r10k_binary_path: default_r10k_binary_path(),
            r10k_config_path: default_r10k_config_path(),
            environments_basedir: default_environments_basedir(),
            r10k_cachedir: default_r10k_cachedir(),
            encryption_key: String::new(),
            webhook_base_url: None,
            retain_history_days: default_retain_history_days(),
        }
    }
}

/// SAML 2.0 SSO Configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SamlConfig {
    /// Enable SAML authentication
    #[serde(default)]
    pub enabled: bool,
    /// Service Provider configuration
    pub sp: SamlSpConfig,
    /// Identity Provider configuration
    pub idp: SamlIdpConfig,
    /// User mapping configuration
    #[serde(default)]
    pub user_mapping: SamlUserMappingConfig,
    /// Session configuration
    #[serde(default)]
    pub session: SamlSessionConfig,
}

/// SAML Service Provider (SP) configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SamlSpConfig {
    /// Entity ID for this SP (typically the application URL)
    pub entity_id: String,
    /// Assertion Consumer Service (ACS) URL - where IdP sends SAML responses
    pub acs_url: String,
    /// Single Logout Service URL (optional)
    #[serde(default)]
    pub slo_url: Option<String>,
    /// SP Certificate file path for signing/encryption
    #[serde(default)]
    pub certificate_file: Option<PathBuf>,
    /// SP Private key file path for signing/encryption
    #[serde(default)]
    pub private_key_file: Option<PathBuf>,
    /// Sign authentication requests to IdP
    #[serde(default = "default_sign_requests")]
    pub sign_requests: bool,
    /// Require signed assertions from IdP
    #[serde(default = "default_require_signed_assertions")]
    pub require_signed_assertions: bool,
    /// Require encrypted assertions from IdP
    #[serde(default)]
    pub require_encrypted_assertions: bool,
}

fn default_sign_requests() -> bool {
    false
}

fn default_require_signed_assertions() -> bool {
    true
}

/// SAML Identity Provider (IdP) configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SamlIdpConfig {
    /// IdP Metadata URL (recommended - auto-discovers IdP config)
    #[serde(default)]
    pub metadata_url: Option<String>,
    /// IdP Metadata file path (alternative to URL)
    #[serde(default)]
    pub metadata_file: Option<PathBuf>,
    /// Manual IdP Entity ID (if metadata not available)
    #[serde(default)]
    pub entity_id: Option<String>,
    /// Manual IdP SSO URL (if metadata not available)
    #[serde(default)]
    pub sso_url: Option<String>,
    /// Manual IdP SLO URL (if metadata not available)
    #[serde(default)]
    pub slo_url: Option<String>,
    /// Manual IdP certificate file (if metadata not available)
    #[serde(default)]
    pub certificate_file: Option<PathBuf>,
}

/// SAML user mapping configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SamlUserMappingConfig {
    /// SAML attribute containing the username (NameID or attribute name)
    #[serde(default = "default_username_attribute")]
    pub username_attribute: String,
    /// SAML attribute containing email (optional)
    #[serde(default)]
    pub email_attribute: Option<String>,
    /// Allow IdP-initiated SSO (security consideration)
    #[serde(default)]
    pub allow_idp_initiated: bool,
    /// Require pre-provisioned users (user must exist before SAML login)
    #[serde(default = "default_require_existing_user")]
    pub require_existing_user: bool,
}

fn default_username_attribute() -> String {
    "NameID".to_string()
}

fn default_require_existing_user() -> bool {
    true
}

impl Default for SamlUserMappingConfig {
    fn default() -> Self {
        Self {
            username_attribute: default_username_attribute(),
            email_attribute: None,
            allow_idp_initiated: false,
            require_existing_user: default_require_existing_user(),
        }
    }
}

/// SAML session configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SamlSessionConfig {
    /// Cookie name for SAML relay state
    #[serde(default = "default_relay_state_cookie")]
    pub relay_state_cookie: String,
    /// Maximum age of authentication request (seconds)
    #[serde(default = "default_request_max_age_secs")]
    pub request_max_age_secs: u64,
}

fn default_relay_state_cookie() -> String {
    "saml_relay_state".to_string()
}

fn default_request_max_age_secs() -> u64 {
    300 // 5 minutes
}

impl Default for SamlSessionConfig {
    fn default() -> Self {
        Self {
            relay_state_cookie: default_relay_state_cookie(),
            request_max_age_secs: default_request_max_age_secs(),
        }
    }
}

// =============================================================================
// Server Backup Configuration
// =============================================================================

/// Server backup configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupConfig {
    /// Whether the backup feature is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Directory where backup files are stored
    #[serde(default = "default_backup_dir")]
    pub backup_dir: PathBuf,
    /// Backup schedule configuration
    #[serde(default)]
    pub schedule: BackupScheduleConfig,
    /// Retention configuration
    #[serde(default)]
    pub retention: BackupRetentionConfig,
    /// Encryption configuration
    #[serde(default)]
    pub encryption: BackupEncryptionConfig,
    /// What to include in backups
    #[serde(default)]
    pub include: BackupIncludeConfig,
}

/// Backup schedule configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupScheduleConfig {
    /// Backup frequency (hourly, daily, weekly, custom, disabled)
    #[serde(default = "default_backup_frequency")]
    pub frequency: BackupFrequency,
    /// Custom cron expression (when frequency is custom)
    #[serde(default)]
    pub cron: Option<String>,
    /// Time of day for daily/weekly backups (HH:MM format)
    #[serde(default = "default_backup_time")]
    pub time: String,
    /// Day of week for weekly backups (0=Sunday, 6=Saturday)
    #[serde(default)]
    pub day_of_week: u8,
}

/// Backup frequency options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupFrequency {
    Hourly,
    Daily,
    Weekly,
    Custom,
    Disabled,
}

/// Backup retention configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupRetentionConfig {
    /// Maximum number of backups to keep
    #[serde(default = "default_max_backups")]
    pub max_backups: u32,
    /// Minimum age in hours before a backup can be deleted
    #[serde(default = "default_min_age_hours")]
    pub min_age_hours: u32,
}

/// Backup encryption configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupEncryptionConfig {
    /// Whether encryption is enabled
    #[serde(default = "default_encryption_enabled")]
    pub enabled: bool,
    /// Whether password is required (if false, uses a system key)
    #[serde(default = "default_require_password")]
    pub require_password: bool,
}

/// What to include in backups
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupIncludeConfig {
    /// Include database files (openvox.db, .wal, .shm)
    #[serde(default = "default_include_database")]
    pub database: bool,
    /// Include configuration files (config.yaml, groups.yaml)
    #[serde(default = "default_include_config")]
    pub config_files: bool,
}

fn default_backup_dir() -> PathBuf {
    PathBuf::from("/var/lib/openvox-webui/backups")
}

fn default_backup_frequency() -> BackupFrequency {
    BackupFrequency::Daily
}

fn default_backup_time() -> String {
    "02:00".to_string()
}

fn default_max_backups() -> u32 {
    30
}

fn default_min_age_hours() -> u32 {
    24
}

fn default_encryption_enabled() -> bool {
    true
}

fn default_require_password() -> bool {
    true
}

fn default_include_database() -> bool {
    true
}

fn default_include_config() -> bool {
    true
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backup_dir: default_backup_dir(),
            schedule: BackupScheduleConfig::default(),
            retention: BackupRetentionConfig::default(),
            encryption: BackupEncryptionConfig::default(),
            include: BackupIncludeConfig::default(),
        }
    }
}

impl Default for BackupScheduleConfig {
    fn default() -> Self {
        Self {
            frequency: default_backup_frequency(),
            cron: None,
            time: default_backup_time(),
            day_of_week: 0,
        }
    }
}

impl Default for BackupRetentionConfig {
    fn default() -> Self {
        Self {
            max_backups: default_max_backups(),
            min_age_hours: default_min_age_hours(),
        }
    }
}

impl Default for BackupEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: default_encryption_enabled(),
            require_password: default_require_password(),
        }
    }
}

impl Default for BackupIncludeConfig {
    fn default() -> Self {
        Self {
            database: default_include_database(),
            config_files: default_include_config(),
        }
    }
}

impl Default for BackupFrequency {
    fn default() -> Self {
        BackupFrequency::Daily
    }
}

// ============================================================================
// Node Removal Configuration
// ============================================================================

/// Node removal tracking configuration
///
/// Configures automatic tracking and removal of nodes with revoked or missing certificates.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeRemovalConfig {
    /// Whether the node removal tracking feature is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Number of days to wait before removing a node (default: 10)
    #[serde(default = "default_node_removal_retention_days")]
    pub retention_days: i64,
    /// How often to check for certificate status changes (in seconds, default: 300 = 5 minutes)
    #[serde(default)]
    pub check_interval_secs: Option<u64>,
    /// How long to keep audit log entries (in days, default: 90)
    #[serde(default)]
    pub audit_retention_days: Option<i64>,
}

fn default_node_removal_retention_days() -> i64 {
    10
}

impl Default for NodeRemovalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: default_node_removal_retention_days(),
            check_interval_secs: Some(300),
            audit_retention_days: Some(90),
        }
    }
}

// ============================================================================
// Node Bootstrap Configuration
// ============================================================================

/// Node bootstrap configuration for adding new agents
///
/// Configures the bootstrap script that new nodes can download to
/// automatically install and configure the OpenVox agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeBootstrapConfig {
    /// OpenVox Server URL (e.g., "openvox.example.com" or "openvox.example.com:8140")
    /// This is the server address that agents will connect to
    #[serde(default, alias = "puppet_server_url")]
    pub openvox_server_url: Option<String>,
    /// Custom repository base URL for OpenVox/Puppet packages
    /// For YUM: e.g., "https://yum.example.com/openvox"
    /// For APT: e.g., "https://apt.example.com/openvox"
    #[serde(default)]
    pub repository_base_url: Option<String>,
    /// Package name to install (default: "openvox-agent")
    #[serde(default = "default_agent_package_name")]
    pub agent_package_name: String,
}

fn default_agent_package_name() -> String {
    "openvox-agent".to_string()
}

/// Classification endpoint configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ClassificationConfig {
    /// Shared key for alternative authentication to the /classify endpoint
    /// This allows debugging without requiring client certificates
    #[serde(default)]
    pub shared_key: Option<String>,
}

impl Default for NodeBootstrapConfig {
    fn default() -> Self {
        Self {
            openvox_server_url: None,
            repository_base_url: None,
            agent_package_name: default_agent_package_name(),
        }
    }
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
        serde_norway::from_str(&contents)
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
            target: default_log_target(),
            log_dir: default_log_dir(),
            log_prefix: default_log_prefix(),
            daily_rotation: default_log_rotation(),
            max_log_files: default_max_log_files(),
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
                tls: None,
                static_dir: default_static_dir(),
                serve_frontend: default_serve_frontend(),
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
            code_deploy: None,
            saml: None,
            backup: None,
            node_removal: None,
            node_bootstrap: None,
            classification: None,
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

        let mut config = if let Some(ref path) = config_path {
            if path.exists() {
                eprintln!("[CONFIG] Loading configuration from: {:?}", path);
                let contents = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read config file: {:?}", path))?;
                let parsed: AppConfig = serde_norway::from_str(&contents)
                    .with_context(|| format!("Failed to parse config file: {:?}", path))?;

                // Debug: Log SAML config right after parsing
                if let Some(ref saml) = parsed.saml {
                    eprintln!("[CONFIG] SAML section found in config file:");
                    eprintln!("[CONFIG]   enabled: {}", saml.enabled);
                    eprintln!("[CONFIG]   sp.entity_id: {}", saml.sp.entity_id);
                    eprintln!("[CONFIG]   sp.acs_url: {}", saml.sp.acs_url);
                    eprintln!("[CONFIG]   idp.metadata_url: {:?}", saml.idp.metadata_url);
                    eprintln!("[CONFIG]   idp.metadata_file: {:?}", saml.idp.metadata_file);
                    eprintln!("[CONFIG]   idp.sso_url: {:?}", saml.idp.sso_url);
                    eprintln!("[CONFIG]   idp.entity_id: {:?}", saml.idp.entity_id);
                } else {
                    eprintln!("[CONFIG] No SAML section found in config file");
                }

                parsed
            } else {
                eprintln!("[CONFIG] Config file path exists but file not found: {:?}", path);
                AppConfig::default()
            }
        } else {
            eprintln!("[CONFIG] No config file found, using defaults");
            AppConfig::default()
        };

        // Apply environment variable overrides
        config.apply_env_overrides();

        // Debug: Log final SAML config after env overrides
        if let Some(ref saml) = config.saml {
            eprintln!("[CONFIG] Final SAML config after env overrides:");
            eprintln!("[CONFIG]   enabled: {}", saml.enabled);
            eprintln!("[CONFIG]   sp.entity_id: {}", saml.sp.entity_id);
            eprintln!("[CONFIG]   sp.acs_url: {}", saml.sp.acs_url);
            eprintln!("[CONFIG]   idp.metadata_url: {:?}", saml.idp.metadata_url);
            eprintln!("[CONFIG]   idp.metadata_file: {:?}", saml.idp.metadata_file);
            eprintln!("[CONFIG]   idp.sso_url: {:?}", saml.idp.sso_url);
            eprintln!("[CONFIG]   idp.entity_id: {:?}", saml.idp.entity_id);
        } else {
            eprintln!("[CONFIG] No SAML section in final config");
        }

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
                ssl: None,
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

        // Puppet CA overrides
        if let Ok(url) = std::env::var("PUPPET_CA_URL") {
            let puppet_ca = self.puppet_ca.get_or_insert_with(|| PuppetCAConfig {
                url: url.clone(),
                timeout_secs: default_timeout(),
                ssl_verify: default_ssl_verify(),
                ssl_cert: None,
                ssl_key: None,
                ssl_ca: None,
                ssl: None,
            });
            puppet_ca.url = url;
        }

        // Puppet CA SSL certificate overrides
        if let Ok(cert) = std::env::var("PUPPET_CA_SSL_CERT") {
            if let Some(ref mut puppet_ca) = self.puppet_ca {
                puppet_ca.ssl_cert = Some(PathBuf::from(cert));
            }
        }
        if let Ok(key) = std::env::var("PUPPET_CA_SSL_KEY") {
            if let Some(ref mut puppet_ca) = self.puppet_ca {
                puppet_ca.ssl_key = Some(PathBuf::from(key));
            }
        }
        if let Ok(ca) = std::env::var("PUPPET_CA_SSL_CA") {
            if let Some(ref mut puppet_ca) = self.puppet_ca {
                puppet_ca.ssl_ca = Some(PathBuf::from(ca));
            }
        }

        // Server TLS overrides
        if let Ok(cert) = std::env::var("OPENVOX_TLS_CERT") {
            let key = std::env::var("OPENVOX_TLS_KEY").unwrap_or_default();
            if !key.is_empty() {
                self.server.tls = Some(TlsConfig {
                    cert_file: PathBuf::from(cert),
                    key_file: PathBuf::from(key),
                    min_version: std::env::var("OPENVOX_TLS_MIN_VERSION")
                        .unwrap_or_else(|_| default_min_tls_version()),
                    ciphers: Vec::new(),
                });
            }
        }

        // Static directory override
        if let Ok(dir) = std::env::var("OPENVOX_STATIC_DIR") {
            self.server.static_dir = Some(PathBuf::from(dir));
        }

        // Serve frontend override
        if let Ok(serve) = std::env::var("OPENVOX_SERVE_FRONTEND") {
            self.server.serve_frontend = serve.parse().unwrap_or(true);
        }

        // Logging target override
        if let Ok(target) = std::env::var("OPENVOX_LOG_TARGET") {
            self.logging.target = match target.to_lowercase().as_str() {
                "file" => LogTarget::File,
                "both" => LogTarget::Both,
                _ => LogTarget::Console,
            };
        }
        // Log directory override
        if let Ok(dir) = std::env::var("OPENVOX_LOG_DIR") {
            self.logging.log_dir = PathBuf::from(dir);
        }
        // Log prefix override
        if let Ok(prefix) = std::env::var("OPENVOX_LOG_PREFIX") {
            self.logging.log_prefix = prefix;
        }
        // Log rotation override
        if let Ok(rotation) = std::env::var("OPENVOX_LOG_ROTATION") {
            self.logging.daily_rotation = rotation.parse().unwrap_or(true);
        }
        // Max log files override
        if let Ok(max_files) = std::env::var("OPENVOX_LOG_MAX_FILES") {
            if let Ok(n) = max_files.parse() {
                self.logging.max_log_files = n;
            }
        }

        // Code Deploy overrides
        if let Ok(enabled) = std::env::var("CODE_DEPLOY_ENABLED") {
            if enabled.to_lowercase() == "true" || enabled == "1" {
                let code_deploy = self.code_deploy.get_or_insert_with(CodeDeployYamlConfig::default);
                code_deploy.enabled = true;
            }
        }
        if let Ok(key) = std::env::var("CODE_DEPLOY_ENCRYPTION_KEY") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.encryption_key = key;
            }
        }
        if let Ok(url) = std::env::var("CODE_DEPLOY_WEBHOOK_BASE_URL") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.webhook_base_url = Some(url);
            }
        }
        if let Ok(dir) = std::env::var("CODE_DEPLOY_REPOS_BASE_DIR") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.repos_base_dir = PathBuf::from(dir);
            }
        }
        if let Ok(dir) = std::env::var("CODE_DEPLOY_SSH_KEYS_DIR") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.ssh_keys_dir = PathBuf::from(dir);
            }
        }
        if let Ok(path) = std::env::var("CODE_DEPLOY_R10K_BINARY_PATH") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.r10k_binary_path = PathBuf::from(path);
            }
        }
        if let Ok(path) = std::env::var("CODE_DEPLOY_R10K_CONFIG_PATH") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.r10k_config_path = PathBuf::from(path);
            }
        }
        if let Ok(dir) = std::env::var("CODE_DEPLOY_ENVIRONMENTS_BASEDIR") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.environments_basedir = PathBuf::from(dir);
            }
        }
        if let Ok(dir) = std::env::var("CODE_DEPLOY_R10K_CACHEDIR") {
            if let Some(ref mut code_deploy) = self.code_deploy {
                code_deploy.r10k_cachedir = PathBuf::from(dir);
            }
        }
        if let Ok(days) = std::env::var("CODE_DEPLOY_RETAIN_HISTORY_DAYS") {
            if let Ok(n) = days.parse() {
                if let Some(ref mut code_deploy) = self.code_deploy {
                    code_deploy.retain_history_days = n;
                }
            }
        }

        // SAML overrides - create SAML config from env vars if SAML_ENABLED is true
        if let Ok(enabled) = std::env::var("SAML_ENABLED") {
            if enabled.to_lowercase() == "true" || enabled == "1" {
                // Create default SAML config if it doesn't exist
                let saml = self.saml.get_or_insert_with(|| SamlConfig {
                    enabled: true,
                    sp: SamlSpConfig {
                        entity_id: String::new(),
                        acs_url: String::new(),
                        slo_url: None,
                        certificate_file: None,
                        private_key_file: None,
                        sign_requests: default_sign_requests(),
                        require_signed_assertions: default_require_signed_assertions(),
                        require_encrypted_assertions: false,
                    },
                    idp: SamlIdpConfig::default(),
                    user_mapping: SamlUserMappingConfig::default(),
                    session: SamlSessionConfig::default(),
                });
                saml.enabled = true;
            }
        }
        if let Ok(entity_id) = std::env::var("SAML_SP_ENTITY_ID") {
            if let Some(ref mut saml) = self.saml {
                saml.sp.entity_id = entity_id;
            }
        }
        if let Ok(acs_url) = std::env::var("SAML_SP_ACS_URL") {
            if let Some(ref mut saml) = self.saml {
                saml.sp.acs_url = acs_url;
            }
        }
        if let Ok(cert) = std::env::var("SAML_SP_CERTIFICATE_FILE") {
            if let Some(ref mut saml) = self.saml {
                saml.sp.certificate_file = Some(PathBuf::from(cert));
            }
        }
        if let Ok(key) = std::env::var("SAML_SP_PRIVATE_KEY_FILE") {
            if let Some(ref mut saml) = self.saml {
                saml.sp.private_key_file = Some(PathBuf::from(key));
            }
        }
        if let Ok(url) = std::env::var("SAML_IDP_METADATA_URL") {
            if let Some(ref mut saml) = self.saml {
                saml.idp.metadata_url = Some(url);
            }
        }
        if let Ok(path) = std::env::var("SAML_IDP_METADATA_FILE") {
            if let Some(ref mut saml) = self.saml {
                saml.idp.metadata_file = Some(PathBuf::from(path));
            }
        }
        if let Ok(entity_id) = std::env::var("SAML_IDP_ENTITY_ID") {
            if let Some(ref mut saml) = self.saml {
                saml.idp.entity_id = Some(entity_id);
            }
        }
        if let Ok(sso_url) = std::env::var("SAML_IDP_SSO_URL") {
            if let Some(ref mut saml) = self.saml {
                saml.idp.sso_url = Some(sso_url);
            }
        }

        // Backup configuration overrides
        if let Ok(enabled) = std::env::var("BACKUP_ENABLED") {
            if enabled.to_lowercase() == "true" || enabled == "1" {
                let backup = self.backup.get_or_insert_with(BackupConfig::default);
                backup.enabled = true;
            }
        }
        if let Ok(dir) = std::env::var("BACKUP_DIR") {
            let backup = self.backup.get_or_insert_with(BackupConfig::default);
            backup.backup_dir = PathBuf::from(dir);
        }
        if let Ok(freq) = std::env::var("BACKUP_FREQUENCY") {
            let backup = self.backup.get_or_insert_with(BackupConfig::default);
            backup.schedule.frequency = match freq.to_lowercase().as_str() {
                "hourly" => BackupFrequency::Hourly,
                "daily" => BackupFrequency::Daily,
                "weekly" => BackupFrequency::Weekly,
                "custom" => BackupFrequency::Custom,
                "disabled" => BackupFrequency::Disabled,
                _ => BackupFrequency::Daily,
            };
        }
        if let Ok(time) = std::env::var("BACKUP_TIME") {
            let backup = self.backup.get_or_insert_with(BackupConfig::default);
            backup.schedule.time = time;
        }
        if let Ok(cron) = std::env::var("BACKUP_CRON") {
            let backup = self.backup.get_or_insert_with(BackupConfig::default);
            backup.schedule.cron = Some(cron);
        }
        if let Ok(max) = std::env::var("BACKUP_MAX_BACKUPS") {
            if let Ok(n) = max.parse() {
                let backup = self.backup.get_or_insert_with(BackupConfig::default);
                backup.retention.max_backups = n;
            }
        }
        if let Ok(encryption) = std::env::var("BACKUP_ENCRYPTION_ENABLED") {
            let backup = self.backup.get_or_insert_with(BackupConfig::default);
            backup.encryption.enabled = encryption.to_lowercase() == "true" || encryption == "1";
        }

        // Node Bootstrap configuration overrides
        // Support both new name (OPENVOX_SERVER) and legacy name (PUPPET_SERVER) for backwards compatibility
        if let Ok(url) = std::env::var("NODE_BOOTSTRAP_OPENVOX_SERVER_URL")
            .or_else(|_| std::env::var("NODE_BOOTSTRAP_PUPPET_SERVER_URL"))
        {
            let bootstrap = self.node_bootstrap.get_or_insert_with(NodeBootstrapConfig::default);
            bootstrap.openvox_server_url = Some(url);
        }
        if let Ok(url) = std::env::var("NODE_BOOTSTRAP_REPOSITORY_BASE_URL") {
            let bootstrap = self.node_bootstrap.get_or_insert_with(NodeBootstrapConfig::default);
            bootstrap.repository_base_url = Some(url);
        }
        if let Ok(name) = std::env::var("NODE_BOOTSTRAP_AGENT_PACKAGE_NAME") {
            let bootstrap = self.node_bootstrap.get_or_insert_with(NodeBootstrapConfig::default);
            bootstrap.agent_package_name = name;
        }

        // Classification endpoint overrides
        if let Ok(key) = std::env::var("CLASSIFICATION_SHARED_KEY") {
            let classification = self.classification.get_or_insert_with(ClassificationConfig::default);
            classification.shared_key = Some(key);
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

        // Validate TLS configuration if present
        if let Some(ref tls) = self.server.tls {
            if !tls.cert_file.exists() {
                anyhow::bail!(
                    "TLS certificate file not found: {:?}",
                    tls.cert_file
                );
            }
            if !tls.key_file.exists() {
                anyhow::bail!(
                    "TLS key file not found: {:?}",
                    tls.key_file
                );
            }
            if tls.min_version != "1.2" && tls.min_version != "1.3" {
                anyhow::bail!(
                    "Invalid TLS minimum version: {}. Must be '1.2' or '1.3'",
                    tls.min_version
                );
            }
        }

        // Validate static directory if specified
        if let Some(ref static_dir) = self.server.static_dir {
            if !static_dir.exists() {
                tracing::warn!(
                    "Static directory does not exist: {:?}. Frontend will not be served.",
                    static_dir
                );
            }
        }

        Ok(())
    }

    /// Create a default configuration file
    pub fn create_default_config(path: &PathBuf) -> Result<()> {
        let config = AppConfig::default();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let yaml = serde_norway::to_string(&config)?;
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
        let yaml = serde_norway::to_string(&config).unwrap();
        let parsed: AppConfig = serde_norway::from_str(&yaml).unwrap();
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
        let config: AppConfig = serde_norway::from_str(yaml).unwrap();
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
        let config: AppConfig = serde_norway::from_str(yaml).unwrap();
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
        let config: AppConfig = serde_norway::from_str(yaml).unwrap();
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
        let config: AppConfig = serde_norway::from_str(yaml).unwrap();
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
        let groups_config: GroupsConfig = serde_norway::from_str(yaml).unwrap();
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
        let parsed: TestWidgets = serde_norway::from_str(yaml).unwrap();
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
        let yaml = serde_norway::to_string(&config).unwrap();
        let parsed: AppConfig = serde_norway::from_str(&yaml).unwrap();

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
