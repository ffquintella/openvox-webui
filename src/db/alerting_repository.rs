//! Repository pattern implementations for alerting database access

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{
    Alert, AlertRule, AlertRuleType, AlertSeverity, AlertSeverityCount, AlertSilence, AlertStats,
    AlertStatus, ChannelType, ConditionOperator, CreateAlertRuleRequest, CreateChannelRequest,
    CreateSilenceRequest, NotificationChannel, NotificationHistory, NotificationStatus,
    UpdateAlertRuleRequest, UpdateChannelRequest,
};

// ============================================================================
// Notification Channel Repository
// ============================================================================

/// Row returned from notification_channels table
#[derive(Debug, sqlx::FromRow)]
struct ChannelRow {
    id: String,
    name: String,
    channel_type: String,
    config: String,
    is_enabled: bool,
    created_by: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Repository for notification channel operations
pub struct NotificationChannelRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> NotificationChannelRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all notification channels
    pub async fn get_all(&self) -> Result<Vec<NotificationChannel>> {
        let rows = sqlx::query_as::<_, ChannelRow>(
            r#"
            SELECT id, name, channel_type, config, is_enabled, created_by, created_at, updated_at
            FROM notification_channels
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch notification channels")?;

        Ok(rows.into_iter().map(row_to_channel).collect())
    }

    /// Get all enabled channels
    pub async fn get_enabled(&self) -> Result<Vec<NotificationChannel>> {
        let rows = sqlx::query_as::<_, ChannelRow>(
            r#"
            SELECT id, name, channel_type, config, is_enabled, created_by, created_at, updated_at
            FROM notification_channels
            WHERE is_enabled = TRUE
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch enabled channels")?;

        Ok(rows.into_iter().map(row_to_channel).collect())
    }

    /// Get a channel by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<NotificationChannel>> {
        let row = sqlx::query_as::<_, ChannelRow>(
            r#"
            SELECT id, name, channel_type, config, is_enabled, created_by, created_at, updated_at
            FROM notification_channels
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch notification channel")?;

        Ok(row.map(row_to_channel))
    }

    /// Create a new notification channel
    pub async fn create(
        &self,
        req: &CreateChannelRequest,
        user_id: Option<Uuid>,
    ) -> Result<NotificationChannel> {
        let id = Uuid::new_v4();
        let config_json = serde_json::to_string(&req.config).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            INSERT INTO notification_channels (id, name, channel_type, config, is_enabled, created_by)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(req.channel_type.as_str())
        .bind(&config_json)
        .bind(req.is_enabled)
        .bind(user_id.map(|u| u.to_string()))
        .execute(self.pool)
        .await
        .context("Failed to create notification channel")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created channel"))
    }

    /// Update a notification channel
    pub async fn update(
        &self,
        id: Uuid,
        req: &UpdateChannelRequest,
    ) -> Result<Option<NotificationChannel>> {
        let existing = self.get_by_id(id).await?;
        if existing.is_none() {
            return Ok(None);
        }
        let existing = existing.unwrap();

        let name = req.name.as_ref().unwrap_or(&existing.name);
        let config = req.config.as_ref().unwrap_or(&existing.config);
        let is_enabled = req.is_enabled.unwrap_or(existing.is_enabled);
        let config_json = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            UPDATE notification_channels
            SET name = ?, config = ?, is_enabled = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(name)
        .bind(&config_json)
        .bind(is_enabled)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update notification channel")?;

        self.get_by_id(id).await
    }

    /// Delete a notification channel
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM notification_channels WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete notification channel")?;

        Ok(result.rows_affected() > 0)
    }
}

fn row_to_channel(row: ChannelRow) -> NotificationChannel {
    NotificationChannel {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        channel_type: ChannelType::from_str(&row.channel_type).unwrap_or(ChannelType::Webhook),
        config: serde_json::from_str(&row.config)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        is_enabled: row.is_enabled,
        created_by: row.created_by.and_then(|s| Uuid::parse_str(&s).ok()),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Alert Rule Repository
// ============================================================================

/// Row returned from alert_rules table
#[derive(Debug, sqlx::FromRow)]
struct AlertRuleRow {
    id: String,
    name: String,
    description: Option<String>,
    rule_type: String,
    conditions: String,
    condition_operator: String,
    severity: String,
    cooldown_minutes: i32,
    is_enabled: bool,
    created_by: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Repository for alert rule operations
pub struct AlertRuleRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AlertRuleRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all alert rules
    pub async fn get_all(&self) -> Result<Vec<AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            r#"
            SELECT id, name, description, rule_type, conditions, condition_operator,
                   severity, cooldown_minutes, is_enabled, created_by, created_at, updated_at
            FROM alert_rules
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch alert rules")?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let rule = self.row_to_rule(row).await?;
            rules.push(rule);
        }
        Ok(rules)
    }

    /// Get all enabled alert rules
    pub async fn get_enabled(&self) -> Result<Vec<AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            r#"
            SELECT id, name, description, rule_type, conditions, condition_operator,
                   severity, cooldown_minutes, is_enabled, created_by, created_at, updated_at
            FROM alert_rules
            WHERE is_enabled = TRUE
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch enabled alert rules")?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let rule = self.row_to_rule(row).await?;
            rules.push(rule);
        }
        Ok(rules)
    }

    /// Get alert rules by type
    pub async fn get_by_type(&self, rule_type: AlertRuleType) -> Result<Vec<AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            r#"
            SELECT id, name, description, rule_type, conditions, condition_operator,
                   severity, cooldown_minutes, is_enabled, created_by, created_at, updated_at
            FROM alert_rules
            WHERE rule_type = ?
            ORDER BY name
            "#,
        )
        .bind(rule_type.as_str())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch alert rules")?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let rule = self.row_to_rule(row).await?;
            rules.push(rule);
        }
        Ok(rules)
    }

    /// Get an alert rule by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<AlertRule>> {
        let row = sqlx::query_as::<_, AlertRuleRow>(
            r#"
            SELECT id, name, description, rule_type, conditions, condition_operator,
                   severity, cooldown_minutes, is_enabled, created_by, created_at, updated_at
            FROM alert_rules
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch alert rule")?;

        match row {
            Some(r) => Ok(Some(self.row_to_rule(r).await?)),
            None => Ok(None),
        }
    }

    /// Create a new alert rule
    pub async fn create(
        &self,
        req: &CreateAlertRuleRequest,
        user_id: Option<Uuid>,
    ) -> Result<AlertRule> {
        let id = Uuid::new_v4();
        let conditions_json =
            serde_json::to_string(&req.conditions).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO alert_rules (id, name, description, rule_type, conditions, condition_operator,
                                     severity, cooldown_minutes, is_enabled, created_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(req.rule_type.as_str())
        .bind(&conditions_json)
        .bind(req.condition_operator.as_str())
        .bind(req.severity.as_str())
        .bind(req.cooldown_minutes)
        .bind(req.is_enabled)
        .bind(user_id.map(|u| u.to_string()))
        .execute(self.pool)
        .await
        .context("Failed to create alert rule")?;

        // Add channel associations
        for channel_id in &req.channel_ids {
            self.add_channel(id, *channel_id).await?;
        }

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created rule"))
    }

    /// Update an alert rule
    pub async fn update(
        &self,
        id: Uuid,
        req: &UpdateAlertRuleRequest,
    ) -> Result<Option<AlertRule>> {
        let existing = self.get_by_id(id).await?;
        if existing.is_none() {
            return Ok(None);
        }
        let existing = existing.unwrap();

        let name = req.name.as_ref().unwrap_or(&existing.name);
        let description = req.description.as_ref().or(existing.description.as_ref());
        let conditions = req.conditions.as_ref().unwrap_or(&existing.conditions);
        let condition_operator = req
            .condition_operator
            .unwrap_or(existing.condition_operator);
        let severity = req.severity.unwrap_or(existing.severity);
        let cooldown_minutes = req.cooldown_minutes.unwrap_or(existing.cooldown_minutes);
        let is_enabled = req.is_enabled.unwrap_or(existing.is_enabled);

        let conditions_json =
            serde_json::to_string(conditions).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            UPDATE alert_rules
            SET name = ?, description = ?, conditions = ?, condition_operator = ?,
                severity = ?, cooldown_minutes = ?, is_enabled = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(&conditions_json)
        .bind(condition_operator.as_str())
        .bind(severity.as_str())
        .bind(cooldown_minutes)
        .bind(is_enabled)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update alert rule")?;

        // Update channel associations if provided
        if let Some(channel_ids) = &req.channel_ids {
            // Remove existing associations
            sqlx::query("DELETE FROM alert_rule_channels WHERE rule_id = ?")
                .bind(id.to_string())
                .execute(self.pool)
                .await?;

            // Add new associations
            for channel_id in channel_ids {
                self.add_channel(id, *channel_id).await?;
            }
        }

        self.get_by_id(id).await
    }

    /// Delete an alert rule
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM alert_rules WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete alert rule")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get channels associated with a rule
    pub async fn get_channels(&self, rule_id: Uuid) -> Result<Vec<Uuid>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT channel_id FROM alert_rule_channels WHERE rule_id = ?")
                .bind(rule_id.to_string())
                .fetch_all(self.pool)
                .await
                .context("Failed to fetch rule channels")?;

        Ok(rows
            .into_iter()
            .filter_map(|(id,)| Uuid::parse_str(&id).ok())
            .collect())
    }

    /// Add a channel to a rule
    pub async fn add_channel(&self, rule_id: Uuid, channel_id: Uuid) -> Result<()> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO alert_rule_channels (id, rule_id, channel_id)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(rule_id.to_string())
        .bind(channel_id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to add channel to rule")?;

        Ok(())
    }

    /// Remove a channel from a rule
    pub async fn remove_channel(&self, rule_id: Uuid, channel_id: Uuid) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM alert_rule_channels WHERE rule_id = ? AND channel_id = ?")
                .bind(rule_id.to_string())
                .bind(channel_id.to_string())
                .execute(self.pool)
                .await
                .context("Failed to remove channel from rule")?;

        Ok(result.rows_affected() > 0)
    }

    async fn row_to_rule(&self, row: AlertRuleRow) -> Result<AlertRule> {
        let id = Uuid::parse_str(&row.id).context("Invalid rule ID")?;
        let channels = self.get_channels(id).await?;

        Ok(AlertRule {
            id,
            name: row.name,
            description: row.description,
            rule_type: AlertRuleType::from_str(&row.rule_type).unwrap_or(AlertRuleType::Custom),
            conditions: serde_json::from_str(&row.conditions).unwrap_or_default(),
            condition_operator: ConditionOperator::from_str(&row.condition_operator)
                .unwrap_or(ConditionOperator::All),
            severity: AlertSeverity::from_str(&row.severity).unwrap_or(AlertSeverity::Warning),
            cooldown_minutes: row.cooldown_minutes,
            is_enabled: row.is_enabled,
            created_by: row.created_by.and_then(|s| Uuid::parse_str(&s).ok()),
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            channels,
        })
    }
}

