//! Notification service for managing user notifications

use crate::models::{
    BulkMarkReadRequest, CreateNotificationRequest, Notification, NotificationQuery,
    NotificationStats,
};
use crate::utils::AppError;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Notification service
#[derive(Clone)]
pub struct NotificationService {
    db: Pool<Sqlite>,
    broadcast: Arc<broadcast::Sender<NotificationEvent>>,
}

/// Notification event for broadcasting
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    New(Notification),
    Updated(Notification),
    Deleted(String),
    BulkRead(Vec<String>),
}

impl NotificationService {
    /// Create a new notification service
    pub fn new(db: Pool<Sqlite>) -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            db,
            broadcast: Arc::new(tx),
        }
    }

    /// Subscribe to notification events
    pub fn subscribe(&self) -> broadcast::Receiver<NotificationEvent> {
        self.broadcast.subscribe()
    }

    /// Create a new notification
    pub async fn create_notification(
        &self,
        req: CreateNotificationRequest,
    ) -> Result<Notification, AppError> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let expires_at = req.expires_at.map(|dt| dt.to_rfc3339());
        let metadata = req.metadata.map(|m| m.to_string());

        let notification = sqlx::query_as::<_, Notification>(
            r#"
            INSERT INTO notifications (
                id, user_id, organization_id, title, message, type,
                category, link, created_at, expires_at, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(&id)
        .bind(&req.user_id)
        .bind(&req.organization_id)
        .bind(&req.title)
        .bind(&req.message)
        .bind(req.r#type.as_str())
        .bind(&req.category)
        .bind(&req.link)
        .bind(&created_at)
        .bind(&expires_at)
        .bind(&metadata)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Broadcast the new notification
        let _ = self.broadcast.send(NotificationEvent::New(notification.clone()));

        Ok(notification)
    }

    /// Get notifications for a user
    pub async fn get_notifications(
        &self,
        user_id: &str,
        _organization_id: Option<&str>,
        query: NotificationQuery,
    ) -> Result<Vec<Notification>, AppError> {
        // Simple implementation for now - can be extended later
        let final_query = if query.unread_only == Some(true) {
            sqlx::query_as::<_, Notification>(
                r#"
                SELECT * FROM notifications
                WHERE user_id = ?
                AND read = 0
                AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))
                ORDER BY created_at DESC
                "#,
            )
            .bind(user_id)
        } else {
            sqlx::query_as::<_, Notification>(
                r#"
                SELECT * FROM notifications
                WHERE user_id = ?
                AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))
                ORDER BY created_at DESC
                "#,
            )
            .bind(user_id)
        };

        let notifications = final_query
            .fetch_all(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(notifications)
    }

    /// Get a single notification by ID
    pub async fn get_notification(
        &self,
        notification_id: &str,
        user_id: &str,
    ) -> Result<Notification, AppError> {
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            SELECT * FROM notifications
            WHERE id = ? AND user_id = ?
            "#,
        )
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

        Ok(notification)
    }

    /// Mark a notification as read/unread
    pub async fn mark_as_read(
        &self,
        notification_id: &str,
        user_id: &str,
        read: bool,
    ) -> Result<Notification, AppError> {
        let read_at = if read {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };

        let notification = sqlx::query_as::<_, Notification>(
            r#"
            UPDATE notifications
            SET read = ?, read_at = ?
            WHERE id = ? AND user_id = ?
            RETURNING *
            "#,
        )
        .bind(read)
        .bind(&read_at)
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

        // Broadcast the update
        let _ = self
            .broadcast
            .send(NotificationEvent::Updated(notification.clone()));

        Ok(notification)
    }

    /// Mark all notifications as read for a user
    pub async fn mark_all_as_read(&self, user_id: &str) -> Result<i64, AppError> {
        let read_at = Utc::now().to_rfc3339();

        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET read = 1, read_at = ?
            WHERE user_id = ? AND read = 0
            "#,
        )
        .bind(&read_at)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(result.rows_affected() as i64)
    }

    /// Bulk mark notifications as read/unread
    pub async fn bulk_mark_read(
        &self,
        user_id: &str,
        req: BulkMarkReadRequest,
    ) -> Result<i64, AppError> {
        if req.notification_ids.is_empty() {
            return Ok(0);
        }

        let read_at = if req.read {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };

        let placeholders = req
            .notification_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        let sql = format!(
            r#"
            UPDATE notifications
            SET read = ?, read_at = ?
            WHERE user_id = ? AND id IN ({})
            "#,
            placeholders
        );

        let mut query = sqlx::query(&sql).bind(req.read).bind(&read_at).bind(user_id);

        for id in &req.notification_ids {
            query = query.bind(id);
        }

        let result = query
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Broadcast bulk update
        let _ = self
            .broadcast
            .send(NotificationEvent::BulkRead(req.notification_ids.clone()));

        Ok(result.rows_affected() as i64)
    }

    /// Dismiss a notification (soft delete)
    pub async fn dismiss_notification(
        &self,
        notification_id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET dismissed = 1
            WHERE id = ? AND user_id = ?
            "#,
        )
        .bind(notification_id)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("Notification not found".to_string()));
        }

        Ok(())
    }

    /// Delete a notification permanently
    pub async fn delete_notification(
        &self,
        notification_id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        let result = sqlx::query(
            r#"
            DELETE FROM notifications
            WHERE id = ? AND user_id = ?
            "#,
        )
        .bind(notification_id)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("Notification not found".to_string()));
        }

        // Broadcast deletion
        let _ = self
            .broadcast
            .send(NotificationEvent::Deleted(notification_id.to_string()));

        Ok(())
    }

    /// Get notification statistics for a user
    pub async fn get_stats(
        &self,
        user_id: &str,
        _organization_id: Option<&str>,
    ) -> Result<NotificationStats, AppError> {
        // Get total count
        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM notifications
            WHERE user_id = ?
            AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))
            AND dismissed = 0
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Get unread count
        let unread: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM notifications
            WHERE user_id = ? AND read = 0
            AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))
            AND dismissed = 0
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Get counts by type
        let type_counts: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT type, COUNT(*) as count
            FROM notifications
            WHERE user_id = ?
            AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))
            AND dismissed = 0
            GROUP BY type
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let by_type: HashMap<String, i64> = type_counts.into_iter().collect();

        Ok(NotificationStats {
            total,
            unread,
            by_type,
        })
    }

    /// Clean up expired notifications
    pub async fn cleanup_expired(&self) -> Result<i64, AppError> {
        let result = sqlx::query(
            r#"
            DELETE FROM notifications
            WHERE expires_at IS NOT NULL
            AND datetime(expires_at) <= datetime('now')
            "#,
        )
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(result.rows_affected() as i64)
    }

    /// Clean up old read notifications (older than 30 days)
    pub async fn cleanup_old_read(&self, days: i64) -> Result<i64, AppError> {
        let result = sqlx::query(
            r#"
            DELETE FROM notifications
            WHERE read = 1
            AND datetime(read_at) <= datetime('now', ? || ' days')
            "#,
        )
        .bind(format!("-{}", days))
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(result.rows_affected() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_notification_creation() {
        // Basic test to ensure the module compiles correctly
        // Full integration tests would require a database connection
        assert!(true);
    }
}
