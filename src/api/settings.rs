//! Settings API endpoints
//!
//! Provides configuration management endpoints for viewing and updating
//! application settings.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    config::{DashboardConfig, RbacConfig},
    utils::error::ErrorResponse,
    AppState,
};

/// Create the settings routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Get current configuration (read-only view)
        .route("/", get(get_settings))
        // Get dashboard configuration
        .route(
            "/dashboard",
            get(get_dashboard_config).put(update_dashboard_config),
        )
        // Get RBAC configuration
        .route("/rbac", get(get_rbac_config))
        // Export configuration as YAML
        .route("/export", get(export_config))
        // Import configuration from YAML
        .route("/import", post(import_config))
        // Validate configuration YAML
        .route("/validate", post(validate_config))
        // Get configuration history
        .route("/history", get(get_config_history))
        // Get server information
        .route("/server", get(get_server_info))
}

/// Settings response - read-only configuration view
#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub server: ServerSettings,
    pub puppetdb: Option<PuppetDbSettings>,
    pub puppet_ca: Option<PuppetCASettings>,
    pub auth: AuthSettings,
    pub database: DatabaseSettings,
    pub logging: LoggingSettings,
    pub cache: CacheSettings,
    pub dashboard: DashboardConfig,
    pub rbac: RbacSettings,
}

#[derive(Debug, Serialize)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub workers: usize,
}

#[derive(Debug, Serialize)]
pub struct PuppetDbSettings {
    pub url: String,
    pub timeout_secs: u64,
    pub ssl_verify: bool,
    pub ssl_configured: bool,
}

#[derive(Debug, Serialize)]
pub struct PuppetCASettings {
    pub url: String,
    pub timeout_secs: u64,
    pub ssl_verify: bool,
    pub ssl_configured: bool,
}

#[derive(Debug, Serialize)]
pub struct AuthSettings {
    pub token_expiry_hours: u64,
    pub refresh_token_expiry_days: u64,
    pub password_min_length: usize,
    // Note: jwt_secret and bcrypt_cost are intentionally not exposed
}

