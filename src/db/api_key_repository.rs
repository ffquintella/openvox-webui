//! API key repository

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::models::{ApiKey, CreateApiKeyRequest};

#[derive(Debug, sqlx::FromRow)]
struct ApiKeyRow {
    id: String,
    organization_id: String,
    user_id: String,
    name: String,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    created_at: String,
}

pub struct ApiKeyRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ApiKeyRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_for_user(&self, organization_id: Uuid, user_id: Uuid) -> Result<Vec<ApiKey>> {
        let rows = sqlx::query_as::<_, ApiKeyRow>(
            r#"
            SELECT id, organization_id, user_id, name, last_used_at, expires_at, created_at
            FROM api_keys
            WHERE organization_id = ? AND user_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(organization_id.to_string())
        .bind(user_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to list api keys")?;

        let mut keys = Vec::with_capacity(rows.len());
        for row in rows {
            keys.push(self.row_to_api_key(row).await?);
        }
        Ok(keys)
    }

    pub async fn get_by_id(&self, organization_id: Uuid, id: Uuid) -> Result<Option<ApiKey>> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            r#"
            SELECT id, organization_id, user_id, name, last_used_at, expires_at, created_at
            FROM api_keys
            WHERE organization_id = ? AND id = ?
            "#,
        )
        .bind(organization_id.to_string())
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to get api key")?;

        match row {
            Some(row) => Ok(Some(self.row_to_api_key(row).await?)),
            None => Ok(None),
        }
    }

    pub async fn delete(&self, organization_id: Uuid, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM api_keys WHERE organization_id = ? AND id = ?")
            .bind(organization_id.to_string())
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete api key")?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn create_hashed_key(
        &self,
        api_key_id: Uuid,
        organization_id: Uuid,
        user_id: Uuid,
        req: &CreateApiKeyRequest,
        key_hash: &str,
        role_ids: &[Uuid],
    ) -> Result<ApiKey> {
        let created_at = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO api_keys (id, user_id, organization_id, name, key_hash, expires_at, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(api_key_id.to_string())
        .bind(user_id.to_string())
        .bind(organization_id.to_string())
        .bind(&req.name)
        .bind(key_hash)
        .bind(req.expires_at.map(|d| d.to_rfc3339()))
        .bind(created_at.to_rfc3339())
        .execute(self.pool)
        .await
        .context("Failed to create api key")?;

        for role_id in role_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO api_key_roles (id, api_key_id, role_id) VALUES (?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(api_key_id.to_string())
            .bind(role_id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to assign api key roles")?;
        }

        self.get_by_id(organization_id, api_key_id)
            .await?
            .context("Failed to retrieve created api key")
    }

    async fn row_to_api_key(&self, row: ApiKeyRow) -> Result<ApiKey> {
        let id = Uuid::parse_str(&row.id).context("Invalid api key id")?;
        let role_ids = self.get_role_ids(id).await?;

        Ok(ApiKey {
            id,
            organization_id: Uuid::parse_str(&row.organization_id)
                .context("Invalid organization id")?,
            user_id: Uuid::parse_str(&row.user_id).context("Invalid user id")?,
            name: row.name,
            role_ids,
            last_used_at: row.last_used_at.as_deref().map(parse_db_timestamp),
            expires_at: row.expires_at.as_deref().map(parse_db_timestamp),
            created_at: parse_db_timestamp(&row.created_at),
        })
    }

    async fn get_role_ids(&self, api_key_id: Uuid) -> Result<Vec<Uuid>> {
        let rows =
            sqlx::query("SELECT role_id FROM api_key_roles WHERE api_key_id = ? ORDER BY role_id")
                .bind(api_key_id.to_string())
                .fetch_all(self.pool)
                .await
                .context("Failed to fetch api key roles")?;

        Ok(rows
            .into_iter()
            .filter_map(|r| r.try_get::<String, _>("role_id").ok())
            .filter_map(|s| Uuid::parse_str(&s).ok())
            .collect())
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
