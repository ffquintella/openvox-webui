//! Plugin manager: install, trust evaluation, lifecycle and the slot registry.
//!
//! Fail closed: a package is only stored, enabled or loaded when its ed25519
//! signature is valid and the signer is an active, unexpired trusted key that
//! is allowed to sign that plugin id.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::manifest::{capabilities, describe_capability};
use super::package::{self, PackageError, PluginPackage};
use super::registry::Registry;
use crate::db::{AuditRepository, PluginRepository};
use crate::models::*;

pub const AUDIT_PLUGINS: &str = "plugins";
pub const AUDIT_TRUST: &str = "plugin_trust";

/// Hook for first-party plugins compiled into the host (`runtime.kind: builtin`).
pub trait BuiltinPlugin: Send + Sync {
    fn on_enable(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn on_disable(&self) {}
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{message}")]
    Conflict { code: &'static str, message: String },
    #[error("{message}")]
    Unprocessable {
        code: &'static str,
        message: String,
        details: Option<Value>,
    },
    #[error("{0}")]
    Forbidden(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
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

impl From<PackageError> for PluginError {
    fn from(e: PackageError) -> Self {
        match e {
            PackageError::Unsigned => unprocessable("unsigned", e.to_string()),
            PackageError::Malformed(_) => unprocessable("invalid_package", e.to_string()),
            PackageError::Integrity(_) => unprocessable("integrity_failed", e.to_string()),
            PackageError::Manifest(list) => PluginError::Unprocessable {
                code: "invalid_manifest",
                message: "The plugin manifest is invalid".to_string(),
                details: Some(json!(list)),
            },
        }
    }
}

impl From<sqlx::Error> for PluginError {
    fn from(e: sqlx::Error) -> Self {
        PluginError::Internal(e.into())
    }
}

/// Confirmation to trust the signer of a package during install. Built by the
/// API layer only after it checked `plugin_trust` permission and re-auth.
pub struct TrustRequest {
    pub confirm_prefix: String,
    pub label: Option<String>,
}

pub struct InstallOutcome {
    pub detail: PluginDetail,
    pub trusted_key_id: Option<String>,
    pub upgraded_from: Option<String>,
}

pub struct PluginManager {
    pool: SqlitePool,
    data_dir: PathBuf,
    max_package_bytes: u64,
    builtin_keys: Vec<(String, [u8; 32])>,
    builtin_runtimes: BTreeMap<String, Arc<dyn BuiltinPlugin>>,
    registry: RwLock<Arc<Registry>>,
}

type Failure = (PluginState, &'static str, String);

impl PluginManager {
    pub fn new(pool: SqlitePool, data_dir: PathBuf, max_package_mb: u64) -> Self {
        Self {
            pool,
            data_dir,
            max_package_bytes: max_package_mb.max(1) * 1024 * 1024,
            builtin_keys: Vec::new(),
            builtin_runtimes: BTreeMap::new(),
            registry: RwLock::new(Arc::new(Registry::default())),
        }
    }

    /// OpenVox project keys embedded in the host (cannot be removed at runtime).
    pub fn with_builtin_keys(mut self, keys: Vec<(String, [u8; 32])>) -> Self {
        self.builtin_keys = keys;
        self
    }

    pub fn with_builtin_runtime(mut self, entry: &str, p: Arc<dyn BuiltinPlugin>) -> Self {
        self.builtin_runtimes.insert(entry.to_string(), p);
        self
    }

    pub fn registry(&self) -> Arc<Registry> {
        self.registry.read().map(|r| r.clone()).unwrap_or_default()
    }

    fn repo(&self) -> PluginRepository<'_> {
        PluginRepository::new(&self.pool)
    }

    fn package_path(&self, id: &str, version: &str) -> PathBuf {
        self.data_dir
            .join("packages")
            .join(id)
            .join(format!("{}.ovxp", version))
    }

    pub async fn audit(
        &self,
        org: Uuid,
        user: Option<Uuid>,
        resource_type: &str,
        action: &str,
        resource_id: &str,
        details: Value,
    ) {
        if let Err(e) = AuditRepository::new(&self.pool)
            .insert(
                org,
                user,
                action,
                resource_type,
                Some(resource_id),
                Some(&details),
                None,
            )
            .await
        {
            tracing::warn!("Failed to write plugin audit entry '{}': {}", action, e);
        }
    }

    // ---- startup and integrity -------------------------------------------

    /// Seeds builtin keys, re-verifies every enabled plugin and rebuilds the
    /// registry. Returns the plugins that failed verification.
    pub async fn reload(&self) -> Result<Vec<(String, String)>, PluginError> {
        self.seed_builtin_keys().await?;
        let mut failed = Vec::new();
        let mut registry = Registry::default();
        for p in self.repo().list_plugins().await? {
            if p.state != PluginState::Enabled {
                continue;
            }
            let Some(version) = p.active_version.clone() else {
                continue;
            };
            match self.load_verified(&p.id, &version).await {
                Ok((pkg, _)) => registry.add_plugin(&pkg),
                Err((state, code, msg)) => {
                    tracing::error!("plugin {} failed verification: {}", p.id, msg);
                    self.repo().set_state(&p.id, state, Some(code)).await?;
                    self.audit(
                        default_organization_uuid(),
                        None,
                        AUDIT_PLUGINS,
                        "plugin.verification_failed",
                        &p.id,
                        json!({"reason": code, "message": msg}),
                    )
                    .await;
                    failed.push((p.id.clone(), code.to_string()));
                }
            }
        }
        if let Ok(mut r) = self.registry.write() {
            *r = Arc::new(registry);
        }
        Ok(failed)
    }

    async fn seed_builtin_keys(&self) -> Result<(), PluginError> {
        for (label, pk) in &self.builtin_keys {
            let key_id = package::key_id_of(pk);
            if self.repo().get_key(&key_id).await?.is_none() {
                self.repo()
                    .insert_key(&TrustedKeyRecord {
                        key_id,
                        public_key: pk.to_vec(),
                        label: label.clone(),
                        origin: KeyOrigin::Builtin,
                        allowed_plugin_ids: None,
                        expires_at: None,
                        revoked: false,
                        added_by: None,
                        added_at: Utc::now(),
                        revoked_at: None,
                        revoked_reason: None,
                    })
                    .await?;
            }
        }
        Ok(())
    }

    /// Reads the stored package, checks its pinned hash and its signature
    /// against the current trust store.
    async fn load_verified(
        &self,
        id: &str,
        version: &str,
    ) -> Result<(PluginPackage, TrustedKeyRecord), Failure> {
        let internal = |e: String| (PluginState::Error, "load_failed", e);
        let rec = self
            .repo()
            .get_version(id, version)
            .await
            .map_err(|e| internal(e.to_string()))?
            .ok_or_else(|| internal("version record is missing".into()))?;
        let bytes = tokio::fs::read(self.package_path(id, version))
            .await
            .map_err(|e| {
                (
                    PluginState::Error,
                    "integrity_failed",
                    format!("package file: {}", e),
                )
            })?;
        if package::sha256_hex(&bytes) != rec.package_sha256 {
            return Err((
                PluginState::Error,
                "integrity_failed",
                "the stored package no longer matches its pinned hash".into(),
            ));
        }
        let pkg = package::read_package(&bytes, self.max_package_bytes)
            .map_err(|e| (PluginState::Error, "integrity_failed", e.to_string()))?;
        let pk = package::verify_signature(&pkg)
            .map_err(|e| (PluginState::Error, "signature_invalid", e))?;
        let keys = self
            .repo()
            .list_keys()
            .await
            .map_err(|e| internal(e.to_string()))?;
        match evaluate_trust(&pkg.manifest.id, &pkg.envelope.header.key_id, &pk, &keys) {
            (SignatureStatus::Valid, Some(k), _) => Ok((pkg, k)),
            (SignatureStatus::KeyRevoked, _, m) => Err((
                PluginState::Disabled,
                "signer_revoked",
                m.unwrap_or_default(),
            )),
            (SignatureStatus::KeyExpired, _, m) => Err((
                PluginState::Disabled,
                "signer_expired",
                m.unwrap_or_default(),
            )),
            (SignatureStatus::KeyScope, _, m) => {
                Err((PluginState::Disabled, "signer_scope", m.unwrap_or_default()))
            }
            (_, _, m) => Err((
                PluginState::Disabled,
                "signer_untrusted",
                m.unwrap_or_else(|| "the signer is not trusted".into()),
            )),
        }
    }

    // ---- preflight and install -------------------------------------------

    pub async fn preflight(&self, bytes: &[u8]) -> Result<PreflightResult, PluginError> {
        Ok(self.inspect(bytes).await?.0)
    }

    async fn inspect(
        &self,
        bytes: &[u8],
    ) -> Result<(PreflightResult, PluginPackage, Option<[u8; 32]>), PluginError> {
        let pkg = package::read_package(bytes, self.max_package_bytes)?;
        let m = &pkg.manifest;
        let header = &pkg.envelope.header;
        let keys = self.repo().list_keys().await?;

        let (status, key, message, pk) = match package::verify_signature(&pkg) {
            Ok(pk) => {
                let (s, k, msg) = evaluate_trust(&m.id, &header.key_id, &pk, &keys);
                (s, k, msg, Some(pk))
            }
            Err(e) => (SignatureStatus::Invalid, None, Some(e), None),
        };

        let mut issues = Vec::new();
        let existing = self.repo().get_plugin(&m.id).await?;
        let installed_version = existing.as_ref().and_then(|p| p.active_version.clone());
        if let Some(active) = &installed_version {
            self.check_upgrade(&pkg, active, &mut issues).await?;
        }
        if let Some(min) = &m.min_webui_version {
            if let (Ok(min), Ok(cur)) = (
                semver::Version::parse(min),
                semver::Version::parse(env!("CARGO_PKG_VERSION")),
            ) {
                if min > cur {
                    issues.push(format!("requires OpenVox WebUI {} or newer", min));
                }
            }
        }
        if m.runtime.kind == RuntimeKind::Builtin
            && !key.as_ref().is_some_and(|k| k.origin == KeyOrigin::Builtin)
        {
            issues.push("built-in plugins must be signed by an OpenVox project key".into());
        }

        let can_install = issues.is_empty()
            && matches!(
                status,
                SignatureStatus::Valid | SignatureStatus::UntrustedKey
            );
        let result = PreflightResult {
            manifest: ManifestSummary {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
                publisher: m.publisher.clone(),
                api_version: m.api_version,
                license: m.license.clone(),
                runtime_kind: m.runtime.kind,
                min_webui_version: m.min_webui_version.clone(),
            },
            package_sha256: pkg.package_sha256.clone(),
            signature: SignatureReport {
                status,
                key_id: header.key_id.clone(),
                key_label: key.as_ref().map(|k| k.label.clone()),
                key_origin: key.as_ref().map(|k| k.origin),
                signed_at: header.signed_at.clone(),
                message,
            },
            capabilities: capabilities(m).iter().map(describe_capability).collect(),
            declared_pii_fields: m
                .telemetry
                .as_ref()
                .map(|t| t.declared_pii_fields.clone())
                .unwrap_or_default(),
            installed_version,
            issues,
            can_install,
        };
        Ok((result, pkg, pk))
    }

    /// A new version of an installed plugin must be newer and must not ask
    /// for capabilities the installed one lacks (approval arrives with M20).
    async fn check_upgrade(
        &self,
        pkg: &PluginPackage,
        active: &str,
        issues: &mut Vec<String>,
    ) -> Result<(), PluginError> {
        let m = &pkg.manifest;
        let (Ok(new), Ok(old)) = (
            semver::Version::parse(&m.version),
            semver::Version::parse(active),
        ) else {
            return Ok(());
        };
        if new <= old {
            issues.push(format!(
                "version {} is not newer than the installed {}",
                new, old
            ));
        }
        if let Some(rec) = self.repo().get_version(&m.id, active).await? {
            if let Ok(old_m) = serde_json::from_str::<PluginManifest>(&rec.manifest_json) {
                let had: Vec<_> = capabilities(&old_m);
                let added: Vec<String> = capabilities(m)
                    .into_iter()
                    .filter(|c| !had.contains(c))
                    .map(|c| c.name)
                    .collect();
                if !added.is_empty() {
                    issues.push(format!(
                        "adds capabilities that need administrator approval, which is not available yet: {}",
                        added.join(", ")
                    ));
                }
            }
        }
        Ok(())
    }

    pub async fn install(
        &self,
        bytes: &[u8],
        actor: &str,
        trust: Option<TrustRequest>,
    ) -> Result<InstallOutcome, PluginError> {
        let (pre, pkg, pk) = self.inspect(bytes).await?;
        let m = &pkg.manifest;
        let mut trusted_key_id = None;

        match pre.signature.status {
            SignatureStatus::Valid => {}
            SignatureStatus::UntrustedKey => {
                let key_id = &pre.signature.key_id;
                let ok = trust.as_ref().is_some_and(|t| {
                    !t.confirm_prefix.is_empty()
                        && key_id.starts_with(&t.confirm_prefix.to_lowercase())
                        && t.confirm_prefix.len() == 8
                });
                if !ok {
                    return Err(PluginError::Unprocessable {
                        code: "untrusted_signer",
                        message: "The package is signed by a key that is not trusted yet".into(),
                        details: Some(json!({"key_id": key_id})),
                    });
                }
            }
            other => {
                return Err(PluginError::Unprocessable {
                    code: "signature_rejected",
                    message: pre
                        .signature
                        .message
                        .clone()
                        .unwrap_or_else(|| "The package signature was rejected".into()),
                    details: Some(json!({"status": other})),
                })
            }
        }
        if !pre.issues.is_empty() {
            // The "built-in needs a project key" rule must hold even for a key
            // the administrator is about to trust, so it is checked after trust.
            return Err(PluginError::Unprocessable {
                code: "install_blocked",
                message: pre.issues.join("; "),
                details: Some(json!(pre.issues)),
            });
        }

        if pre.signature.status == SignatureStatus::UntrustedKey {
            let pk = pk.ok_or_else(|| PluginError::BadRequest("missing public key".into()))?;
            let t = trust.expect("checked above");
            let label = t
                .label
                .filter(|l| !l.trim().is_empty())
                .unwrap_or_else(|| format!("Trusted while installing {}", m.id));
            validate_label(&label)?;
            let key_id = package::key_id_of(&pk);
            self.repo()
                .insert_key(&TrustedKeyRecord {
                    key_id: key_id.clone(),
                    public_key: pk.to_vec(),
                    label,
                    origin: KeyOrigin::Admin,
                    allowed_plugin_ids: None,
                    expires_at: None,
                    revoked: false,
                    added_by: Some(actor.to_string()),
                    added_at: Utc::now(),
                    revoked_at: None,
                    revoked_reason: None,
                })
                .await?;
            trusted_key_id = Some(key_id);
        }

        if self.repo().get_version(&m.id, &m.version).await?.is_some() {
            return Err(conflict(
                "version_exists",
                format!("{} {} is already installed", m.id, m.version),
            ));
        }

        let path = self.package_path(&m.id, &m.version);
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| PluginError::Internal(e.into()))?;
        }
        let tmp = path.with_extension("ovxp.tmp");
        tokio::fs::write(&tmp, bytes)
            .await
            .map_err(|e| PluginError::Internal(e.into()))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| PluginError::Internal(e.into()))?;

        let upgraded_from = pre.installed_version.clone();
        let record = PluginVersionRecord {
            plugin_id: m.id.clone(),
            version: m.version.clone(),
            api_version: m.api_version,
            package_sha256: pkg.package_sha256.clone(),
            signed_by_key_id: pkg.envelope.header.key_id.clone(),
            signed_at: pkg.envelope.header.signed_at.clone(),
            revoked: false,
            added_at: Utc::now(),
            manifest_json: serde_json::to_string(m).map_err(|e| PluginError::Internal(e.into()))?,
            signature: serde_json::to_string(&pkg.envelope)
                .map_err(|e| PluginError::Internal(e.into()))?,
        };
        let db = async {
            if upgraded_from.is_none() {
                self.repo()
                    .insert_plugin(&m.id, &m.version, Some(actor))
                    .await?;
            }
            self.repo().insert_version(&record).await?;
            if upgraded_from.is_some() {
                self.repo().set_active_version(&m.id, &m.version).await?;
            }
            anyhow::Ok(())
        };
        if let Err(e) = db.await {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(PluginError::Internal(e));
        }
        if upgraded_from.is_some() {
            self.reload().await?;
        }
        Ok(InstallOutcome {
            detail: self.get(&m.id).await?,
            trusted_key_id,
            upgraded_from,
        })
    }

    // ---- lifecycle --------------------------------------------------------

    pub async fn enable(&self, id: &str) -> Result<PluginDetail, PluginError> {
        let rec = self.require(id).await?;
        let version = rec
            .active_version
            .ok_or_else(|| conflict("no_active_version", "The plugin has no active version"))?;
        let (pkg, signer) = match self.load_verified(id, &version).await {
            Ok(v) => v,
            Err((state, code, msg)) => {
                self.repo().set_state(id, state, Some(code)).await?;
                self.reload().await?;
                return Err(unprocessable(code, msg));
            }
        };
        let m = &pkg.manifest;
        if m.api_version < PLUGIN_API_MIN_VERSION || m.api_version > PLUGIN_API_VERSION {
            return Err(conflict(
                "api_unsupported",
                "The plugin API version is not supported",
            ));
        }
        if let Some(min) = &m.min_webui_version {
            let cur = semver::Version::parse(env!("CARGO_PKG_VERSION"));
            if matches!((semver::Version::parse(min), cur), (Ok(min), Ok(cur)) if min > cur) {
                return Err(conflict(
                    "webui_too_old",
                    format!("Requires OpenVox WebUI {} or newer", min),
                ));
            }
        }
        match m.runtime.kind {
            RuntimeKind::Builtin => {
                let entry = m.runtime.entry.clone().unwrap_or_default();
                if signer.origin != KeyOrigin::Builtin {
                    return Err(conflict(
                        "builtin_needs_project_key",
                        "Built-in plugins must be signed by an OpenVox project key",
                    ));
                }
                let runtime = self.builtin_runtimes.get(&entry).ok_or_else(|| {
                    conflict(
                        "runtime_unavailable",
                        format!("No built-in plugin '{}' in this build", entry),
                    )
                })?;
                runtime
                    .on_enable()
                    .map_err(|e| unprocessable("enable_failed", e.to_string()))?;
            }
            RuntimeKind::Wasm | RuntimeKind::Remote => {
                return Err(conflict(
                    "runtime_unavailable",
                    format!(
                        "The {} runtime is not available yet",
                        m.runtime.kind.as_str()
                    ),
                ));
            }
        }
        self.repo()
            .set_state(id, PluginState::Enabled, None)
            .await?;
        self.reload().await?;
        self.get(id).await
    }

    pub async fn disable(&self, id: &str) -> Result<PluginDetail, PluginError> {
        let rec = self.require(id).await?;
        self.notify_disable(&rec).await;
        self.repo()
            .set_state(id, PluginState::Disabled, Some("disabled_by_admin"))
            .await?;
        self.reload().await?;
        self.get(id).await
    }

    pub async fn uninstall(&self, id: &str) -> Result<(), PluginError> {
        let rec = self.require(id).await?;
        self.notify_disable(&rec).await;
        self.repo().delete_plugin(id).await?;
        let _ = tokio::fs::remove_dir_all(self.data_dir.join("packages").join(id)).await;
        self.reload().await?;
        Ok(())
    }

    async fn notify_disable(&self, rec: &PluginRecord) {
        if rec.state != PluginState::Enabled {
            return;
        }
        let Some(v) = &rec.active_version else { return };
        if let Ok(Some(vr)) = self.repo().get_version(&rec.id, v).await {
            if let Ok(m) = serde_json::from_str::<PluginManifest>(&vr.manifest_json) {
                if let Some(rt) = m
                    .runtime
                    .entry
                    .as_ref()
                    .and_then(|e| self.builtin_runtimes.get(e))
                {
                    rt.on_disable();
                }
            }
        }
    }

    /// Re-verifies one plugin on demand and records a failure on its state.
    pub async fn verify(&self, id: &str) -> Result<PluginDetail, PluginError> {
        let rec = self.require(id).await?;
        let version = rec
            .active_version
            .ok_or_else(|| conflict("no_active_version", "The plugin has no active version"))?;
        if let Err((state, code, msg)) = self.load_verified(id, &version).await {
            self.repo().set_state(id, state, Some(code)).await?;
            self.reload().await?;
            return Err(unprocessable(code, msg));
        }
        self.get(id).await
    }

    // ---- queries ----------------------------------------------------------

    async fn require(&self, id: &str) -> Result<PluginRecord, PluginError> {
        self.repo()
            .get_plugin(id)
            .await?
            .ok_or_else(|| PluginError::NotFound(format!("Plugin '{}' is not installed", id)))
    }

    pub async fn list(&self) -> Result<Vec<PluginSummary>, PluginError> {
        let keys = self.repo().list_keys().await?;
        let mut out = Vec::new();
        for p in self.repo().list_plugins().await? {
            out.push(self.summarize(p, &keys).await?);
        }
        Ok(out)
    }

    pub async fn get(&self, id: &str) -> Result<PluginDetail, PluginError> {
        let rec = self.require(id).await?;
        let keys = self.repo().list_keys().await?;
        let versions = self.repo().list_versions(id).await?;
        let manifest = versions
            .iter()
            .find(|v| Some(&v.version) == rec.active_version.as_ref())
            .and_then(|v| serde_json::from_str::<PluginManifest>(&v.manifest_json).ok());
        let capabilities = manifest
            .as_ref()
            .map(|m| capabilities(m).iter().map(describe_capability).collect())
            .unwrap_or_default();
        Ok(PluginDetail {
            summary: self.summarize(rec, &keys).await?,
            manifest,
            capabilities,
            versions,
        })
    }

    async fn summarize(
        &self,
        p: PluginRecord,
        keys: &[TrustedKeyRecord],
    ) -> Result<PluginSummary, PluginError> {
        let version = match &p.active_version {
            Some(v) => self.repo().get_version(&p.id, v).await?,
            None => None,
        };
        let manifest = version
            .as_ref()
            .and_then(|v| serde_json::from_str::<PluginManifest>(&v.manifest_json).ok());
        let signer = version
            .as_ref()
            .and_then(|v| keys.iter().find(|k| k.key_id == v.signed_by_key_id));
        Ok(PluginSummary {
            name: manifest
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| p.id.clone()),
            publisher: manifest
                .as_ref()
                .map(|m| m.publisher.clone())
                .unwrap_or_default(),
            runtime_kind: manifest.as_ref().map(|m| m.runtime.kind),
            signer_key_id: version.as_ref().map(|v| v.signed_by_key_id.clone()),
            signer_label: signer.map(|k| k.label.clone()),
            id: p.id,
            active_version: p.active_version,
            state: p.state,
            state_reason: p.state_reason,
            installed_by: p.installed_by,
            installed_at: p.installed_at,
            updated_at: p.updated_at,
        })
    }