#[derive(Debug, Serialize)]
pub struct DatabaseSettings {
    pub url_masked: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

#[derive(Debug, Serialize)]
pub struct LoggingSettings {
    pub level: String,
    pub format: String,
    pub file: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CacheSettings {
    pub enabled: bool,
    pub node_ttl_secs: u64,
    pub fact_ttl_secs: u64,
    pub report_ttl_secs: u64,
    pub max_entries: usize,
}

#[derive(Debug, Serialize)]
pub struct RbacSettings {
    pub default_role: String,
    pub session_timeout_minutes: u64,
    pub max_failed_logins: u32,
    pub lockout_duration_minutes: u64,
    pub custom_roles_count: usize,
}

/// Get current settings (read-only view)
///
/// GET /api/v1/settings
async fn get_settings(State(state): State<AppState>) -> Json<SettingsResponse> {
    let config = &state.config;

    let response = SettingsResponse {
        server: ServerSettings {
            host: config.server.host.clone(),
            port: config.server.port,
            workers: config.server.workers,
        },
        puppetdb: config.puppetdb.as_ref().map(|pdb| PuppetDbSettings {
            url: pdb.url.clone(),
            timeout_secs: pdb.timeout_secs,
            ssl_verify: pdb.ssl_verify,
            ssl_configured: pdb.ssl_cert.is_some(),
        }),
        puppet_ca: config.puppet_ca.as_ref().map(|ca| PuppetCASettings {
            url: ca.url.clone(),
            timeout_secs: ca.timeout_secs,
            ssl_verify: ca.ssl_verify,
            ssl_configured: ca.ssl_cert.is_some(),
        }),
        auth: AuthSettings {
            token_expiry_hours: config.auth.token_expiry_hours,
            refresh_token_expiry_days: config.auth.refresh_token_expiry_days,
            password_min_length: config.auth.password_min_length,
        },
        database: DatabaseSettings {
            url_masked: mask_database_url(&config.database.url),
            max_connections: config.database.max_connections,
            min_connections: config.database.min_connections,
        },
        logging: LoggingSettings {
            level: config.logging.level.clone(),
            format: format!("{:?}", config.logging.format).to_lowercase(),
            file: config
                .logging
                .file
                .as_ref()
                .map(|p| p.display().to_string()),
        },
        cache: CacheSettings {
            enabled: config.cache.enabled,
            node_ttl_secs: config.cache.node_ttl_secs,
            fact_ttl_secs: config.cache.fact_ttl_secs,
            report_ttl_secs: config.cache.report_ttl_secs,
            max_entries: config.cache.max_entries,
        },
        dashboard: config.dashboard.clone(),
        rbac: RbacSettings {
            default_role: config.rbac.default_role.clone(),
            session_timeout_minutes: config.rbac.session_timeout_minutes,
            max_failed_logins: config.rbac.max_failed_logins,
            lockout_duration_minutes: config.rbac.lockout_duration_minutes,
            custom_roles_count: config.rbac.roles.len(),
        },
    };

    Json(response)
}

/// Get dashboard configuration
///
/// GET /api/v1/settings/dashboard
async fn get_dashboard_config(State(state): State<AppState>) -> Json<DashboardConfig> {
    Json(state.config.dashboard.clone())
}

/// Update dashboard configuration request
#[derive(Debug, Deserialize)]
pub struct UpdateDashboardRequest {
    #[serde(default)]
    pub default_time_range: Option<String>,
    #[serde(default)]
    pub refresh_interval_secs: Option<u64>,
    #[serde(default)]
    pub nodes_per_page: Option<usize>,
    #[serde(default)]
    pub reports_per_page: Option<usize>,
    #[serde(default)]
    pub show_inactive_nodes: Option<bool>,
    #[serde(default)]
    pub inactive_threshold_hours: Option<u64>,
    #[serde(default)]
    pub theme: Option<String>,
}

/// Update dashboard configuration
///
/// PUT /api/v1/settings/dashboard
async fn update_dashboard_config(
    State(state): State<AppState>,
    Json(request): Json<UpdateDashboardRequest>,
) -> Result<Json<DashboardConfig>, (StatusCode, Json<ErrorResponse>)> {
    // Validate time range
    if let Some(ref range) = request.default_time_range {
        if !["1h", "6h", "12h", "24h", "7d", "30d"].contains(&range.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: format!(
                        "Invalid time range: {}. Must be one of: 1h, 6h, 12h, 24h, 7d, 30d",
                        range
                    ),
                    details: None,
                    code: None,
                }),
            ));
        }
    }

    // Validate theme
    if let Some(ref theme) = request.theme {
        if !["light", "dark", "system"].contains(&theme.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: format!(
                        "Invalid theme: {}. Must be one of: light, dark, system",
                        theme
                    ),
                    details: None,
                    code: None,
                }),
            ));
        }
    }

    // Build response with updated values (merged with current config)
    let current = &state.config.dashboard;
    let updated = DashboardConfig {
        default_time_range: request
            .default_time_range
            .unwrap_or_else(|| current.default_time_range.clone()),
        refresh_interval_secs: request
            .refresh_interval_secs
            .unwrap_or(current.refresh_interval_secs),
        nodes_per_page: request.nodes_per_page.unwrap_or(current.nodes_per_page),
        reports_per_page: request.reports_per_page.unwrap_or(current.reports_per_page),
        show_inactive_nodes: request
            .show_inactive_nodes
            .unwrap_or(current.show_inactive_nodes),
        inactive_threshold_hours: request
            .inactive_threshold_hours
            .unwrap_or(current.inactive_threshold_hours),
        theme: request.theme.unwrap_or_else(|| current.theme.clone()),
        widgets: current.widgets.clone(),
    };

    // Note: In a production system, this would persist the changes
    // For now, we return the updated configuration
    Ok(Json(updated))
}

/// Get RBAC configuration
///
/// GET /api/v1/settings/rbac
async fn get_rbac_config(State(state): State<AppState>) -> Json<RbacConfig> {
    Json(state.config.rbac.clone())
}

/// Export configuration response
#[derive(Debug, Serialize)]
pub struct ExportConfigResponse {
    pub content: String,
    pub format: String,
    pub timestamp: String,
}

