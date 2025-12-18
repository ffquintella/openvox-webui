//! Alerting and notification models
//!
//! This module defines the data structures for the alerting system including
//! alert rules, notification channels, and alert instances.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Enums
// ============================================================================

/// Types of notification channels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Webhook,
    Email,
    Slack,
    Teams,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::Webhook => "webhook",
            ChannelType::Email => "email",
            ChannelType::Slack => "slack",
            ChannelType::Teams => "teams",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "webhook" => Some(ChannelType::Webhook),
            "email" => Some(ChannelType::Email),
            "slack" => Some(ChannelType::Slack),
            "teams" => Some(ChannelType::Teams),
            _ => None,
        }
    }
}

/// Types of alert rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertRuleType {
    NodeStatus,
    Compliance,
    Drift,
    ReportFailure,
    Custom,
}

impl AlertRuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertRuleType::NodeStatus => "node_status",
            AlertRuleType::Compliance => "compliance",
            AlertRuleType::Drift => "drift",
            AlertRuleType::ReportFailure => "report_failure",
            AlertRuleType::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "node_status" => Some(AlertRuleType::NodeStatus),
            "compliance" => Some(AlertRuleType::Compliance),
            "drift" => Some(AlertRuleType::Drift),
            "report_failure" => Some(AlertRuleType::ReportFailure),
            "custom" => Some(AlertRuleType::Custom),
            _ => None,
        }
    }
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertSeverity::Info => "info",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Critical => "critical",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "info" => Some(AlertSeverity::Info),
            "warning" => Some(AlertSeverity::Warning),
            "critical" => Some(AlertSeverity::Critical),
            _ => None,
        }
    }
}

impl Default for AlertSeverity {
    fn default() -> Self {
        AlertSeverity::Warning
    }
}

/// Condition operator for combining multiple conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    All, // AND - all conditions must match
    Any, // OR - any condition must match
}

impl ConditionOperator {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConditionOperator::All => "all",
            ConditionOperator::Any => "any",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "all" => Some(ConditionOperator::All),
            "any" => Some(ConditionOperator::Any),
            _ => None,
        }
    }
}

impl Default for ConditionOperator {
    fn default() -> Self {
        ConditionOperator::All
    }
}

/// Alert status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    Active,
    Acknowledged,
    Resolved,
    Silenced,
}

impl AlertStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertStatus::Active => "active",
            AlertStatus::Acknowledged => "acknowledged",
            AlertStatus::Resolved => "resolved",
            AlertStatus::Silenced => "silenced",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "active" => Some(AlertStatus::Active),
            "acknowledged" => Some(AlertStatus::Acknowledged),
            "resolved" => Some(AlertStatus::Resolved),
            "silenced" => Some(AlertStatus::Silenced),
            _ => None,
        }
    }
}

impl Default for AlertStatus {
    fn default() -> Self {
        AlertStatus::Active
    }
}

/// Notification delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationStatus {
    Pending,
    Sent,
    Failed,
    Retrying,
}

impl NotificationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationStatus::Pending => "pending",
            NotificationStatus::Sent => "sent",
            NotificationStatus::Failed => "failed",
            NotificationStatus::Retrying => "retrying",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(NotificationStatus::Pending),
            "sent" => Some(NotificationStatus::Sent),
            "failed" => Some(NotificationStatus::Failed),
            "retrying" => Some(NotificationStatus::Retrying),
            _ => None,
        }
    }
}

// ============================================================================
// Channel Configuration Types
// ============================================================================

/// Webhook channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub retry_count: Option<u32>,
}

fn default_method() -> String {
    "POST".to_string()
}

/// Email channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub use_tls: bool,
}

fn default_smtp_port() -> u16 {
    587
}

/// Slack channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub webhook_url: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub icon_emoji: Option<String>,
}

/// Microsoft Teams channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsConfig {
    pub webhook_url: String,
}

/// Unified channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChannelConfig {
    Webhook(WebhookConfig),
    Email(EmailConfig),
    Slack(SlackConfig),
    Teams(TeamsConfig),
}

// ============================================================================
// Alert Condition Types
// ============================================================================

/// A single condition in an alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCondition {
    /// Field to evaluate (e.g., "node.status", "compliance.rate", "drift.count")
    pub field: String,
    /// Comparison operator
    pub operator: String, // eq, ne, gt, gte, lt, lte, contains, regex
    /// Value to compare against
    pub value: serde_json::Value,
}

// ============================================================================
// Main Models
// ============================================================================