    // ---- trusted keys -----------------------------------------------------

    pub async fn list_keys(&self) -> Result<Vec<TrustedKeyView>, PluginError> {
        let mut out = Vec::new();
        for k in self.repo().list_keys().await? {
            out.push(self.key_view(k).await?);
        }
        Ok(out)
    }

    pub async fn get_key(&self, key_id: &str) -> Result<TrustedKeyView, PluginError> {
        let k = self.require_key(key_id).await?;
        self.key_view(k).await
    }

    async fn require_key(&self, key_id: &str) -> Result<TrustedKeyRecord, PluginError> {
        self.repo()
            .get_key(key_id)
            .await?
            .ok_or_else(|| PluginError::NotFound(format!("Trusted key '{}' not found", key_id)))
    }

    async fn key_view(&self, k: TrustedKeyRecord) -> Result<TrustedKeyView, PluginError> {
        let now = Utc::now();
        let expired = k.expires_at.is_some_and(|e| e <= now);
        Ok(TrustedKeyView {
            fingerprint: k.key_id.clone(),
            state: if k.revoked {
                "revoked"
            } else if expired {
                "expired"
            } else {
                "active"
            }
            .to_string(),
            expires_soon: !expired && k.expires_at.is_some_and(|e| e <= now + Duration::days(30)),
            dependent_plugins: self.repo().plugins_signed_by(&k.key_id).await?,
            key_id: k.key_id,
            label: k.label,
            origin: k.origin,
            allowed_plugin_ids: k.allowed_plugin_ids,
            expires_at: k.expires_at,
            added_by: k.added_by,
            added_at: k.added_at,
            revoked_at: k.revoked_at,
            revoked_reason: k.revoked_reason,
        })
    }

