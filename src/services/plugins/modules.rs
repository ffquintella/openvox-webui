//! Trusted module pages (M31, T105): `module` contributions run in the host
//! page, in its origin and JavaScript context, with no sandbox. They are
//! served only while `plugins.allow_trusted_modules` is on, for the exact
//! plugin version an administrator approved. See PLUGIN_PLATFORM.md §17.13.

use std::collections::{BTreeMap, HashMap};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha384, Sha512};

use super::events::EventBus;
use super::manager::{PluginError, PluginManager, AUDIT_TRUST};
use super::manifest::capabilities;
use crate::models::*;

/// Audit action and event of an approval (`plugin_trust` audit entries).
pub const MODULE_TRUSTED: &str = "plugin.trust.module_trusted";
/// Audit action and event of a withdrawal, by an administrator or the host.
pub const MODULE_UNTRUSTED: &str = "plugin.trust.module_untrusted";

/// Capabilities of authentication plugins (§11.9), which can never run as
/// trusted modules in the browser.
pub const AUTH_CAPABILITIES: &[&str] = &[
    "auth:provider",
    "auth:factor",
    "auth:credentials",
    "users:provider",
];

/// Module rendered when a contribution does not name one.
pub const DEFAULT_EXPOSE: &str = "./Page";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SriAlgorithm {
    Sha384,
    Sha512,
}

impl SriAlgorithm {
    fn name(self) -> &'static str {
        match self {
            SriAlgorithm::Sha384 => "sha384",
            SriAlgorithm::Sha512 => "sha512",
        }
    }

    fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            SriAlgorithm::Sha384 => Sha384::digest(data).to_vec(),
            SriAlgorithm::Sha512 => Sha512::digest(data).to_vec(),
        }
    }

    fn len(self) -> usize {
        match self {
            SriAlgorithm::Sha384 => 48,
            SriAlgorithm::Sha512 => 64,
        }
    }
}

/// Parses one Subresource Integrity value, `sha384-<base64>` or
/// `sha512-<base64>`, without options or alternatives.
pub fn parse_sri(s: &str) -> Result<(SriAlgorithm, Vec<u8>), String> {
    let (alg, b64) = s
        .split_once('-')
        .ok_or_else(|| format!("integrity '{s}' must look like sha384-<base64>"))?;
    let alg = match alg {
        "sha384" => SriAlgorithm::Sha384,
        "sha512" => SriAlgorithm::Sha512,
        _ => {
            return Err(format!(
                "integrity '{s}': only sha384 and sha512 are accepted"
            ))
        }
    };
    let digest = B64
        .decode(b64)
        .map_err(|_| format!("integrity '{s}' is not valid base64"))?;
    if digest.len() != alg.len() {
        return Err(format!(
            "integrity '{s}' has the wrong length for {}",
            alg.name()
        ));
    }
    Ok((alg, digest))
}

/// The SRI of `data` with the given algorithm.
pub fn sri_of(alg: SriAlgorithm, data: &[u8]) -> String {
    format!("{}-{}", alg.name(), B64.encode(alg.digest(data)))
}

/// True when `declared` is a valid SRI value that matches `data`.
pub fn sri_matches(declared: &str, data: &[u8]) -> bool {
    parse_sri(declared).is_ok_and(|(alg, digest)| alg.digest(data) == digest)
}

/// A module entry must be an ES module file.
pub fn is_module_file(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, e)| e.eq_ignore_ascii_case("js") || e.eq_ignore_ascii_case("mjs"))
}

/// `./Name` or `./dir/Name`: what a Module Federation container exposes.
pub fn valid_expose(e: &str) -> bool {
    let Some(rest) = e.strip_prefix("./") else {
        return false;
    };
    !rest.is_empty()
        && e.len() <= 64
        && rest
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
}

/// An authentication or user-provider plugin (§11.9).
pub fn is_auth_plugin(m: &PluginManifest) -> bool {
    capabilities(m)
        .iter()
        .any(|c| AUTH_CAPABILITIES.contains(&c.name.as_str()))
}

/// The `module` contributions of a manifest that name an entry and an SRI.
pub fn module_entries(m: &PluginManifest) -> Vec<ModuleEntryInfo> {
    m.contributes
        .slots
        .iter()
        .filter(|s| s.render == RenderKind::Module)
        .filter_map(|s| {
            Some(ModuleEntryInfo {
                id: s.id.clone(),
                slot: s.slot.clone(),
                src: s.src.clone()?,
                integrity: s.integrity.clone()?,
            })
        })
        .collect()
}

