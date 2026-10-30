//! Group-to-role mapping of SCIM groups. Mappings live in the M30 table
//! `auth_role_mappings` with `directory_id = 'scim'`, and grants in
//! `directory_role_grants`, so `directory::mapping::reconcile` applies:
//! only mapped roles are granted, never `super_admin`, and roles assigned
//! by hand are left alone.

use std::collections::BTreeSet;

use chrono::Utc;
use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::resources::GroupRef;
use super::PROVIDER_ID;
use crate::services::directory::mapping::{self, MappingError, RoleMapping, PROTECTED_ROLES};

/// How a mapping names a SCIM group.
pub const KINDS: &[&str] = &["display_name", "external_id", "scim_group"];

#[derive(Debug, Clone, Deserialize)]
pub struct ScimMappingInput {
    /// The group's displayName, externalId or SCIM id (see `group_ref_kind`).
    pub group_ref: String,
    #[serde(default = "default_kind")]
    pub group_ref_kind: String,
    pub role_id: Uuid,
    #[serde(default)]
    pub group_label: Option<String>,
}

fn default_kind() -> String {
    "display_name".into()
}

/// Whether a mapping names this group. Display names compare without case.
pub fn group_matches(m: &RoleMapping, g: &GroupRef) -> bool {
    match m.group_ref_kind.as_str() {
        "display_name" => g.display_name.to_lowercase() == m.group_ref.to_lowercase(),
        "external_id" => g.external_id.as_deref() == Some(m.group_ref.as_str()),
        "scim_group" => g.id == m.group_ref,
        _ => false,
    }
}

/// Role ids the groups map to, protected roles excluded.
pub fn desired_roles(mappings: &[RoleMapping], groups: &[GroupRef]) -> BTreeSet<String> {
    mappings
        .iter()
        .filter(|m| !PROTECTED_ROLES.contains(&m.role_name.as_str()))
        .filter(|m| groups.iter().any(|g| group_matches(m, g)))
        .map(|m| m.role_id.clone())
        .collect()
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<RoleMapping>, sqlx::Error> {
    mapping::list(pool, PROVIDER_ID).await
}

pub async fn create(
    pool: &SqlitePool,
    input: &ScimMappingInput,
    created_by: Option<&str>,
) -> Result<RoleMapping, MappingError> {
    let kind = input.group_ref_kind.trim().to_ascii_lowercase();
    if !KINDS.contains(&kind.as_str()) {
        return Err(MappingError::Invalid(
            "group_ref_kind must be display_name, external_id or scim_group".into(),
        ));
    }
    let raw = input.group_ref.trim();
    if raw.is_empty() || raw.chars().count() > 512 || raw.chars().any(|c| c.is_control()) {
        return Err(MappingError::Invalid(
            "group_ref must have 1 to 512 printable characters".into(),
        ));
    }
    let group_ref = if kind == "scim_group" {
        Uuid::parse_str(raw)
            .map_err(|_| MappingError::Invalid("A SCIM group id is a UUID".into()))?
            .to_string()
    } else {
        raw.to_string()
    };
    if input.group_label.as_deref().is_some_and(|l| l.len() > 256) {
        return Err(MappingError::Invalid("group_label is too long".into()));
    }
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM roles WHERE id = ?")
        .bind(input.role_id.to_string())
        .fetch_optional(pool)
        .await?;
    let name = name.ok_or_else(|| MappingError::Invalid("Role not found".into()))?;
    if PROTECTED_ROLES.contains(&name.as_str()) {
        return Err(MappingError::Invalid(
            "The super_admin role cannot be granted by SCIM".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let res = sqlx::query(
        "INSERT OR IGNORE INTO auth_role_mappings \
         (id, directory_id, group_ref, group_ref_kind, group_label, role_id, created_by, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(PROVIDER_ID)
    .bind(&group_ref)
    .bind(&kind)
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
    list(pool)
        .await?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| MappingError::Invalid("mapping vanished".into()))
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    mapping::delete(pool, PROVIDER_ID, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(kind: &str, r: &str, role: &str, name: &str) -> RoleMapping {
        RoleMapping {
            id: "m".into(),
            directory_id: PROVIDER_ID.into(),
            group_ref: r.into(),
            group_ref_kind: kind.into(),
            group_label: None,
            role_id: role.into(),
            role_name: name.into(),
            created_by: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn groups_match_by_name_external_id_or_id() {
        let g = GroupRef {
            id: "6d1f0f8e-0000-4000-8000-000000000001".into(),
            display_name: "WebUI Operators".into(),
            external_id: Some("Ext-G".into()),
        };
        let maps = vec![
            m("display_name", "webui operators", "r-op", "operator"),
            m("external_id", "Ext-G", "r-view", "viewer"),
            m("external_id", "ext-g", "r-x", "x"),
            m("scim_group", &g.id, "r-admin", "admin"),
            m("display_name", "WebUI Operators", "r-super", "super_admin"),
            m("dn", "cn=webui operators", "r-dn", "dn"),
        ];
        let d: Vec<String> = desired_roles(&maps, std::slice::from_ref(&g))
            .into_iter()
            .collect();
        assert_eq!(d, vec!["r-admin", "r-op", "r-view"]);
        assert!(desired_roles(&maps, &[]).is_empty());
    }
}