    pub async fn add_key(
        &self,
        req: &AddTrustedKeyRequest,
        actor: &str,
    ) -> Result<TrustedKeyView, PluginError> {
        validate_label(&req.label)?;
        let pk = package::parse_public_key(&req.public_key).map_err(PluginError::BadRequest)?;
        validate_scope(req.allowed_plugin_ids.as_deref())?;
        if req.expires_at.is_some_and(|e| e <= Utc::now()) {
            return Err(PluginError::BadRequest(
                "expires_at must be in the future".into(),
            ));
        }
        let key_id = package::key_id_of(&pk);
        if self.repo().get_key(&key_id).await?.is_some() {
            return Err(conflict(
                "key_exists",
                "This key is already in the trust store",
            ));
        }
        self.repo()
            .insert_key(&TrustedKeyRecord {
                key_id: key_id.clone(),
                public_key: pk.to_vec(),
                label: req.label.trim().to_string(),
                origin: KeyOrigin::Admin,
                allowed_plugin_ids: req.allowed_plugin_ids.clone(),
                expires_at: req.expires_at,
                revoked: false,
                added_by: Some(actor.to_string()),
                added_at: Utc::now(),
                revoked_at: None,
                revoked_reason: None,
            })
            .await?;
        self.get_key(&key_id).await
    }