/// Notification channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub name: String,
    pub channel_type: ChannelType,
    pub config: serde_json::Value,
    pub is_enabled: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub rule_type: AlertRuleType,
    pub conditions: Vec<AlertCondition>,
    pub condition_operator: ConditionOperator,
    pub severity: AlertSeverity,
    pub cooldown_minutes: i32,
    pub is_enabled: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Associated notification channels (populated on fetch)
    #[serde(default)]
    pub channels: Vec<Uuid>,
}

/// Alert instance (a triggered alert)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub title: String,
    pub message: String,
    pub severity: AlertSeverity,
    pub context: Option<serde_json::Value>,
    pub status: AlertStatus,
    pub acknowledged_by: Option<Uuid>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_at: DateTime<Utc>,
    pub last_notified_at: Option<DateTime<Utc>>,
}

/// Notification delivery history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationHistory {
    pub id: Uuid,
    pub alert_id: Uuid,
    pub channel_id: Uuid,
    pub status: NotificationStatus,
    pub attempt_count: i32,
    pub response_code: Option<i32>,
    pub response_body: Option<String>,
    pub error_message: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Alert silence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSilence {
    pub id: Uuid,
    pub rule_id: Option<Uuid>,
    pub matchers: Option<serde_json::Value>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub reason: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Request Types
// ============================================================================

/// Request to create a notification channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: ChannelType,
    pub config: serde_json::Value,
    #[serde(default = "default_enabled")]
    pub is_enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Request to update a notification channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub config: Option<serde_json::Value>,
    pub is_enabled: Option<bool>,
}

/// Request to create an alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAlertRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub rule_type: AlertRuleType,
    pub conditions: Vec<AlertCondition>,
    #[serde(default)]
    pub condition_operator: ConditionOperator,
    #[serde(default)]
    pub severity: AlertSeverity,
    #[serde(default = "default_cooldown")]
    pub cooldown_minutes: i32,
    #[serde(default = "default_enabled")]
    pub is_enabled: bool,
    #[serde(default)]
    pub channel_ids: Vec<Uuid>,
}

fn default_cooldown() -> i32 {
    60
}

/// Request to update an alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAlertRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub conditions: Option<Vec<AlertCondition>>,
    pub condition_operator: Option<ConditionOperator>,
    pub severity: Option<AlertSeverity>,
    pub cooldown_minutes: Option<i32>,
    pub is_enabled: Option<bool>,
    pub channel_ids: Option<Vec<Uuid>>,
}

/// Request to acknowledge an alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcknowledgeAlertRequest {
    pub comment: Option<String>,
}

/// Request to create a silence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSilenceRequest {
    pub rule_id: Option<Uuid>,
    pub matchers: Option<serde_json::Value>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: DateTime<Utc>,
    pub reason: String,
}

/// Request to test a notification channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestChannelRequest {
    pub message: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

/// Alert statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertStats {
    pub total_active: i64,
    pub by_severity: AlertSeverityCount,
    pub total_today: i64,
    pub total_acknowledged: i64,
}

/// Count of alerts by severity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSeverityCount {
    pub info: i64,
    pub warning: i64,
    pub critical: i64,
}

/// Notification channel test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestChannelResponse {
    pub success: bool,
    pub message: String,
    pub response_code: Option<i32>,
}

// ============================================================================
// Webhook Payload Types
// ============================================================================

/// Payload sent to webhook endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event_type: String,
    pub alert: AlertWebhookData,
    pub timestamp: DateTime<Utc>,
}

/// Alert data included in webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertWebhookData {
    pub id: String,
    pub rule_name: String,
    pub title: String,
    pub message: String,
    pub severity: String,
    pub status: String,
    pub triggered_at: DateTime<Utc>,
    pub context: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_conversion() {
        assert_eq!(ChannelType::Webhook.as_str(), "webhook");
        assert_eq!(ChannelType::from_str("slack"), Some(ChannelType::Slack));
        assert_eq!(ChannelType::from_str("invalid"), None);
    }

    #[test]
    fn test_alert_rule_type_conversion() {
        assert_eq!(AlertRuleType::NodeStatus.as_str(), "node_status");
        assert_eq!(
            AlertRuleType::from_str("compliance"),
            Some(AlertRuleType::Compliance)
        );
    }

    #[test]
    fn test_severity_conversion() {
        assert_eq!(AlertSeverity::Critical.as_str(), "critical");
        assert_eq!(
            AlertSeverity::from_str("warning"),
            Some(AlertSeverity::Warning)
        );
    }

    #[test]
    fn test_webhook_config_serialization() {
        let config = WebhookConfig {
            url: "https://example.com/webhook".to_string(),
            method: "POST".to_string(),
            headers: std::collections::HashMap::new(),
            timeout_secs: Some(30),
            retry_count: Some(3),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("https://example.com/webhook"));
    }
}
