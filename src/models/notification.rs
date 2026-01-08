//! Notification model and types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Notification type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "lowercase")]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationType {
    pub fn as_str(&self) -> &str {
        match self {
            NotificationType::Info => "info",
            NotificationType::Success => "success",
            NotificationType::Warning => "warning",
            NotificationType::Error => "error",
        }
    }
}

impl std::fmt::Display for NotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for NotificationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "info" => Ok(NotificationType::Info),
            "success" => Ok(NotificationType::Success),
            "warning" => Ok(NotificationType::Warning),
            "error" => Ok(NotificationType::Error),
            _ => Err(format!("Invalid notification type: {}", s)),
        }
    }
}

/// Notification model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    pub organization_id: Option<String>,
    pub title: String,
    pub message: String,
    pub r#type: NotificationType,
    pub category: Option<String>,
    pub link: Option<String>,
    pub read: bool,
    pub dismissed: bool,
    pub created_at: String,
    pub read_at: Option<String>,
    pub expires_at: Option<String>,
    pub metadata: Option<String>,
}

impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for Notification {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            organization_id: row.try_get("organization_id")?,
            title: row.try_get("title")?,
            message: row.try_get("message")?,
            r#type: {
                let type_str: String = row.try_get("type")?;
                type_str.parse().map_err(|e: String| {
                    sqlx::Error::ColumnDecode {
                        index: "type".to_string(),
                        source: Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            e,
                        )),
                    }
                })?
            },
            category: row.try_get("category")?,
            link: row.try_get("link")?,
            read: {
                let read_int: i64 = row.try_get("read")?;
                read_int != 0
            },
            dismissed: {
                let dismissed_int: i64 = row.try_get("dismissed")?;
                dismissed_int != 0
            },
            created_at: row.try_get("created_at")?,
            read_at: row.try_get("read_at")?,
            expires_at: row.try_get("expires_at")?,
            metadata: row.try_get("metadata")?,
        })
    }
}

/// Create notification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    pub title: String,
    pub message: String,
    pub r#type: NotificationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Notification query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<NotificationType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

/// Mark notification as read request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkNotificationReadRequest {
    pub read: bool,
}

/// Bulk mark notifications request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkMarkReadRequest {
    pub notification_ids: Vec<String>,
    pub read: bool,
}

/// Notification statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationStats {
    pub total: i64,
    pub unread: i64,
    pub by_type: std::collections::HashMap<String, i64>,
}

/// Notification event for WebSocket/SSE
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum NotificationEvent {
    #[serde(rename = "new")]
    New { notification: Notification },
    #[serde(rename = "updated")]
    Updated { notification: Notification },
    #[serde(rename = "deleted")]
    Deleted { notification_id: String },
    #[serde(rename = "bulk_read")]
    BulkRead { notification_ids: Vec<String> },
}

impl Notification {
    /// Check if notification has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = &self.expires_at {
            if let Ok(expires) = DateTime::parse_from_rfc3339(expires_at) {
                return expires.with_timezone(&Utc) < Utc::now();
            }
        }
        false
    }

    /// Parse metadata as JSON
    pub fn metadata_json(&self) -> Option<serde_json::Value> {
        self.metadata
            .as_ref()
            .and_then(|m| serde_json::from_str(m).ok())
    }
}
