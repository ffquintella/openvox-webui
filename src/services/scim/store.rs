//! Storage and lifecycle of SCIM users and groups: provisioning, updates,
//! deactivation, memberships, role reconciliation, SSO links, audit and
//! events. Every query is limited to SCIM accounts (`directory_id = 'scim'`).

use std::collections::{BTreeSet, HashMap};

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::filter::{matches, Filter};
use super::mapping;
use super::resources::{
    group_json, placeholder_email, user_json, GroupAttrs, GroupRef, GroupRow, UserAttrs, UserRow,
};
use super::{ScimContext, ScimError, ScimLinkAttribute, PROVIDER_ID};
use crate::models::default_organization_uuid;
use crate::services::directory::mapping::reconcile;
use crate::services::directory::service::{DIRECTORY_LEGACY_ROLE, DIRECTORY_PASSWORD_PLACEHOLDER};
use crate::services::AuthService;

/// Most resources a filter is evaluated against.
pub const MAX_SCAN: usize = 100_000;
/// Most ids bound in one `IN (...)` list.
const IN_CHUNK: usize = 500;

const USER_COLUMNS: &str = "u.id, u.username, u.email, u.deactivated_at, s.external_id, \
    s.display_name, s.given_name, s.family_name, s.formatted_name, s.version, s.created_at, \
    s.updated_at";
const USER_FROM: &str = "FROM users u JOIN scim_users s ON s.user_id = u.id \
    WHERE u.directory_id = 'scim' AND s.deleted_at IS NULL";
const GROUP_COLUMNS: &str = "id, display_name, external_id, version, created_at, updated_at";

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .is_some_and(|d| d.is_unique_violation())
}

fn conflict_or(e: sqlx::Error, what: &str) -> ScimError {
    if is_unique_violation(&e) {
        ScimError::uniqueness(format!("{what} is already in use"))
    } else {
        ScimError::internal(e)
    }
}

/// One page of a list response.
#[derive(Debug, Clone)]
pub struct Page {
    pub total: usize,
    pub resources: Vec<Value>,
}

// ---------------------------------------------------------------- reads

pub async fn get_user(pool: &SqlitePool, id: &str) -> Result<Option<UserRow>, ScimError> {
    if Uuid::parse_str(id).is_err() {
        return Ok(None);
    }
    Ok(sqlx::query_as::<_, UserRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} {USER_FROM} AND u.id = ?"
    )))
    .bind(id.to_ascii_lowercase())
    .fetch_optional(pool)
    .await?)
}

/// Groups of the given users (all memberships when the list is long).
pub async fn groups_of(
    pool: &SqlitePool,
    user_ids: &[String],
) -> Result<HashMap<String, Vec<GroupRef>>, ScimError> {
    let mut out: HashMap<String, Vec<GroupRef>> = HashMap::new();
    if user_ids.is_empty() {
        return Ok(out);
    }
    let sql = "SELECT m.user_id, g.id, g.display_name, g.external_id FROM scim_group_members m \
               JOIN scim_groups g ON g.id = m.group_id";
    let rows: Vec<(String, String, String, Option<String>)> = if user_ids.len() > IN_CHUNK {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{sql} ORDER BY g.display_name"
        )))
        .fetch_all(pool)
        .await?
    } else {
        let mut q = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{sql} WHERE m.user_id IN ({}) ORDER BY g.display_name",
            placeholders(user_ids.len())
        )));
        for id in user_ids {
            q = q.bind(id);
        }
        q.fetch_all(pool).await?
    };
    for (uid, id, display_name, external_id) in rows {
        out.entry(uid).or_default().push(GroupRef {
            id,
            display_name,
            external_id,
        });
    }
    Ok(out)
}

/// Members `(user id, userName)` of the given groups (all when the list is
/// long). Only SCIM accounts that are not deleted are members.
pub async fn members_of(
    pool: &SqlitePool,
    group_ids: &[String],
) -> Result<HashMap<String, Vec<(String, String)>>, ScimError> {
    let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if group_ids.is_empty() {
        return Ok(out);
    }
    let sql = "SELECT m.group_id, u.id, u.username FROM scim_group_members m \
               JOIN users u ON u.id = m.user_id JOIN scim_users s ON s.user_id = u.id \
               WHERE u.directory_id = 'scim' AND s.deleted_at IS NULL";
    let rows: Vec<(String, String, String)> = if group_ids.len() > IN_CHUNK {
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{sql} ORDER BY u.username")))
            .fetch_all(pool)
            .await?
    } else {
        let mut q = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{sql} AND m.group_id IN ({}) ORDER BY u.username",
            placeholders(group_ids.len())
        )));
        for id in group_ids {
            q = q.bind(id);
        }
        q.fetch_all(pool).await?
    };
    for (gid, uid, name) in rows {
        out.entry(gid).or_default().push((uid, name));
    }
    Ok(out)
}