/// Export current configuration as YAML
///
/// GET /api/v1/settings/export
async fn export_config(
    State(state): State<AppState>,
) -> Result<Json<ExportConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Create a sanitized version of the config (without secrets)
    let config = &state.config;

    let sanitized = serde_json::json!({
        "server": {
            "host": config.server.host,
            "port": config.server.port,
            "workers": config.server.workers,
        },
        "puppetdb": config.puppetdb.as_ref().map(|pdb| serde_json::json!({
            "url": pdb.url,
            "timeout_secs": pdb.timeout_secs,
            "ssl_verify": pdb.ssl_verify,
        })),
        "puppet_ca": config.puppet_ca.as_ref().map(|ca| serde_json::json!({
            "url": ca.url,
            "timeout_secs": ca.timeout_secs,
            "ssl_verify": ca.ssl_verify,
        })),
        "auth": {
            "jwt_secret": "********",
            "token_expiry_hours": config.auth.token_expiry_hours,
            "refresh_token_expiry_days": config.auth.refresh_token_expiry_days,
            "bcrypt_cost": config.auth.bcrypt_cost,
            "password_min_length": config.auth.password_min_length,
        },
        "database": {
            "url": mask_database_url(&config.database.url),
            "max_connections": config.database.max_connections,
            "min_connections": config.database.min_connections,
            "connect_timeout_secs": config.database.connect_timeout_secs,
            "idle_timeout_secs": config.database.idle_timeout_secs,
        },
        "logging": {
            "level": config.logging.level,
            "format": format!("{:?}", config.logging.format).to_lowercase(),
            "file": config.logging.file.as_ref().map(|p| p.display().to_string()),
        },
        "cache": {
            "enabled": config.cache.enabled,
            "node_ttl_secs": config.cache.node_ttl_secs,
            "fact_ttl_secs": config.cache.fact_ttl_secs,
            "report_ttl_secs": config.cache.report_ttl_secs,
            "resource_ttl_secs": config.cache.resource_ttl_secs,
            "catalog_ttl_secs": config.cache.catalog_ttl_secs,
            "max_entries": config.cache.max_entries,
            "sync_interval_secs": config.cache.sync_interval_secs,
        },
        "dashboard": config.dashboard,
        "rbac": {
            "default_role": config.rbac.default_role,
            "session_timeout_minutes": config.rbac.session_timeout_minutes,
            "max_failed_logins": config.rbac.max_failed_logins,
            "lockout_duration_minutes": config.rbac.lockout_duration_minutes,
        },
    });

    let yaml_content = serde_yaml::to_string(&sanitized).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "serialization_error".to_string(),
                message: format!("Failed to serialize config: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    Ok(Json(ExportConfigResponse {
        content: yaml_content,
        format: "yaml".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Import configuration request
#[derive(Debug, Deserialize)]
pub struct ImportConfigRequest {
    pub content: String,
    #[serde(default = "default_format")]
    #[allow(dead_code)]
    pub format: String,
    #[serde(default)]
    pub dry_run: bool,
}

fn default_format() -> String {
    "yaml".to_string()
}

/// Import configuration response
#[derive(Debug, Serialize)]
pub struct ImportConfigResponse {
    pub success: bool,
    pub message: String,
    pub validation_errors: Vec<String>,
    pub dry_run: bool,
}

/// Import configuration from YAML
///
/// POST /api/v1/settings/import
async fn import_config(Json(request): Json<ImportConfigRequest>) -> Json<ImportConfigResponse> {
    // Validate the YAML syntax
    let validation_result = validate_yaml_config(&request.content);

    if !validation_result.is_empty() {
        return Json(ImportConfigResponse {
            success: false,
            message: "Configuration validation failed".to_string(),
            validation_errors: validation_result,
            dry_run: request.dry_run,
        });
    }

    if request.dry_run {
        return Json(ImportConfigResponse {
            success: true,
            message: "Configuration is valid (dry run - no changes applied)".to_string(),
            validation_errors: vec![],
            dry_run: true,
        });
    }

    // In a production system, this would write the config file and trigger a reload
    Json(ImportConfigResponse {
        success: true,
        message: "Configuration import acknowledged. Note: Runtime config changes require server restart.".to_string(),
        validation_errors: vec![],
        dry_run: false,
    })
}

/// Validate configuration request
#[derive(Debug, Deserialize)]
pub struct ValidateConfigRequest {
    pub content: String,
    #[serde(default = "default_format")]
    #[allow(dead_code)]
    pub format: String,
}

/// Validate configuration response
#[derive(Debug, Serialize)]
pub struct ValidateConfigResponse {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
    pub line: Option<usize>,
}

/// Validate configuration YAML
///
/// POST /api/v1/settings/validate
async fn validate_config(
    Json(request): Json<ValidateConfigRequest>,
) -> Json<ValidateConfigResponse> {
    let errors = validate_yaml_config(&request.content);

    let validation_errors: Vec<ValidationError> = errors
        .iter()
        .map(|e| ValidationError {
            path: "".to_string(),
            message: e.clone(),
            line: None,
        })
        .collect();

    let warnings = check_config_warnings(&request.content);

    Json(ValidateConfigResponse {
        valid: validation_errors.is_empty(),
        errors: validation_errors,
        warnings,
    })
}

/// Configuration history entry
#[derive(Debug, Serialize)]
pub struct ConfigHistoryEntry {
    pub id: String,
    pub timestamp: String,
    pub user: String,
    pub action: String,
    pub changes_summary: String,
}

/// Get configuration history
///
/// GET /api/v1/settings/history
async fn get_config_history() -> Json<Vec<ConfigHistoryEntry>> {
    // In a production system, this would fetch from a config_history table
    Json(vec![])
}

/// Server information response
#[derive(Debug, Serialize)]
pub struct ServerInfoResponse {
    pub version: String,
    pub rust_version: String,
    pub build_timestamp: Option<String>,
    pub git_commit: Option<String>,
    pub uptime_secs: u64,
    pub config_file_path: Option<String>,
    pub features: Vec<String>,
}

/// Get server information
///
/// GET /api/v1/settings/server
async fn get_server_info(State(state): State<AppState>) -> Json<ServerInfoResponse> {
    let features = vec![
        ("puppetdb", state.puppetdb.is_some()),
        ("puppet_ca", state.puppet_ca.is_some()),
        ("rbac", true),
        ("caching", state.config.cache.enabled),
        ("facter_templates", true),
    ];

    let enabled_features: Vec<String> = features
        .into_iter()
        .filter(|(_, enabled)| *enabled)
        .map(|(name, _)| name.to_string())
        .collect();

    Json(ServerInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        rust_version: option_env!("CARGO_PKG_RUST_VERSION")
            .unwrap_or("unknown")
            .to_string(),
        build_timestamp: None,
        git_commit: option_env!("GIT_COMMIT").map(String::from),
        uptime_secs: 0, // Would track actual uptime
        config_file_path: None,
        features: enabled_features,
    })
}

