//! Plugin updates, rollback and local revocation (M24, T86 to T88).
//!
//! Design and decisions: docs/architecture/PLUGIN_PLATFORM.md sections 7.4
//! and 17.5.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use super::events::EventBus;
use super::manager::{version_refusal, PluginError, PluginManager, AUDIT_PLUGINS};
use super::manifest::{capabilities, parse_cron};
use super::package::PluginPackage;
use super::storage::{collect_migrations, MigrationFile};
use crate::config::PluginUpdatesConfig;
use crate::db::PluginGovernanceRepository;
use crate::models::*;

const MAX_BATCH: usize = 100;
const MAX_REASON_CHARS: usize = 500;
const MAX_WINDOW_CHARS: usize = 100;
const USAGE_DAYS: i64 = 7;

/// Who acts: an administrator through the API or the update scheduler.
#[derive(Debug, Clone)]
pub struct UpdateActor {
    pub user_id: Option<Uuid>,
    pub organization_id: Uuid,
    pub name: String,
}

impl UpdateActor {
    pub fn user(user_id: Uuid, organization_id: Uuid, name: &str) -> Self {
        Self {
            user_id: Some(user_id),
            organization_id,
            name: name.to_string(),
        }
    }

    pub fn scheduler() -> Self {
        Self {
            user_id: None,
            organization_id: default_organization_uuid(),
            name: "update-scheduler".to_string(),
        }
    }

    fn event_actor(&self) -> EventActor {
        match self.user_id {
            Some(_) => EventActor::user(self.name.clone()),
            None => EventActor::system(),
        }
    }
}

/// How an update was started; recorded in the audit entry and the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateTrigger {
    Manual,
    Batch,
    Schedule,
}

impl UpdateTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateTrigger::Manual => "manual",
            UpdateTrigger::Batch => "batch",
            UpdateTrigger::Schedule => "schedule",
        }
    }
}

// ---- pure helpers -----------------------------------------------------------

fn semver_of(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v).ok()
}

fn runnable<'a>(
    versions: &'a [PluginVersionRecord],
    blocked: &'a HashSet<String>,
) -> impl Iterator<Item = (semver::Version, &'a PluginVersionRecord)> + 'a {
    versions
        .iter()
        .filter(move |v| version_refusal(v, blocked).is_none())
        .filter_map(|v| semver_of(&v.version).map(|s| (s, v)))
}

/// Newest stored version above `active` that may run (not revoked and not
/// blocked by hash).
pub fn newest_available<'a>(
    versions: &'a [PluginVersionRecord],
    active: Option<&str>,
    blocked: &'a HashSet<String>,
) -> Option<&'a PluginVersionRecord> {
    let active = semver_of(active?)?;
    runnable(versions, blocked)
        .filter(|(s, _)| *s > active)
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, v)| v)
}

/// Newest stored version below `active` that may run: the default target of
/// a rollback.
pub fn newest_older<'a>(
    versions: &'a [PluginVersionRecord],
    active: &str,
    blocked: &'a HashSet<String>,
) -> Option<&'a PluginVersionRecord> {
    let active = semver_of(active)?;
    runnable(versions, blocked)
        .filter(|(s, _)| *s < active)
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, v)| v)
}

/// Highest migration a version brings to the plugin database.
pub fn schema_level(files: &[MigrationFile], target: Option<u32>) -> u32 {
    let max = files.iter().map(|f| f.number).max().unwrap_or(0);
    target.map_or(max, |t| t.min(max))
}