async fn users_json(ctx: ScimContext<'_>, rows: &[UserRow]) -> Result<Vec<Value>, ScimError> {
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let mut groups = groups_of(ctx.pool, &ids).await?;
    let base = ctx.base();
    Ok(rows
        .iter()
        .map(|r| user_json(r, &groups.remove(&r.id).unwrap_or_default(), &base))
        .collect())
}

pub async fn user_resource(ctx: ScimContext<'_>, row: &UserRow) -> Result<Value, ScimError> {
    Ok(users_json(ctx, std::slice::from_ref(row))
        .await?
        .pop()
        .unwrap_or(Value::Null))
}

pub async fn list_users(
    ctx: ScimContext<'_>,
    filter: Option<&Filter>,
    start_index: usize,
    count: usize,
) -> Result<Page, ScimError> {
    let offset = start_index.saturating_sub(1);
    let Some(f) = filter else {
        let total: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) {USER_FROM}")))
                .fetch_one(ctx.pool)
                .await?;
        let rows = sqlx::query_as::<_, UserRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {USER_COLUMNS} {USER_FROM} ORDER BY s.created_at, u.id LIMIT ? OFFSET ?"
        )))
        .bind(count as i64)
        .bind(offset as i64)
        .fetch_all(ctx.pool)
        .await?;
        return Ok(Page {
            total: total as usize,
            resources: users_json(ctx, &rows).await?,
        });
    };
    let (clause, bind) = match f.required_eq() {
        Some(("username", v)) => ("AND lower(u.username) = lower(?)", Some(v)),
        Some(("externalid", v)) => ("AND s.external_id = ?", Some(v)),
        Some(("id", v)) => ("AND u.id = ?", Some(v)),
        Some(("emails" | "emails.value", v)) => ("AND lower(u.email) = lower(?)", Some(v)),
        _ => ("", None),
    };
    let mut q = sqlx::query_as::<_, UserRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} {USER_FROM} {clause} ORDER BY s.created_at, u.id LIMIT ?"
    )));
    if let Some(v) = bind {
        q = q.bind(v);
    }
    let rows = q.bind((MAX_SCAN + 1) as i64).fetch_all(ctx.pool).await?;
    if rows.len() > MAX_SCAN {
        return Err(ScimError::bad(
            "tooMany",
            "The filter is too broad; narrow it with userName, externalId or id",
        ));
    }
    let all: Vec<Value> = users_json(ctx, &rows)
        .await?
        .into_iter()
        .filter(|v| matches(f, v))
        .collect();
    Ok(Page {
        total: all.len(),
        resources: all.into_iter().skip(offset).take(count).collect(),
    })
}

pub async fn get_group(pool: &SqlitePool, id: &str) -> Result<Option<GroupRow>, ScimError> {
    if Uuid::parse_str(id).is_err() {
        return Ok(None);
    }
    Ok(sqlx::query_as::<_, GroupRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {GROUP_COLUMNS} FROM scim_groups WHERE id = ?"
    )))
    .bind(id.to_ascii_lowercase())
    .fetch_optional(pool)
    .await?)
}

async fn groups_json(ctx: ScimContext<'_>, rows: &[GroupRow]) -> Result<Vec<Value>, ScimError> {
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let mut members = members_of(ctx.pool, &ids).await?;
    let base = ctx.base();
    Ok(rows
        .iter()
        .map(|r| group_json(r, &members.remove(&r.id).unwrap_or_default(), &base))
        .collect())
}

pub async fn group_resource(ctx: ScimContext<'_>, row: &GroupRow) -> Result<Value, ScimError> {
    Ok(groups_json(ctx, std::slice::from_ref(row))
        .await?
        .pop()
        .unwrap_or(Value::Null))
}

