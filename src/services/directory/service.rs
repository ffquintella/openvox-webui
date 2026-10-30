//! Host side of the directories: login through a directory, linking by stable
//! identifier, provisioning, role reconciliation, deactivation and the
//! synchronization run.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::escape::{login_for_directory, AccountState};
use super::mapping::{self, RoleChange};
use super::{
    BindOutcome, Directory, DirectoryGroup, DirectoryRegistry, DirectoryUser, AUDIT_DIRECTORY,
};
use crate::db::AuditRepository;
use crate::models::{default_organization_uuid, EventActor, User};
use crate::services::{plugins::events::EventBus, AuthService, DbRbacService};

/// Password hash of a directory account: never a valid argon2 hash, so no
/// local password can match it.
pub const DIRECTORY_PASSWORD_PLACEHOLDER: &str = "!DIRECTORY!";
/// Legacy `users.role` value of directory accounts. Authorization uses
/// `user_roles`; this name matches no role.
pub const DIRECTORY_LEGACY_ROLE: &str = "directory";

/// What the host needs around a directory operation.
#[derive(Clone, Copy)]
pub struct DirectoryContext<'a> {
    pub pool: &'a SqlitePool,
    /// Permission cache to invalidate when roles change.
    pub rbac: Option<&'a DbRbacService>,
}

impl<'a> DirectoryContext<'a> {
    pub fn new(pool: &'a SqlitePool, rbac: Option<&'a DbRbacService>) -> Self {
        Self { pool, rbac }
    }

    pub async fn audit(
        &self,
        user: Option<Uuid>,
        action: &str,
        rtype: &str,
        target: &str,
        details: Value,
    ) {
        if let Err(e) = AuditRepository::new(self.pool)
            .insert(
                default_organization_uuid(),
                user,
                action,
                rtype,
                Some(target),
                Some(&details),
                None,
            )
            .await
        {
            tracing::warn!("Failed to write audit entry '{}': {}", action, e);
        }
    }

    fn invalidate(&self, user: &Uuid) {
        if let Some(r) = self.rbac {
            r.invalidate_user_cache(user);
        }
    }
}

/// Result of a login attempt through the directories.
pub enum LoginResult {
    /// The credentials are valid and the account may sign in.
    User {
        user: Box<User>,
        directory_id: String,
    },
    /// No directory accepted the login. The reason is in the audit log only.
    Denied,
}

/// Who may be created when an identity is not linked yet.
#[derive(Clone, Copy)]
struct CreatePolicy {
    allowed: bool,
    require_role: bool,
}

/// Outcome of linking an identity to a local account.
pub enum Resolved {
    User {
        user: Box<User>,
        created: bool,
        updated: bool,
        reactivated: bool,
        roles: RoleChange,
    },
    Denied(&'static str),
}

async fn linked_user_id(pool: &SqlitePool, dir: &str, external_id: &str) -> Result<Option<Uuid>> {
    let id: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM user_identities WHERE provider_id = ? AND external_id = ?",
    )
    .bind(dir)
    .bind(external_id)
    .fetch_optional(pool)
    .await?;
    Ok(id.and_then(|s| Uuid::parse_str(&s).ok()))
}

fn email_for(dir: &Directory, du: &DirectoryUser) -> String {
    du.email
        .clone()
        .filter(|e| e.contains('@'))
        .unwrap_or_else(|| format!("{}@{}.directory.invalid", du.username, dir.id()))
}

/// Keeps username and e-mail in step with the directory. A value already
/// taken by another account is left unchanged (and logged).
async fn sync_identity_fields(
    ctx: DirectoryContext<'_>,
    dir: &Directory,
    user: &User,
    du: &DirectoryUser,
) -> Result<bool> {
    let auth = AuthService::new(ctx.pool.clone());
    let mut username = user.username.clone();
    let mut email = user.email.clone();
    if du.username != user.username {
        match auth.get_user_by_username(&du.username).await? {
            Some(other) if other.id != user.id => tracing::warn!(
                "Directory '{}': cannot rename '{}' to '{}', the name is taken",
                dir.id(),
                user.username,
                du.username
            ),
            _ => username = du.username.clone(),
        }
    }
    let wanted_email = email_for(dir, du);
    if wanted_email != user.email {
        match auth.get_user_by_email(&wanted_email).await? {
            Some(other) if other.id != user.id => tracing::warn!(
                "Directory '{}': e-mail of '{}' is used by another account",
                dir.id(),
                user.username
            ),
            _ => email = wanted_email,
        }
    }
    if username == user.username && email == user.email {
        return Ok(false);
    }
    sqlx::query("UPDATE users SET username = ?, email = ?, updated_at = ? WHERE id = ?")
        .bind(&username)
        .bind(&email)
        .bind(Utc::now().to_rfc3339())
        .bind(user.id.to_string())
        .execute(ctx.pool)
        .await?;
    Ok(true)
}