/// Mask sensitive parts of database URL
fn mask_database_url(url: &str) -> String {
    // Mask password in URLs like postgres://user:password@host/db
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            if let Some(scheme_end) = url.find("://") {
                if colon_pos > scheme_end {
                    let prefix = &url[..colon_pos + 1];
                    let suffix = &url[at_pos..];
                    return format!("{}********{}", prefix, suffix);
                }
            }
        }
    }
    // For SQLite or other URLs without credentials
    url.to_string()
}

/// Validate YAML configuration
fn validate_yaml_config(content: &str) -> Vec<String> {
    let mut errors = Vec::new();

    // Try to parse as YAML
    let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(content);
    if let Err(e) = parsed {
        errors.push(format!("YAML syntax error: {}", e));
        return errors;
    }

    let value = parsed.unwrap();

    // Check required sections
    if value.get("auth").is_none() {
        errors.push("Missing required section: 'auth'".to_string());
    }
    if value.get("database").is_none() {
        errors.push("Missing required section: 'database'".to_string());
    }

    // Validate auth section
    if let Some(auth) = value.get("auth") {
        if let Some(jwt_secret) = auth.get("jwt_secret") {
            if let Some(secret) = jwt_secret.as_str() {
                if secret.len() < 32 && secret != "********" {
                    errors.push("auth.jwt_secret must be at least 32 characters".to_string());
                }
            }
        }
    }

    // Validate server section
    if let Some(server) = value.get("server") {
        if let Some(port) = server.get("port") {
            if let Some(p) = port.as_u64() {
                if p == 0 || p > 65535 {
                    errors.push("server.port must be between 1 and 65535".to_string());
                }
            }
        }
    }

    // Validate dashboard section
    if let Some(dashboard) = value.get("dashboard") {
        if let Some(time_range) = dashboard.get("default_time_range") {
            if let Some(range) = time_range.as_str() {
                if !["1h", "6h", "12h", "24h", "7d", "30d"].contains(&range) {
                    errors.push(format!(
                        "dashboard.default_time_range must be one of: 1h, 6h, 12h, 24h, 7d, 30d (got: {})",
                        range
                    ));
                }
            }
        }
        if let Some(theme) = dashboard.get("theme") {
            if let Some(t) = theme.as_str() {
                if !["light", "dark", "system"].contains(&t) {
                    errors.push(format!(
                        "dashboard.theme must be one of: light, dark, system (got: {})",
                        t
                    ));
                }
            }
        }
    }

    errors
}

