//! SCIM bearer tokens: `ovscim_<id>_<secret>`.
//!
//! The secret is 32 random bytes; only a SHA-256 hash is stored (the secret
//! has full entropy, so a slow hash adds nothing) and it is compared in
//! constant time. A token is shown once, when it is created or rotated.

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::services::auth_pipeline::crypto::ct_eq;

pub const TOKEN_PREFIX: &str = "ovscim_";
const HASH_DOMAIN: &[u8] = b"openvox-webui/scim-token/v1\0";
const MAX_SECRET_LEN: usize = 128;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    /// Last characters of the secret, to tell tokens apart.
    pub token_hint: String,
    pub created_by: Option<String>,
    pub created_at: String,
    pub rotated_at: Option<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
    pub revoked_by: Option<String>,
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

impl TokenInfo {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at
            .as_deref()
            .and_then(parse_ts)
            .is_some_and(|e| now >= e)
    }

    /// `active`, `expired` or `revoked`.
    pub fn status(&self, now: DateTime<Utc>) -> &'static str {
        if self.revoked_at.is_some() {
            "revoked"
        } else if self.is_expired(now) {
            "expired"
        } else {
            "active"
        }
    }
}

/// A token as returned once to the administrator.
#[derive(Debug, Clone, Serialize)]
pub struct NewToken {
    pub token: String,
    pub info: TokenInfo,
}

fn hash_secret(secret: &str) -> String {
    let mut h = Sha256::new();
    h.update(HASH_DOMAIN);
    h.update(secret.as_bytes());
    hex::encode(h.finalize())
}

