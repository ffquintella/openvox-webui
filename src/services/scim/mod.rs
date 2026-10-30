//! Inbound SCIM 2.0 provisioning (M31, T106; RFC 7643 and RFC 7644).
//!
//! An identity provider pushes users and groups to `/api/v1/scim/v2` with a
//! bearer token issued by an administrator ([`tokens`]). Accounts reuse the
//! M30 directory model: `users.directory_id = 'scim'`, deactivation through
//! `AuthService::deactivate_user` and roles through `auth_role_mappings`
//! with the provider id `scim` ([`mapping`]).

// SCIM only sees its own accounts: local accounts (the break-glass
// administrator among them) and directory accounts are invisible to it, and
// `super_admin` is never granted.

pub mod config;
pub mod error;
pub mod filter;
pub mod mapping;
pub mod patch;
pub mod resources;
pub mod schemas;
pub mod store;
pub mod tokens;

use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

pub use config::{ScimConfig, ScimLinkAttribute};
pub use error::ScimError;

use crate::db::AuditRepository;
use crate::models::{default_organization_uuid, EventActor};
use crate::services::{plugins::events::EventBus, DbRbacService};

/// Provider id of SCIM accounts (`users.directory_id`, role grants, mappings).
pub const PROVIDER_ID: &str = "scim";
/// Resource type of SCIM entries in the audit log.
pub const AUDIT_SCIM: &str = "scim";
/// Where the SCIM service is mounted.
pub const ENDPOINT_PATH: &str = "/api/v1/scim/v2";
pub const SCIM_CONTENT_TYPE: &str = "application/scim+json";
/// Domain of the e-mail of a user provisioned without one (`.invalid` is
/// reserved, so it never reaches anybody).
pub const PLACEHOLDER_EMAIL_DOMAIN: &str = "scim.invalid";

pub const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
pub const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
pub const ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
pub const LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
pub const PATCH_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
pub const SPC_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig";
pub const RESOURCE_TYPE_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ResourceType";
pub const SCHEMA_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Schema";

/// Who changes SCIM data: a SCIM client (by its token) or an administrator
/// (group mappings).
#[derive(Debug, Clone, Default)]
pub struct ScimActor {
    pub token_id: Option<String>,
    pub token_name: Option<String>,
    pub admin: Option<Uuid>,
    pub ip: Option<String>,
}

impl ScimActor {
    pub fn token(id: &str, name: &str, ip: Option<String>) -> Self {
        Self {
            token_id: Some(id.to_string()),
            token_name: Some(name.to_string()),
            admin: None,
            ip,
        }
    }

    pub fn admin(user: Uuid) -> Self {
        Self {
            admin: Some(user),
            ..Default::default()
        }
    }

    fn event_actor(&self) -> EventActor {
        match (&self.admin, &self.token_id) {
            (Some(u), _) => EventActor::user(u.to_string()),
            (None, Some(t)) => EventActor {
                kind: "system".into(),
                id: Some(format!("scim:{t}")),
            },
            _ => EventActor::system(),
        }
    }
}

/// What a SCIM operation needs from the host.
#[derive(Clone, Copy)]
pub struct ScimContext<'a> {
    pub pool: &'a SqlitePool,
    pub rbac: Option<&'a DbRbacService>,
    pub config: &'a ScimConfig,
    pub actor: &'a ScimActor,
}

impl<'a> ScimContext<'a> {
    /// Base URL of the resources (`meta.location`, `$ref`).
    pub fn base(&self) -> String {
        format!(
            "{}{}",
            self.config.base_url.as_deref().unwrap_or(""),
            ENDPOINT_PATH
        )
    }

    /// Audit entry; the token (never its secret) goes into the details.
    pub async fn audit(&self, user: Option<Uuid>, action: &str, target: &str, mut details: Value) {
        if let Some(obj) = details.as_object_mut() {
            if let Some(t) = &self.actor.token_id {
                obj.insert("token".into(), Value::String(t.clone()));
            }
            if let Some(n) = &self.actor.token_name {
                obj.insert("token_name".into(), Value::String(n.clone()));
            }
        }
        let who = self.actor.admin.or(user);
        if let Err(e) = AuditRepository::new(self.pool)
            .insert(
                default_organization_uuid(),
                who,
                action,
                AUDIT_SCIM,
                Some(target),
                Some(&details),
                self.actor.ip.as_deref(),
            )
            .await
        {
            tracing::warn!("Failed to write audit entry '{}': {}", action, e);
        }
    }

    pub fn invalidate(&self, user: &Uuid) {
        if let Some(r) = self.rbac {
            r.invalidate_user_cache(user);
        }
    }

    /// Publishes a core event (`user.created|updated|deactivated`).
    pub async fn emit(&self, event: &str, payload: Value) {
        EventBus::new(self.pool.clone())
            .emit(event, payload, self.actor.event_actor())
            .await;
    }
}