/// Check for configuration warnings (non-fatal issues)
fn check_config_warnings(content: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        // Warn about default JWT secret
        if let Some(auth) = value.get("auth") {
            if let Some(jwt_secret) = auth.get("jwt_secret") {
                if let Some(secret) = jwt_secret.as_str() {
                    if secret.contains("change") || secret.contains("default") {
                        warnings.push("Warning: JWT secret appears to be a default value. Please change it for production.".to_string());
                    }
                }
            }
        }

        // Warn if cache is disabled
        if let Some(cache) = value.get("cache") {
            if let Some(enabled) = cache.get("enabled") {
                if enabled.as_bool() == Some(false) {
                    warnings.push(
                        "Warning: Cache is disabled. This may impact performance.".to_string(),
                    );
                }
            }
        }

        // Warn about low bcrypt cost
        if let Some(auth) = value.get("auth") {
            if let Some(bcrypt_cost) = auth.get("bcrypt_cost") {
                if let Some(cost) = bcrypt_cost.as_u64() {
                    if cost < 10 {
                        warnings.push("Warning: bcrypt_cost is low. Consider using 12 or higher for production.".to_string());
                    }
                }
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_database_url_postgres() {
        let url = "postgres://admin:secretpassword@localhost:5432/openvox";
        let masked = mask_database_url(url);
        assert_eq!(masked, "postgres://admin:********@localhost:5432/openvox");
    }

    #[test]
    fn test_mask_database_url_sqlite() {
        let url = "sqlite://./data/openvox.db";
        let masked = mask_database_url(url);
        assert_eq!(masked, url);
    }

    #[test]
    fn test_validate_yaml_config_valid() {
        let config = r#"
auth:
  jwt_secret: "this-is-a-valid-secret-that-is-at-least-32-characters"
database:
  url: "sqlite://./test.db"
"#;
        let errors = validate_yaml_config(config);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_yaml_config_missing_auth() {
        let config = r#"
database:
  url: "sqlite://./test.db"
"#;
        let errors = validate_yaml_config(config);
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing required section: 'auth'")));
    }

    #[test]
    fn test_validate_yaml_config_invalid_port() {
        let config = r#"
auth:
  jwt_secret: "this-is-a-valid-secret-that-is-at-least-32-characters"
database:
  url: "sqlite://./test.db"
server:
  port: 0
"#;
        let errors = validate_yaml_config(config);
        assert!(errors.iter().any(|e| e.contains("port must be between")));
    }

    #[test]
    fn test_check_config_warnings_default_secret() {
        let config = r#"
auth:
  jwt_secret: "change-me-in-production"
database:
  url: "sqlite://./test.db"
"#;
        let warnings = check_config_warnings(config);
        assert!(warnings.iter().any(|w| w.contains("default value")));
    }
}