pub async fn list_groups(
    ctx: ScimContext<'_>,
    filter: Option<&Filter>,
    start_index: usize,
    count: usize,
) -> Result<Page, ScimError> {
    let offset = start_index.saturating_sub(1);
    let Some(f) = filter else {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scim_groups")
            .fetch_one(ctx.pool)
            .await?;
        let rows = sqlx::query_as::<_, GroupRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {GROUP_COLUMNS} FROM scim_groups ORDER BY created_at, id LIMIT ? OFFSET ?"
        )))
        .bind(count as i64)
        .bind(offset as i64)
        .fetch_all(ctx.pool)
        .await?;
        return Ok(Page {
            total: total as usize,
            resources: groups_json(ctx, &rows).await?,
        });
    };
    let (clause, bind) = match f.required_eq() {
        Some(("displayname", v)) => ("WHERE display_name = ? COLLATE NOCASE", Some(v)),
        Some(("externalid", v)) => ("WHERE external_id = ?", Some(v)),
        Some(("id", v)) => ("WHERE id = ?", Some(v)),
        _ => ("", None),
    };
    let mut q = sqlx::query_as::<_, GroupRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {GROUP_COLUMNS} FROM scim_groups {clause} ORDER BY created_at, id LIMIT ?"
    )));
    if let Some(v) = bind {
        q = q.bind(v);
    }
    let rows = q.bind((MAX_SCAN + 1) as i64).fetch_all(ctx.pool).await?;
    if rows.len() > MAX_SCAN {
        return Err(ScimError::bad("tooMany", "The filter is too broad"));
    }
    let all: Vec<Value> = groups_json(ctx, &rows)
        .await?
        .into_iter()
        .filter(|v| matches(f, v))
        .collect();
    Ok(Page {
        total: all.len(),
        resources: all.into_iter().skip(offset).take(count).collect(),
    })
}

// ---------------------------------------------------------------- SSO links

fn link_value(ctx: ScimContext<'_>, attrs: &UserAttrs) -> Option<String> {
    match ctx.config.sso_link_attribute {
        ScimLinkAttribute::UserName => Some(attrs.user_name.clone()),
        ScimLinkAttribute::ExternalId => attrs.external_id.clone(),
    }
}

/// Refuses an SSO identity that is linked to another account.
async fn check_sso(ctx: ScimContext<'_>, uid: &str, attrs: &UserAttrs) -> Result<(), ScimError> {
    let (Some(provider), Some(value)) =
        (ctx.config.sso_provider.as_deref(), link_value(ctx, attrs))
    else {
        return Ok(());
    };
    let owner: Option<String> = if provider == "saml" {
        sqlx::query_scalar("SELECT id FROM users WHERE external_id = ? AND id != ? LIMIT 1")
            .bind(&value)
            .bind(uid)
            .fetch_optional(ctx.pool)
            .await?
    } else {
        sqlx::query_scalar(
            "SELECT user_id FROM user_identities WHERE provider_id = ? AND external_id = ? AND user_id != ?",
        )
        .bind(provider)
        .bind(&value)
        .bind(uid)
        .fetch_optional(ctx.pool)
        .await?
    };
    if owner.is_some() {
        return Err(ScimError::uniqueness(
            "The single sign-on identity of this user is linked to another account",
        ));
    }
    Ok(())
}

/// Links the account to its SSO identity (SAML NameID or OIDC subject).
async fn apply_sso(ctx: ScimContext<'_>, uid: &str, attrs: &UserAttrs) -> Result<(), ScimError> {
    let Some(provider) = ctx.config.sso_provider.as_deref() else {
        return Ok(());
    };
    let value = link_value(ctx, attrs);
    if provider == "saml" {
        sqlx::query("UPDATE users SET auth_provider = 'saml', external_id = ? WHERE id = ?")
            .bind(&value)
            .bind(uid)
            .execute(ctx.pool)
            .await
            .map_err(|e| conflict_or(e, "The single sign-on identity"))?;
        return Ok(());
    }
    let mut tx = ctx.pool.begin().await?;
    sqlx::query("DELETE FROM user_identities WHERE provider_id = ? AND user_id = ?")
        .bind(provider)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    if let Some(v) = &value {
        sqlx::query(
            "INSERT INTO user_identities (provider_id, external_id, user_id, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(provider)
        .bind(v)
        .bind(uid)
        .bind(now())
        .execute(&mut *tx)
        .await
        .map_err(|e| conflict_or(e, "The single sign-on identity"))?;
    }
    tx.commit().await?;
    Ok(())
}

async fn unlink_sso(ctx: ScimContext<'_>, uid: &str) -> Result<(), ScimError> {
    match ctx.config.sso_provider.as_deref() {
        Some("saml") => {
            sqlx::query("UPDATE users SET external_id = NULL WHERE id = ?")
                .bind(uid)
                .execute(ctx.pool)
                .await?;
        }
        Some(provider) => {
            sqlx::query("DELETE FROM user_identities WHERE provider_id = ? AND user_id = ?")
                .bind(provider)
                .bind(uid)
                .execute(ctx.pool)
                .await?;
        }
        None => {}
    }
    Ok(())
}

// ---------------------------------------------------------------- users

async fn email_taken(pool: &SqlitePool, email: &str, except: &str) -> Result<bool, ScimError> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE lower(email) = lower(?) AND id != ?")
            .bind(email)
            .bind(except)
            .fetch_one(pool)
            .await?;
    Ok(n > 0)
}

