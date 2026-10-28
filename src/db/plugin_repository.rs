//! Persistence for plugins, plugin versions and trusted signing keys.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::models::{
    KeyOrigin, PluginGrant, PluginRecord, PluginState, PluginVersionRecord, TrustedKeyRecord,
};

pub struct PluginRepository<'a> {
    pool: &'a SqlitePool,
}

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn plugin_from_row(r: &sqlx::sqlite::SqliteRow) -> PluginRecord {
    PluginRecord {
        id: r.get("id"),
        active_version: r.get("active_version"),
        pending_version: r.get("pending_version"),
        schema_version: r.get::<i64, _>("schema_version").max(0) as u32,
        state: PluginState::parse(&r.get::<String, _>("state")),
        state_reason: r.get("state_reason"),
        installed_by: r.get("installed_by"),
        installed_at: ts(&r.get::<String, _>("installed_at")),
        updated_at: ts(&r.get::<String, _>("updated_at")),
    }
}

fn version_from_row(r: &sqlx::sqlite::SqliteRow) -> PluginVersionRecord {
    PluginVersionRecord {
        plugin_id: r.get("plugin_id"),
        version: r.get("version"),
        api_version: r.get::<i64, _>("api_version") as u32,
        package_sha256: r.get("package_sha256"),
        signed_by_key_id: r.get("signed_by_key_id"),
        signed_at: r.get("signed_at"),
        revoked: r.get::<i64, _>("revoked") != 0,
        added_at: ts(&r.get::<String, _>("added_at")),
        manifest_json: r.get("manifest_json"),
        signature: r.get("signature"),
    }
}

fn key_from_row(r: &sqlx::sqlite::SqliteRow) -> TrustedKeyRecord {
    let origin: String = r.get("origin");
    let allowed: Option<String> = r.get("allowed_plugin_ids");
    let revoked: String = r.get("state");
    TrustedKeyRecord {
        key_id: r.get("key_id"),
        public_key: r.get("public_key"),
        label: r.get("label"),
        origin: if origin == "builtin" {
            KeyOrigin::Builtin
        } else {
            KeyOrigin::Admin
        },
        allowed_plugin_ids: allowed.and_then(|a| serde_json::from_str(&a).ok()),
        expires_at: r.get::<Option<String>, _>("expires_at").map(|s| ts(&s)),
        revoked: revoked == "revoked",
        added_by: r.get("added_by"),
        added_at: ts(&r.get::<String, _>("added_at")),
        revoked_at: r.get::<Option<String>, _>("revoked_at").map(|s| ts(&s)),
        revoked_reason: r.get("revoked_reason"),
    }
}