/// Every `module` contribution's integrity must match its entry file. Run
/// whenever a package is read, so at install and at every load.
pub fn check_integrity(m: &PluginManifest, files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut problems = Vec::new();
    for e in module_entries(m) {
        match files.get(&e.src) {
            Some(bytes) if sri_matches(&e.integrity, bytes) => {}
            Some(_) => problems.push(format!(
                "contribution '{}': the integrity does not match {}",
                e.id, e.src
            )),
            None => problems.push(format!(
                "contribution '{}': {} is not in the package",
                e.id, e.src
            )),
        }
    }
    problems
}

fn conflict(code: &'static str, message: impl Into<String>) -> PluginError {
    PluginError::Conflict {
        code,
        message: message.into(),
    }
}

fn unprocessable(code: &'static str, message: impl Into<String>) -> PluginError {
    PluginError::Unprocessable {
        code,
        message: message.into(),
        details: None,
    }
}

const AUTH_REFUSAL: &str =
    "Authentication and user-provider plugins can never run as trusted modules";

impl PluginManager {
    /// Marks the active version of a plugin as trusted to run its `module`
    /// contributions in the host page. The caller checks `plugins:trust`
    /// and re-authentication, and audits.
    pub async fn trust_modules(
        &self,
        id: &str,
        version: Option<&str>,
        actor: &str,
    ) -> Result<ModuleTrustRecord, PluginError> {
        if !self.allows_trusted_modules() {
            return Err(conflict(
                "trusted_modules_disabled",
                "Trusted module pages are turned off (plugins.allow_trusted_modules is false)",
            ));
        }
        let rec = self.require(id).await?;
        let active = rec
            .active_version
            .clone()
            .ok_or_else(|| conflict("no_active_version", "The plugin has no active version"))?;
        if version.is_some_and(|v| v != active) {
            return Err(conflict(
                "version_not_active",
                format!("Only the active version ({active}) can be trusted"),
            ));
        }
        let pkg = match self.load_verified(id, &active).await {
            Ok((pkg, _)) => pkg,
            Err((_, code, msg)) => return Err(unprocessable(code, msg)),
        };
        if is_auth_plugin(&pkg.manifest) {
            return Err(unprocessable("auth_plugin_not_trustable", AUTH_REFUSAL));
        }
        let modules = module_entries(&pkg.manifest);
        if modules.is_empty() {
            return Err(unprocessable(
                "no_module_contributions",
                format!("{id} {active} has no module contributions"),
            ));
        }
        let problems = check_integrity(&pkg.manifest, &pkg.files);
        if !problems.is_empty() {
            return Err(PluginError::Unprocessable {
                code: "module_integrity_mismatch",
                message: problems.join("; "),
                details: Some(json!(problems)),
            });
        }
        let record = ModuleTrustRecord {
            plugin_id: id.to_string(),
            version: active,
            package_sha256: pkg.package_sha256.clone(),
            modules,
            trusted_by: actor.to_string(),
            trusted_at: Utc::now(),
        };
        self.repo().put_module_trust(&record).await?;
        self.reload().await?;
        Ok(record)
    }

    /// Withdraws the approval; the modules stop being served at once.
    pub async fn untrust_modules(&self, id: &str) -> Result<ModuleTrustRecord, PluginError> {
        self.require(id).await?;
        let removed =
            self.repo().delete_module_trust(id).await?.ok_or_else(|| {
                PluginError::NotFound(format!("{id} has no trusted module version"))
            })?;
        self.reload().await?;
        Ok(removed)
    }

    /// Drops an approval that does not cover `keep` (another version became
    /// active), so new code always needs a new approval. Audited.
    pub(super) async fn drop_module_trust_unless(&self, id: &str, keep: &str, reason: &str) {
        if let Ok(Some(t)) = self.repo().get_module_trust(id).await {
            if t.version != keep {
                self.drop_module_trust(id, &t.version, reason).await;
            }
        }
    }