fn new_secret() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hint(secret: &str) -> String {
    secret
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Splits a presented token into `(id, secret)`.
pub fn parse(token: &str) -> Option<(&str, &str)> {
    let rest = token.strip_prefix(TOKEN_PREFIX)?;
    if rest.len() < 34 || !rest.is_char_boundary(32) || !rest.is_char_boundary(33) {
        return None;
    }
    let (id, tail) = rest.split_at(32);
    let secret = tail.strip_prefix('_')?;
    if !id
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        || secret.is_empty()
        || secret.len() > MAX_SECRET_LEN
    {
        return None;
    }
    Some((id, secret))
}

const COLUMNS: &str = "id, name, token_hint, created_by, created_at, rotated_at, expires_at, \
                       last_used_at, revoked_at, revoked_by";

pub async fn list(pool: &SqlitePool) -> Result<Vec<TokenInfo>, sqlx::Error> {
    sqlx::query_as::<_, TokenInfo>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM scim_tokens ORDER BY created_at DESC, id"
    )))
    .fetch_all(pool)
    .await
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<TokenInfo>, sqlx::Error> {
    sqlx::query_as::<_, TokenInfo>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM scim_tokens WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Checks a token name typed by an administrator.
pub fn validate_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 100 || n.chars().any(|c| c.is_control()) {
        return Err("The token name must have 1 to 100 printable characters".into());
    }
    Ok(n.to_string())
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    expires_at: Option<DateTime<Utc>>,
    created_by: Option<Uuid>,
) -> Result<NewToken> {
    let id = Uuid::new_v4().simple().to_string();
    let secret = new_secret();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO scim_tokens (id, name, token_hash, token_hint, created_by, created_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(hash_secret(&secret))
    .bind(hint(&secret))
    .bind(created_by.map(|u| u.to_string()))
    .bind(&now)
    .bind(expires_at.map(|e| e.to_rfc3339()))
    .execute(pool)
    .await?;
    let info = get(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("token vanished"))?;
    Ok(NewToken {
        token: format!("{TOKEN_PREFIX}{id}_{secret}"),
        info,
    })
}

/// Replaces the secret of a token that is not revoked; the old secret stops
/// working at once. `None` when there is no such token.
pub async fn rotate(
    pool: &SqlitePool,
    id: &str,
    expires_at: Option<DateTime<Utc>>,
) -> Result<Option<NewToken>> {
    let secret = new_secret();
    let now = Utc::now().to_rfc3339();
    let changed = sqlx::query(
        "UPDATE scim_tokens SET token_hash = ?, token_hint = ?, rotated_at = ?, expires_at = ? \
         WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(hash_secret(&secret))
    .bind(hint(&secret))
    .bind(&now)
    .bind(expires_at.map(|e| e.to_rfc3339()))
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(None);
    }
    let info = get(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("token vanished"))?;
    Ok(Some(NewToken {
        token: format!("{TOKEN_PREFIX}{id}_{secret}"),
        info,
    }))
}

/// Revokes a token; `false` when it does not exist or is already revoked.
pub async fn revoke(pool: &SqlitePool, id: &str, by: Option<Uuid>) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE scim_tokens SET revoked_at = ?, revoked_by = ? WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(by.map(|u| u.to_string()))
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

pub async fn touch(pool: &SqlitePool, id: &str) {
    let _ = sqlx::query("UPDATE scim_tokens SET last_used_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(pool)
        .await;
}

/// Outcome of presenting a bearer token.
#[derive(Debug)]
pub enum AuthResult {
    Valid(Box<TokenInfo>),
    /// The reason goes to the audit log, never to the client.
    Invalid(&'static str),
}

/// Verifies a presented token. Every failure costs one hash and one
/// constant-time comparison, whether the id exists or not.
pub async fn authenticate(pool: &SqlitePool, presented: &str) -> Result<AuthResult, sqlx::Error> {
    let dummy = hash_secret("openvox-scim-dummy");
    let Some((id, secret)) = parse(presented) else {
        let _ = ct_eq(
            hash_secret(&presented.chars().take(MAX_SECRET_LEN).collect::<String>()).as_bytes(),
            dummy.as_bytes(),
        );
        return Ok(AuthResult::Invalid("malformed_token"));
    };
    let computed = hash_secret(secret);
    let row: Option<(String,)> = sqlx::query_as("SELECT token_hash FROM scim_tokens WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let Some((stored,)) = row else {
        let _ = ct_eq(computed.as_bytes(), dummy.as_bytes());
        return Ok(AuthResult::Invalid("unknown_token"));
    };
    if !ct_eq(computed.as_bytes(), stored.as_bytes()) {
        return Ok(AuthResult::Invalid("wrong_secret"));
    }
    let Some(info) = get(pool, id).await? else {
        return Ok(AuthResult::Invalid("unknown_token"));
    };
    if info.revoked_at.is_some() {
        return Ok(AuthResult::Invalid("revoked_token"));
    }
    if info.is_expired(Utc::now()) {
        return Ok(AuthResult::Invalid("expired_token"));
    }
    Ok(AuthResult::Valid(Box::new(info)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_parse_only_in_the_expected_shape() {
        let id = Uuid::new_v4().simple().to_string();
        let secret = new_secret();
        let token = format!("{TOKEN_PREFIX}{id}_{secret}");
        assert_eq!(parse(&token), Some((id.as_str(), secret.as_str())));
        assert!(parse("ovk_abc_def").is_none());
        assert!(parse(&format!("{TOKEN_PREFIX}{id}_")).is_none());
        assert!(parse(&format!("{TOKEN_PREFIX}{}_{secret}", id.to_uppercase())).is_none());
        assert!(parse(&format!("{TOKEN_PREFIX}{id}{secret}")).is_none());
        assert!(parse(&format!("{TOKEN_PREFIX}{id}_{}", "a".repeat(200))).is_none());
        assert!(parse(&format!("{TOKEN_PREFIX}é{id}_{secret}")).is_none());
    }

    #[test]
    fn secrets_are_random_and_hashed_with_a_domain() {
        let a = new_secret();
        let b = new_secret();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(hash_secret(&a), hash_secret(&a));
        assert_ne!(hash_secret(&a), hash_secret(&b));
        assert_eq!(hash_secret(&a).len(), 64);
        assert_eq!(hint("abcdef"), "cdef");
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(validate_name("  Entra ID  ").unwrap(), "Entra ID");
        assert!(validate_name("").is_err());
        assert!(validate_name("a\nb").is_err());
        assert!(validate_name(&"x".repeat(101)).is_err());
    }
}