impl<'a> PluginRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ---- plugins ----------------------------------------------------------

    pub async fn list_plugins(&self) -> Result<Vec<PluginRecord>> {
        let rows = sqlx::query("SELECT * FROM plugins ORDER BY id")
            .fetch_all(self.pool)
            .await
            .context("list plugins")?;
        Ok(rows.iter().map(plugin_from_row).collect())
    }

    pub async fn get_plugin(&self, id: &str) -> Result<Option<PluginRecord>> {
        let row = sqlx::query("SELECT * FROM plugins WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool)
            .await
            .context("get plugin")?;
        Ok(row.as_ref().map(plugin_from_row))
    }

    pub async fn insert_plugin(
        &self,
        id: &str,
        version: &str,
        installed_by: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO plugins (id, active_version, state, installed_by, installed_at, updated_at)
             VALUES (?, ?, 'installed', ?, ?, ?)",
        )
        .bind(id)
        .bind(version)
        .bind(installed_by)
        .bind(&now)
        .bind(&now)
        .execute(self.pool)
        .await
        .context("insert plugin")?;
        Ok(())
    }

    pub async fn set_active_version(&self, id: &str, version: &str) -> Result<()> {
        sqlx::query("UPDATE plugins SET active_version = ?, updated_at = ? WHERE id = ?")
            .bind(version)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(self.pool)
            .await
            .context("set active version")?;
        Ok(())
    }

    pub async fn set_state(
        &self,
        id: &str,
        state: PluginState,
        reason: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE plugins SET state = ?, state_reason = ?, updated_at = ? WHERE id = ?")
            .bind(state.as_str())
            .bind(reason)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(self.pool)
            .await
            .context("set plugin state")?;
        Ok(())
    }

    pub async fn delete_plugin(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM plugin_versions WHERE plugin_id = ?")
            .bind(id)
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM plugins WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await
            .context("delete plugin")?;
        Ok(())
    }

    // ---- versions ---------------------------------------------------------

    pub async fn insert_version(&self, v: &PluginVersionRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_versions (plugin_id, version, api_version, manifest_json,
                package_sha256, signature, signed_by_key_id, signed_at, revoked, added_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(&v.plugin_id)
        .bind(&v.version)
        .bind(v.api_version as i64)
        .bind(&v.manifest_json)
        .bind(&v.package_sha256)
        .bind(&v.signature)
        .bind(&v.signed_by_key_id)
        .bind(&v.signed_at)
        .bind(v.added_at.to_rfc3339())
        .execute(self.pool)
        .await
        .context("insert plugin version")?;
        Ok(())
    }

    pub async fn list_versions(&self, plugin_id: &str) -> Result<Vec<PluginVersionRecord>> {
        let rows =
            sqlx::query("SELECT * FROM plugin_versions WHERE plugin_id = ? ORDER BY added_at DESC")
                .bind(plugin_id)
                .fetch_all(self.pool)
                .await
                .context("list versions")?;
        Ok(rows.iter().map(version_from_row).collect())
    }

    pub async fn get_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> Result<Option<PluginVersionRecord>> {
        let row = sqlx::query("SELECT * FROM plugin_versions WHERE plugin_id = ? AND version = ?")
            .bind(plugin_id)
            .bind(version)
            .fetch_optional(self.pool)
            .await
            .context("get version")?;
        Ok(row.as_ref().map(version_from_row))
    }

    /// Ids of plugins whose active version was signed by `key_id`.
    pub async fn plugins_signed_by(&self, key_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT p.id FROM plugins p JOIN plugin_versions v
               ON v.plugin_id = p.id AND v.version = p.active_version
             WHERE v.signed_by_key_id = ? ORDER BY p.id",
        )
        .bind(key_id)
        .fetch_all(self.pool)
        .await
        .context("plugins signed by key")?;
        Ok(rows.iter().map(|r| r.get("id")).collect())
    }

    // ---- trusted keys -----------------------------------------------------

    pub async fn list_keys(&self) -> Result<Vec<TrustedKeyRecord>> {
        let rows = sqlx::query("SELECT * FROM plugin_trusted_keys ORDER BY added_at")
            .fetch_all(self.pool)
            .await
            .context("list keys")?;
        Ok(rows.iter().map(key_from_row).collect())
    }

    pub async fn get_key(&self, key_id: &str) -> Result<Option<TrustedKeyRecord>> {
        let row = sqlx::query("SELECT * FROM plugin_trusted_keys WHERE key_id = ?")
            .bind(key_id)
            .fetch_optional(self.pool)
            .await
            .context("get key")?;
        Ok(row.as_ref().map(key_from_row))
    }

    pub async fn insert_key(&self, k: &TrustedKeyRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_trusted_keys (key_id, public_key, label, origin, allowed_plugin_ids,
                expires_at, state, added_by, added_at)
             VALUES (?, ?, ?, ?, ?, ?, 'active', ?, ?)",
        )
        .bind(&k.key_id)
        .bind(&k.public_key)
        .bind(&k.label)
        .bind(k.origin.as_str())
        .bind(
            k.allowed_plugin_ids
                .as_ref()
                .map(|a| serde_json::to_string(a).unwrap_or_default()),
        )
        .bind(k.expires_at.map(|d| d.to_rfc3339()))
        .bind(&k.added_by)
        .bind(k.added_at.to_rfc3339())
        .execute(self.pool)
        .await
        .context("insert key")?;
        Ok(())
    }

    pub async fn update_key(&self, k: &TrustedKeyRecord) -> Result<()> {
        sqlx::query(
            "UPDATE plugin_trusted_keys SET label = ?, allowed_plugin_ids = ?, expires_at = ?
             WHERE key_id = ?",
        )
        .bind(&k.label)
        .bind(
            k.allowed_plugin_ids
                .as_ref()
                .map(|a| serde_json::to_string(a).unwrap_or_default()),
        )
        .bind(k.expires_at.map(|d| d.to_rfc3339()))
        .bind(&k.key_id)
        .execute(self.pool)
        .await
        .context("update key")?;
        Ok(())
    }

    pub async fn revoke_key(&self, key_id: &str, reason: &str) -> Result<()> {
        sqlx::query(
            "UPDATE plugin_trusted_keys SET state = 'revoked', revoked_at = ?, revoked_reason = ?
             WHERE key_id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(reason)
        .bind(key_id)
        .execute(self.pool)
        .await
        .context("revoke key")?;
        Ok(())
    }

    pub async fn delete_key(&self, key_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM plugin_trusted_keys WHERE key_id = ?")
            .bind(key_id)
            .execute(self.pool)
            .await
            .context("delete key")?;
        Ok(())
    }

    // ---- grants, pending version, schema version, config -----------------

    pub async fn list_grants(&self, plugin_id: &str) -> Result<Vec<PluginGrant>> {
        let rows =
            sqlx::query("SELECT * FROM plugin_grants WHERE plugin_id = ? ORDER BY capability")
                .bind(plugin_id)
                .fetch_all(self.pool)
                .await
                .context("list grants")?;
        Ok(rows
            .iter()
            .map(|r| PluginGrant {
                plugin_id: r.get("plugin_id"),
                capability: r.get("capability"),
                scope: r
                    .get::<Option<String>, _>("scope_json")
                    .and_then(|j| serde_json::from_str(&j).ok()),
                granted_by: r.get("granted_by"),
                granted_at: ts(&r.get::<String, _>("granted_at")),
            })
            .collect())
    }

    /// Replaces every grant of the plugin in one transaction.
    pub async fn replace_grants(
        &self,
        plugin_id: &str,
        grants: &[(String, Option<serde_json::Value>)],
        granted_by: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM plugin_grants WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(&mut *tx)
            .await?;
        let now = Utc::now().to_rfc3339();
        for (cap, scope) in grants {
            sqlx::query(
                "INSERT INTO plugin_grants (plugin_id, capability, scope_json, granted_by, granted_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(plugin_id)
            .bind(cap)
            .bind(scope.as_ref().map(|v| v.to_string()))
            .bind(granted_by)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await.context("replace grants")?;
        Ok(())
    }

    pub async fn delete_grants(&self, plugin_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM plugin_grants WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(self.pool)
            .await
            .context("delete grants")?;
        Ok(())
    }

    pub async fn set_pending_version(&self, id: &str, version: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE plugins SET pending_version = ?, updated_at = ? WHERE id = ?")
            .bind(version)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(self.pool)
            .await
            .context("set pending version")?;
        Ok(())
    }

    pub async fn set_schema_version(&self, id: &str, v: u32) -> Result<()> {
        sqlx::query("UPDATE plugins SET schema_version = ? WHERE id = ?")
            .bind(v as i64)
            .bind(id)
            .execute(self.pool)
            .await
            .context("set schema version")?;
        Ok(())
    }

    pub async fn get_config(
        &self,
        plugin_id: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let rows = sqlx::query("SELECT key, value_json FROM plugin_config WHERE plugin_id = ?")
            .bind(plugin_id)
            .fetch_all(self.pool)
            .await
            .context("get config")?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let v = serde_json::from_str(&r.get::<String, _>("value_json")).ok()?;
                Some((r.get::<String, _>("key"), v))
            })
            .collect())
    }

    /// Replaces the plugin settings.
    pub async fn replace_config(
        &self,
        plugin_id: &str,
        values: &serde_json::Map<String, serde_json::Value>,
        by: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM plugin_config WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(&mut *tx)
            .await?;
        let now = Utc::now().to_rfc3339();
        for (k, v) in values {
            sqlx::query(
                "INSERT INTO plugin_config (plugin_id, key, value_json, updated_by, updated_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(plugin_id)
            .bind(k)
            .bind(v.to_string())
            .bind(by)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await.context("replace config")?;
        Ok(())
    }
}