    /// Drops the approval of `version` (revoked, for example). Audited.
    pub(super) async fn drop_module_trust(&self, id: &str, version: &str, reason: &str) {
        match self.repo().get_module_trust(id).await {
            Ok(Some(t)) if t.version == version => {}
            _ => return,
        }
        if let Err(e) = self.repo().delete_module_trust(id).await {
            tracing::warn!("could not remove the module trust of {}: {}", id, e);
            return;
        }
        self.audit(
            default_organization_uuid(),
            None,
            AUDIT_TRUST,
            MODULE_UNTRUSTED,
            id,
            json!({"version": version, "reason": reason, "actor": "system"}),
        )
        .await;
        EventBus::new(self.pool().clone())
            .emit(
                MODULE_UNTRUSTED,
                json!({"plugin_id": id, "version": version, "reason": reason}),
                EventActor::system(),
            )
            .await;
    }

    /// Approvals by plugin id, to build the registry.
    pub(super) async fn module_trust_index(&self) -> HashMap<String, ModuleTrustRecord> {
        match self.repo().list_module_trust().await {
            Ok(list) => list.into_iter().map(|t| (t.plugin_id.clone(), t)).collect(),
            Err(e) => {
                tracing::warn!("could not read the module trust: {}", e);
                HashMap::new()
            }
        }
    }

