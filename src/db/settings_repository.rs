//! Settings repository - database operations for settings

use crate::models::{Setting, SmtpSettings, UpdateSmtpSettingsRequest};
use crate::utils::AppError;
use chrono::Utc;
use sqlx::{Pool, Sqlite};

pub struct SettingsRepository {
    pool: Pool<Sqlite>,
}

impl SettingsRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Get a setting by key
    pub async fn get_setting(&self, key: &str) -> Result<Option<Setting>, AppError> {
        let setting = sqlx::query_as::<_, Setting>(
            r#"
            SELECT key, value, description, created_at, updated_at
            FROM settings
            WHERE key = ?
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(setting)
    }

    /// Get all settings with a key prefix
    pub async fn get_settings_by_prefix(&self, prefix: &str) -> Result<Vec<Setting>, AppError> {
        let pattern = format!("{}%", prefix);
        let settings = sqlx::query_as::<_, Setting>(
            r#"
            SELECT key, value, description, created_at, updated_at
            FROM settings
            WHERE key LIKE ?
            ORDER BY key
            "#,
        )
        .bind(pattern)
        .fetch_all(&self.pool)
        .await?;

        Ok(settings)
    }

    /// Set or update a setting
    pub async fn set_setting(
        &self,
        key: &str,
        value: &str,
        description: Option<&str>,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO settings (key, value, description, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                description = COALESCE(excluded.description, description),
                updated_at = excluded.updated_at
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(description)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get SMTP settings
    pub async fn get_smtp_settings(&self) -> Result<SmtpSettings, AppError> {
        let settings = self.get_settings_by_prefix("smtp.").await?;
        let settings_vec: Vec<(String, String)> = settings
            .into_iter()
            .map(|s| (s.key, s.value))
            .collect();

        Ok(SmtpSettings::from_settings(&settings_vec))
    }

    /// Update SMTP settings
    pub async fn update_smtp_settings(
        &self,
        req: &UpdateSmtpSettingsRequest,
    ) -> Result<SmtpSettings, AppError> {
        let smtp_settings = SmtpSettings {
            host: req.host.clone(),
            port: req.port,
            username: req.username.clone(),
            password: req.password.clone(),
            from_address: req.from_address.clone(),
            use_tls: req.use_tls,
            configured: !req.host.is_empty() && !req.from_address.is_empty(),
        };

        // Save all settings
        for (key, value) in smtp_settings.to_settings() {
            self.set_setting(&key, &value, None).await?;
        }

        Ok(smtp_settings)
    }
}