    pub async fn update_key(
        &self,
        key_id: &str,
        req: &UpdateTrustedKeyRequest,
    ) -> Result<TrustedKeyView, PluginError> {
        let mut k = self.require_key(key_id).await?;
        if k.origin == KeyOrigin::Builtin {
            return Err(PluginError::Forbidden(
                "Built-in keys cannot be changed".into(),
            ));
        }
        if let Some(l) = &req.label {
            validate_label(l)?;
            k.label = l.trim().to_string();
        }
        if let Some(e) = req.expires_at {
            k.expires_at = e;
        }
        if let Some(s) = &req.allowed_plugin_ids {
            validate_scope(s.as_deref())?;
            k.allowed_plugin_ids = s.clone();
        }
        self.repo().update_key(&k).await?;
        self.reload().await?;
        self.get_key(key_id).await
    }

    /// Revokes a key and disables every enabled plugin it signed.
    pub async fn revoke_key(
        &self,
        key_id: &str,
        reason: &str,
    ) -> Result<(TrustedKeyView, Vec<String>), PluginError> {
        let k = self.require_key(key_id).await?;
        if k.origin == KeyOrigin::Builtin {
            return Err(PluginError::Forbidden(
                "Built-in keys cannot be revoked".into(),
            ));
        }
        if reason.trim().is_empty() || reason.len() > 500 {
            return Err(PluginError::BadRequest(
                "A reason of up to 500 characters is required".into(),
            ));
        }
        if k.revoked {
            return Err(conflict("already_revoked", "The key is already revoked"));
        }
        let affected = self.repo().plugins_signed_by(key_id).await?;
        self.repo().revoke_key(key_id, reason.trim()).await?;
        self.reload().await?;
        Ok((self.get_key(key_id).await?, affected))
    }

