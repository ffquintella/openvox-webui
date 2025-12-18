//! Audit log repository

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{AuditLogEntry, AuditLogQuery};

#[derive(Debug, sqlx::FromRow)]
struct AuditRow {
    id: String,
    organization_id: String,
    user_id: Option<String>,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    details: Option<String>,
    ip_address: Option<String>,
    created_at: String,
}

pub struct AuditRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AuditRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(
        &self,
        organization_id: Uuid,
        user_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: Option<&serde_json::Value>,
        ip_address: Option<&str>,
    ) -> Result<AuditLogEntry> {
        let id = Uuid::new_v4();
        let created_at = Utc::now().to_rfc3339();
        let details_str = details.map(|d| d.to_string());

        sqlx::query(
            r#"
            INSERT INTO audit_log (id, organization_id, user_id, action, resource_type, resource_id, details, ip_address, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(organization_id.to_string())
        .bind(user_id.map(|u| u.to_string()))
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(details_str.as_deref())
        .bind(ip_address)
        .bind(&created_at)
        .execute(self.pool)
        .await
        .context("Failed to insert audit log entry")?;

        Ok(AuditLogEntry {
            id,
            organization_id,
            user_id,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.map(|s| s.to_string()),
            details: details.map(|d| d.clone()),
            ip_address: ip_address.map(|s| s.to_string()),
            created_at: parse_db_timestamp(&created_at),
        })
    }

    pub async fn list(
        &self,
        organization_id: Uuid,
        query: &AuditLogQuery,
    ) -> Result<Vec<AuditLogEntry>> {
        let mut sql = String::from(
            "SELECT id, organization_id, user_id, action, resource_type, resource_id, details, ip_address, created_at FROM audit_log WHERE organization_id = ?",
        );

        if query.user_id.is_some() {
            sql.push_str(" AND user_id = ?");
        }
        if query.resource_type.is_some() {
            sql.push_str(" AND resource_type = ?");
        }
        if query.action.is_some() {
            sql.push_str(" AND action = ?");
        }

        sql.push_str(" ORDER BY created_at DESC");

        if query.limit.is_some() {
            sql.push_str(" LIMIT ?");
        } else {
            sql.push_str(" LIMIT 100");
        }
        if query.offset.is_some() {
            sql.push_str(" OFFSET ?");
        }

        let mut q = sqlx::query_as::<_, AuditRow>(&sql).bind(organization_id.to_string());
        if let Some(user_id) = query.user_id {
            q = q.bind(user_id.to_string());
        }
        if let Some(ref resource_type) = query.resource_type {
            q = q.bind(resource_type);
        }
        if let Some(ref action) = query.action {
            q = q.bind(action);
        }
        if let Some(limit) = query.limit {
            q = q.bind(limit as i64);
        }
        if let Some(offset) = query.offset {
            q = q.bind(offset as i64);
        }

        let rows = q
            .fetch_all(self.pool)
            .await
            .context("Failed to list audit logs")?;

        Ok(rows.into_iter().map(row_to_audit).collect())
    }
}

fn parse_db_timestamp(ts: &str) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return dt.with_timezone(&Utc);
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
    }
    Utc::now()
}

fn row_to_audit(row: AuditRow) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::nil()),
        organization_id: Uuid::parse_str(&row.organization_id).unwrap_or_else(|_| Uuid::nil()),
        user_id: row.user_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        action: row.action,
        resource_type: row.resource_type,
        resource_id: row.resource_id,
        details: row.details.and_then(|s| serde_json::from_str(&s).ok()),
        ip_address: row.ip_address,
        created_at: parse_db_timestamp(&row.created_at),
    }
}