/// The migrations among `numbers` above `level` that have no `.down` file
/// in `files` (a number absent from `files` cannot be reverted either).
pub fn missing_downs(
    files: &[MigrationFile],
    numbers: impl IntoIterator<Item = u32>,
    level: u32,
) -> Vec<u32> {
    let mut out: Vec<u32> = numbers
        .into_iter()
        .filter(|n| *n > level)
        .filter(|n| {
            files
                .iter()
                .find(|f| f.number == *n)
                .is_none_or(|f| f.down.is_none())
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Capabilities `new` declares that `old` did not and that are not already
/// approved with the same scope. Grants are per plugin, so going back to a
/// version whose capabilities were approved before does not ask again.
pub fn added_unapproved(
    old: &PluginManifest,
    new: &PluginManifest,
    grants: &[PluginGrant],
) -> Vec<String> {
    let had = capabilities(old);
    capabilities(new)
        .into_iter()
        .filter(|c| !had.iter().any(|h| h.same_grant(c)))
        .filter(|c| {
            !grants
                .iter()
                .any(|g| g.capability == c.name && g.scope == c.scope)
        })
        .map(|c| c.name)
        .collect()
}

/// Checks an update window: a five-field UTC cron expression.
pub fn validate_window(window: &str) -> Result<(), String> {
    if window.chars().count() > MAX_WINDOW_CHARS {
        return Err(format!(
            "the update window is longer than {} characters",
            MAX_WINDOW_CHARS
        ));
    }
    parse_cron(window).map(|_| ())
}

/// True when a minute matched by the window falls in `(since, now]`. Without
/// a window updates may run at any time; an invalid one never opens.
pub fn window_open(window: Option<&str>, since: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    let Some(w) = window else { return true };
    match parse_cron(w) {
        Ok(s) => s.after(&since).next().is_some_and(|t| t <= now),
        Err(_) => false,
    }
}

/// Next minute the window opens after `now` (None without a window).
pub fn next_window(window: Option<&str>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    parse_cron(window?).ok()?.after(&now).next()
}

/// Migration files of a package and the schema level it brings.
fn package_migrations(pkg: &PluginPackage) -> Result<(Vec<MigrationFile>, u32), String> {
    let Some(spec) = &pkg.manifest.migrations else {
        return Ok((Vec::new(), 0));
    };
    let prefix = format!("{}/", spec.dir.trim_end_matches('/'));
    let files: Vec<(String, Vec<u8>)> = pkg
        .files
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let migrations = collect_migrations(&files, &spec.dir)?;
    let level = schema_level(&migrations, spec.target);
    Ok((migrations, level))
}

fn join_numbers(n: &[u32]) -> String {
    n.iter()
        .map(|x| format!("{:04}", x))
        .collect::<Vec<_>>()
        .join(", ")
}

fn error_parts(e: &PluginError) -> (String, String) {
    let code = match e {
        PluginError::BadRequest(_) => "bad_request",
        PluginError::NotFound(_) => "not_found",
        PluginError::Forbidden(_) => "forbidden",
        PluginError::Conflict { code, .. } => code,
        PluginError::Unprocessable { code, .. } => code,
        PluginError::Internal(_) => "internal",
    };
    let message = match e {
        PluginError::Internal(inner) => {
            tracing::error!("plugin update failed: {:#}", inner);
            "Plugin operation failed".to_string()
        }
        other => other.to_string(),
    };
    (code.to_string(), message)
}

fn unprocessable(code: &'static str, message: impl Into<String>) -> PluginError {
    PluginError::Unprocessable {
        code,
        message: message.into(),
        details: None,
    }
}

fn conflict(code: &'static str, message: impl Into<String>) -> PluginError {
    PluginError::Conflict {
        code,
        message: message.into(),
    }
}

fn parse_manifest(rec: &PluginVersionRecord) -> Result<PluginManifest, PluginError> {
    serde_json::from_str(&rec.manifest_json).map_err(|e| PluginError::Internal(e.into()))
}

fn validate_reason(reason: &str) -> Result<&str, PluginError> {
    let r = reason.trim();
    if r.is_empty() || r.chars().count() > MAX_REASON_CHARS || r.chars().any(|c| c.is_control()) {
        return Err(PluginError::BadRequest(format!(
            "A reason of 1 to {} printable characters is required",
            MAX_REASON_CHARS
        )));
    }
    Ok(r)
}

/// Migration files and level of one stored version.
struct VersionMigrations {
    files: Vec<MigrationFile>,
    level: u32,
}

type UpdateFailure = (PluginError, Option<String>, Option<String>);

// ---- manager side -------------------------------------------------------------

impl PluginManager {
    /// Writes the audit entry and, when `event` is set, emits the event of
    /// the same name.
    async fn record(
        &self,
        actor: &UpdateActor,
        action: &str,
        plugin_id: &str,
        mut details: Value,
        event: Option<Value>,
    ) {
        if let Some(d) = details.as_object_mut() {
            d.insert("actor".into(), json!(actor.name));
        }
        self.audit(
            actor.organization_id,
            actor.user_id,
            AUDIT_PLUGINS,
            action,
            plugin_id,
            details,
        )
        .await;
        if let Some(payload) = event {
            EventBus::new(self.pool().clone())
                .emit(action, payload, actor.event_actor())
                .await;
        }
    }

    /// Reads the package only when the manifest declares migrations.
    async fn version_migrations(
        &self,
        id: &str,
        version: &str,
        manifest: &PluginManifest,
    ) -> Result<VersionMigrations, PluginError> {
        if manifest.migrations.is_none() {
            return Ok(VersionMigrations {
                files: Vec::new(),
                level: 0,
            });
        }
        let pkg = self.read_stored(id, version).await?;
        let (files, level) =
            package_migrations(&pkg).map_err(|e| unprocessable("migration_invalid", e))?;
        Ok(VersionMigrations { files, level })
    }

    async fn applied_migrations(&self, id: &str) -> Vec<u32> {
        match self.storage() {
            Some(st) => st.applied_numbers(id).await.unwrap_or_else(|e| {
                tracing::warn!("could not read the migrations of {}: {}", id, e);
                Vec::new()
            }),
            None => Vec::new(),
        }
    }

    /// Warning shown before updating from `active` to `pkg`: the migrations
    /// the update adds that a rollback could not revert.
    pub(super) async fn rollback_warning_for(
        &self,
        pkg: &PluginPackage,
        active: &str,
    ) -> Option<String> {
        let (new_files, new_level) = package_migrations(pkg).ok()?;
        if new_level == 0 {
            return None;
        }
        let active_level = match self.manifest_of(&pkg.manifest.id, active).await {
            Ok(m) => self
                .version_migrations(&pkg.manifest.id, active, &m)
                .await
                .map(|v| v.level)
                .unwrap_or(0),
            Err(_) => 0,
        };
        let added = new_files
            .iter()
            .map(|f| f.number)
            .filter(|n| *n <= new_level);
        let missing = missing_downs(&new_files, added, active_level);
        (!missing.is_empty()).then(|| {
            format!(
                "Rolling back from {} to {} will be blocked: migration(s) {} have no .down file",
                pkg.manifest.version,
                active,
                join_numbers(&missing)
            )
        })
    }

    /// The same warning for a stored version, read from its package.
    async fn stored_rollback_warning(
        &self,
        id: &str,
        active: &str,
        rec: &PluginVersionRecord,
    ) -> Option<String> {
        let m = parse_manifest(rec).ok()?;
        m.migrations.as_ref()?;
        let pkg = self.read_stored(id, &rec.version).await.ok()?;
        self.rollback_warning_for(&pkg, active).await
    }

    // ---- versions (T87) -----------------------------------------------------

    /// Every stored version with what can be done with it, newest first.
    pub async fn list_versions(&self, id: &str) -> Result<Vec<PluginVersionView>, PluginError> {
        let rec = self.require(id).await?;
        let mut stored = self.repo().list_versions(id).await?;
        stored.sort_by(
            |a, b| match (semver_of(&a.version), semver_of(&b.version)) {
                (Some(x), Some(y)) => y.cmp(&x),
                _ => b.version.cmp(&a.version),
            },
        );
        let active = rec.active_version.clone();
        let active_sv = active.as_deref().and_then(semver_of);
        let active_manifest = match &active {
            Some(v) => self.manifest_of(id, v).await.ok(),
            None => None,
        };
        let active_migs = match (&active, &active_manifest) {
            (Some(v), Some(m)) => self.version_migrations(id, v, m).await.ok(),
            _ => None,
        };
        let applied = self.applied_migrations(id).await;
        let grants = self.repo().list_grants(id).await?;
        let usage = PluginGovernanceRepository::new(self.pool())
            .usage_since(Some(id), Utc::now() - Duration::days(USAGE_DAYS))
            .await?;

        let mut out = Vec::new();
        for v in stored {
            let manifest = parse_manifest(&v).ok();
            let relation = if Some(&v.version) == active.as_ref() {
                VersionRelation::Active
            } else {
                match (semver_of(&v.version), &active_sv) {
                    (Some(s), Some(a)) if s > *a => VersionRelation::Newer,
                    _ => VersionRelation::Older,
                }
            };
            let level = match (&relation, &active_migs, &manifest) {
                (VersionRelation::Active, Some(m), _) => m.level,
                (_, _, Some(m)) => self
                    .version_migrations(id, &v.version, m)
                    .await
                    .map(|x| x.level)
                    .unwrap_or(0),
                _ => 0,
            };
            let added = match (&relation, &active_manifest, &manifest) {
                (VersionRelation::Active, _, _) => Vec::new(),
                (_, Some(old), Some(new)) => added_unapproved(old, new, &grants),
                _ => Vec::new(),
            };
            let refusal = version_refusal(&v, self.blocked_hashes()).map(|(_, m)| m);
            let (can_activate, blocked_reason, rollback_warning) = match relation {
                VersionRelation::Active => (false, refusal, None),
                _ if refusal.is_some() => (false, refusal, None),
                VersionRelation::Newer => {
                    let warning = match &active {
                        Some(a) => self.stored_rollback_warning(id, a, &v).await,
                        None => None,
                    };
                    (true, None, warning)
                }
                VersionRelation::Older => {
                    let files = active_migs.as_ref().map(|m| m.files.as_slice());
                    let missing =
                        missing_downs(files.unwrap_or_default(), applied.iter().copied(), level);
                    if missing.is_empty() {
                        (true, None, None)
                    } else {
                        (
                            false,
                            Some(format!(
                                "Rollback is blocked: migration(s) {} have no .down file",
                                join_numbers(&missing)
                            )),
                            None,
                        )
                    }
                }
            };
            let totals = UsageTotals::from_rows(usage.iter().filter(|r| r.version == v.version));
            out.push(PluginVersionView {
                pending: rec.pending_version.as_deref() == Some(v.version.as_str()),
                blocked: self.is_blocked_hash(&v.package_sha256),
                schema_level: level,
                added_capabilities: added,
                can_activate,
                blocked_reason,
                rollback_warning,
                usage: totals,
                relation,
                record: v,
            });
        }
        Ok(out)
    }

    // ---- updates (T86) --------------------------------------------------------

    /// Update state of every installed plugin.
    pub async fn update_overview(&self) -> Result<Vec<PluginUpdateInfo>, PluginError> {
        let now = Utc::now();
        let mut out = Vec::new();
        for p in self.list().await? {
            let stored = self.repo().list_versions(&p.id).await?;
            let available =
                newest_available(&stored, p.active_version.as_deref(), self.blocked_hashes())
                    .cloned();
            let (added, warning) = match (&available, &p.active_version) {
                (Some(v), Some(active)) => {
                    let grants = self.repo().list_grants(&p.id).await?;
                    let added = match (self.manifest_of(&p.id, active).await, parse_manifest(v)) {
                        (Ok(old), Ok(new)) => added_unapproved(&old, &new, &grants),
                        _ => Vec::new(),
                    };
                    let warning = self.stored_rollback_warning(&p.id, active, v).await;
                    (added, warning)
                }
                _ => (Vec::new(), None),
            };
            out.push(PluginUpdateInfo {
                requires_approval: !added.is_empty(),
                added_capabilities: added,
                rollback_warning: warning,
                next_window_at: match p.auto_update {
                    AutoUpdatePolicy::Off => None,
                    _ => next_window(p.update_window.as_deref(), now),
                },
                plugin_id: p.id,
                name: p.name,
                state: p.state,
                active_version: p.active_version,
                available_version: available.map(|v| v.version),
                pending_version: p.pending_version,
                auto_update: p.auto_update,
                update_window: p.update_window,
                update_hold_version: p.update_hold_version,
                update_hold_reason: p.update_hold_reason,
            });
        }
        Ok(out)
    }

    /// Applies an update to one plugin: `target` or the newest stored
    /// version that may run. Errors are returned as HTTP errors.
    pub async fn apply_update(
        &self,
        id: &str,
        target: Option<&str>,
        actor: &UpdateActor,
        trigger: UpdateTrigger,
    ) -> Result<PluginUpdateResult, PluginError> {
        self.update_one(id, target, actor, trigger)
            .await
            .map_err(|(e, _, _)| e)
    }

    /// Applies the available update of each listed plugin; a failure of one
    /// does not stop the others.
    pub async fn apply_updates(
        &self,
        ids: &[String],
        actor: &UpdateActor,
    ) -> Result<Vec<PluginUpdateResult>, PluginError> {
        if ids.is_empty() || ids.len() > MAX_BATCH {
            return Err(PluginError::BadRequest(format!(
                "Select between 1 and {} plugins",
                MAX_BATCH
            )));
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for id in ids.iter().filter(|i| seen.insert(i.as_str())) {
            let r = match self.update_one(id, None, actor, UpdateTrigger::Batch).await {
                Ok(r) => r,
                Err((e, from, to)) => failed_result(id, &e, from, to),
            };
            out.push(r);
        }
        Ok(out)
    }

    async fn update_one(
        &self,
        id: &str,
        target: Option<&str>,
        actor: &UpdateActor,
        trigger: UpdateTrigger,
    ) -> Result<PluginUpdateResult, UpdateFailure> {
        let rec = self.require(id).await.map_err(|e| (e, None, None))?;
        let active = rec.active_version.clone();
        let fail = |e: PluginError, to: Option<String>| (e, active.clone(), to);
        let Some(from) = active.clone() else {
            return Err(fail(
                conflict("no_active_version", "The plugin has no active version"),
                None,
            ));
        };
        let stored = self
            .repo()
            .list_versions(id)
            .await
            .map_err(|e| fail(e.into(), None))?;
        let candidate = match target {
            Some(t) => {
                let v = stored.iter().find(|v| v.version == t).ok_or_else(|| {
                    fail(
                        PluginError::NotFound(format!("{} {} is not stored", id, t)),
                        Some(t.to_string()),
                    )
                })?;
                if let Some((code, msg)) = version_refusal(v, self.blocked_hashes()) {
                    return Err(fail(unprocessable(code, msg), Some(t.to_string())));
                }
                let newer = matches!(
                    (semver_of(t), semver_of(&from)),
                    (Some(n), Some(o)) if n > o
                );
                if !newer {
                    return Err(fail(
                        PluginError::BadRequest(format!(
                            "{} is not newer than the active {}; use a rollback to go back",
                            t, from
                        )),
                        Some(t.to_string()),
                    ));
                }
                v.clone()
            }
            None => match newest_available(&stored, Some(&from), self.blocked_hashes()) {
                Some(v) => v.clone(),
                None => {
                    return Ok(PluginUpdateResult {
                        plugin_id: id.to_string(),
                        outcome: UpdateOutcome::UpToDate,
                        from_version: Some(from.clone()),
                        to_version: None,
                        added_capabilities: Vec::new(),
                        code: None,
                        message: None,
                    })
                }
            },
        };
        let to = candidate.version.clone();
        let result = |outcome, added: Vec<String>| PluginUpdateResult {
            plugin_id: id.to_string(),
            outcome,
            from_version: Some(from.clone()),
            to_version: Some(to.clone()),
            added_capabilities: added,
            code: None,
            message: None,
        };

        let old = self
            .manifest_of(id, &from)
            .await
            .map_err(|e| fail(e, Some(to.clone())))?;
        let new = parse_manifest(&candidate).map_err(|e| fail(e, Some(to.clone())))?;
        let grants = self
            .repo()
            .list_grants(id)
            .await
            .map_err(|e| fail(e.into(), Some(to.clone())))?;
        let added = added_unapproved(&old, &new, &grants);
        if !added.is_empty() {
            // Escalation through an update needs a new approval (section 6.2):
            // the version waits and the previous one keeps running.
            if rec.pending_version.as_deref() != Some(to.as_str()) {
                self.repo()
                    .set_pending_version(id, Some(&to))
                    .await
                    .map_err(|e| fail(e.into(), Some(to.clone())))?;
                self.record(
                    actor,
                    "plugin.update_pending",
                    id,
                    json!({"from": from, "to": to, "added_capabilities": added,
                           "trigger": trigger.as_str()}),
                    Some(
                        json!({"plugin_id": id, "version": to, "active_version": from,
                                "added_capabilities": added, "trigger": trigger.as_str()}),
                    ),
                )
                .await;
            }
            return Ok(result(UpdateOutcome::PendingApproval, added));
        }

        match self.activate_version(id, &to, Some(&from)).await {
            Ok(()) => {
                // The update supersedes a held version that is not newer.
                let superseded = rec.update_hold_version.as_deref().is_some_and(
                    |h| matches!((semver_of(h), semver_of(&to)), (Some(h), Some(t)) if h <= t),
                );
                if superseded {
                    let _ = self.repo().set_update_hold(id, None, None).await;
                }
                self.record(
                    actor,
                    "plugin.upgraded",
                    id,
                    json!({"from": from, "to": to, "trigger": trigger.as_str()}),
                    Some(
                        json!({"plugin_id": id, "version": to, "upgraded_from": from,
                                "trigger": trigger.as_str()}),
                    ),
                )
                .await;
                Ok(result(UpdateOutcome::Updated, Vec::new()))
            }
            Err(e) => {
                let (code, message) = error_parts(&e);
                if trigger == UpdateTrigger::Schedule {
                    // Do not retry the same version in every window.
                    let reason = format!("automatic update failed: {}", message);
                    let _ = self
                        .repo()
                        .set_update_hold(id, Some(&to), Some(&reason))
                        .await;
                }
                self.record(
                    actor,
                    "plugin.update_failed",
                    id,
                    json!({"from": from, "to": to, "trigger": trigger.as_str(),
                           "code": code, "error": message}),
                    Some(
                        json!({"plugin_id": id, "version": to, "active_version": from,
                                "code": code, "trigger": trigger.as_str()}),
                    ),
                )
                .await;
                Err(fail(e, Some(to)))
            }
        }
    }

    /// Sets the automatic update policy and window of one plugin.
    pub async fn set_update_policy(
        &self,
        id: &str,
        req: &UpdatePolicyRequest,
        actor: &UpdateActor,
    ) -> Result<PluginSummary, PluginError> {
        let window = req
            .update_window
            .as_deref()
            .map(str::trim)
            .filter(|w| !w.is_empty());
        if let Some(w) = window {
            validate_window(w).map_err(PluginError::BadRequest)?;
        }
        self.require(id).await?;
        self.repo()
            .set_update_policy(id, req.auto_update, window)
            .await?;
        self.record(
            actor,
            "plugin.update_policy_changed",
            id,
            json!({"auto_update": req.auto_update, "update_window": window}),
            None,
        )
        .await;
        Ok(self.get(id).await?.summary)
    }

    // ---- rollback (T87) -----------------------------------------------------------

    /// Reactivates an older stored version. The plugin database is first
    /// reverted with the `.down` migrations of the version being left; when
    /// one is missing nothing changes and `rollback_blocked` is returned.
    pub async fn rollback(
        &self,
        id: &str,
        target: Option<&str>,
        actor: &UpdateActor,
    ) -> Result<RollbackResult, PluginError> {
        let rec = self.require(id).await?;
        let from = rec
            .active_version
            .clone()
            .ok_or_else(|| conflict("no_active_version", "The plugin has no active version"))?;
        let stored = self.repo().list_versions(id).await?;
        let to_rec = match target {
            Some(t) => {
                let v = stored
                    .iter()
                    .find(|v| v.version == t)
                    .ok_or_else(|| PluginError::NotFound(format!("{} {} is not stored", id, t)))?;
                if let Some((code, msg)) = version_refusal(v, self.blocked_hashes()) {
                    return Err(unprocessable(code, msg));
                }
                let older = matches!(
                    (semver_of(t), semver_of(&from)),
                    (Some(n), Some(o)) if n < o
                );
                if !older {
                    return Err(PluginError::BadRequest(format!(
                        "{} is not older than the active {}; apply it as an update",
                        t, from
                    )));
                }
                v.clone()
            }
            None => newest_older(&stored, &from, self.blocked_hashes())
                .cloned()
                .ok_or_else(|| {
                    conflict(
                        "no_previous_version",
                        "No older stored version can be activated",
                    )
                })?,
        };
        let to = to_rec.version.clone();
        match self.rollback_to(&rec, &from, &to).await {
            Ok((schema_from, schema_to)) => {
                // The scheduler must not re-apply the version just left.
                let reason = format!("rolled back by {}", actor.name);
                self.repo()
                    .set_update_hold(id, Some(&from), Some(&reason))
                    .await?;
                self.record(
                    actor,
                    "plugin.rolled_back",
                    id,
                    json!({"from": from, "to": to, "schema_from": schema_from,
                           "schema_to": schema_to}),
                    Some(
                        json!({"plugin_id": id, "version": to, "rolled_back_from": from,
                                "schema_from": schema_from, "schema_to": schema_to}),
                    ),
                )
                .await;
                Ok(RollbackResult {
                    plugin_id: id.to_string(),
                    from_version: from,
                    to_version: to,
                    schema_from,
                    schema_to,
                    plugin: self.get(id).await?,
                })
            }
            Err(e) => {
                let (code, message) = error_parts(&e);
                self.record(
                    actor,
                    "plugin.rollback_failed",
                    id,
                    json!({"from": from, "to": to, "code": code, "error": message}),
                    None,
                )
                .await;
                Err(e)
            }
        }
    }

    async fn rollback_to(
        &self,
        rec: &PluginRecord,
        from: &str,
        to: &str,
    ) -> Result<(u32, u32), PluginError> {
        let id = rec.id.as_str();
        // Everything that can refuse the rollback is checked before the
        // plugin database changes.
        let to_pkg = self
            .load_verified(id, to)
            .await
            .map(|(p, _)| p)
            .map_err(|(_, code, msg)| unprocessable(code, msg))?;
        if rec.state == PluginState::Enabled && to_pkg.manifest.runtime.kind == RuntimeKind::Wasm {
            let missing = self.missing_required(&to_pkg.manifest, id).await;
            if !missing.is_empty() {
                return Err(PluginError::Unprocessable {
                    code: "capabilities_not_approved",
                    message: format!(
                        "Required capabilities of {} are not approved: {}",
                        to,
                        missing.join(", ")
                    ),
                    details: Some(json!(missing)),
                });
            }
        }
        let (_, to_level) =
            package_migrations(&to_pkg).map_err(|e| unprocessable("migration_invalid", e))?;
        let applied = self.applied_migrations(id).await;
        let schema_from = applied.iter().copied().max().unwrap_or(0);
        let needed: Vec<u32> = applied.iter().copied().filter(|n| *n > to_level).collect();

        // The `.down` files come from the version being left (only it knows
        // how to undo its migrations), read from its pinned package even when
        // that version is revoked; they run in the plugin's own database.
        let mut reverted: Option<(Vec<MigrationFile>, Option<u32>)> = None;
        let mut schema_to = schema_from;
        if !needed.is_empty() {
            let from_pkg = self.read_stored(id, from).await?;
            let (files, _) =
                package_migrations(&from_pkg).map_err(|e| unprocessable("migration_invalid", e))?;
            let missing = missing_downs(&files, needed.iter().copied(), to_level);
            if !missing.is_empty() {
                return Err(PluginError::Unprocessable {
                    code: "rollback_blocked",
                    message: format!(
                        "Rollback from {} to {} is blocked: migration(s) {} have no .down file",
                        from,
                        to,
                        join_numbers(&missing)
                    ),
                    details: Some(json!({"missing_down": missing})),
                });
            }
            let storage = self.storage().ok_or_else(|| {
                conflict(
                    "runtime_unavailable",
                    "Plugin storage is not available, so the database cannot be reverted",
                )
            })?;
            schema_to = storage
                .migrate_down(id, &files, to_level)
                .await
                .map_err(|e| unprocessable("migration_failed", e.to_string()))?;
            self.repo().set_schema_version(id, schema_to).await?;
            let target = from_pkg.manifest.migrations.as_ref().and_then(|m| m.target);
            reverted = Some((files, target));
        }

        if let Err(e) = self.activate_version(id, to, Some(from)).await {
            // The version being left stays (or is put back) active: bring its
            // schema back. Data removed by the `.down` files is not restored.
            if let (Some((files, target)), Some(storage)) = (reverted, self.storage()) {
                match storage.migrate_up(id, &files, target).await {
                    Ok(v) => {
                        let _ = self.repo().set_schema_version(id, v).await;
                    }
                    Err(up) => {
                        tracing::error!("could not restore the schema of {}: {}", id, up);
                        let _ = self
                            .repo()
                            .set_state(id, PluginState::Error, Some("migration_failed"))
                            .await;
                        let _ = self.reload().await;
                    }
                }
            }
            return Err(e);
        }
        Ok((schema_from, schema_to))
    }

    // ---- local revocation (T88) -------------------------------------------------

    /// Marks a stored version revoked. When it is the running version the
    /// plugin is taken out of play at once (state `disabled`, reason
    /// `version_revoked`) without running its hooks.
    pub async fn revoke_version(
        &self,
        id: &str,
        version: &str,
        reason: &str,
        actor: &UpdateActor,
    ) -> Result<RevokeVersionResult, PluginError> {
        let reason = validate_reason(reason)?;
        let rec = self.require(id).await?;
        let v =
            self.repo().get_version(id, version).await?.ok_or_else(|| {
                PluginError::NotFound(format!("{} {} is not stored", id, version))
            })?;
        if v.revoked {
            return Err(conflict(
                "already_revoked",
                "The version is already revoked",
            ));
        }
        self.repo()
            .set_version_revoked(id, version, Some((reason, &actor.name)))
            .await?;
        if rec.pending_version.as_deref() == Some(version) {
            self.repo().set_pending_version(id, None).await?;
        }
        // A revoked version loses its trusted module approval for good (M31).
        self.drop_module_trust(id, version, "version_revoked").await;
        let running = rec.active_version.as_deref() == Some(version)
            && matches!(rec.state, PluginState::Enabled | PluginState::Error);
        if running {
            if let Some(w) = self.wasm_runtime() {
                w.forget(id);
            }
            self.repo()
                .set_state(id, PluginState::Disabled, Some("version_revoked"))
                .await?;
            self.clear_governance(id, false).await;
        }
        self.reload().await?;
        self.record(
            actor,
            "plugin.version_revoked",
            id,
            json!({"version": version, "reason": reason, "deactivated": running}),
            Some(
                json!({"plugin_id": id, "version": version, "reason": reason,
                        "deactivated": running}),
            ),
        )
        .await;
        Ok(RevokeVersionResult {
            plugin_id: id.to_string(),
            version: version.to_string(),
            deactivated: running,
            plugin: self.get(id).await?,
        })
    }

    /// Clears the revoked flag. Nothing is re-enabled; a version whose hash
    /// is in `plugins.blocked_hashes` stays blocked.
    pub async fn unrevoke_version(
        &self,
        id: &str,
        version: &str,
        actor: &UpdateActor,
    ) -> Result<PluginDetail, PluginError> {
        self.require(id).await?;
        let v =
            self.repo().get_version(id, version).await?.ok_or_else(|| {
                PluginError::NotFound(format!("{} {} is not stored", id, version))
            })?;
        if !v.revoked {
            return Err(conflict("not_revoked", "The version is not revoked"));
        }
        self.repo().set_version_revoked(id, version, None).await?;
        self.record(
            actor,
            "plugin.version_unrevoked",
            id,
            json!({"version": version, "previous_reason": v.revoked_reason}),
            Some(json!({"plugin_id": id, "version": version})),
        )
        .await;
        self.get(id).await
    }

    // ---- scheduler (T86) ----------------------------------------------------------

    /// One pass of the update scheduler over `(since, now]`: each plugin with
    /// a policy other than `off` whose window opened gets the newest stored
    /// version its policy allows, except the held one.
    pub async fn update_tick(
        &self,
        since: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Vec<PluginUpdateResult> {
        let plugins = match self.repo().list_plugins().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("plugin update scheduler could not list plugins: {}", e);
                return Vec::new();
            }
        };
        let actor = UpdateActor::scheduler();
        let mut out = Vec::new();
        for p in plugins {
            if p.auto_update == AutoUpdatePolicy::Off {
                continue;
            }
            let Some(active) = p.active_version.as_deref().and_then(semver_of) else {
                continue;
            };
            if !window_open(p.update_window.as_deref(), since, now) {
                continue;
            }
            let stored = match self.repo().list_versions(&p.id).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("could not list the versions of {}: {}", p.id, e);
                    continue;
                }
            };
            let candidate = runnable(&stored, self.blocked_hashes())
                .filter(|(_, v)| p.update_hold_version.as_deref() != Some(v.version.as_str()))
                .filter(|(s, _)| p.auto_update.allows(&active, s))
                .max_by(|a, b| a.0.cmp(&b.0))
                .map(|(_, v)| v.version.clone());
            let Some(to) = candidate else { continue };
            if p.pending_version.as_deref() == Some(to.as_str()) {
                continue; // already waiting for approval
            }
            match self
                .update_one(&p.id, Some(&to), &actor, UpdateTrigger::Schedule)
                .await
            {
                Ok(r) => out.push(r),
                Err((e, from, to)) => {
                    let r = failed_result(&p.id, &e, from, to);
                    tracing::warn!(
                        "automatic update of {} failed: {}",
                        p.id,
                        r.message.as_deref().unwrap_or_default()
                    );
                    out.push(r);
                }
            }
        }
        out
    }
}

fn failed_result(
    id: &str,
    e: &PluginError,
    from: Option<String>,
    to: Option<String>,
) -> PluginUpdateResult {
    let (code, message) = error_parts(e);
    PluginUpdateResult {
        plugin_id: id.to_string(),
        outcome: UpdateOutcome::Failed,
        from_version: from,
        to_version: to,
        added_capabilities: Vec::new(),
        code: Some(code),
        message: Some(message),
    }
}

/// Checks the update windows every `tick_secs` and applies due updates.
/// A window that passed while the host was down is not caught up.
pub fn start_update_scheduler(manager: Arc<PluginManager>, cfg: PluginUpdatesConfig) {
    if !cfg.scheduler_enabled {
        tracing::info!("plugin update scheduler is disabled");
        return;
    }
    let secs = cfg.tick_secs.max(10);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(secs));
        let mut since = Utc::now();
        tick.tick().await;
        loop {
            tick.tick().await;
            let now = Utc::now();
            for r in manager.update_tick(since, now).await {
                tracing::info!(
                    "automatic plugin update {}: {:?} {:?} -> {:?}",
                    r.plugin_id,
                    r.outcome,
                    r.from_version,
                    r.to_version
                );
            }
            since = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::plugins::host::{HostApiImpl, PackageModuleSource};
    use crate::services::plugins::manager::BuiltinPlugin;
    use crate::services::plugins::package::{build_signed_package, sha256_hex};
    use crate::services::plugins::storage::{PluginStorage, StorageLimits};
    use crate::services::plugins::wasm::{WasmConfig, WasmInvoker};
    use crate::services::plugins::InstallOptions;
    use chrono::TimeZone;
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    const SEED: [u8; 32] = [7; 32];

    struct Noop;
    impl BuiltinPlugin for Noop {}

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir() -> TempDir {
        let p = std::env::temp_dir().join(format!("ovx_m24_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    fn project_key() -> Vec<(String, [u8; 32])> {
        vec![(
            "OpenVox".into(),
            SigningKey::from_bytes(&SEED).verifying_key().to_bytes(),
        )]
    }

    fn builtin_manager(pool: SqlitePool, dir: &TempDir, blocked: &[String]) -> PluginManager {
        PluginManager::new(pool, dir.0.clone(), 8)
            .with_builtin_runtime("demo", Arc::new(Noop))
            .with_builtin_keys(project_key())
            .with_blocked_hashes(blocked)
    }

    /// A builtin plugin; `extra` adds capability lines.
    fn builtin(version: &str, extra: &str) -> Vec<u8> {
        let y = format!(
            "api_version: 2\nid: demo\nname: Demo\nversion: {version}\npublisher: ACME\n\
             runtime: {{ kind: builtin, entry: demo }}\ncapabilities:\n  - ui:slot:page\n{extra}\
             contributes:\n  slots:\n    - {{ slot: page, id: home, render: declarative, spec: ui/home.json, path: /p/demo/home }}\n"
        );
        build_signed_package(
            &[
                ("plugin.yaml", y.as_bytes()),
                ("ui/home.json", br#"{"layout":"markdown","content":"hi"}"#),
            ],
            "demo",
            version,
            2,
            SEED,
        )
    }

    /// The seeded administrator (audit entries reference a real user).
    fn admin() -> UpdateActor {
        let id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        UpdateActor::user(id, default_organization_uuid(), "admin")
    }

    fn stage() -> InstallOptions {
        InstallOptions { stage: true }
    }

    fn rec(version: &str, revoked: bool, sha: &str) -> PluginVersionRecord {
        PluginVersionRecord {
            plugin_id: "p".into(),
            version: version.into(),
            api_version: 2,
            package_sha256: sha.into(),
            signed_by_key_id: "k".into(),
            signed_at: String::new(),
            revoked,
            revoked_reason: None,
            revoked_by: None,
            revoked_at: None,
            added_at: Utc::now(),
            manifest_json: String::new(),
            signature: String::new(),
        }
    }

    fn mig(n: u32, down: bool) -> MigrationFile {
        MigrationFile {
            number: n,
            name: format!("m{n}"),
            up: "SELECT 1".into(),
            down: down.then(|| "SELECT 1".into()),
        }
    }

    #[test]
    fn policies_allow_only_newer_versions_within_their_range() {
        let v = |s: &str| semver::Version::parse(s).unwrap();
        let from = v("1.2.3");
        assert!(!AutoUpdatePolicy::Off.allows(&from, &v("1.2.4")));
        assert!(AutoUpdatePolicy::Patch.allows(&from, &v("1.2.9")));
        assert!(!AutoUpdatePolicy::Patch.allows(&from, &v("1.3.0")));
        assert!(AutoUpdatePolicy::Minor.allows(&from, &v("1.9.0")));
        assert!(!AutoUpdatePolicy::Minor.allows(&from, &v("2.0.0")));
        assert!(AutoUpdatePolicy::Any.allows(&from, &v("3.0.0")));
        assert!(!AutoUpdatePolicy::Any.allows(&from, &v("1.2.3")));
        assert!(!AutoUpdatePolicy::Any.allows(&from, &v("1.0.0")));
    }

    #[test]
    fn a_window_opens_when_a_matching_minute_falls_in_the_tick() {
        let at = |h, m, s| Utc.with_ymd_and_hms(2026, 10, 31, h, m, s).unwrap();
        // Every minute from 02:00 to 03:59 UTC.
        let w = Some("* 2-3 * * *");
        assert!(window_open(w, at(2, 29, 30), at(2, 30, 30)));
        assert!(!window_open(w, at(4, 0, 0), at(4, 1, 0)));
        // A single-minute window is not missed when the tick drifts past it.
        assert!(window_open(Some("0 2 * * *"), at(1, 59, 30), at(2, 0, 45)));
        assert!(!window_open(Some("0 2 * * *"), at(2, 0, 0), at(2, 0, 59)));
        // No window: any time. An invalid one never opens.
        assert!(window_open(None, at(1, 0, 0), at(1, 1, 0)));
        assert!(!window_open(Some("bogus"), at(1, 0, 0), at(5, 0, 0)));
        assert_eq!(next_window(w, at(1, 0, 0)), Some(at(2, 0, 0)));
        assert!(validate_window("0 2 * * 6").is_ok());
        assert!(validate_window("0 2 * *").is_err());
        assert!(validate_window(&"1 ".repeat(60)).is_err());
    }

    #[test]
    fn rollback_needs_a_down_file_for_every_migration_above_the_target() {
        let files = vec![mig(1, true), mig(2, true), mig(3, false)];
        assert_eq!(schema_level(&files, None), 3);
        assert_eq!(schema_level(&files, Some(2)), 2);
        assert_eq!(schema_level(&[], Some(5)), 0);
        assert!(missing_downs(&files, [1, 2], 1).is_empty());
        assert_eq!(missing_downs(&files, [1, 2, 3], 1), vec![3]);
        // An applied migration the package does not know cannot be reverted.
        assert_eq!(missing_downs(&files, [4], 2), vec![4]);
    }

    #[test]
    fn revoked_and_blocked_versions_are_never_candidates() {
        let blocked: HashSet<String> = ["bad".to_string()].into();
        let versions = vec![
            rec("1.0.0", false, "a"),
            rec("1.1.0", false, "b"),
            rec("1.2.0", true, "c"),
            rec("1.3.0", false, "bad"),
            rec("0.9.0", true, "d"),
            rec("0.8.0", false, "e"),
        ];
        let avail = newest_available(&versions, Some("1.0.0"), &blocked).unwrap();
        assert_eq!(avail.version, "1.1.0");
        assert!(newest_available(&versions, Some("1.1.0"), &blocked).is_none());
        let older = newest_older(&versions, "1.1.0", &blocked).unwrap();
        assert_eq!(older.version, "1.0.0");
        assert_eq!(
            newest_older(&versions, "1.0.0", &blocked).unwrap().version,
            "0.8.0"
        );
    }

    #[tokio::test]
    async fn staged_update_rollback_and_hold() {
        let dir = temp_dir();
        let m = builtin_manager(pool().await, &dir, &[]);
        m.reload().await.unwrap();
        m.install(&builtin("1.0.0", ""), "admin", None)
            .await
            .unwrap();
        m.enable("demo").await.unwrap();

        // Staging stores the version without activating it.
        let out = m
            .install_with(&builtin("1.1.0", ""), "admin", None, stage())
            .await
            .unwrap();
        assert!(out.staged);
        assert_eq!(out.detail.summary.active_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            out.detail.summary.available_version.as_deref(),
            Some("1.1.0")
        );

        let r = m
            .apply_update("demo", None, &admin(), UpdateTrigger::Manual)
            .await
            .unwrap();
        assert_eq!(r.outcome, UpdateOutcome::Updated);
        let d = m.get("demo").await.unwrap();
        assert_eq!(d.summary.active_version.as_deref(), Some("1.1.0"));
        assert_eq!(d.summary.state, PluginState::Enabled);
        let r = m
            .apply_update("demo", None, &admin(), UpdateTrigger::Manual)
            .await
            .unwrap();
        assert_eq!(r.outcome, UpdateOutcome::UpToDate);

        // An explicit target must be newer.
        let err = m
            .apply_update("demo", Some("1.0.0"), &admin(), UpdateTrigger::Manual)
            .await
            .unwrap_err();
        assert!(matches!(err, PluginError::BadRequest(_)), "{err}");

        let rb = m.rollback("demo", None, &admin()).await.unwrap();
        assert_eq!(
            (rb.from_version.as_str(), rb.to_version.as_str()),
            ("1.1.0", "1.0.0")
        );
        assert_eq!(rb.plugin.summary.state, PluginState::Enabled);
        assert_eq!(
            rb.plugin.summary.update_hold_version.as_deref(),
            Some("1.1.0")
        );
        assert!(!m.registry().is_empty());

        // The scheduler does not re-apply the version that was rolled back.
        let policy = UpdatePolicyRequest {
            auto_update: AutoUpdatePolicy::Any,
            update_window: None,
        };
        m.set_update_policy("demo", &policy, &admin())
            .await
            .unwrap();
        let now = Utc::now();
        assert!(m
            .update_tick(now - Duration::minutes(1), now)
            .await
            .is_empty());
        // A manual update releases the hold.
        m.apply_update("demo", Some("1.1.0"), &admin(), UpdateTrigger::Manual)
            .await
            .unwrap();
        assert!(m
            .get("demo")
            .await
            .unwrap()
            .summary
            .update_hold_version
            .is_none());

        let versions = m.list_versions("demo").await.unwrap();
        assert_eq!(versions[0].record.version, "1.1.0");
        assert_eq!(versions[0].relation, VersionRelation::Active);
        assert_eq!(versions[1].relation, VersionRelation::Older);
        assert!(versions[1].can_activate);

        let audit: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE action IN ('plugin.upgraded', 'plugin.rolled_back')",
        )
        .fetch_one(m.pool())
        .await
        .unwrap();
        assert_eq!(audit, 3);
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_outbox WHERE type = 'plugin.rolled_back'",
        )
        .fetch_one(m.pool())
        .await
        .unwrap();
        assert_eq!(events, 1);
    }

    #[tokio::test]
    async fn an_update_that_adds_a_capability_waits_for_approval() {
        let dir = temp_dir();
        let m = builtin_manager(pool().await, &dir, &[]);
        m.reload().await.unwrap();
        m.install(&builtin("1.0.0", ""), "admin", None)
            .await
            .unwrap();
        m.install_with(&builtin("1.1.0", "  - kv:rw\n"), "admin", None, stage())
            .await
            .unwrap();
        let info = m.update_overview().await.unwrap();
        assert_eq!(info[0].available_version.as_deref(), Some("1.1.0"));
        assert!(info[0].requires_approval);
        assert_eq!(info[0].added_capabilities, vec!["kv:rw".to_string()]);

        let results = m
            .apply_updates(&["demo".into(), "ghost".into(), "demo".into()], &admin())
            .await
            .unwrap();
        // Duplicates are applied once; an unknown plugin fails alone.
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].outcome, UpdateOutcome::PendingApproval);
        assert_eq!(results[1].outcome, UpdateOutcome::Failed);
        assert_eq!(results[1].code.as_deref(), Some("not_found"));
        let d = m.get("demo").await.unwrap();
        assert_eq!(d.summary.active_version.as_deref(), Some("1.0.0"));
        assert_eq!(d.summary.pending_version.as_deref(), Some("1.1.0"));

        let req = ApproveCapabilitiesRequest {
            version: None,
            capabilities: vec!["ui:slot:page".into(), "kv:rw".into()],
        };
        let d = m.approve_capabilities("demo", &req, "admin").await.unwrap();
        assert_eq!(d.summary.active_version.as_deref(), Some("1.1.0"));
        // Going back and forth does not ask again for an approved capability.
        m.rollback("demo", None, &admin()).await.unwrap();
        let r = m
            .apply_update("demo", Some("1.1.0"), &admin(), UpdateTrigger::Manual)
            .await
            .unwrap();
        assert_eq!(r.outcome, UpdateOutcome::Updated);
        assert!(m.apply_updates(&[], &admin()).await.is_err());
    }

    #[tokio::test]
    async fn the_scheduler_follows_policy_and_window() {
        let dir = temp_dir();
        let m = builtin_manager(pool().await, &dir, &[]);
        m.reload().await.unwrap();
        m.install(&builtin("1.0.0", ""), "admin", None)
            .await
            .unwrap();
        for v in ["1.0.1", "1.1.0"] {
            m.install_with(&builtin(v, ""), "admin", None, stage())
                .await
                .unwrap();
        }
        let at = |h, m| Utc.with_ymd_and_hms(2026, 11, 7, h, m, 0).unwrap();
        // Off by default: nothing happens.
        assert!(m.update_tick(at(1, 0), at(23, 0)).await.is_empty());

        let policy = UpdatePolicyRequest {
            auto_update: AutoUpdatePolicy::Patch,
            update_window: Some("* 2 * * *".into()),
        };
        let s = m
            .set_update_policy("demo", &policy, &admin())
            .await
            .unwrap();
        assert_eq!(s.update_window.as_deref(), Some("* 2 * * *"));
        let bad = UpdatePolicyRequest {
            auto_update: AutoUpdatePolicy::Patch,
            update_window: Some("every night".into()),
        };
        assert!(matches!(
            m.set_update_policy("demo", &bad, &admin()).await,
            Err(PluginError::BadRequest(_))
        ));

        // Outside the window.
        assert!(m.update_tick(at(3, 0), at(3, 1)).await.is_empty());
        // Inside: the newest patch release only.
        let r = m.update_tick(at(2, 10), at(2, 11)).await;
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].to_version.as_deref(), Some("1.0.1"));
        assert_eq!(
            m.get("demo")
                .await
                .unwrap()
                .summary
                .active_version
                .as_deref(),
            Some("1.0.1")
        );
        // Nothing left for a patch policy; `any` takes the minor release.
        assert!(m.update_tick(at(2, 11), at(2, 12)).await.is_empty());
        let any = UpdatePolicyRequest {
            auto_update: AutoUpdatePolicy::Any,
            update_window: None,
        };
        m.set_update_policy("demo", &any, &admin()).await.unwrap();
        let r = m.update_tick(at(9, 0), at(9, 1)).await;
        assert_eq!(r[0].outcome, UpdateOutcome::Updated);
        assert_eq!(r[0].to_version.as_deref(), Some("1.1.0"));
        let actor: String = sqlx::query_scalar(
            "SELECT json_extract(payload, '$.trigger') FROM event_outbox
             WHERE type = 'plugin.upgraded' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(m.pool())
        .await
        .unwrap();
        assert_eq!(actor, "schedule");
    }

    #[tokio::test]
    async fn revoking_the_running_version_takes_the_plugin_out_of_play() {
        let dir = temp_dir();
        let m = builtin_manager(pool().await, &dir, &[]);
        m.reload().await.unwrap();
        m.install(&builtin("1.0.0", ""), "admin", None)
            .await
            .unwrap();
        m.install(&builtin("1.1.0", ""), "admin", None)
            .await
            .unwrap();
        m.enable("demo").await.unwrap();
        assert!(!m.registry().is_empty());

        assert!(m
            .revoke_version("demo", "1.1.0", " ", &admin())
            .await
            .is_err());
        let out = m
            .revoke_version("demo", "1.1.0", "supply chain incident", &admin())
            .await
            .unwrap();
        assert!(out.deactivated);
        assert_eq!(out.plugin.summary.state, PluginState::Disabled);
        assert_eq!(
            out.plugin.summary.state_reason.as_deref(),
            Some("version_revoked")
        );
        assert!(m.registry().is_empty());
        let err = m.enable("demo").await.unwrap_err();
        assert!(err.to_string().contains("revoked"), "{err}");
        assert!(matches!(
            m.revoke_version("demo", "1.1.0", "again", &admin()).await,
            Err(PluginError::Conflict {
                code: "already_revoked",
                ..
            })
        ));
        // A revoked version cannot be the target of an update either.
        assert!(m
            .apply_update("demo", Some("1.1.0"), &admin(), UpdateTrigger::Manual)
            .await
            .is_err());

        // Going back to the previous version and enabling it works.
        let rb = m.rollback("demo", None, &admin()).await.unwrap();
        assert_eq!(rb.to_version, "1.0.0");
        m.enable("demo").await.unwrap();
        // 1.1.0 is revoked, so it is not offered as an update.
        assert!(m
            .get("demo")
            .await
            .unwrap()
            .summary
            .available_version
            .is_none());
        let d = m.unrevoke_version("demo", "1.1.0", &admin()).await.unwrap();
        assert_eq!(d.summary.available_version.as_deref(), Some("1.1.0"));
        assert!(matches!(
            m.unrevoke_version("demo", "1.1.0", &admin()).await,
            Err(PluginError::Conflict {
                code: "not_revoked",
                ..
            })
        ));
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_outbox
             WHERE type IN ('plugin.version_revoked', 'plugin.version_unrevoked')",
        )
        .fetch_one(m.pool())
        .await
        .unwrap();
        assert_eq!(events, 2);
    }

    #[tokio::test]
    async fn blocked_hashes_are_refused_at_install_and_disabled_at_reload() {
        let dir = temp_dir();
        let pool = pool().await;
        let pkg = builtin("1.0.0", "");
        let sha = sha256_hex(&pkg);

        let m = builtin_manager(pool.clone(), &dir, &[]);
        m.reload().await.unwrap();
        m.install(&pkg, "admin", None).await.unwrap();
        m.enable("demo").await.unwrap();

        // The administrator adds the hash to config.yaml and restarts.
        let blocked = builtin_manager(pool.clone(), &dir, &[sha.to_uppercase(), "nope".into()]);
        let failed = blocked.reload().await.unwrap();
        assert_eq!(
            failed,
            vec![("demo".to_string(), "version_blocked".to_string())]
        );
        let d = blocked.get("demo").await.unwrap();
        assert_eq!(d.summary.state, PluginState::Disabled);
        assert!(blocked.registry().is_empty());
        let v = blocked.list_versions("demo").await.unwrap();
        assert!(v[0].blocked);

        // A fresh install of the same package is refused before anything is stored.
        blocked.uninstall("demo", true).await.unwrap();
        let pre = blocked.preflight(&pkg).await.unwrap();
        assert!(!pre.can_install);
        assert!(pre.issues.iter().any(|i| i.contains("blocked_hashes")));
        assert!(matches!(
            blocked.install(&pkg, "admin", None).await,
            Err(PluginError::Unprocessable {
                code: "install_blocked",
                ..
            })
        ));
    }

    // ---- update with migrations and rollback (definition of done, phase 6) ----

    /// A guest whose every handler answers `{"ok":{}}`.
    fn guest() -> Vec<u8> {
        wat::parse_str(
            r#"(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))
  (data (i32.const 16) "{\"ok\":{}}")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $p))
  (func (export "handle") (param i32 i32) (result i64)
    (i64.or (i64.shl (i64.const 16) (i64.const 32)) (i64.const 9))))"#,
        )
        .unwrap()
    }

    /// Version with migrations 1..=n; the last one has no `.down` file when
    /// `last_down` is false.
    fn wasm_package(version: &str, n: u32, last_down: bool) -> Vec<u8> {
        let y = format!(
            "api_version: 2\nid: wdemo\nname: W\nversion: {version}\npublisher: ACME\n\
             runtime: {{ kind: wasm, entry: plugin.wasm }}\ncapabilities:\n  - db:own\n\
             migrations: {{ dir: migrations }}\n"
        );
        let wasm = guest();
        let mut owned: Vec<(String, Vec<u8>)> = vec![
            ("plugin.yaml".into(), y.into_bytes()),
            ("plugin.wasm".into(), wasm),
        ];
        for i in 1..=n {
            owned.push((
                format!("migrations/{i:04}_t{i}.up.sql"),
                format!("CREATE TABLE t{i}(v TEXT)").into_bytes(),
            ));
            if i < n || last_down {
                owned.push((
                    format!("migrations/{i:04}_t{i}.down.sql"),
                    format!("DROP TABLE t{i}").into_bytes(),
                ));
            }
        }
        let files: Vec<(&str, &[u8])> = owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        build_signed_package(&files, "wdemo", version, 2, SEED)
    }

    async fn wasm_manager(dir: &TempDir) -> (Arc<PluginManager>, Arc<PluginStorage>) {
        let pool = pool().await;
        let manager = Arc::new(
            PluginManager::new(pool.clone(), dir.0.clone(), 8).with_builtin_keys(project_key()),
        );
        manager.reload().await.unwrap();
        let storage = Arc::new(PluginStorage::new(
            pool,
            dir.0.clone(),
            StorageLimits::default(),
        ));
        let host = Arc::new(HostApiImpl::new(
            &manager,
            Default::default(),
            storage.clone(),
            None,
        ));
        let source = Arc::new(PackageModuleSource {
            data_dir: dir.0.clone(),
            max_package_bytes: 8 * 1024 * 1024,
        });
        let wasm = Arc::new(WasmInvoker::new(WasmConfig::default(), source, host).unwrap());
        manager.attach_wasm(storage.clone(), wasm);
        (manager, storage)
    }

    async fn table_exists(st: &PluginStorage, t: &str) -> bool {
        st.db_query("wdemo", &format!("SELECT COUNT(*) FROM {t}"), &[])
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn update_with_migration_and_rollback() {
        let dir = temp_dir();
        let (m, st) = wasm_manager(&dir).await;
        m.install(&wasm_package("1.0.0", 1, true), "admin", None)
            .await
            .unwrap();
        let approve = ApproveCapabilitiesRequest {
            version: None,
            capabilities: vec!["db:own".into()],
        };
        m.approve_capabilities("wdemo", &approve, "admin")
            .await
            .unwrap();
        m.enable("wdemo").await.unwrap();
        assert_eq!(st.schema_version("wdemo").await.unwrap(), 1);

        // 1.1.0 adds a reversible migration: no warning, applied by update.
        let v11 = wasm_package("1.1.0", 2, true);
        assert!(m.preflight(&v11).await.unwrap().rollback_warning.is_none());
        m.install_with(&v11, "admin", None, stage()).await.unwrap();
        let r = m
            .apply_update("wdemo", None, &admin(), UpdateTrigger::Manual)
            .await
            .unwrap();
        assert_eq!(r.outcome, UpdateOutcome::Updated);
        assert_eq!(st.schema_version("wdemo").await.unwrap(), 2);
        assert!(table_exists(&st, "t2").await);

        // Rollback reverts the plugin database with the .down files.
        let rb = m.rollback("wdemo", None, &admin()).await.unwrap();
        assert_eq!((rb.schema_from, rb.schema_to), (2, 1));
        assert_eq!(rb.plugin.summary.active_version.as_deref(), Some("1.0.0"));
        assert_eq!(rb.plugin.summary.state, PluginState::Enabled);
        assert!(!table_exists(&st, "t2").await);
        assert!(table_exists(&st, "t1").await);

        // 1.2.0 brings an irreversible migration: the warning comes first.
        let v12 = wasm_package("1.2.0", 3, false);
        let pre = m.preflight(&v12).await.unwrap();
        let warning = pre.rollback_warning.expect("warned before updating");
        assert!(warning.contains("0003"), "{warning}");
        m.install(&v12, "admin", None).await.unwrap();
        assert_eq!(st.schema_version("wdemo").await.unwrap(), 3);

        let versions = m.list_versions("wdemo").await.unwrap();
        let older: Vec<_> = versions
            .iter()
            .filter(|v| v.relation == VersionRelation::Older)
            .collect();
        assert_eq!(older.len(), 2);
        assert!(older.iter().all(|v| !v.can_activate));
        assert!(older[0].blocked_reason.as_deref().unwrap().contains("0003"));

        // The rollback is refused and nothing changes.
        let err = m.rollback("wdemo", None, &admin()).await.unwrap_err();
        assert!(
            matches!(
                err,
                PluginError::Unprocessable {
                    code: "rollback_blocked",
                    ..
                }
            ),
            "{err}"
        );
        assert_eq!(st.schema_version("wdemo").await.unwrap(), 3);
        let d = m.get("wdemo").await.unwrap();
        assert_eq!(d.summary.active_version.as_deref(), Some("1.2.0"));
        assert_eq!(d.summary.state, PluginState::Enabled);
        let failed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'plugin.rollback_failed'",
        )
        .fetch_one(m.pool())
        .await
        .unwrap();
        assert_eq!(failed, 1);
    }
}