    pub async fn delete_key(&self, key_id: &str) -> Result<(), PluginError> {
        let k = self.require_key(key_id).await?;
        if k.origin == KeyOrigin::Builtin {
            return Err(PluginError::Forbidden(
                "Built-in keys cannot be removed".into(),
            ));
        }
        let dependents = self.repo().plugins_signed_by(key_id).await?;
        if !dependents.is_empty() {
            return Err(conflict(
                "key_in_use",
                format!("Plugins depend on this key: {}", dependents.join(", ")),
            ));
        }
        self.repo().delete_key(key_id).await?;
        Ok(())
    }
}

/// Decides whether `key_id` may vouch for plugin `plugin_id`. A key that is
/// not in the store is merely untrusted; everything else is a rejection.
pub fn evaluate_trust(
    plugin_id: &str,
    key_id: &str,
    public_key: &[u8; 32],
    keys: &[TrustedKeyRecord],
) -> (SignatureStatus, Option<TrustedKeyRecord>, Option<String>) {
    let Some(k) = keys.iter().find(|k| k.key_id == key_id) else {
        return (
            SignatureStatus::UntrustedKey,
            None,
            Some("The signing key is not in the trusted key store".into()),
        );
    };
    let status = if k.public_key.as_slice() != public_key.as_slice() {
        (
            SignatureStatus::Invalid,
            "The key material does not match the trusted key",
        )
    } else if k.revoked {
        (SignatureStatus::KeyRevoked, "The signing key was revoked")
    } else if k.expires_at.is_some_and(|e| e <= Utc::now()) {
        (SignatureStatus::KeyExpired, "The signing key has expired")
    } else if k
        .allowed_plugin_ids
        .as_ref()
        .is_some_and(|ids| !ids.iter().any(|i| i == plugin_id))
    {
        (
            SignatureStatus::KeyScope,
            "The signing key may not sign this plugin id",
        )
    } else {
        return (SignatureStatus::Valid, Some(k.clone()), None);
    };
    (status.0, Some(k.clone()), Some(status.1.to_string()))
}