    /// The trusted module state of a plugin for the admin views.
    pub(super) async fn module_trust_view(
        &self,
        rec: &PluginRecord,
        manifest: Option<&PluginManifest>,
    ) -> Option<ModuleTrustView> {
        let contributions = manifest.map(module_entries).unwrap_or_default();
        let trusted = self
            .repo()
            .get_module_trust(&rec.id)
            .await
            .ok()
            .flatten()
            .filter(|t| rec.active_version.as_deref() == Some(t.version.as_str()));
        if contributions.is_empty() && trusted.is_none() {
            return None;
        }
        let refusal = manifest
            .filter(|m| is_auth_plugin(m))
            .map(|_| AUTH_REFUSAL.to_string());
        Some(ModuleTrustView {
            allowed_by_config: self.allows_trusted_modules(),
            served: self.registry().has_modules(&rec.id),
            contributions,
            trusted,
            refusal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sri_values_are_parsed_strictly() {
        let data = b"export const x = 1;";
        let s384 = sri_of(SriAlgorithm::Sha384, data);
        let s512 = sri_of(SriAlgorithm::Sha512, data);
        assert!(s384.starts_with("sha384-") && s512.starts_with("sha512-"));
        assert!(sri_matches(&s384, data) && sri_matches(&s512, data));
        assert!(!sri_matches(&s384, b"export const x = 2;"));
        let sha256 = format!("sha256-{}", B64.encode(sha2::Sha256::digest(data)));
        for bad in [
            sha256.as_str(),
            "sha384",
            "sha384-",
            "sha384-not base64!",
            "sha384-AAAA",
            "md5-AAAA",
        ] {
            assert!(parse_sri(bad).is_err(), "{bad}");
            assert!(!sri_matches(bad, data), "{bad}");
        }
        // Options and several hashes are not accepted either.
        assert!(parse_sri(&format!("{s384}?opt")).is_err());
        assert!(parse_sri(&format!("{s384} {s512}")).is_err());
    }

    #[test]
    fn module_files_and_exposed_names() {
        assert!(is_module_file("ui/remoteEntry.js") && is_module_file("ui/a.MJS"));
        assert!(!is_module_file("ui/app.html") && !is_module_file("ui/js"));
        for ok in ["./Page", "./pages/Console", "./a-b_c.d"] {
            assert!(valid_expose(ok), "{ok}");
        }
        for bad in [
            "Page", "./", "./../x", ".//x", "./a b", "../Page", "./x/./y",
        ] {
            assert!(!valid_expose(bad), "{bad}");
        }
    }

    #[test]
    fn integrity_is_checked_against_the_package_files() {
        let yaml = |sri: &str| {
            format!(
                "api_version: 2\nid: demo\nname: Demo\nversion: 1.0.0\npublisher: ACME\n\
                 runtime: {{ kind: builtin, entry: demo }}\ncapabilities: [ui:slot:page]\n\
                 contributes:\n  slots:\n    - {{ slot: page, id: app, render: module, \
                 src: ui/remoteEntry.js, integrity: \"{sri}\", path: /p/demo/app }}\n"
            )
        };
        let js = b"export function get() {}".to_vec();
        let mut files = BTreeMap::new();
        files.insert("ui/remoteEntry.js".to_string(), js.clone());
        let good: PluginManifest =
            serde_norway::from_str(&yaml(&sri_of(SriAlgorithm::Sha384, &js))).unwrap();
        assert!(check_integrity(&good, &files).is_empty());
        assert_eq!(module_entries(&good)[0].src, "ui/remoteEntry.js");
        let bad: PluginManifest =
            serde_norway::from_str(&yaml(&sri_of(SriAlgorithm::Sha384, b"other"))).unwrap();
        assert!(check_integrity(&bad, &files)[0].contains("does not match"));
        files.clear();
        assert!(check_integrity(&good, &files)[0].contains("not in the package"));
    }

    // ---- trust flow through the manager -----------------------------------

    use crate::services::plugins::package::build_signed_package;
    use crate::services::plugins::{BuiltinPlugin, UpdateActor};
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::Arc;

    const SEED: [u8; 32] = [7; 32];
    const JS: &[u8] = b"export async function get() { return () => ({ default: () => null }); }";

    struct Noop;
    impl BuiltinPlugin for Noop {}

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    async fn manager(allow: bool) -> (PluginManager, TempDir) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ovx_mod_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = reopen(pool, &dir, allow).await;
        (m, TempDir(dir))
    }

    async fn reopen(pool: sqlx::SqlitePool, dir: &std::path::Path, allow: bool) -> PluginManager {
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let m = PluginManager::new(pool, dir.to_path_buf(), 8)
            .with_builtin_keys(vec![("OpenVox".into(), pk)])
            .with_builtin_runtime("demo", Arc::new(Noop))
            .with_trusted_modules(allow);
        m.reload().await.unwrap();
        m
    }

    fn package(version: &str, js: &[u8], extra_caps: &str) -> Vec<u8> {
        let sri = sri_of(SriAlgorithm::Sha384, JS);
        let y = format!(
            "api_version: 2\nid: demo\nname: Demo\nversion: {version}\npublisher: ACME\n\
             runtime: {{ kind: builtin, entry: demo }}\ncapabilities: [ui:slot:page{extra_caps}]\n\
             contributes:\n  slots:\n    - {{ slot: page, id: console, render: module, \
             src: ui/remoteEntry.js, integrity: \"{sri}\", path: /p/demo/console }}\n"
        );
        build_signed_package(
            &[("plugin.yaml", y.as_bytes()), ("ui/remoteEntry.js", js)],
            "demo",
            version,
            2,
            SEED,
        )
    }

    fn served(m: &PluginManager) -> bool {
        m.registry()
            .module_asset("demo", "1.0.0", "console")
            .is_some()
    }

    fn code(e: PluginError) -> &'static str {
        match e {
            PluginError::Conflict { code, .. } | PluginError::Unprocessable { code, .. } => code,
            PluginError::NotFound(_) => "not_found",
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_mismatched_integrity_is_refused_at_install() {
        let (m, _d) = manager(true).await;
        let err = m
            .install(&package("1.0.0", b"tampered", ""), "a", None)
            .await
            .err()
            .unwrap();
        assert_eq!(code(err), "invalid_manifest");
    }

    #[tokio::test]
    async fn trusting_needs_the_flag_and_never_covers_auth_plugins() {
        let (m, _d) = manager(false).await;
        m.install(&package("1.0.0", JS, ""), "a", None)
            .await
            .unwrap();
        m.enable("demo").await.unwrap();
        let err = m.trust_modules("demo", None, "a").await.err().unwrap();
        assert_eq!(code(err), "trusted_modules_disabled");
        assert!(!served(&m));
        let view = m.get("demo").await.unwrap().module_trust.unwrap();
        assert!(!view.allowed_by_config && view.trusted.is_none() && !view.served);
        assert_eq!(view.contributions[0].id, "console");

        // An authentication plugin cannot even declare a module, and
        // trusting it is refused whatever it contributes.
        let (m, _d) = manager(true).await;
        let auth = package("1.0.0", JS, ", auth:factor");
        match m.install(&auth, "a", None).await.err().unwrap() {
            PluginError::Unprocessable { code, details, .. } => {
                assert_eq!(code, "invalid_manifest");
                assert!(details.unwrap().to_string().contains("authentication"));
            }
            other => panic!("unexpected {other:?}"),
        }
        let y = "api_version: 2\nid: demo\nname: Demo\nversion: 1.0.0\npublisher: ACME\n\
                 runtime: { kind: builtin, entry: demo }\ncapabilities: [users:provider]\n";
        let pkg = build_signed_package(&[("plugin.yaml", y.as_bytes())], "demo", "1.0.0", 2, SEED);
        m.install(&pkg, "a", None).await.unwrap();
        let err = m.trust_modules("demo", None, "a").await.err().unwrap();
        assert_eq!(code(err), "auth_plugin_not_trustable");
    }

    #[tokio::test]
    async fn a_trusted_module_is_served_only_for_the_approved_version() {
        let (m, _d) = manager(true).await;
        m.install(&package("1.0.0", JS, ""), "a", None)
            .await
            .unwrap();
        m.enable("demo").await.unwrap();
        // Installed and enabled, but not trusted: not served, not listed.
        assert!(!served(&m));
        assert!(!m.registry().to_ui(&|_| true).slots.contains_key("page"));
        assert_eq!(
            code(
                m.trust_modules("demo", Some("0.9.0"), "a")
                    .await
                    .err()
                    .unwrap()
            ),
            "version_not_active"
        );

        let t = m.trust_modules("demo", None, "alice").await.unwrap();
        assert_eq!(
            (t.version.as_str(), t.trusted_by.as_str()),
            ("1.0.0", "alice")
        );
        assert!(served(&m));
        let ui = m.registry().to_ui(&|_| true);
        let c = &ui.slots["page"][0];
        assert_eq!(c.render, RenderKind::Module);
        assert_eq!(
            c.integrity.as_deref(),
            Some(sri_of(SriAlgorithm::Sha384, JS).as_str())
        );
        assert_eq!(c.expose.as_deref(), Some(DEFAULT_EXPOSE));
        let asset = m
            .registry()
            .find_module("demo", "console", &|_| true)
            .unwrap();
        assert_eq!(asset.bytes.as_slice(), JS);
        assert!(m
            .registry()
            .find_module("demo", "console", &|_| false)
            .is_none());
        assert!(m
            .registry()
            .module_asset("demo", "0.9.0", "console")
            .is_none());
        assert!(m.get("demo").await.unwrap().module_trust.unwrap().served);

        // Disabling takes it out of play; the approval survives for the same code.
        m.disable("demo").await.unwrap();
        assert!(!served(&m));
        m.enable("demo").await.unwrap();
        assert!(served(&m));

        // The same database with the flag off serves nothing.
        let off = reopen(m.pool().clone(), &_d.0, false).await;
        assert!(!off.registry().has_modules("demo"));
        assert!(off.registry().to_ui(&|_| true).slots.is_empty());

        // A new version needs a new approval.
        m.install(&package("1.1.0", JS, ""), "a", None)
            .await
            .unwrap();
        assert!(!m.registry().has_modules("demo"));
        let view = m.get("demo").await.unwrap().module_trust.unwrap();
        assert!(view.trusted.is_none() && !view.served);
        let audited: Vec<String> = sqlx::query_scalar(
            "SELECT details FROM audit_log WHERE action = 'plugin.trust.module_untrusted'",
        )
        .fetch_all(m.pool())
        .await
        .unwrap();
        assert!(audited[0].contains("version_changed"), "{audited:?}");

        // Withdrawing works once.
        m.trust_modules("demo", None, "alice").await.unwrap();
        assert!(m.registry().has_modules("demo"));
        m.untrust_modules("demo").await.unwrap();
        assert!(!m.registry().has_modules("demo"));
        assert_eq!(
            code(m.untrust_modules("demo").await.err().unwrap()),
            "not_found"
        );

        // Revoking the trusted version drops the approval for good.
        m.trust_modules("demo", None, "alice").await.unwrap();
        m.revoke_version("demo", "1.1.0", "bad build", &UpdateActor::scheduler())
            .await
            .unwrap();
        assert!(!m.registry().has_modules("demo"));
        assert!(m.repo().get_module_trust("demo").await.unwrap().is_none());

        // Uninstall leaves no approval behind.
        m.uninstall("demo", false).await.unwrap();
        assert!(m.repo().list_module_trust().await.unwrap().is_empty());
    }
}