/// Creates the local account of a directory user and links the identity.
async fn create_user(
    ctx: DirectoryContext<'_>,
    dir: &Directory,
    du: &DirectoryUser,
) -> Result<Result<Uuid, &'static str>> {
    let auth = AuthService::new(ctx.pool.clone());
    if auth.get_user_by_username(&du.username).await?.is_some() {
        // Never take over an existing account by name: an administrator
        // resolves the conflict.
        return Ok(Err("username_conflict"));
    }
    let email = email_for(dir, du);
    if auth.get_user_by_email(&email).await?.is_some() {
        return Ok(Err("email_already_registered"));
    }
    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    let mut tx = ctx.pool.begin().await?;
    sqlx::query(
        "INSERT INTO users (id, organization_id, username, email, password_hash, role, auth_provider, \
         directory_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, 'local', ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(default_organization_uuid().to_string())
    .bind(&du.username)
    .bind(&email)
    .bind(DIRECTORY_PASSWORD_PLACEHOLDER)
    .bind(DIRECTORY_LEGACY_ROLE)
    .bind(dir.id())
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO user_identities (provider_id, external_id, user_id, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(dir.id())
    .bind(&du.external_id)
    .bind(id.to_string())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Ok(id))
}

/// Links a directory identity to its local account (creating it when the
/// policy allows), refreshes the identity fields, lifts a deactivation and
/// reconciles the mapped roles.
async fn resolve(
    ctx: DirectoryContext<'_>,
    dir: &Directory,
    du: &DirectoryUser,
    groups: &[DirectoryGroup],
    policy: CreatePolicy,
) -> Result<Resolved> {
    let auth = AuthService::new(ctx.pool.clone());
    let mappings = mapping::list(ctx.pool, dir.id()).await?;
    let desired = mapping::desired_roles(&mappings, groups);

    let (user_id, created, updated, reactivated) =
        match linked_user_id(ctx.pool, dir.id(), &du.external_id).await? {
            Some(uid) => {
                let Some(user) = auth.get_user_by_id(&uid).await? else {
                    return Ok(Resolved::Denied("linked_user_missing"));
                };
                let updated = sync_identity_fields(ctx, dir, &user, du).await?;
                let reactivated =
                    user.deactivated_at.is_some() && auth.reactivate_user(&uid).await?;
                if reactivated {
                    ctx.audit(
                        Some(uid),
                        "directory.user_reactivated",
                        AUDIT_DIRECTORY,
                        &user.username,
                        json!({"directory": dir.id()}),
                    )
                    .await;
                }
                (uid, false, updated, reactivated)
            }
            None => {
                if !policy.allowed {
                    return Ok(Resolved::Denied("identity_not_provisioned"));
                }
                if policy.require_role && desired.is_empty() {
                    return Ok(Resolved::Denied("no_mapped_role"));
                }
                match create_user(ctx, dir, du).await? {
                    Ok(uid) => {
                        ctx.audit(
                            Some(uid),
                            "directory.user_provisioned",
                            AUDIT_DIRECTORY,
                            &du.username,
                            json!({"directory": dir.id(), "external_id": du.external_id}),
                        )
                        .await;
                        (uid, true, false, false)
                    }
                    Err(reason) => return Ok(Resolved::Denied(reason)),
                }
            }
        };

    let roles = mapping::reconcile(ctx.pool, user_id, dir.id(), &desired).await?;
    if !roles.is_empty() {
        ctx.invalidate(&user_id);
        ctx.audit(
            Some(user_id),
            "directory.roles_changed",
            AUDIT_DIRECTORY,
            &du.username,
            json!({"directory": dir.id(), "added": roles.added, "removed": roles.removed,
                   "groups": groups.iter().map(|g| &g.dn).collect::<Vec<_>>()}),
        )
        .await;
    }
    let user = auth
        .get_user_by_id(&user_id)
        .await?
        .ok_or_else(|| anyhow!("user vanished"))?;
    Ok(Resolved::User {
        user: Box::new(user),
        created,
        updated,
        reactivated,
        roles,
    })
}