async fn username_taken(pool: &SqlitePool, name: &str, except: &str) -> Result<bool, ScimError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE lower(username) = lower(?) AND id != ?",
    )
    .bind(name)
    .bind(except)
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

fn user_event(row: &UserRow) -> Value {
    json!({
        "user_id": row.id,
        "username": row.username,
        "source": PROVIDER_ID,
        "external_id": row.external_id,
        "active": row.deactivated_at.is_none(),
    })
}

async fn reload_user(pool: &SqlitePool, id: &str) -> Result<UserRow, ScimError> {
    get_user(pool, id)
        .await?
        .ok_or_else(|| ScimError::internal("provisioned user vanished"))
}

/// The SCIM account a POST may bring back: a deleted one with the same
/// externalId, or with the same userName when neither has an externalId.
/// Anything else holding the userName or externalId is a conflict.
async fn revivable(pool: &SqlitePool, attrs: &UserAttrs) -> Result<Option<String>, ScimError> {
    type Owner = (String, Option<String>, Option<String>, Option<String>);
    let name_owners: Vec<Owner> = sqlx::query_as(
        "SELECT u.id, u.directory_id, s.deleted_at, s.external_id FROM users u \
         LEFT JOIN scim_users s ON s.user_id = u.id WHERE lower(u.username) = lower(?)",
    )
    .bind(&attrs.user_name)
    .fetch_all(pool)
    .await?;
    let ext_owner: Option<(String, Option<String>)> = match &attrs.external_id {
        Some(x) => {
            sqlx::query_as("SELECT user_id, deleted_at FROM scim_users WHERE external_id = ?")
                .bind(x)
                .fetch_optional(pool)
                .await?
        }
        None => None,
    };
    match ext_owner {
        Some((_, None)) => Err(ScimError::uniqueness("externalId is already in use")),
        Some((uid, Some(_))) => {
            if name_owners.iter().any(|o| o.0 != uid) {
                return Err(ScimError::uniqueness("userName is already in use"));
            }
            Ok(Some(uid))
        }
        None => match name_owners.as_slice() {
            [] => Ok(None),
            [(uid, Some(dir), Some(_), None)]
                if dir == PROVIDER_ID && attrs.external_id.is_none() =>
            {
                Ok(Some(uid.clone()))
            }
            _ => Err(ScimError::uniqueness("userName is already in use")),
        },
    }
}

