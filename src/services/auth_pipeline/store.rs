//! Persistence of enrolled factors. Data is sealed; only the host opens it.

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::crypto;

#[derive(Debug, Clone, Serialize)]
pub struct FactorSummary {
    pub id: Uuid,
    pub kind: String,
    pub label: String,
    pub verified: bool,
    pub created_at: String,
    pub last_used_at: Option<String>,
    /// Recovery codes still unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<usize>,
}

/// A stored record with its sealed form, for compare-and-set updates.
#[derive(Debug, Clone)]
pub struct StoredRecord {
    pub id: String,
    pub kind: String,
    pub sealed: String,
    pub data: Value,
}

pub struct FactorStore<'a> {
    pub pool: &'a SqlitePool,
    pub key: &'a str,
}

impl<'a> FactorStore<'a> {
    pub fn new(pool: &'a SqlitePool, key: &'a str) -> Self {
        Self { pool, key }
    }

    pub async fn list(&self, user: Uuid) -> Result<Vec<FactorSummary>> {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(String, String, String, String, i64, String, Option<String>)> =
            sqlx::query_as(
                "SELECT id, kind, label, data_encrypted, verified, created_at, last_used_at \
                 FROM user_factors WHERE user_id = ? ORDER BY created_at",
            )
            .bind(user.to_string())
            .fetch_all(self.pool)
            .await?;
        rows.into_iter()
            .map(
                |(id, kind, label, sealed, verified, created_at, last_used_at)| {
                    let remaining = if kind == "recovery" {
                        let data: Value = crypto::open_json(self.key, &sealed)?;
                        Some(
                            data.get("codes")
                                .and_then(Value::as_array)
                                .map(|c| {
                                    c.iter()
                                        .filter(|e| {
                                            !e.get("used").and_then(Value::as_bool).unwrap_or(true)
                                        })
                                        .count()
                                })
                                .unwrap_or(0),
                        )
                    } else {
                        None
                    };
                    Ok(FactorSummary {
                        id: Uuid::parse_str(&id)?,
                        kind,
                        label,
                        verified: verified != 0,
                        created_at,
                        last_used_at,
                        remaining,
                    })
                },
            )
            .collect()
    }

    /// Kinds with at least one verified record. Recovery codes only count when
    /// one is left.
    pub async fn usable_kinds(&self, user: Uuid) -> Result<Vec<String>> {
        let mut kinds: Vec<String> = Vec::new();
        for f in self.list(user).await? {
            if !f.verified || f.remaining == Some(0) {
                continue;
            }
            if !kinds.contains(&f.kind) {
                kinds.push(f.kind);
            }
        }
        Ok(kinds)
    }

    pub async fn records(
        &self,
        user: Uuid,
        kind: &str,
        verified: bool,
    ) -> Result<Vec<StoredRecord>> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id, kind, data_encrypted FROM user_factors \
             WHERE user_id = ? AND kind = ? AND verified = ? ORDER BY created_at",
        )
        .bind(user.to_string())
        .bind(kind)
        .bind(verified as i64)
        .fetch_all(self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, kind, sealed)| {
                let data = crypto::open_json(self.key, &sealed)?;
                Ok(StoredRecord {
                    id,
                    kind,
                    sealed,
                    data,
                })
            })
            .collect()
    }

    /// Stores a pending (unverified) enrollment, replacing earlier ones.
    pub async fn put_pending(&self, user: Uuid, kind: &str, data: &Value) -> Result<StoredRecord> {
        sqlx::query("DELETE FROM user_factors WHERE user_id = ? AND kind = ? AND verified = 0")
            .bind(user.to_string())
            .bind(kind)
            .execute(self.pool)
            .await?;
        self.insert(user, kind, "", data, false).await
    }

    pub async fn insert(
        &self,
        user: Uuid,
        kind: &str,
        label: &str,
        data: &Value,
        verified: bool,
    ) -> Result<StoredRecord> {
        let id = Uuid::new_v4().to_string();
        let sealed = crypto::seal_json(self.key, data)?;
        sqlx::query(
            "INSERT INTO user_factors (id, user_id, kind, label, data_encrypted, verified, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(user.to_string())
        .bind(kind)
        .bind(label)
        .bind(&sealed)
        .bind(verified as i64)
        .bind(Utc::now().to_rfc3339())
        .execute(self.pool)
        .await?;
        Ok(StoredRecord {
            id,
            kind: kind.to_string(),
            sealed,
            data: data.clone(),
        })
    }

    /// Turns a pending record into a verified one.
    pub async fn finalize(&self, rec: &StoredRecord, label: &str, data: &Value) -> Result<()> {
        let sealed = crypto::seal_json(self.key, data)?;
        let r = sqlx::query(
            "UPDATE user_factors SET data_encrypted = ?, label = ?, verified = 1 \
             WHERE id = ? AND data_encrypted = ? AND verified = 0",
        )
        .bind(sealed)
        .bind(label)
        .bind(&rec.id)
        .bind(&rec.sealed)
        .execute(self.pool)
        .await?;
        if r.rows_affected() != 1 {
            return Err(anyhow!("enrollment already completed"));
        }
        Ok(())
    }

    /// Compare-and-set: returns false when the record changed meanwhile, which
    /// is how a replayed code or assertion loses.
    pub async fn update_if_unchanged(&self, rec: &StoredRecord, data: &Value) -> Result<bool> {
        let sealed = crypto::seal_json(self.key, data)?;
        let r = sqlx::query(
            "UPDATE user_factors SET data_encrypted = ?, last_used_at = ? \
             WHERE id = ? AND data_encrypted = ?",
        )
        .bind(sealed)
        .bind(Utc::now().to_rfc3339())
        .bind(&rec.id)
        .bind(&rec.sealed)
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn touch(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE user_factors SET last_used_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete(&self, user: Uuid, id: &str) -> Result<bool> {
        let r = sqlx::query("DELETE FROM user_factors WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user.to_string())
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn delete_all(&self, user: Uuid) -> Result<u64> {
        let r = sqlx::query("DELETE FROM user_factors WHERE user_id = ?")
            .bind(user.to_string())
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    pub async fn replace_recovery(&self, user: Uuid, data: &Value) -> Result<()> {
        sqlx::query("DELETE FROM user_factors WHERE user_id = ? AND kind = 'recovery'")
            .bind(user.to_string())
            .execute(self.pool)
            .await?;
        self.insert(user, "recovery", "Recovery codes", data, true)
            .await?;
        Ok(())
    }
}