async fn deny(ctx: DirectoryContext<'_>, dir: &str, reason: &str, who: Option<&str>) {
    tracing::info!("Directory login via '{}' denied: {}", dir, reason);
    ctx.audit(
        None,
        "auth.login_denied",
        "auth",
        who.unwrap_or(dir),
        json!({"provider": dir, "reason": reason}),
    )
    .await;
}

/// Tries the enabled directories in configuration order. The first directory
/// that knows the login decides; a directory that is down is skipped. Every
/// failure looks the same to the caller.
pub async fn login(
    ctx: DirectoryContext<'_>,
    registry: &DirectoryRegistry,
    login: &str,
    password: &str,
) -> LoginResult {
    if password.is_empty() {
        return LoginResult::Denied;
    }
    for dir in registry.enabled() {
        let Some(name) = login_for_directory(login, &dir.config.domains) else {
            continue;
        };
        let identity = match dir.authenticate(&name, password).await {
            Ok(BindOutcome::Authenticated(i)) => i,
            Ok(BindOutcome::Rejected("unknown_user")) => continue,
            Ok(BindOutcome::Rejected(reason)) => {
                // The account exists in this directory: it decides.
                deny(ctx, dir.id(), reason, Some(&name)).await;
                return LoginResult::Denied;
            }
            Err(e) => {
                tracing::warn!("Directory '{}' failed during login: {}", dir.id(), e);
                deny(ctx, dir.id(), e.code(), None).await;
                continue;
            }
        };
        let policy = CreatePolicy {
            allowed: dir.config.jit,
            require_role: dir.config.deny_without_role,
        };
        let resolved = match resolve(ctx, dir, &identity.user, &identity.groups, policy).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    "Directory '{}': linking the account failed: {:#}",
                    dir.id(),
                    e
                );
                deny(ctx, dir.id(), "link_failed", Some(&identity.user.username)).await;
                return LoginResult::Denied;
            }
        };
        let user = match resolved {
            Resolved::User { user, .. } => user,
            Resolved::Denied(reason) => {
                deny(ctx, dir.id(), reason, Some(&identity.user.username)).await;
                return LoginResult::Denied;
            }
        };
        if dir.config.deny_without_role {
            let roles = AuthService::new(ctx.pool.clone())
                .get_user_roles(&user.id)
                .await
                .unwrap_or_default();
            if roles.is_empty() {
                deny(ctx, dir.id(), "no_role", Some(&user.username)).await;
                return LoginResult::Denied;
            }
        }
        let _ = sqlx::query(
            "UPDATE user_identities SET last_login_at = ? WHERE provider_id = ? AND external_id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(dir.id())
        .bind(&identity.user.external_id)
        .execute(ctx.pool)
        .await;
        return LoginResult::User {
            user,
            directory_id: dir.id().to_string(),
        };
    }
    LoginResult::Denied
}

/// Creates (or refreshes) the account of one directory user on an
/// administrator's request, whatever the JIT setting; roles still come only
/// from mappings.
pub async fn provision(
    ctx: DirectoryContext<'_>,
    dir: &Directory,
    login: &str,
) -> Result<Resolved> {
    let Some(du) = dir.find_user(login).await? else {
        return Ok(Resolved::Denied("not_found"));
    };
    if du.state != AccountState::Active {
        return Ok(Resolved::Denied(du.state.as_str()));
    }
    let groups = dir.groups_of(&du).await?;
    resolve(
        ctx,
        dir,
        &du,
        &groups,
        CreatePolicy {
            allowed: true,
            require_role: false,
        },
    )
    .await
}

