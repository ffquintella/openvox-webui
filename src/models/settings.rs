//! Settings models

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Application setting (key-value)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// SMTP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
    pub use_tls: bool,
    pub configured: bool,
}

impl SmtpSettings {
    /// Create from settings key-value pairs
    pub fn from_settings(settings: &[(String, String)]) -> Self {
        let get_value = |key: &str| -> Option<String> {
            settings
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };

        Self {
            host: get_value("smtp.host").unwrap_or_default(),
            port: get_value("smtp.port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(587),
            username: get_value("smtp.username").filter(|s| !s.is_empty()),
            password: get_value("smtp.password").filter(|s| !s.is_empty()),
            from_address: get_value("smtp.from_address").unwrap_or_default(),
            use_tls: get_value("smtp.use_tls")
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
            configured: get_value("smtp.configured")
                .and_then(|v| v.parse().ok())
                .unwrap_or(false),
        }
    }

    /// Convert to settings key-value pairs for storage
    pub fn to_settings(&self) -> Vec<(String, String)> {
        vec![
            ("smtp.host".to_string(), self.host.clone()),
            ("smtp.port".to_string(), self.port.to_string()),
            (
                "smtp.username".to_string(),
                self.username.clone().unwrap_or_default(),
            ),
            (
                "smtp.password".to_string(),
                self.password.clone().unwrap_or_default(),
            ),
            ("smtp.from_address".to_string(), self.from_address.clone()),
            ("smtp.use_tls".to_string(), self.use_tls.to_string()),
            ("smtp.configured".to_string(), self.configured.to_string()),
        ]
    }
}

/// Request to update SMTP settings
#[derive(Debug, Deserialize)]
pub struct UpdateSmtpSettingsRequest {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
    pub use_tls: bool,
}