// ============================================================================
// Alert Repository
// ============================================================================

/// Row returned from alerts table
#[derive(Debug, sqlx::FromRow)]
struct AlertRow {
    id: String,
    rule_id: String,
    title: String,
    message: String,
    severity: String,
    context: Option<String>,
    status: String,
    acknowledged_by: Option<String>,
    acknowledged_at: Option<String>,
    resolved_at: Option<String>,
    triggered_at: String,
    last_notified_at: Option<String>,
}

/// Repository for alert operations
pub struct AlertRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AlertRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all alerts with optional filtering
    pub async fn get_all(
        &self,
        status: Option<AlertStatus>,
        severity: Option<AlertSeverity>,
        rule_id: Option<Uuid>,
        limit: Option<u32>,
    ) -> Result<Vec<Alert>> {
        let limit = limit.unwrap_or(100);
        let mut query = String::from(
            r#"
            SELECT id, rule_id, title, message, severity, context, status,
                   acknowledged_by, acknowledged_at, resolved_at, triggered_at, last_notified_at
            FROM alerts
            WHERE 1=1
            "#,
        );

        if status.is_some() {
            query.push_str(" AND status = ?");
        }
        if severity.is_some() {
            query.push_str(" AND severity = ?");
        }
        if rule_id.is_some() {
            query.push_str(" AND rule_id = ?");
        }

        query.push_str(" ORDER BY triggered_at DESC LIMIT ?");

        let mut q = sqlx::query_as::<_, AlertRow>(&query);

        if let Some(s) = status {
            q = q.bind(s.as_str());
        }
        if let Some(s) = severity {
            q = q.bind(s.as_str());
        }
        if let Some(r) = rule_id {
            q = q.bind(r.to_string());
        }
        q = q.bind(limit);

        let rows = q
            .fetch_all(self.pool)
            .await
            .context("Failed to fetch alerts")?;

        Ok(rows.into_iter().map(row_to_alert).collect())
    }

    /// Get active alerts
    pub async fn get_active(&self) -> Result<Vec<Alert>> {
        self.get_all(Some(AlertStatus::Active), None, None, None)
            .await
    }

    /// Get an alert by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<Alert>> {
        let row = sqlx::query_as::<_, AlertRow>(
            r#"
            SELECT id, rule_id, title, message, severity, context, status,
                   acknowledged_by, acknowledged_at, resolved_at, triggered_at, last_notified_at
            FROM alerts
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch alert")?;

        Ok(row.map(row_to_alert))
    }

    /// Create a new alert
    pub async fn create(
        &self,
        rule_id: Uuid,
        title: &str,
        message: &str,
        severity: AlertSeverity,
        context: Option<serde_json::Value>,
    ) -> Result<Alert> {
        let id = Uuid::new_v4();
        let context_json = context.map(|c| serde_json::to_string(&c).unwrap_or_default());

        sqlx::query(
            r#"
            INSERT INTO alerts (id, rule_id, title, message, severity, context, status)
            VALUES (?, ?, ?, ?, ?, ?, 'active')
            "#,
        )
        .bind(id.to_string())
        .bind(rule_id.to_string())
        .bind(title)
        .bind(message)
        .bind(severity.as_str())
        .bind(&context_json)
        .execute(self.pool)
        .await
        .context("Failed to create alert")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created alert"))
    }

    /// Acknowledge an alert
    pub async fn acknowledge(&self, id: Uuid, user_id: Uuid) -> Result<Option<Alert>> {
        sqlx::query(
            r#"
            UPDATE alerts
            SET status = 'acknowledged', acknowledged_by = ?, acknowledged_at = CURRENT_TIMESTAMP
            WHERE id = ? AND status = 'active'
            "#,
        )
        .bind(user_id.to_string())
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to acknowledge alert")?;

        self.get_by_id(id).await
    }

    /// Resolve an alert
    pub async fn resolve(&self, id: Uuid) -> Result<Option<Alert>> {
        sqlx::query(
            r#"
            UPDATE alerts
            SET status = 'resolved', resolved_at = CURRENT_TIMESTAMP
            WHERE id = ? AND status IN ('active', 'acknowledged')
            "#,
        )
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to resolve alert")?;

        self.get_by_id(id).await
    }

    /// Silence an alert
    pub async fn silence(&self, id: Uuid) -> Result<Option<Alert>> {
        sqlx::query("UPDATE alerts SET status = 'silenced' WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to silence alert")?;

        self.get_by_id(id).await
    }

    /// Update last notified timestamp
    pub async fn update_last_notified(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE alerts SET last_notified_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to update last notified")?;

        Ok(())
    }

    /// Get alert statistics
    pub async fn get_stats(&self) -> Result<AlertStats> {
        // Total active alerts
        let total_active: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM alerts WHERE status = 'active'")
                .fetch_one(self.pool)
                .await?;

        // Active alerts by severity
        let info: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM alerts WHERE status = 'active' AND severity = 'info'",
        )
        .fetch_one(self.pool)
        .await?;

        let warning: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM alerts WHERE status = 'active' AND severity = 'warning'",
        )
        .fetch_one(self.pool)
        .await?;

        let critical: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM alerts WHERE status = 'active' AND severity = 'critical'",
        )
        .fetch_one(self.pool)
        .await?;

        // Alerts triggered today
        let total_today: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM alerts WHERE DATE(triggered_at) = DATE('now')")
                .fetch_one(self.pool)
                .await?;

        // Total acknowledged (not resolved yet)
        let total_acknowledged: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM alerts WHERE status = 'acknowledged'")
                .fetch_one(self.pool)
                .await?;

        Ok(AlertStats {
            total_active: total_active.0,
            by_severity: AlertSeverityCount {
                info: info.0,
                warning: warning.0,
                critical: critical.0,
            },
            total_today: total_today.0,
            total_acknowledged: total_acknowledged.0,
        })
    }

    /// Check if rule is in cooldown (has recent alert)
    pub async fn is_in_cooldown(&self, rule_id: Uuid, cooldown_minutes: i32) -> Result<bool> {
        let result: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM alerts
            WHERE rule_id = ?
              AND triggered_at > datetime('now', '-' || ? || ' minutes')
              AND status IN ('active', 'acknowledged')
            "#,
        )
        .bind(rule_id.to_string())
        .bind(cooldown_minutes)
        .fetch_one(self.pool)
        .await?;

        Ok(result.0 > 0)
    }

    /// Delete old resolved alerts
    pub async fn delete_old_resolved(&self, older_than_days: i32) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM alerts
            WHERE status = 'resolved'
              AND resolved_at < datetime('now', '-' || ? || ' days')
            "#,
        )
        .bind(older_than_days)
        .execute(self.pool)
        .await
        .context("Failed to delete old alerts")?;

        Ok(result.rows_affected())
    }
}

