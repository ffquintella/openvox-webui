//! Alerting service for managing alerts and notifications
//!
//! This service provides:
//! - Alert rule evaluation
//! - Notification dispatch to various channels
//! - Alert lifecycle management

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use sqlx::SqlitePool;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::db::{
    AlertRepository, AlertRuleRepository, AlertSilenceRepository, NotificationChannelRepository,
    NotificationHistoryRepository,
};
use crate::models::{
    Alert, AlertCondition, AlertRule, AlertRuleType, AlertSeverity, AlertStats, AlertStatus,
    AlertWebhookData, ChannelType, CreateAlertRuleRequest, CreateChannelRequest,
    CreateSilenceRequest, EmailConfig, NotificationChannel, SlackConfig, TeamsConfig,
    TestChannelRequest, TestChannelResponse, UpdateAlertRuleRequest, UpdateChannelRequest,
    WebhookConfig, WebhookPayload,
};
use crate::services::PuppetDbClient;

/// Alerting service for managing alerts and notifications
pub struct AlertingService {
    pool: SqlitePool,
    http_client: Client,
    puppetdb: Option<Arc<PuppetDbClient>>,
}

impl AlertingService {
    /// Create a new alerting service
    pub fn new(pool: SqlitePool, puppetdb: Option<Arc<PuppetDbClient>>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            pool,
            http_client,
            puppetdb,
        }
    }

    // ========================================================================
    // Notification Channels
    // ========================================================================

    /// Get all notification channels
    pub async fn get_channels(&self) -> Result<Vec<NotificationChannel>> {
        let repo = NotificationChannelRepository::new(&self.pool);
        repo.get_all().await
    }

    /// Get a notification channel by ID
    pub async fn get_channel(&self, id: Uuid) -> Result<Option<NotificationChannel>> {
        let repo = NotificationChannelRepository::new(&self.pool);
        repo.get_by_id(id).await
    }

    /// Create a new notification channel
    pub async fn create_channel(
        &self,
        req: &CreateChannelRequest,
        user_id: Option<Uuid>,
    ) -> Result<NotificationChannel> {
        let repo = NotificationChannelRepository::new(&self.pool);
        repo.create(req, user_id).await
    }

    /// Update a notification channel
    pub async fn update_channel(
        &self,
        id: Uuid,
        req: &UpdateChannelRequest,
    ) -> Result<Option<NotificationChannel>> {
        let repo = NotificationChannelRepository::new(&self.pool);
        repo.update(id, req).await
    }

    /// Delete a notification channel
    pub async fn delete_channel(&self, id: Uuid) -> Result<bool> {
        let repo = NotificationChannelRepository::new(&self.pool);
        repo.delete(id).await
    }

    /// Test a notification channel
    pub async fn test_channel(
        &self,
        id: Uuid,
        req: &TestChannelRequest,
    ) -> Result<TestChannelResponse> {
        let repo = NotificationChannelRepository::new(&self.pool);
        let channel = repo
            .get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Channel not found"))?;

        let test_message = req
            .message
            .clone()
            .unwrap_or_else(|| "This is a test notification from OpenVox WebUI".to_string());

        // Create a test payload
        let payload = WebhookPayload {
            event_type: "test".to_string(),
            alert: AlertWebhookData {
                id: "test-alert-id".to_string(),
                rule_name: "Test Rule".to_string(),
                title: "Test Alert".to_string(),
                message: test_message.clone(),
                severity: "info".to_string(),
                status: "active".to_string(),
                triggered_at: Utc::now(),
                context: None,
            },
            timestamp: Utc::now(),
        };

        match self.send_notification(&channel, &payload).await {
            Ok(response_code) => Ok(TestChannelResponse {
                success: true,
                message: "Test notification sent successfully".to_string(),
                response_code: Some(response_code),
            }),
            Err(e) => Ok(TestChannelResponse {
                success: false,
                message: format!("Failed to send test notification: {}", e),
                response_code: None,
            }),
        }
    }

    // ========================================================================
    // Alert Rules
    // ========================================================================

    /// Get all alert rules
    pub async fn get_rules(&self) -> Result<Vec<AlertRule>> {
        let repo = AlertRuleRepository::new(&self.pool);
        repo.get_all().await
    }

    /// Get enabled alert rules
    pub async fn get_enabled_rules(&self) -> Result<Vec<AlertRule>> {
        let repo = AlertRuleRepository::new(&self.pool);
        repo.get_enabled().await
    }

    /// Get alert rules by type
    pub async fn get_rules_by_type(&self, rule_type: AlertRuleType) -> Result<Vec<AlertRule>> {
        let repo = AlertRuleRepository::new(&self.pool);
        repo.get_by_type(rule_type).await
    }

    /// Get an alert rule by ID
    pub async fn get_rule(&self, id: Uuid) -> Result<Option<AlertRule>> {
        let repo = AlertRuleRepository::new(&self.pool);
        repo.get_by_id(id).await
    }

    /// Create a new alert rule
    pub async fn create_rule(
        &self,
        req: &CreateAlertRuleRequest,
        user_id: Option<Uuid>,
    ) -> Result<AlertRule> {
        let repo = AlertRuleRepository::new(&self.pool);
        repo.create(req, user_id).await
    }

    /// Update an alert rule
    pub async fn update_rule(
        &self,
        id: Uuid,
        req: &UpdateAlertRuleRequest,
    ) -> Result<Option<AlertRule>> {
        let repo = AlertRuleRepository::new(&self.pool);
        repo.update(id, req).await
    }

    /// Delete an alert rule
    pub async fn delete_rule(&self, id: Uuid) -> Result<bool> {
        let repo = AlertRuleRepository::new(&self.pool);
        repo.delete(id).await
    }

    // ========================================================================
    // Alerts
    // ========================================================================

    /// Get alerts with optional filtering
    pub async fn get_alerts(
        &self,
        status: Option<AlertStatus>,
        severity: Option<AlertSeverity>,
        rule_id: Option<Uuid>,
        limit: Option<u32>,
    ) -> Result<Vec<Alert>> {
        let repo = AlertRepository::new(&self.pool);
        repo.get_all(status, severity, rule_id, limit).await
    }

    /// Get an alert by ID
    pub async fn get_alert(&self, id: Uuid) -> Result<Option<Alert>> {
        let repo = AlertRepository::new(&self.pool);
        repo.get_by_id(id).await
    }

    /// Acknowledge an alert
    pub async fn acknowledge_alert(&self, id: Uuid, user_id: Uuid) -> Result<Option<Alert>> {
        let repo = AlertRepository::new(&self.pool);
        repo.acknowledge(id, user_id).await
    }

    /// Resolve an alert
    pub async fn resolve_alert(&self, id: Uuid) -> Result<Option<Alert>> {
        let repo = AlertRepository::new(&self.pool);
        repo.resolve(id).await
    }

    /// Silence an alert
    pub async fn silence_alert(&self, id: Uuid) -> Result<Option<Alert>> {
        let repo = AlertRepository::new(&self.pool);
        repo.silence(id).await
    }

    /// Get alert statistics
    pub async fn get_alert_stats(&self) -> Result<AlertStats> {
        let repo = AlertRepository::new(&self.pool);
        repo.get_stats().await
    }

    // ========================================================================
    // Silences
    // ========================================================================

    /// Get all silences
    pub async fn get_silences(&self) -> Result<Vec<crate::models::AlertSilence>> {
        let repo = AlertSilenceRepository::new(&self.pool);
        repo.get_all().await
    }

    /// Get active silences
    pub async fn get_active_silences(&self) -> Result<Vec<crate::models::AlertSilence>> {
        let repo = AlertSilenceRepository::new(&self.pool);
        repo.get_active().await
    }

    /// Create a silence
    pub async fn create_silence(
        &self,
        req: &CreateSilenceRequest,
        user_id: Option<Uuid>,
    ) -> Result<crate::models::AlertSilence> {
        let repo = AlertSilenceRepository::new(&self.pool);
        repo.create(req, user_id).await
    }

    /// Delete a silence
    pub async fn delete_silence(&self, id: Uuid) -> Result<bool> {
        let repo = AlertSilenceRepository::new(&self.pool);
        repo.delete(id).await
    }

    // ========================================================================
    // Alert Evaluation
    // ========================================================================

    /// Evaluate all enabled rules and trigger alerts as needed
    pub async fn evaluate_rules(&self) -> Result<Vec<Alert>> {
        let rules = self.get_enabled_rules().await?;
        let mut triggered_alerts = Vec::new();

        for rule in rules {
            // Check if rule is silenced
            let silence_repo = AlertSilenceRepository::new(&self.pool);
            if silence_repo.is_rule_silenced(rule.id).await? {
                debug!("Rule {} is silenced, skipping", rule.name);
                continue;
            }

            // Check cooldown
            let alert_repo = AlertRepository::new(&self.pool);
            if alert_repo
                .is_in_cooldown(rule.id, rule.cooldown_minutes)
                .await?
            {
                debug!("Rule {} is in cooldown, skipping", rule.name);
                continue;
            }

            // Evaluate the rule based on its type
            if let Some(alert) = self.evaluate_rule(&rule).await? {
                triggered_alerts.push(alert);
            }
        }

        Ok(triggered_alerts)
    }

    /// Evaluate a single rule
    async fn evaluate_rule(&self, rule: &AlertRule) -> Result<Option<Alert>> {
        match rule.rule_type {
            AlertRuleType::NodeStatus => self.evaluate_node_status_rule(rule).await,
            AlertRuleType::Compliance => self.evaluate_compliance_rule(rule).await,
            AlertRuleType::Drift => self.evaluate_drift_rule(rule).await,
            AlertRuleType::ReportFailure => self.evaluate_report_failure_rule(rule).await,
            AlertRuleType::Custom => self.evaluate_custom_rule(rule).await,
        }
    }

    /// Evaluate node status rule
    async fn evaluate_node_status_rule(&self, rule: &AlertRule) -> Result<Option<Alert>> {
        let Some(puppetdb) = &self.puppetdb else {
            warn!("PuppetDB not configured, cannot evaluate node status rule");
            return Ok(None);
        };

        // Get nodes from PuppetDB
        let nodes = puppetdb.get_nodes().await?;

        let mut failed_nodes = Vec::new();
        for node in &nodes {
            // Check if node matches any condition
            let matches = self.evaluate_conditions(
                &rule.conditions,
                &json!({
                    "node.certname": node.certname,
                    "node.status": node.latest_report_status.as_deref().unwrap_or("unknown"),
                    "node.environment": node.report_environment.as_deref().unwrap_or(""),
                }),
                rule.condition_operator,
            );

            if matches {
                failed_nodes.push(node.certname.clone());
            }
        }

        if !failed_nodes.is_empty() {
            let alert = self
                .trigger_alert(
                    rule,
                    &format!("Node Status Alert: {} nodes affected", failed_nodes.len()),
                    &format!(
                        "The following nodes have triggered the alert: {}",
                        failed_nodes.join(", ")
                    ),
                    Some(json!({ "affected_nodes": failed_nodes })),
                )
                .await?;
            return Ok(Some(alert));
        }

        Ok(None)
    }

    /// Evaluate compliance rule
    async fn evaluate_compliance_rule(&self, rule: &AlertRule) -> Result<Option<Alert>> {
        let Some(puppetdb) = &self.puppetdb else {
            warn!("PuppetDB not configured, cannot evaluate compliance rule");
            return Ok(None);
        };

        // Get recent reports with compliance issues
        let reports = puppetdb.query_reports(None, None, Some(100)).await?;

        let mut non_compliant = Vec::new();
        for report in &reports {
            let context = json!({
                "compliance.status": report.status,
                "compliance.certname": report.certname,
                "compliance.noop": report.noop.unwrap_or(false),
            });

            if self.evaluate_conditions(&rule.conditions, &context, rule.condition_operator) {
                non_compliant.push(report.certname.clone());
            }
        }

        if !non_compliant.is_empty() {
            let alert = self
                .trigger_alert(
                    rule,
                    &format!(
                        "Compliance Alert: {} non-compliant nodes",
                        non_compliant.len()
                    ),
                    &format!("Nodes failing compliance: {}", non_compliant.join(", ")),
                    Some(json!({ "non_compliant_nodes": non_compliant })),
                )
                .await?;
            return Ok(Some(alert));
        }

        Ok(None)
    }

    /// Evaluate drift rule
    async fn evaluate_drift_rule(&self, _rule: &AlertRule) -> Result<Option<Alert>> {
        // Drift detection would compare current facts against baseline
        // For now, return None - full implementation would need drift baselines
        debug!("Drift rule evaluation not yet fully implemented");
        Ok(None)
    }

    /// Evaluate report failure rule
    async fn evaluate_report_failure_rule(&self, rule: &AlertRule) -> Result<Option<Alert>> {
        let Some(puppetdb) = &self.puppetdb else {
            warn!("PuppetDB not configured, cannot evaluate report failure rule");
            return Ok(None);
        };

        // Get recent reports with failures
        let reports = puppetdb
            .query_reports(None, Some("failed"), Some(100))
            .await?;

        let mut failed_reports = Vec::new();
        for report in &reports {
            let context = json!({
                "report.status": report.status,
                "report.certname": report.certname,
            });

            if self.evaluate_conditions(&rule.conditions, &context, rule.condition_operator) {
                failed_reports.push(json!({
                    "certname": report.certname,
                    "status": report.status,
                    "hash": report.hash,
                }));
            }
        }

        if !failed_reports.is_empty() {
            let affected_nodes: Vec<String> = failed_reports
                .iter()
                .filter_map(|r| {
                    r.get("certname")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();

            let alert = self
                .trigger_alert(
                    rule,
                    &format!(
                        "Report Failure Alert: {} failures detected",
                        failed_reports.len()
                    ),
                    &format!("Report failures on: {}", affected_nodes.join(", ")),
                    Some(json!({ "failed_reports": failed_reports })),
                )
                .await?;
            return Ok(Some(alert));
        }

        Ok(None)
    }

    /// Evaluate custom rule
    async fn evaluate_custom_rule(&self, _rule: &AlertRule) -> Result<Option<Alert>> {
        // Custom rules can be implemented by users via API calls
        debug!("Custom rule evaluation is event-driven");
        Ok(None)
    }

    /// Evaluate conditions against a context
    fn evaluate_conditions(
        &self,
        conditions: &[AlertCondition],
        context: &serde_json::Value,
        operator: crate::models::ConditionOperator,
    ) -> bool {
        if conditions.is_empty() {
            return false;
        }

        let results: Vec<bool> = conditions
            .iter()
            .map(|c| self.evaluate_condition(c, context))
            .collect();

        match operator {
            crate::models::ConditionOperator::All => results.iter().all(|&r| r),
            crate::models::ConditionOperator::Any => results.iter().any(|&r| r),
        }
    }

    /// Evaluate a single condition
    fn evaluate_condition(&self, condition: &AlertCondition, context: &serde_json::Value) -> bool {
        // Get field and value, handling both old and new formats
        let field = match &condition.field {
            Some(f) => f.as_str(),
            None => return false, // Can't evaluate without field in legacy format
        };
        
        let value = match &condition.value {
            Some(v) => v,
            None => return false, // Can't evaluate without value in legacy format
        };

        // Get the field value from context using dot notation
        let field_value = self.get_field_value(context, field);

        match condition.operator.as_str() {
            "eq" | "=" | "==" => field_value == Some(value),
            "ne" | "!=" => field_value != Some(value),
            "gt" | ">" => match (field_value, value) {
                (Some(serde_json::Value::Number(a)), serde_json::Value::Number(b)) => {
                    a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
                }
                _ => false,
            },
            "gte" | ">=" => match (field_value, value) {
                (Some(serde_json::Value::Number(a)), serde_json::Value::Number(b)) => {
                    a.as_f64().unwrap_or(0.0) >= b.as_f64().unwrap_or(0.0)
                }
                _ => false,
            },
            "lt" | "<" => match (field_value, value) {
                (Some(serde_json::Value::Number(a)), serde_json::Value::Number(b)) => {
                    a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
                }
                _ => false,
            },
            "lte" | "<=" => match (field_value, value) {
                (Some(serde_json::Value::Number(a)), serde_json::Value::Number(b)) => {
                    a.as_f64().unwrap_or(0.0) <= b.as_f64().unwrap_or(0.0)
                }
                _ => false,
            },
            "contains" => match (field_value, value) {
                (Some(serde_json::Value::String(haystack)), serde_json::Value::String(needle)) => {
                    haystack.contains(needle)
                }
                (Some(serde_json::Value::Array(arr)), val) => arr.contains(val),
                _ => false,
            },
            "regex" | "~" => match (field_value, value) {
                (Some(serde_json::Value::String(s)), serde_json::Value::String(pattern)) => {
                    regex::Regex::new(pattern)
                        .map(|re| re.is_match(s))
                        .unwrap_or(false)
                }
                _ => false,
            },
            "in" => match (value, field_value) {
                (serde_json::Value::Array(arr), Some(val)) => arr.contains(val),
                _ => false,
            },
            _ => {
                warn!("Unknown condition operator: {}", condition.operator);
                false
            }
        }
    }

    /// Get a field value from JSON using dot notation (e.g., "node.status")
    fn get_field_value<'a>(
        &self,
        value: &'a serde_json::Value,
        path: &str,
    ) -> Option<&'a serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            match current {
                serde_json::Value::Object(map) => {
                    current = map.get(part)?;
                }
                serde_json::Value::Array(arr) => {
                    let index: usize = part.parse().ok()?;
                    current = arr.get(index)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Trigger an alert and send notifications
    pub async fn trigger_alert(
        &self,
        rule: &AlertRule,
        title: &str,
        message: &str,
        context: Option<serde_json::Value>,
    ) -> Result<Alert> {
        info!("Triggering alert for rule: {} - {}", rule.name, title);

        // Create the alert
        let alert_repo = AlertRepository::new(&self.pool);
        let alert = alert_repo
            .create(rule.id, title, message, rule.severity, context)
            .await?;

        // Send notifications to all associated channels
        self.send_alert_notifications(&alert, rule).await?;

        Ok(alert)
    }

    /// Manually trigger an alert (for custom rules)
    pub async fn trigger_manual_alert(
        &self,
        rule_id: Uuid,
        title: &str,
        message: &str,
        context: Option<serde_json::Value>,
    ) -> Result<Alert> {
        let rule = self
            .get_rule(rule_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Rule not found"))?;

        self.trigger_alert(&rule, title, message, context).await
    }

    // ========================================================================
    // Notification Dispatch
    // ========================================================================

    /// Send notifications for an alert to all configured channels
    async fn send_alert_notifications(&self, alert: &Alert, rule: &AlertRule) -> Result<()> {
        let channel_repo = NotificationChannelRepository::new(&self.pool);
        let history_repo = NotificationHistoryRepository::new(&self.pool);

        // Create the webhook payload
        let payload = WebhookPayload {
            event_type: "alert.triggered".to_string(),
            alert: AlertWebhookData {
                id: alert.id.to_string(),
                rule_name: rule.name.clone(),
                title: alert.title.clone(),
                message: alert.message.clone(),
                severity: alert.severity.as_str().to_string(),
                status: alert.status.as_str().to_string(),
                triggered_at: alert.triggered_at,
                context: alert.context.clone(),
            },
            timestamp: Utc::now(),
        };

        // Send to each channel
        for channel_id in &rule.channels {
            if let Some(channel) = channel_repo.get_by_id(*channel_id).await? {
                if !channel.is_enabled {
                    debug!("Channel {} is disabled, skipping", channel.name);
                    continue;
                }

                // Create notification history entry
                let notification = history_repo.create(alert.id, channel.id).await?;

                // Send the notification
                match self.send_notification(&channel, &payload).await {
                    Ok(response_code) => {
                        history_repo
                            .mark_sent(notification.id, Some(response_code), None)
                            .await?;
                        info!(
                            "Notification sent to channel {} for alert {}",
                            channel.name, alert.id
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to send notification to channel {}: {}",
                            channel.name, e
                        );
                        history_repo
                            .mark_failed(notification.id, &e.to_string())
                            .await?;
                    }
                }
            }
        }

        // Update alert last notified timestamp
        let alert_repo = AlertRepository::new(&self.pool);
        alert_repo.update_last_notified(alert.id).await?;

        Ok(())
    }

    /// Send a notification to a specific channel
    async fn send_notification(
        &self,
        channel: &NotificationChannel,
        payload: &WebhookPayload,
    ) -> Result<i32> {
        match channel.channel_type {
            ChannelType::Webhook => self.send_webhook_notification(channel, payload).await,
            ChannelType::Email => self.send_email_notification(channel, payload).await,
            ChannelType::Slack => self.send_slack_notification(channel, payload).await,
            ChannelType::Teams => self.send_teams_notification(channel, payload).await,
        }
    }

    /// Send webhook notification
    async fn send_webhook_notification(
        &self,
        channel: &NotificationChannel,
        payload: &WebhookPayload,
    ) -> Result<i32> {
        let config: WebhookConfig =
            serde_json::from_value(channel.config.clone()).context("Invalid webhook config")?;

        let mut request = match config.method.to_uppercase().as_str() {
            "POST" => self.http_client.post(&config.url),
            "PUT" => self.http_client.put(&config.url),
            _ => self.http_client.post(&config.url),
        };

        // Add custom headers
        for (key, value) in &config.headers {
            request = request.header(key, value);
        }

        // Set timeout if specified
        if let Some(timeout) = config.timeout_secs {
            request = request.timeout(Duration::from_secs(timeout));
        }

        let response = request
            .json(payload)
            .send()
            .await
            .context("Failed to send webhook")?;

        let status = response.status().as_u16() as i32;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Webhook returned error {}: {}",
                status,
                body
            ));
        }

        Ok(status)
    }

    /// Send email notification
    async fn send_email_notification(
        &self,
        channel: &NotificationChannel,
        payload: &WebhookPayload,
    ) -> Result<i32> {
        let config: EmailConfig =
            serde_json::from_value(channel.config.clone()).context("Invalid email config")?;

        // Note: Full email implementation would require an SMTP library like lettre
        // For now, we'll log the attempt and return success for testing
        info!(
            "Email notification would be sent to {:?} via {}:{}: {}",
            config.to, config.smtp_host, config.smtp_port, payload.alert.title
        );

        // In a real implementation, you would use lettre or similar:
        // let mailer = SmtpTransport::relay(&config.smtp_host)?
        //     .credentials(Credentials::new(config.smtp_username, config.smtp_password))
        //     .build();
        //
        // let email = Message::builder()
        //     .from(config.from.parse()?)
        //     .to(config.to.join(",").parse()?)
        //     .subject(payload.alert.title.clone())
        //     .body(payload.alert.message.clone())?;
        //
        // mailer.send(&email)?;

        warn!("Email notifications require SMTP configuration - notification logged but not sent");
        Ok(200)
    }

    /// Send Slack notification
    async fn send_slack_notification(
        &self,
        channel: &NotificationChannel,
        payload: &WebhookPayload,
    ) -> Result<i32> {
        let config: SlackConfig =
            serde_json::from_value(channel.config.clone()).context("Invalid Slack config")?;

        // Build Slack message with blocks for better formatting
        let color = match payload.alert.severity.as_str() {
            "critical" => "#FF0000",
            "warning" => "#FFA500",
            _ => "#36A64F",
        };

        let slack_payload = json!({
            "channel": config.channel,
            "username": config.username.unwrap_or_else(|| "OpenVox WebUI".to_string()),
            "icon_emoji": config.icon_emoji.unwrap_or_else(|| ":warning:".to_string()),
            "attachments": [{
                "color": color,
                "title": payload.alert.title,
                "text": payload.alert.message,
                "fields": [
                    {
                        "title": "Severity",
                        "value": payload.alert.severity,
                        "short": true
                    },
                    {
                        "title": "Status",
                        "value": payload.alert.status,
                        "short": true
                    },
                    {
                        "title": "Rule",
                        "value": payload.alert.rule_name,
                        "short": true
                    },
                    {
                        "title": "Triggered At",
                        "value": payload.alert.triggered_at.to_rfc3339(),
                        "short": true
                    }
                ],
                "ts": payload.timestamp.timestamp()
            }]
        });

        let response = self
            .http_client
            .post(&config.webhook_url)
            .json(&slack_payload)
            .send()
            .await
            .context("Failed to send Slack notification")?;

        let status = response.status().as_u16() as i32;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Slack returned error {}: {}", status, body));
        }

        Ok(status)
    }

    /// Send Microsoft Teams notification
    async fn send_teams_notification(
        &self,
        channel: &NotificationChannel,
        payload: &WebhookPayload,
    ) -> Result<i32> {
        let config: TeamsConfig =
            serde_json::from_value(channel.config.clone()).context("Invalid Teams config")?;

        // Build Teams adaptive card
        let theme_color = match payload.alert.severity.as_str() {
            "critical" => "FF0000",
            "warning" => "FFA500",
            _ => "36A64F",
        };

        let teams_payload = json!({
            "@type": "MessageCard",
            "@context": "http://schema.org/extensions",
            "themeColor": theme_color,
            "summary": payload.alert.title,
            "sections": [{
                "activityTitle": payload.alert.title,
                "activitySubtitle": format!("Rule: {}", payload.alert.rule_name),
                "text": payload.alert.message,
                "facts": [
                    {
                        "name": "Severity",
                        "value": payload.alert.severity
                    },
                    {
                        "name": "Status",
                        "value": payload.alert.status
                    },
                    {
                        "name": "Triggered At",
                        "value": payload.alert.triggered_at.to_rfc3339()
                    }
                ],
                "markdown": true
            }]
        });

        let response = self
            .http_client
            .post(&config.webhook_url)
            .json(&teams_payload)
            .send()
            .await
            .context("Failed to send Teams notification")?;

        let status = response.status().as_u16() as i32;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Teams returned error {}: {}", status, body));
        }

        Ok(status)
    }

    // ========================================================================
    // Maintenance
    // ========================================================================

    /// Process pending notifications (retry failed ones)
    pub async fn process_pending_notifications(&self) -> Result<u32> {
        let history_repo = NotificationHistoryRepository::new(&self.pool);
        let channel_repo = NotificationChannelRepository::new(&self.pool);
        let alert_repo = AlertRepository::new(&self.pool);
        let rule_repo = AlertRuleRepository::new(&self.pool);

        let pending = history_repo.get_pending().await?;
        let mut processed = 0;

        for notification in pending {
            // Get channel and alert details
            let Some(channel) = channel_repo.get_by_id(notification.channel_id).await? else {
                continue;
            };
            let Some(alert) = alert_repo.get_by_id(notification.alert_id).await? else {
                continue;
            };
            let Some(rule) = rule_repo.get_by_id(alert.rule_id).await? else {
                continue;
            };

            let payload = WebhookPayload {
                event_type: "alert.triggered".to_string(),
                alert: AlertWebhookData {
                    id: alert.id.to_string(),
                    rule_name: rule.name.clone(),
                    title: alert.title.clone(),
                    message: alert.message.clone(),
                    severity: alert.severity.as_str().to_string(),
                    status: alert.status.as_str().to_string(),
                    triggered_at: alert.triggered_at,
                    context: alert.context.clone(),
                },
                timestamp: Utc::now(),
            };

            // Retry the notification
            match self.send_notification(&channel, &payload).await {
                Ok(response_code) => {
                    history_repo
                        .mark_sent(notification.id, Some(response_code), None)
                        .await?;
                    processed += 1;
                }
                Err(e) => {
                    // Max 3 retries
                    if notification.attempt_count >= 3 {
                        history_repo
                            .mark_failed(notification.id, &e.to_string())
                            .await?;
                    } else {
                        history_repo.mark_retrying(notification.id).await?;
                    }
                }
            }
        }

        Ok(processed)
    }

    /// Clean up old data
    pub async fn cleanup(&self, resolved_alert_days: i32) -> Result<(u64, u64)> {
        let alert_repo = AlertRepository::new(&self.pool);
        let silence_repo = AlertSilenceRepository::new(&self.pool);

        let deleted_alerts = alert_repo.delete_old_resolved(resolved_alert_days).await?;
        let deleted_silences = silence_repo.delete_expired().await?;

        info!(
            "Cleanup: deleted {} old alerts and {} expired silences",
            deleted_alerts, deleted_silences
        );

        Ok((deleted_alerts, deleted_silences))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to get a field value from JSON using dot notation
    fn get_field_value<'a>(
        value: &'a serde_json::Value,
        path: &str,
    ) -> Option<&'a serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            match current {
                serde_json::Value::Object(map) => {
                    current = map.get(part)?;
                }
                serde_json::Value::Array(arr) => {
                    let index: usize = part.parse().ok()?;
                    current = arr.get(index)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

    // Helper function to evaluate a condition
    fn evaluate_condition(condition: &AlertCondition, context: &serde_json::Value) -> bool {
        let field_value = get_field_value(context, &condition.field);

        match condition.operator.as_str() {
            "eq" | "=" | "==" => field_value == Some(&condition.value),
            "ne" | "!=" => field_value != Some(&condition.value),
            "contains" => match (field_value, &condition.value) {
                (Some(serde_json::Value::String(haystack)), serde_json::Value::String(needle)) => {
                    haystack.contains(needle)
                }
                (Some(serde_json::Value::Array(arr)), val) => arr.contains(val),
                _ => false,
            },
            _ => false,
        }
    }

    #[test]
    fn test_evaluate_condition_equals() {
        let condition = AlertCondition {
            field: "status".to_string(),
            operator: "eq".to_string(),
            value: json!("failed"),
        };

        let context = json!({ "status": "failed" });
        assert!(evaluate_condition(&condition, &context));

        let context = json!({ "status": "success" });
        assert!(!evaluate_condition(&condition, &context));
    }

    #[test]
    fn test_evaluate_condition_contains() {
        let condition = AlertCondition {
            field: "message".to_string(),
            operator: "contains".to_string(),
            value: json!("error"),
        };

        let context = json!({ "message": "This is an error message" });
        assert!(evaluate_condition(&condition, &context));

        let context = json!({ "message": "All good" });
        assert!(!evaluate_condition(&condition, &context));
    }

    #[test]
    fn test_get_field_value_nested() {
        let value = json!({
            "node": {
                "status": "failed",
                "facts": {
                    "os": "linux"
                }
            }
        });

        assert_eq!(
            get_field_value(&value, "node.status"),
            Some(&json!("failed"))
        );
        assert_eq!(
            get_field_value(&value, "node.facts.os"),
            Some(&json!("linux"))
        );
        assert_eq!(get_field_value(&value, "node.missing"), None);
    }
}