fn validate_label(label: &str) -> Result<(), PluginError> {
    let l = label.trim();
    if l.is_empty()
        || l.chars().count() > 80
        || l.chars().any(|c| c.is_control() || c == '<' || c == '>')
    {
        return Err(PluginError::BadRequest(
            "label must be 1 to 80 printable characters".into(),
        ));
    }
    Ok(())
}

fn validate_scope(ids: Option<&[String]>) -> Result<(), PluginError> {
    if let Some(ids) = ids {
        if ids.is_empty() || ids.iter().any(|i| !super::manifest::valid_plugin_id(i)) {
            return Err(PluginError::BadRequest(
                "allowed_plugin_ids must be valid plugin ids".into(),
            ));
        }
    }
    Ok(())
}

/// Re-verifies the enabled plugins periodically, so tampering with a stored
/// package and key expiry are noticed without a restart.
pub fn start_integrity_monitor(manager: Arc<PluginManager>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
        tick.tick().await;
        loop {
            tick.tick().await;
            match manager.reload().await {
                Ok(failed) if failed.is_empty() => {}
                Ok(failed) => tracing::error!("plugin integrity check failed for: {:?}", failed),
                Err(e) => tracing::warn!("plugin integrity check could not run: {}", e),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::plugins::package::build_signed_package;
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;

    const SEED: [u8; 32] = [3; 32];
    const OTHER: [u8; 32] = [4; 32];

    fn yaml(version: &str, extra: &str) -> String {
        format!(
            "api_version: 2\nid: demo\nname: Demo\nversion: {version}\npublisher: ACME\n\
             runtime: {{ kind: builtin, entry: demo }}\ncapabilities:\n  - ui:slot:page\n  - ui:slot:nav.main\n{extra}\
             contributes:\n  slots:\n    - {{ slot: page, id: home, render: declarative, spec: ui/home.json, path: /p/demo/home }}\n  menus:\n    - {{ id: m, section: main, label: Demo, icon: Puzzle, href: /p/demo/home }}\n"
        )
    }

    fn package(version: &str, seed: [u8; 32]) -> Vec<u8> {
        build_signed_package(
            &[
                ("plugin.yaml", yaml(version, "").as_bytes()),
                (
                    "ui/home.json",
                    br#"{"layout":"markdown","content":"hello"}"#,
                ),
            ],
            "demo",
            version,
            2,
            seed,
        )
    }

    fn remote_package(seed: [u8; 32]) -> Vec<u8> {
        let y = "api_version: 2\nid: demo\nname: Demo\nversion: 1.0.0\npublisher: ACME\n\
                 runtime: { kind: remote, endpoint: \"https://demo.example.com\" }\n";
        build_signed_package(&[("plugin.yaml", y.as_bytes())], "demo", "1.0.0", 2, seed)
    }

    struct Noop;
    impl BuiltinPlugin for Noop {}

    async fn manager(builtin_seed: Option<[u8; 32]>) -> (PluginManager, tempfile_dir::Dir) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = tempfile_dir::Dir::new();
        let mut m = PluginManager::new(pool, dir.path.clone(), 8)
            .with_builtin_runtime("demo", Arc::new(Noop));
        if let Some(s) = builtin_seed {
            let pk = SigningKey::from_bytes(&s).verifying_key().to_bytes();
            m = m.with_builtin_keys(vec![("OpenVox".into(), pk)]);
        }
        m.reload().await.unwrap();
        (m, dir)
    }

    fn trust(prefix: &str) -> Option<TrustRequest> {
        Some(TrustRequest {
            confirm_prefix: prefix.to_string(),
            label: Some("dev".into()),
        })
    }

    fn key_id(seed: [u8; 32]) -> String {
        package::key_id_of(&SigningKey::from_bytes(&seed).verifying_key().to_bytes())
    }

    #[tokio::test]
    async fn unknown_signer_is_refused_until_trusted_with_the_fingerprint_prefix() {
        let (m, _d) = manager(None).await;
        let pkg = remote_package(OTHER);
        let pre = m.preflight(&pkg).await.unwrap();
        assert_eq!(pre.signature.status, SignatureStatus::UntrustedKey);

        let err = m.install(&pkg, "admin", None).await.err().unwrap();
        assert!(matches!(
            err,
            PluginError::Unprocessable {
                code: "untrusted_signer",
                ..
            }
        ));
        let err = m
            .install(&pkg, "admin", trust("deadbeef"))
            .await
            .err()
            .unwrap();
        assert!(matches!(
            err,
            PluginError::Unprocessable {
                code: "untrusted_signer",
                ..
            }
        ));
        assert!(m.list().await.unwrap().is_empty());

        let prefix = key_id(OTHER)[..8].to_string();
        let out = m.install(&pkg, "admin", trust(&prefix)).await.unwrap();
        assert_eq!(out.trusted_key_id, Some(key_id(OTHER)));
        assert_eq!(m.list_keys().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn builtin_runtime_needs_a_project_key() {
        let (m, _d) = manager(Some(SEED)).await;
        // Signed by an admin-trustable key: blocked even with a confirmation.
        let pkg = package("1.0.0", OTHER);
        let prefix = key_id(OTHER)[..8].to_string();
        assert!(m.install(&pkg, "a", trust(&prefix)).await.is_err());
        // Signed by the embedded project key: installs and enables.
        let ok = package("1.0.0", SEED);
        m.install(&ok, "a", None).await.unwrap();
        let d = m.enable("demo").await.unwrap();
        assert_eq!(d.summary.state, PluginState::Enabled);
        let ui = m.registry().to_ui(&|_| true);
        assert_eq!(ui.slots["page"].len(), 1);
        assert_eq!(ui.slots["nav.main"][0].label.as_deref(), Some("Demo"));
    }

    #[tokio::test]
    async fn revoking_the_signer_disables_the_plugin_and_empties_the_registry() {
        let (m, _d) = manager(Some(SEED)).await;
        let pkg = package("1.0.0", SEED);
        m.install(&pkg, "a", None).await.unwrap();
        m.enable("demo").await.unwrap();

        // Builtin keys cannot be revoked; use an admin copy of a second key.
        assert!(matches!(
            m.revoke_key(&key_id(SEED), "nope").await,
            Err(PluginError::Forbidden(_))
        ));
        let rec = m.repo().get_key(&key_id(SEED)).await.unwrap().unwrap();
        m.repo().revoke_key(&rec.key_id, "test").await.unwrap();
        let failed = m.reload().await.unwrap();
        assert_eq!(
            failed,
            vec![("demo".to_string(), "signer_revoked".to_string())]
        );
        assert_eq!(
            m.get("demo").await.unwrap().summary.state,
            PluginState::Disabled
        );
        assert!(m.registry().is_empty());
        assert!(m.enable("demo").await.is_err());
    }

    #[tokio::test]
    async fn tampering_with_the_stored_package_is_detected() {
        let (m, d) = manager(Some(SEED)).await;
        m.install(&package("1.0.0", SEED), "a", None).await.unwrap();
        m.enable("demo").await.unwrap();
        let path = d.path.join("packages/demo/1.0.0.ovxp");
        let mut bytes = std::fs::read(&path).unwrap();
        let n = bytes.len() / 2;
        bytes[n] ^= 0xFF;
        std::fs::write(&path, bytes).unwrap();
        let failed = m.reload().await.unwrap();
        assert_eq!(failed[0].1, "integrity_failed");
        assert_eq!(
            m.get("demo").await.unwrap().summary.state,
            PluginState::Error
        );
        assert!(m.registry().is_empty());
    }

    #[tokio::test]
    async fn key_scope_expiry_and_dependents_are_enforced() {
        let (m, _d) = manager(None).await;
        let pk = SigningKey::from_bytes(&OTHER).verifying_key().to_bytes();
        let add = |scope: Option<Vec<String>>| AddTrustedKeyRequest {
            public_key: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pk),
            label: "dev".into(),
            expires_at: None,
            allowed_plugin_ids: scope,
            password: String::new(),
        };
        m.add_key(&add(Some(vec!["other-plugin".into()])), "a")
            .await
            .unwrap();
        let pre = m.preflight(&remote_package(OTHER)).await.unwrap();
        assert_eq!(pre.signature.status, SignatureStatus::KeyScope);
        assert!(!pre.can_install);

        m.update_key(
            &key_id(OTHER),
            &UpdateTrustedKeyRequest {
                label: None,
                expires_at: None,
                allowed_plugin_ids: Some(None),
            },
        )
        .await
        .unwrap();
        let pre = m.preflight(&remote_package(OTHER)).await.unwrap();
        assert_eq!(pre.signature.status, SignatureStatus::Valid);
        // Builtin runtime is refused for an admin key, so use a wasm-less check
        // of expiry instead.
        m.update_key(
            &key_id(OTHER),
            &UpdateTrustedKeyRequest {
                label: None,
                expires_at: Some(Some(Utc::now() - Duration::days(1))),
                allowed_plugin_ids: None,
            },
        )
        .await
        .unwrap();
        let pre = m.preflight(&remote_package(OTHER)).await.unwrap();
        assert_eq!(pre.signature.status, SignatureStatus::KeyExpired);
        assert!(!pre.can_install);
    }

    #[tokio::test]
    async fn upgrades_must_be_newer_and_cannot_add_capabilities() {
        let (m, _d) = manager(Some(SEED)).await;
        m.install(&package("1.0.0", SEED), "a", None).await.unwrap();
        assert!(m.install(&package("1.0.0", SEED), "a", None).await.is_err());
        let wider = build_signed_package(
            &[
                ("plugin.yaml", yaml("1.1.0", "  - kv:rw\n").as_bytes()),
                (
                    "ui/home.json",
                    br#"{"layout":"markdown","content":"hello"}"#,
                ),
            ],
            "demo",
            "1.1.0",
            2,
            SEED,
        );
        let err = m.install(&wider, "a", None).await.err().unwrap();
        assert!(err.to_string().contains("kv:rw"));
        let out = m.install(&package("1.2.0", SEED), "a", None).await.unwrap();
        assert_eq!(out.upgraded_from.as_deref(), Some("1.0.0"));
        assert_eq!(out.detail.summary.active_version.as_deref(), Some("1.2.0"));
    }

    #[tokio::test]
    async fn uninstall_removes_rows_and_files() {
        let (m, d) = manager(Some(SEED)).await;
        m.install(&package("1.0.0", SEED), "a", None).await.unwrap();
        m.enable("demo").await.unwrap();
        m.uninstall("demo").await.unwrap();
        assert!(m.list().await.unwrap().is_empty());
        assert!(!d.path.join("packages/demo").exists());
        assert!(m.registry().is_empty());
        // The key now has no dependents and the builtin one still cannot go.
        assert!(matches!(
            m.delete_key(&key_id(SEED)).await,
            Err(PluginError::Forbidden(_))
        ));
    }

    mod tempfile_dir {
        use std::path::PathBuf;
        pub struct Dir {
            pub path: PathBuf,
        }
        impl Dir {
            pub fn new() -> Self {
                let path = std::env::temp_dir()
                    .join(format!("ovx_plugins_{}", uuid::Uuid::new_v4().simple()));
                std::fs::create_dir_all(&path).unwrap();
                Self { path }
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }
}