fn row_to_alert(row: AlertRow) -> Alert {
    Alert {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        rule_id: Uuid::parse_str(&row.rule_id).unwrap_or_default(),
        title: row.title,
        message: row.message,
        severity: AlertSeverity::from_str(&row.severity).unwrap_or(AlertSeverity::Warning),
        context: row.context.and_then(|s| serde_json::from_str(&s).ok()),
        status: AlertStatus::from_str(&row.status).unwrap_or(AlertStatus::Active),
        acknowledged_by: row.acknowledged_by.and_then(|s| Uuid::parse_str(&s).ok()),
        acknowledged_at: row.acknowledged_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        resolved_at: row.resolved_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        triggered_at: DateTime::parse_from_rfc3339(&row.triggered_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        last_notified_at: row.last_notified_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
    }
}

// ============================================================================
// Notification History Repository
// ============================================================================

/// Row returned from notification_history table
#[derive(Debug, sqlx::FromRow)]
struct NotificationHistoryRow {
    id: String,
    alert_id: String,
    channel_id: String,
    status: String,
    attempt_count: i32,
    response_code: Option<i32>,
    response_body: Option<String>,
    error_message: Option<String>,
    sent_at: Option<String>,
    created_at: String,
}

/// Repository for notification history operations
pub struct NotificationHistoryRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> NotificationHistoryRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get notification history for an alert
    pub async fn get_by_alert(&self, alert_id: Uuid) -> Result<Vec<NotificationHistory>> {
        let rows = sqlx::query_as::<_, NotificationHistoryRow>(
            r#"
            SELECT id, alert_id, channel_id, status, attempt_count, response_code,
                   response_body, error_message, sent_at, created_at
            FROM notification_history
            WHERE alert_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(alert_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch notification history")?;

        Ok(rows.into_iter().map(row_to_notification_history).collect())
    }

    /// Get notification history by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<NotificationHistory>> {
        let row = sqlx::query_as::<_, NotificationHistoryRow>(
            r#"
            SELECT id, alert_id, channel_id, status, attempt_count, response_code,
                   response_body, error_message, sent_at, created_at
            FROM notification_history
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch notification")?;

        Ok(row.map(row_to_notification_history))
    }

    /// Create a pending notification
    pub async fn create(&self, alert_id: Uuid, channel_id: Uuid) -> Result<NotificationHistory> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO notification_history (id, alert_id, channel_id, status, attempt_count)
            VALUES (?, ?, ?, 'pending', 0)
            "#,
        )
        .bind(id.to_string())
        .bind(alert_id.to_string())
        .bind(channel_id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to create notification")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created notification"))
    }

    /// Mark notification as sent
    pub async fn mark_sent(
        &self,
        id: Uuid,
        response_code: Option<i32>,
        response_body: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE notification_history
            SET status = 'sent', sent_at = CURRENT_TIMESTAMP,
                response_code = ?, response_body = ?,
                attempt_count = attempt_count + 1
            WHERE id = ?
            "#,
        )
        .bind(response_code)
        .bind(response_body)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to mark notification sent")?;

        Ok(())
    }

    /// Mark notification as failed
    pub async fn mark_failed(&self, id: Uuid, error_message: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE notification_history
            SET status = 'failed', error_message = ?, attempt_count = attempt_count + 1
            WHERE id = ?
            "#,
        )
        .bind(error_message)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to mark notification failed")?;

        Ok(())
    }

    /// Mark notification for retry
    pub async fn mark_retrying(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE notification_history SET status = 'retrying' WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to mark notification retrying")?;

        Ok(())
    }

    /// Get pending or retrying notifications
    pub async fn get_pending(&self) -> Result<Vec<NotificationHistory>> {
        let rows = sqlx::query_as::<_, NotificationHistoryRow>(
            r#"
            SELECT id, alert_id, channel_id, status, attempt_count, response_code,
                   response_body, error_message, sent_at, created_at
            FROM notification_history
            WHERE status IN ('pending', 'retrying')
            ORDER BY created_at
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch pending notifications")?;

        Ok(rows.into_iter().map(row_to_notification_history).collect())
    }
}