/// Deactivates a directory account (sessions and API keys revoked).
pub async fn deactivate(
    ctx: DirectoryContext<'_>,
    user: &User,
    dir: &str,
    reason: &str,
) -> Result<bool> {
    let changed = AuthService::new(ctx.pool.clone())
        .deactivate_user(&user.id, reason)
        .await?;
    if changed {
        ctx.invalidate(&user.id);
        ctx.audit(
            Some(user.id),
            "directory.user_deactivated",
            AUDIT_DIRECTORY,
            &user.username,
            json!({"directory": dir, "reason": reason, "sessions_revoked": true, "api_keys_revoked": true}),
        )
        .await;
    }
    Ok(changed)
}

// ----------------------------------------------------------------- sync

#[derive(Debug, Clone, Default, Serialize, sqlx::FromRow)]
pub struct SyncRun {
    pub id: String,
    pub directory_id: String,
    pub trigger_kind: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub created: i64,
    pub updated: i64,
    pub deactivated: i64,
    pub reactivated: i64,
    pub errors: i64,
    pub error: Option<String>,
    pub started_by: Option<String>,
}

/// Holds a directory's "sync running" flag for the duration of a run.
struct RunGuard<'a>(&'a AtomicBool);

impl<'a> RunGuard<'a> {
    fn acquire(dir: &'a Directory) -> Option<Self> {
        dir.sync_flag()
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| RunGuard(dir.sync_flag()))
    }
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Whether a synchronization of this directory is running.
pub fn is_running(dir: &Directory) -> bool {
    dir.sync_flag().load(Ordering::SeqCst)
}

