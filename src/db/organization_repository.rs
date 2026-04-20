//! Organization (tenant) repository

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{CreateOrganizationRequest, Organization, UpdateOrganizationRequest};

#[derive(Debug, sqlx::FromRow)]
struct OrganizationRow {
    id: String,
    name: String,
    slug: String,
    created_at: String,
    updated_at: String,
}

pub struct OrganizationRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> OrganizationRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Organization>> {
        let rows = sqlx::query_as::<_, OrganizationRow>(
            r#"
            SELECT id, name, slug, created_at, updated_at
            FROM organizations
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to list organizations")?;

        Ok(rows.into_iter().map(row_to_org).collect())
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<Organization>> {
        let row = sqlx::query_as::<_, OrganizationRow>(
            r#"
            SELECT id, name, slug, created_at, updated_at
            FROM organizations
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to get organization")?;

        Ok(row.map(row_to_org))
    }

    pub async fn create(&self, req: &CreateOrganizationRequest) -> Result<Organization> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO organizations (id, name, slug, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.slug)
        .bind(&now)
        .bind(&now)
        .execute(self.pool)
        .await
        .context("Failed to create organization")?;

        self.get_by_id(id)
            .await?
            .context("Failed to retrieve created organization")
    }

    pub async fn update(
        &self,
        id: Uuid,
        req: &UpdateOrganizationRequest,
    ) -> Result<Option<Organization>> {
        let existing = self.get_by_id(id).await?;
        let Some(existing) = existing else {
            return Ok(None);
        };

        let name = req.name.clone().unwrap_or(existing.name);
        let slug = req.slug.clone().unwrap_or(existing.slug);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE organizations
            SET name = ?, slug = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&name)
        .bind(&slug)
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update organization")?;

        self.get_by_id(id).await
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM organizations WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete organization")?;

        Ok(result.rows_affected() > 0)
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

fn row_to_org(row: OrganizationRow) -> Organization {
    Organization {
        id: Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::nil()),
        name: row.name,
        slug: row.slug,
        created_at: parse_db_timestamp(&row.created_at),
        updated_at: parse_db_timestamp(&row.updated_at),
    }
}
