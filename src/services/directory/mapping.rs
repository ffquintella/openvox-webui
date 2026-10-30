//! Group-to-role mapping (`auth_role_mappings`) and role reconciliation.
//!
//! A directory only grants roles an administrator mapped, never
//! `super_admin`, and only removes roles it granted itself
//! (`directory_role_grants`); roles assigned by hand are left alone.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::escape::{normalize_dn, parse_group_ref, GroupRefKind};
use super::DirectoryGroup;

/// Roles a directory may never grant: the break-glass role stays local.
pub const PROTECTED_ROLES: &[&str] = &["super_admin", "superadmin"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoleMapping {
    pub id: String,
    pub directory_id: String,
    pub group_ref: String,
    pub group_ref_kind: String,
    pub group_label: Option<String>,
    pub role_id: String,
    pub role_name: String,
    pub created_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MappingInput {
    /// Group DN, SID (`S-1-5-...`) or objectGUID.
    pub group_ref: String,
    pub role_id: Uuid,
    #[serde(default)]
    pub group_label: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MappingError {
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Whether a mapping reference names this group.
pub fn group_matches(kind: &str, group_ref: &str, g: &DirectoryGroup) -> bool {
    match kind {
        "dn" => normalize_dn(&g.dn) == group_ref,
        "sid" => g
            .sid
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(group_ref)),
        "guid" => g
            .guid
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(group_ref)),
        _ => false,
    }
}

/// Role ids the groups map to, protected roles excluded.
pub fn desired_roles(mappings: &[RoleMapping], groups: &[DirectoryGroup]) -> BTreeSet<String> {
    mappings
        .iter()
        .filter(|m| !PROTECTED_ROLES.contains(&m.role_name.as_str()))
        .filter(|m| {
            groups
                .iter()
                .any(|g| group_matches(&m.group_ref_kind, &m.group_ref, g))
        })
        .map(|m| m.role_id.clone())
        .collect()
}

pub async fn list(pool: &SqlitePool, directory_id: &str) -> Result<Vec<RoleMapping>, sqlx::Error> {
    sqlx::query_as::<_, RoleMapping>(
        "SELECT m.id, m.directory_id, m.group_ref, m.group_ref_kind, m.group_label, m.role_id, \
                r.name AS role_name, m.created_by, m.created_at \
         FROM auth_role_mappings m JOIN roles r ON r.id = m.role_id \
         WHERE m.directory_id = ? ORDER BY m.created_at, m.id",
    )
    .bind(directory_id)
    .fetch_all(pool)
    .await
}

async fn role_name(pool: &SqlitePool, role_id: &Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT name FROM roles WHERE id = ?")
        .bind(role_id.to_string())
        .fetch_optional(pool)
        .await
}

/// Validates an input and returns `(kind, canonical ref)`.
async fn validate(
    pool: &SqlitePool,
    input: &MappingInput,
) -> Result<(GroupRefKind, String), MappingError> {
    let (kind, canonical) = parse_group_ref(&input.group_ref).ok_or_else(|| {
        MappingError::Invalid(
            "group_ref must be a group DN, a SID (S-1-...) or an objectGUID".into(),
        )
    })?;
    let name = role_name(pool, &input.role_id)
        .await?
        .ok_or_else(|| MappingError::Invalid("Role not found".into()))?;
    if PROTECTED_ROLES.contains(&name.as_str()) {
        return Err(MappingError::Invalid(
            "The super_admin role cannot be granted by a directory".into(),
        ));
    }
    if input.group_label.as_deref().is_some_and(|l| l.len() > 256) {
        return Err(MappingError::Invalid("group_label is too long".into()));
    }
    Ok((kind, canonical))
}

pub async fn create(
    pool: &SqlitePool,
    directory_id: &str,
    input: &MappingInput,
    created_by: Option<&str>,
) -> Result<RoleMapping, MappingError> {
    let (kind, canonical) = validate(pool, input).await?;
    let id = Uuid::new_v4().to_string();
    let res = sqlx::query(
        "INSERT OR IGNORE INTO auth_role_mappings \
         (id, directory_id, group_ref, group_ref_kind, group_label, role_id, created_by, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(directory_id)
    .bind(&canonical)
    .bind(kind.as_str())
    .bind(input.group_label.as_deref())
    .bind(input.role_id.to_string())
    .bind(created_by)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(MappingError::Invalid(
            "This group is already mapped to that role".into(),
        ));
    }
    list(pool, directory_id)
        .await?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| MappingError::Invalid("mapping vanished".into()))
}

/// Replaces every mapping of a directory in one transaction.
pub async fn replace(
    pool: &SqlitePool,
    directory_id: &str,
    inputs: &[MappingInput],
    created_by: Option<&str>,
) -> Result<Vec<RoleMapping>, MappingError> {
    let mut rows = Vec::with_capacity(inputs.len());
    for i in inputs {
        let (kind, canonical) = validate(pool, i).await?;
        rows.push((kind, canonical, i));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM auth_role_mappings WHERE directory_id = ?")
        .bind(directory_id)
        .execute(&mut *tx)
        .await?;
    let now = Utc::now().to_rfc3339();
    for (kind, canonical, i) in rows {
        sqlx::query(
            "INSERT OR IGNORE INTO auth_role_mappings \
             (id, directory_id, group_ref, group_ref_kind, group_label, role_id, created_by, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(directory_id)
        .bind(&canonical)
        .bind(kind.as_str())
        .bind(i.group_label.as_deref())
        .bind(i.role_id.to_string())
        .bind(created_by)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(list(pool, directory_id).await?)
}

pub async fn delete(pool: &SqlitePool, directory_id: &str, id: &str) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM auth_role_mappings WHERE directory_id = ? AND id = ?")
            .bind(directory_id)
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

/// Role changes made by one reconciliation (role names).
#[derive(Debug, Clone, Default, Serialize)]
pub struct RoleChange {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl RoleChange {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Makes the user's directory-granted roles equal to `desired`: grants the
/// missing ones and revokes those this directory granted that are no longer
/// mapped. Roles assigned by an administrator are never touched.
pub async fn reconcile(
    pool: &SqlitePool,
    user_id: Uuid,
    directory_id: &str,
    desired: &BTreeSet<String>,
) -> Result<RoleChange, sqlx::Error> {
    let uid = user_id.to_string();
    let names: BTreeMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, name FROM roles")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let mut tx = pool.begin().await?;
    let current: BTreeSet<String> =
        sqlx::query_scalar::<_, String>("SELECT role_id FROM user_roles WHERE user_id = ?")
            .bind(&uid)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let granted: BTreeSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT role_id FROM directory_role_grants WHERE user_id = ? AND directory_id = ?",
    )
    .bind(&uid)
    .bind(directory_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let now = Utc::now().to_rfc3339();
    let mut change = RoleChange::default();

    for role in desired {
        let Some(name) = names.get(role) else {
            continue;
        };
        if PROTECTED_ROLES.contains(&name.as_str()) {
            continue;
        }
        if !current.contains(role) {
            sqlx::query(
                "INSERT OR IGNORE INTO user_roles (id, user_id, role_id, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&uid)
            .bind(role)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT OR IGNORE INTO directory_role_grants (user_id, role_id, directory_id, granted_at) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&uid)
            .bind(role)
            .bind(directory_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            change.added.push(name.clone());
        }
    }
    for role in granted.difference(desired) {
        sqlx::query("DELETE FROM directory_role_grants WHERE user_id = ? AND role_id = ? AND directory_id = ?")
            .bind(&uid)
            .bind(role)
            .bind(directory_id)
            .execute(&mut *tx)
            .await?;
        if current.contains(role) {
            sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_id = ?")
                .bind(&uid)
                .bind(role)
                .execute(&mut *tx)
                .await?;
            change
                .removed
                .push(names.get(role).cloned().unwrap_or_else(|| role.clone()));
        }
    }
    tx.commit().await?;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(dn: &str, guid: Option<&str>, sid: Option<&str>) -> DirectoryGroup {
        DirectoryGroup {
            dn: dn.into(),
            name: "g".into(),
            guid: guid.map(String::from),
            sid: sid.map(String::from),
        }
    }

    fn mapping(kind: &str, r: &str, role_id: &str, role_name: &str) -> RoleMapping {
        RoleMapping {
            id: "m".into(),
            directory_id: "d".into(),
            group_ref: r.into(),
            group_ref_kind: kind.into(),
            group_label: None,
            role_id: role_id.into(),
            role_name: role_name.into(),
            created_by: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn mappings_match_by_dn_sid_or_guid() {
        let g = group(
            "CN=Ops , OU=Groups,DC=X",
            Some("33221100-5544-7766-8899-aabbccddeeff"),
            Some("S-1-5-21-1-2-3-1105"),
        );
        assert!(group_matches("dn", "cn=ops,ou=groups,dc=x", &g));
        assert!(group_matches("sid", "S-1-5-21-1-2-3-1105", &g));
        assert!(group_matches(
            "guid",
            "33221100-5544-7766-8899-AABBCCDDEEFF",
            &g
        ));
        assert!(!group_matches("dn", "cn=dev,ou=groups,dc=x", &g));
        assert!(!group_matches("sid", "S-1-5-21-1-2-3-512", &g));
        assert!(!group_matches("other", "cn=ops,ou=groups,dc=x", &g));
    }

    #[test]
    fn only_mapped_and_unprotected_roles_are_desired() {
        let groups = vec![
            group("CN=Ops,DC=x", None, None),
            group("CN=Admins,DC=x", None, None),
        ];
        let maps = vec![
            mapping("dn", "cn=ops,dc=x", "r-operator", "operator"),
            mapping("dn", "cn=admins,dc=x", "r-super", "super_admin"),
            mapping("dn", "cn=other,dc=x", "r-admin", "admin"),
        ];
        let d = desired_roles(&maps, &groups);
        assert_eq!(
            d.into_iter().collect::<Vec<_>>(),
            vec!["r-operator".to_string()]
        );
        assert!(desired_roles(&maps, &[]).is_empty());
    }
}