pub async fn list_runs(
    pool: &SqlitePool,
    dir: &str,
    limit: i64,
) -> Result<Vec<SyncRun>, sqlx::Error> {
    sqlx::query_as::<_, SyncRun>(
        "SELECT id, directory_id, trigger_kind, status, started_at, finished_at, created, updated, \
         deactivated, reactivated, errors, error, started_by FROM directory_sync_runs \
         WHERE directory_id = ? ORDER BY started_at DESC LIMIT ?",
    )
    .bind(dir)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Linked accounts of a directory: `(external_id, user)`.
async fn linked_accounts(ctx: DirectoryContext<'_>, dir: &str) -> Result<Vec<(String, User)>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT external_id, user_id FROM user_identities WHERE provider_id = ?")
            .bind(dir)
            .fetch_all(ctx.pool)
            .await?;
    let auth = AuthService::new(ctx.pool.clone());
    let mut out = Vec::with_capacity(rows.len());
    for (ext, uid) in rows {
        if let Ok(id) = Uuid::parse_str(&uid) {
            if let Some(u) = auth.get_user_by_id(&id).await? {
                out.push((ext, u));
            }
        }
    }
    Ok(out)
}

/// Reconciles the accounts of one directory: refreshes identity fields and
/// roles, deactivates accounts that are gone or disabled, reactivates those
/// that came back and creates the users a mapping selects. A run that cannot
/// list the directory changes nothing.
pub async fn run_sync(
    ctx: DirectoryContext<'_>,
    dir: &Directory,
    trigger: &str,
    started_by: Option<Uuid>,
) -> Result<SyncRun> {
    let Some(_guard) = RunGuard::acquire(dir) else {
        return Err(anyhow!(
            "a synchronization of '{}' is already running",
            dir.id()
        ));
    };
    let mut run = SyncRun {
        id: Uuid::new_v4().to_string(),
        directory_id: dir.id().to_string(),
        trigger_kind: trigger.to_string(),
        status: "running".into(),
        started_at: Utc::now().to_rfc3339(),
        started_by: started_by.map(|u| u.to_string()),
        ..Default::default()
    };
    sqlx::query(
        "INSERT INTO directory_sync_runs (id, directory_id, trigger_kind, status, started_at, started_by) \
         VALUES (?, ?, ?, 'running', ?, ?)",
    )
    .bind(&run.id)
    .bind(&run.directory_id)
    .bind(&run.trigger_kind)
    .bind(&run.started_at)
    .bind(&run.started_by)
    .execute(ctx.pool)
    .await?;

    if let Err(e) = sync_body(ctx, dir, &mut run).await {
        run.status = "failed".into();
        run.error = Some(format!("{e:#}"));
        tracing::warn!("Directory '{}' synchronization failed: {:#}", dir.id(), e);
    } else {
        run.status = if run.errors > 0 { "partial" } else { "ok" }.into();
    }
    run.finished_at = Some(Utc::now().to_rfc3339());
    sqlx::query(
        "UPDATE directory_sync_runs SET status = ?, finished_at = ?, created = ?, updated = ?, \
         deactivated = ?, reactivated = ?, errors = ?, error = ? WHERE id = ?",
    )
    .bind(&run.status)
    .bind(&run.finished_at)
    .bind(run.created)
    .bind(run.updated)
    .bind(run.deactivated)
    .bind(run.reactivated)
    .bind(run.errors)
    .bind(&run.error)
    .bind(&run.id)
    .execute(ctx.pool)
    .await?;

    let summary = json!({
        "directory": run.directory_id, "run_id": run.id, "trigger": run.trigger_kind,
        "status": run.status, "created": run.created, "updated": run.updated,
        "deactivated": run.deactivated, "reactivated": run.reactivated,
        "errors": run.errors, "error": run.error,
    });
    ctx.audit(
        started_by,
        "directory.sync",
        AUDIT_DIRECTORY,
        dir.id(),
        summary.clone(),
    )
    .await;
    EventBus::new(ctx.pool.clone())
        .emit("directory.sync.finished", summary, EventActor::system())
        .await;
    Ok(run)
}

async fn sync_body(ctx: DirectoryContext<'_>, dir: &Directory, run: &mut SyncRun) -> Result<()> {
    let listed = dir.list_users().await?;
    let linked = linked_accounts(ctx, dir.id()).await?;
    if listed.is_empty() && linked.iter().any(|(_, u)| u.is_active()) {
        // An empty answer is far more likely a broken filter or base DN than
        // a directory without users: refuse to deactivate everyone.
        return Err(anyhow!(
            "the directory returned no users; nothing was deactivated"
        ));
    }
    let by_id: HashMap<&str, &DirectoryUser> =
        listed.iter().map(|u| (u.external_id.as_str(), u)).collect();
    let policy = CreatePolicy {
        allowed: true,
        require_role: true,
    };

    for (ext, user) in &linked {
        match by_id.get(ext.as_str()) {
            Some(du) if du.state == AccountState::Active => {
                let groups = match dir.groups_of(du).await {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!(
                            "Directory '{}': groups of '{}': {}",
                            dir.id(),
                            du.username,
                            e
                        );
                        run.errors += 1;
                        continue;
                    }
                };
                match resolve(ctx, dir, du, &groups, policy).await {
                    Ok(Resolved::User {
                        updated,
                        reactivated,
                        roles,
                        ..
                    }) => {
                        if reactivated {
                            run.reactivated += 1;
                        }
                        if updated || !roles.is_empty() {
                            run.updated += 1;
                        }
                    }
                    Ok(Resolved::Denied(_)) | Err(_) => run.errors += 1,
                }
            }
            Some(du) => {
                if deactivate(ctx, user, dir.id(), du.state.as_str()).await? {
                    run.deactivated += 1;
                }
            }
            None => {
                if deactivate(ctx, user, dir.id(), "not_found_in_directory").await? {
                    run.deactivated += 1;
                }
            }
        }
    }

    let linked_ids: HashSet<&str> = linked.iter().map(|(e, _)| e.as_str()).collect();
    let has_mappings = !mapping::list(ctx.pool, dir.id()).await?.is_empty();
    if has_mappings {
        for du in listed.iter().filter(|u| {
            u.state == AccountState::Active && !linked_ids.contains(u.external_id.as_str())
        }) {
            let groups = match dir.groups_of(du).await {
                Ok(g) => g,
                Err(_) => {
                    run.errors += 1;
                    continue;
                }
            };
            match resolve(ctx, dir, du, &groups, policy).await {
                Ok(Resolved::User { created: true, .. }) => run.created += 1,
                Ok(Resolved::Denied("no_mapped_role")) | Ok(Resolved::User { .. }) => {}
                Ok(Resolved::Denied(reason)) => {
                    tracing::info!(
                        "Directory '{}': '{}' not provisioned: {}",
                        dir.id(),
                        du.username,
                        reason
                    );
                    run.errors += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        "Directory '{}': provisioning '{}': {:#}",
                        dir.id(),
                        du.username,
                        e
                    );
                    run.errors += 1;
                }
            }
        }
    }
    Ok(())
}