fn row_to_notification_history(row: NotificationHistoryRow) -> NotificationHistory {
    NotificationHistory {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        alert_id: Uuid::parse_str(&row.alert_id).unwrap_or_default(),
        channel_id: Uuid::parse_str(&row.channel_id).unwrap_or_default(),
        status: NotificationStatus::from_str(&row.status).unwrap_or(NotificationStatus::Pending),
        attempt_count: row.attempt_count,
        response_code: row.response_code,
        response_body: row.response_body,
        error_message: row.error_message,
        sent_at: row.sent_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Alert Silence Repository
// ============================================================================

/// Row returned from alert_silences table
#[derive(Debug, sqlx::FromRow)]
struct SilenceRow {
    id: String,
    rule_id: Option<String>,
    matchers: Option<String>,
    starts_at: String,
    ends_at: String,
    reason: String,
    created_by: Option<String>,
    created_at: String,
}

/// Repository for alert silence operations
pub struct AlertSilenceRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AlertSilenceRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all silences
    pub async fn get_all(&self) -> Result<Vec<AlertSilence>> {
        let rows = sqlx::query_as::<_, SilenceRow>(
            r#"
            SELECT id, rule_id, matchers, starts_at, ends_at, reason, created_by, created_at
            FROM alert_silences
            ORDER BY ends_at DESC
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch silences")?;

        Ok(rows.into_iter().map(row_to_silence).collect())
    }

    /// Get active silences
    pub async fn get_active(&self) -> Result<Vec<AlertSilence>> {
        let rows = sqlx::query_as::<_, SilenceRow>(
            r#"
            SELECT id, rule_id, matchers, starts_at, ends_at, reason, created_by, created_at
            FROM alert_silences
            WHERE starts_at <= datetime('now') AND ends_at > datetime('now')
            ORDER BY ends_at DESC
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch active silences")?;

        Ok(rows.into_iter().map(row_to_silence).collect())
    }

    /// Get a silence by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<AlertSilence>> {
        let row = sqlx::query_as::<_, SilenceRow>(
            r#"
            SELECT id, rule_id, matchers, starts_at, ends_at, reason, created_by, created_at
            FROM alert_silences
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch silence")?;

        Ok(row.map(row_to_silence))
    }

    /// Create a new silence
    pub async fn create(
        &self,
        req: &CreateSilenceRequest,
        user_id: Option<Uuid>,
    ) -> Result<AlertSilence> {
        let id = Uuid::new_v4();
        let starts_at = req.starts_at.unwrap_or_else(Utc::now);
        let matchers_json = req
            .matchers
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

        sqlx::query(
            r#"
            INSERT INTO alert_silences (id, rule_id, matchers, starts_at, ends_at, reason, created_by)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(req.rule_id.map(|r| r.to_string()))
        .bind(&matchers_json)
        .bind(starts_at.to_rfc3339())
        .bind(req.ends_at.to_rfc3339())
        .bind(&req.reason)
        .bind(user_id.map(|u| u.to_string()))
        .execute(self.pool)
        .await
        .context("Failed to create silence")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created silence"))
    }

    /// Delete a silence
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM alert_silences WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete silence")?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if a rule is silenced
    pub async fn is_rule_silenced(&self, rule_id: Uuid) -> Result<bool> {
        let result: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM alert_silences
            WHERE (rule_id = ? OR rule_id IS NULL)
              AND starts_at <= datetime('now')
              AND ends_at > datetime('now')
            "#,
        )
        .bind(rule_id.to_string())
        .fetch_one(self.pool)
        .await?;

        Ok(result.0 > 0)
    }

    /// Delete expired silences
    pub async fn delete_expired(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM alert_silences WHERE ends_at < datetime('now')")
            .execute(self.pool)
            .await
            .context("Failed to delete expired silences")?;

        Ok(result.rows_affected())
    }
}

fn row_to_silence(row: SilenceRow) -> AlertSilence {
    AlertSilence {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        rule_id: row.rule_id.and_then(|s| Uuid::parse_str(&s).ok()),
        matchers: row.matchers.and_then(|s| serde_json::from_str(&s).ok()),
        starts_at: DateTime::parse_from_rfc3339(&row.starts_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        ends_at: DateTime::parse_from_rfc3339(&row.ends_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        reason: row.reason,
        created_by: row.created_by.and_then(|s| Uuid::parse_str(&s).ok()),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_roundtrip() {
        let types = vec![
            ChannelType::Webhook,
            ChannelType::Email,
            ChannelType::Slack,
            ChannelType::Teams,
        ];

        for ct in types {
            let s = ct.as_str();
            let parsed = ChannelType::from_str(s);
            assert_eq!(Some(ct), parsed);
        }
    }

    #[test]
    fn test_severity_roundtrip() {
        let severities = vec![
            AlertSeverity::Info,
            AlertSeverity::Warning,
            AlertSeverity::Critical,
        ];

        for sev in severities {
            let s = sev.as_str();
            let parsed = AlertSeverity::from_str(s);
            assert_eq!(Some(sev), parsed);
        }
    }
}