/// POST /Users: provisions an account (or brings back a deleted one).
pub async fn create_user(ctx: ScimContext<'_>, attrs: UserAttrs) -> Result<UserRow, ScimError> {
    attrs.validate()?;
    let pool = ctx.pool;
    let revive = revivable(pool, &attrs).await?;
    let uid = revive.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Some(e) = &attrs.email {
        if email_taken(pool, e, &uid).await? {
            return Err(ScimError::uniqueness(
                "emails.value is already used by another account",
            ));
        }
    }
    check_sso(ctx, &uid, &attrs).await?;
    let ts = now();
    let email = attrs
        .email
        .clone()
        .unwrap_or_else(|| placeholder_email(&uid));
    let auth_provider = if ctx.config.sso_provider.as_deref() == Some("saml") {
        "saml"
    } else {
        "local"
    };
    let mut tx = pool.begin().await?;
    if revive.is_some() {
        sqlx::query("UPDATE users SET username = ?, email = ?, updated_at = ? WHERE id = ?")
            .bind(&attrs.user_name)
            .bind(&email)
            .bind(&ts)
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .map_err(|e| conflict_or(e, "userName or emails.value"))?;
        sqlx::query(
            "UPDATE scim_users SET external_id = ?, display_name = ?, given_name = ?, family_name = ?, \
             formatted_name = ?, version = version + 1, updated_at = ?, deleted_at = NULL WHERE user_id = ?",
        )
        .bind(&attrs.external_id)
        .bind(&attrs.display_name)
        .bind(&attrs.given_name)
        .bind(&attrs.family_name)
        .bind(&attrs.formatted_name)
        .bind(&ts)
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .map_err(|e| conflict_or(e, "externalId"))?;
    } else {
        sqlx::query(
            "INSERT INTO users (id, organization_id, username, email, password_hash, role, auth_provider, \
             directory_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&uid)
        .bind(default_organization_uuid().to_string())
        .bind(&attrs.user_name)
        .bind(&email)
        .bind(DIRECTORY_PASSWORD_PLACEHOLDER)
        .bind(DIRECTORY_LEGACY_ROLE)
        .bind(auth_provider)
        .bind(PROVIDER_ID)
        .bind(&ts)
        .bind(&ts)
        .execute(&mut *tx)
        .await
        .map_err(|e| conflict_or(e, "userName or emails.value"))?;
        sqlx::query(
            "INSERT INTO scim_users (user_id, external_id, display_name, given_name, family_name, \
             formatted_name, version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(&uid)
        .bind(&attrs.external_id)
        .bind(&attrs.display_name)
        .bind(&attrs.given_name)
        .bind(&attrs.family_name)
        .bind(&attrs.formatted_name)
        .bind(&ts)
        .bind(&ts)
        .execute(&mut *tx)
        .await
        .map_err(|e| conflict_or(e, "externalId"))?;
    }
    tx.commit().await?;
    apply_sso(ctx, &uid, &attrs).await?;

    let id = Uuid::parse_str(&uid).map_err(ScimError::internal)?;
    let auth = AuthService::new(pool.clone());
    if attrs.active {
        if revive.is_some() {
            auth.reactivate_user(&id).await?;
        }
    } else {
        auth.deactivate_user(&id, "scim_inactive").await?;
    }
    let row = reload_user(pool, &uid).await?;
    ctx.audit(
        Some(id),
        "scim.user_created",
        &row.username,
        json!({"user_id": uid, "external_id": row.external_id, "active": attrs.active,
               "revived": revive.is_some()}),
    )
    .await;
    if revive.is_some() {
        let mut p = user_event(&row);
        p["reactivated"] = json!(attrs.active);
        p["changed"] = json!(["active"]);
        ctx.emit("user.updated", p).await;
    } else {
        ctx.emit("user.created", user_event(&row)).await;
    }
    Ok(row)
}

/// PUT and PATCH /Users/{id}: writes the new attributes, deactivating or
/// reactivating the account when `active` changes.
pub async fn update_user(
    ctx: ScimContext<'_>,
    current: &UserRow,
    new: UserAttrs,
) -> Result<UserRow, ScimError> {
    new.validate()?;
    let pool = ctx.pool;
    let old = UserAttrs::from_row(current);
    let changed = old.changed_fields(&new);
    if changed.is_empty() {
        return Ok(current.clone());
    }
    let uid = current.id.as_str();
    if new.user_name != old.user_name && username_taken(pool, &new.user_name, uid).await? {
        return Err(ScimError::uniqueness("userName is already in use"));
    }
    if new.email != old.email {
        if let Some(e) = &new.email {
            if email_taken(pool, e, uid).await? {
                return Err(ScimError::uniqueness(
                    "emails.value is already used by another account",
                ));
            }
        }
    }
    if new.external_id != old.external_id {
        if let Some(x) = &new.external_id {
            let other: Option<String> = sqlx::query_scalar(
                "SELECT user_id FROM scim_users WHERE external_id = ? AND user_id != ?",
            )
            .bind(x)
            .bind(uid)
            .fetch_optional(pool)
            .await?;
            if other.is_some() {
                return Err(ScimError::uniqueness("externalId is already in use"));
            }
        }
    }
    check_sso(ctx, uid, &new).await?;
    let ts = now();
    let email = new.email.clone().unwrap_or_else(|| placeholder_email(uid));
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE users SET username = ?, email = ?, updated_at = ? WHERE id = ?")
        .bind(&new.user_name)
        .bind(&email)
        .bind(&ts)
        .bind(uid)
        .execute(&mut *tx)
        .await
        .map_err(|e| conflict_or(e, "userName or emails.value"))?;
    sqlx::query(
        "UPDATE scim_users SET external_id = ?, display_name = ?, given_name = ?, family_name = ?, \
         formatted_name = ?, version = version + 1, updated_at = ? WHERE user_id = ?",
    )
    .bind(&new.external_id)
    .bind(&new.display_name)
    .bind(&new.given_name)
    .bind(&new.family_name)
    .bind(&new.formatted_name)
    .bind(&ts)
    .bind(uid)
    .execute(&mut *tx)
    .await
    .map_err(|e| conflict_or(e, "externalId"))?;
    tx.commit().await?;
    apply_sso(ctx, uid, &new).await?;

    let id = Uuid::parse_str(uid).map_err(ScimError::internal)?;
    let auth = AuthService::new(pool.clone());
    let mut reactivated = false;
    if old.active && !new.active {
        auth.deactivate_user(&id, "scim_inactive").await?;
        ctx.invalidate(&id);
    } else if !old.active && new.active {
        reactivated = auth.reactivate_user(&id).await?;
    }
    let row = reload_user(pool, uid).await?;
    if old.active && !new.active {
        ctx.audit(
            Some(id),
            "scim.user_deactivated",
            &row.username,
            json!({"user_id": uid, "reason": "scim_inactive",
                   "sessions_revoked": true, "api_keys_revoked": true}),
        )
        .await;
        let mut p = user_event(&row);
        p["reason"] = json!("inactive");
        ctx.emit("user.deactivated", p).await;
    }
    let others: Vec<&str> = changed.iter().copied().filter(|c| *c != "active").collect();
    if !others.is_empty() || reactivated {
        ctx.audit(
            Some(id),
            "scim.user_updated",
            &row.username,
            json!({"user_id": uid, "changed": changed, "reactivated": reactivated}),
        )
        .await;
        let mut p = user_event(&row);
        p["changed"] = json!(changed);
        p["reactivated"] = json!(reactivated);
        ctx.emit("user.updated", p).await;
    }
    Ok(row)
}

/// DELETE /Users/{id}: the account is deactivated (sessions and API keys
/// revoked), loses its SCIM groups, roles and SSO link, and is kept.
pub async fn delete_user(ctx: ScimContext<'_>, row: &UserRow) -> Result<(), ScimError> {
    let pool = ctx.pool;
    let uid = row.id.as_str();
    let ts = now();
    let groups: Vec<String> =
        sqlx::query_scalar("SELECT group_id FROM scim_group_members WHERE user_id = ?")
            .bind(uid)
            .fetch_all(pool)
            .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM scim_group_members WHERE user_id = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    for g in &groups {
        sqlx::query("UPDATE scim_groups SET version = version + 1, updated_at = ? WHERE id = ?")
            .bind(&ts)
            .bind(g)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE scim_users SET deleted_at = ?, version = version + 1, updated_at = ? WHERE user_id = ?",
    )
    .bind(&ts)
    .bind(&ts)
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let id = Uuid::parse_str(uid).map_err(ScimError::internal)?;
    AuthService::new(pool.clone())
        .deactivate_user(&id, "scim_deleted")
        .await?;
    unlink_sso(ctx, uid).await?;
    reconcile_users(ctx, std::slice::from_ref(&row.id)).await?;
    ctx.invalidate(&id);
    ctx.audit(
        Some(id),
        "scim.user_deleted",
        &row.username,
        json!({"user_id": uid, "groups_left": groups.len(),
               "sessions_revoked": true, "api_keys_revoked": true}),
    )
    .await;
    let mut p = user_event(row);
    p["active"] = json!(false);
    p["reason"] = json!("deleted");
    ctx.emit("user.deactivated", p).await;
    Ok(())
}

// ---------------------------------------------------------------- roles

/// Recomputes the SCIM-granted roles of the given accounts from their groups.
pub async fn reconcile_users(ctx: ScimContext<'_>, user_ids: &[String]) -> Result<(), ScimError> {
    if user_ids.is_empty() {
        return Ok(());
    }
    let mappings = mapping::list(ctx.pool).await?;
    let memberships = groups_of(ctx.pool, user_ids).await?;
    for uid in user_ids {
        let Ok(id) = Uuid::parse_str(uid) else {
            continue;
        };
        let account: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT u.username, s.deleted_at FROM users u JOIN scim_users s ON s.user_id = u.id \
             WHERE u.id = ? AND u.directory_id = 'scim'",
        )
        .bind(uid)
        .fetch_optional(ctx.pool)
        .await?;
        let Some((username, deleted)) = account else {
            continue;
        };
        let groups = if deleted.is_some() {
            Vec::new()
        } else {
            memberships.get(uid).cloned().unwrap_or_default()
        };
        let desired = mapping::desired_roles(&mappings, &groups);
        let change = reconcile(ctx.pool, id, PROVIDER_ID, &desired).await?;
        if !change.is_empty() {
            ctx.invalidate(&id);
            ctx.audit(
                Some(id),
                "scim.roles_changed",
                &username,
                json!({"user_id": uid, "added": change.added, "removed": change.removed,
                       "groups": groups.iter().map(|g| &g.display_name).collect::<Vec<_>>()}),
            )
            .await;
        }
    }
    Ok(())
}

/// Reconciles every SCIM account (after a mapping changed).
pub async fn reconcile_all(ctx: ScimContext<'_>) -> Result<usize, ScimError> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM users WHERE directory_id = 'scim'")
        .fetch_all(ctx.pool)
        .await?;
    reconcile_users(ctx, &ids).await?;
    Ok(ids.len())
}

// ---------------------------------------------------------------- groups

async fn check_group_unique(
    pool: &SqlitePool,
    attrs: &GroupAttrs,
    except: &str,
) -> Result<(), ScimError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scim_groups WHERE display_name = ? COLLATE NOCASE AND id != ?",
    )
    .bind(&attrs.display_name)
    .bind(except)
    .fetch_one(pool)
    .await?;
    if n > 0 {
        return Err(ScimError::uniqueness("displayName is already in use"));
    }
    if let Some(x) = &attrs.external_id {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM scim_groups WHERE external_id = ? AND id != ?",
        )
        .bind(x)
        .bind(except)
        .fetch_one(pool)
        .await?;
        if n > 0 {
            return Err(ScimError::uniqueness("externalId is already in use"));
        }
    }
    Ok(())
}

/// Every member must be a SCIM account that is not deleted: SCIM can never
/// put a local or directory account into a group.
async fn check_members(pool: &SqlitePool, ids: &BTreeSet<String>) -> Result<(), ScimError> {
    let list: Vec<&String> = ids.iter().collect();
    for chunk in list.chunks(IN_CHUNK) {
        let mut q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(format!(
            "SELECT u.id {USER_FROM} AND u.id IN ({})",
            placeholders(chunk.len())
        )));
        for id in chunk {
            q = q.bind(id.as_str());
        }
        let found: BTreeSet<String> = q.fetch_all(pool).await?.into_iter().collect();
        if let Some(missing) = chunk.iter().find(|id| !found.contains(id.as_str())) {
            return Err(ScimError::invalid_value(format!(
                "Member '{missing}' is not a user provisioned by SCIM"
            )));
        }
    }
    Ok(())
}

pub async fn group_members(pool: &SqlitePool, gid: &str) -> Result<BTreeSet<String>, ScimError> {
    Ok(
        sqlx::query_scalar("SELECT user_id FROM scim_group_members WHERE group_id = ?")
            .bind(gid)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect(),
    )
}

async fn bump_users(
    tx: &mut sqlx::SqliteConnection,
    ids: &BTreeSet<String>,
    ts: &str,
) -> Result<(), sqlx::Error> {
    for id in ids {
        sqlx::query(
            "UPDATE scim_users SET version = version + 1, updated_at = ? WHERE user_id = ?",
        )
        .bind(ts)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn reload_group(pool: &SqlitePool, id: &str) -> Result<GroupRow, ScimError> {
    get_group(pool, id)
        .await?
        .ok_or_else(|| ScimError::internal("group vanished"))
}

/// POST /Groups.
pub async fn create_group(
    ctx: ScimContext<'_>,
    attrs: GroupAttrs,
    members: Vec<String>,
) -> Result<GroupRow, ScimError> {
    attrs.validate()?;
    let members: BTreeSet<String> = members.into_iter().collect();
    let gid = Uuid::new_v4().to_string();
    check_group_unique(ctx.pool, &attrs, &gid).await?;
    check_members(ctx.pool, &members).await?;
    let ts = now();
    let mut tx = ctx.pool.begin().await?;
    sqlx::query(
        "INSERT INTO scim_groups (id, display_name, external_id, version, created_at, updated_at) \
         VALUES (?, ?, ?, 1, ?, ?)",
    )
    .bind(&gid)
    .bind(&attrs.display_name)
    .bind(&attrs.external_id)
    .bind(&ts)
    .bind(&ts)
    .execute(&mut *tx)
    .await
    .map_err(|e| conflict_or(e, "displayName or externalId"))?;
    for m in &members {
        sqlx::query(
            "INSERT INTO scim_group_members (group_id, user_id, added_at) VALUES (?, ?, ?)",
        )
        .bind(&gid)
        .bind(m)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
    }
    bump_users(&mut tx, &members, &ts).await?;
    tx.commit().await?;
    let ids: Vec<String> = members.iter().cloned().collect();
    reconcile_users(ctx, &ids).await?;
    ctx.audit(
        None,
        "scim.group_created",
        &attrs.display_name,
        json!({"group_id": gid, "external_id": attrs.external_id, "members": members.len()}),
    )
    .await;
    reload_group(ctx.pool, &gid).await
}

/// PUT and PATCH /Groups/{id}: new attributes and the full member list.
pub async fn update_group(
    ctx: ScimContext<'_>,
    current: &GroupRow,
    attrs: GroupAttrs,
    members: BTreeSet<String>,
) -> Result<GroupRow, ScimError> {
    attrs.validate()?;
    let gid = current.id.as_str();
    let old_attrs = GroupAttrs::from_row(current);
    let old_members = group_members(ctx.pool, gid).await?;
    if attrs == old_attrs && members == old_members {
        return Ok(current.clone());
    }
    if attrs != old_attrs {
        check_group_unique(ctx.pool, &attrs, gid).await?;
    }
    let added: BTreeSet<String> = members.difference(&old_members).cloned().collect();
    let removed: BTreeSet<String> = old_members.difference(&members).cloned().collect();
    check_members(ctx.pool, &added).await?;
    let ts = now();
    let mut tx = ctx.pool.begin().await?;
    sqlx::query(
        "UPDATE scim_groups SET display_name = ?, external_id = ?, version = version + 1, updated_at = ? \
         WHERE id = ?",
    )
    .bind(&attrs.display_name)
    .bind(&attrs.external_id)
    .bind(&ts)
    .bind(gid)
    .execute(&mut *tx)
    .await
    .map_err(|e| conflict_or(e, "displayName or externalId"))?;
    for m in &removed {
        sqlx::query("DELETE FROM scim_group_members WHERE group_id = ? AND user_id = ?")
            .bind(gid)
            .bind(m)
            .execute(&mut *tx)
            .await?;
    }
    for m in &added {
        sqlx::query("INSERT OR IGNORE INTO scim_group_members (group_id, user_id, added_at) VALUES (?, ?, ?)")
            .bind(gid)
            .bind(m)
            .bind(&ts)
            .execute(&mut *tx)
            .await?;
    }
    let touched: BTreeSet<String> = added.union(&removed).cloned().collect();
    // A rename changes what display-name mappings select: every member.
    let affected: BTreeSet<String> = if attrs != old_attrs {
        members.union(&old_members).cloned().collect()
    } else {
        touched.clone()
    };
    bump_users(&mut tx, &affected, &ts).await?;
    tx.commit().await?;
    let ids: Vec<String> = affected.into_iter().collect();
    reconcile_users(ctx, &ids).await?;
    ctx.audit(
        None,
        "scim.group_updated",
        &attrs.display_name,
        json!({"group_id": gid, "renamed": attrs.display_name != old_attrs.display_name,
               "external_id_changed": attrs.external_id != old_attrs.external_id,
               "members_added": added.len(), "members_removed": removed.len()}),
    )
    .await;
    reload_group(ctx.pool, gid).await
}

/// DELETE /Groups/{id}: the members lose the roles the group mapped to.
pub async fn delete_group(ctx: ScimContext<'_>, row: &GroupRow) -> Result<(), ScimError> {
    let members = group_members(ctx.pool, &row.id).await?;
    let ts = now();
    let mut tx = ctx.pool.begin().await?;
    sqlx::query("DELETE FROM scim_group_members WHERE group_id = ?")
        .bind(&row.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM scim_groups WHERE id = ?")
        .bind(&row.id)
        .execute(&mut *tx)
        .await?;
    bump_users(&mut tx, &members, &ts).await?;
    tx.commit().await?;
    let ids: Vec<String> = members.iter().cloned().collect();
    reconcile_users(ctx, &ids).await?;
    ctx.audit(
        None,
        "scim.group_deleted",
        &row.display_name,
        json!({"group_id": row.id, "members": members.len()}),
    )
    .await;
    Ok(())
}

// ---------------------------------------------------------------- admin view

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GroupSummary {
    pub id: String,
    pub display_name: String,
    pub external_id: Option<String>,
    pub members: i64,
    pub updated_at: String,
}

pub async fn group_summaries(pool: &SqlitePool) -> Result<Vec<GroupSummary>, sqlx::Error> {
    sqlx::query_as::<_, GroupSummary>(
        "SELECT g.id, g.display_name, g.external_id, COUNT(m.user_id) AS members, g.updated_at \
         FROM scim_groups g LEFT JOIN scim_group_members m ON m.group_id = g.id \
         GROUP BY g.id ORDER BY g.display_name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub users_active: i64,
    pub users_deactivated: i64,
    pub users_deleted: i64,
    pub groups: i64,
}

pub async fn counts(pool: &SqlitePool) -> Result<Counts, sqlx::Error> {
    let (active, deactivated, deleted): (i64, i64, i64) = sqlx::query_as(
        "SELECT \
           COALESCE(SUM(CASE WHEN s.deleted_at IS NULL AND u.deactivated_at IS NULL THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN s.deleted_at IS NULL AND u.deactivated_at IS NOT NULL THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN s.deleted_at IS NOT NULL THEN 1 ELSE 0 END), 0) \
         FROM users u JOIN scim_users s ON s.user_id = u.id WHERE u.directory_id = 'scim'",
    )
    .fetch_one(pool)
    .await?;
    let groups: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scim_groups")
        .fetch_one(pool)
        .await?;
    Ok(Counts {
        users_active: active,
        users_deactivated: deactivated,
        users_deleted: deleted,
        groups,
    })
}
