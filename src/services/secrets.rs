//! Secret resolver with the built-in `env` and `file` providers (M28).
//!
//! A configuration value may be `secret://<provider>/<path>[#<key>]` instead
//! of the credential. A resolved [`Secret`] prints as `[REDACTED]`, is wiped on
//! drop and cannot be serialized. Resolution fails closed with a [`SecretError`]
//! that names the provider and path, never a value.

use std::collections::HashMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use zeroize::Zeroizing;

pub const SECRET_SCHEME: &str = "secret://";

/// `resource_type` of the audit entries for secret access.
pub const AUDIT_SECRETS: &str = "secrets";

/// Largest value a built-in provider returns.
const MAX_VALUE_BYTES: u64 = 64 * 1024;

/// A resolved credential: Debug and Display print `[REDACTED]`, the memory is
/// zeroed on drop, and there is deliberately no `Serialize`.
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: String) -> Self {
        Secret(Zeroizing::new(value))
    }

    /// The only way to read the value. Call it at the point of use.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// A parsed `secret://` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    pub provider: String,
    pub path: String,
    /// Field of a JSON object value (`#key`).
    pub key: Option<String>,
    /// Provider specific version (`?version=<n>`).
    pub version: Option<u32>,
}

impl SecretRef {
    /// Whether a configuration string is a reference.
    pub fn is_reference(s: &str) -> bool {
        s.starts_with(SECRET_SCHEME)
    }

    pub fn parse(s: &str) -> Result<Self, SecretError> {
        let rest = s
            .strip_prefix(SECRET_SCHEME)
            .ok_or_else(|| SecretError::Invalid("a reference starts with secret://".into()))?;
        let (rest, query) = match rest.split_once('?') {
            Some((r, q)) => (r, Some(q)),
            None => (rest, None),
        };
        let (rest, key) = match rest.split_once('#') {
            Some((r, k)) if !k.is_empty() => (r, Some(k.to_string())),
            Some(_) => return Err(SecretError::Invalid("empty #key".into())),
            None => (rest, None),
        };
        let (provider, path) = rest
            .split_once('/')
            .ok_or_else(|| SecretError::Invalid("expected secret://<provider>/<path>".into()))?;
        if provider.is_empty()
            || !provider
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(SecretError::Invalid("invalid provider id".into()));
        }
        if path.is_empty() {
            return Err(SecretError::Invalid("empty path".into()));
        }
        let mut version = None;
        if let Some(q) = query {
            for pair in q.split('&') {
                match pair.split_once('=') {
                    Some(("version", v)) => {
                        version = Some(
                            v.parse::<u32>()
                                .map_err(|_| SecretError::Invalid("invalid version".into()))?,
                        );
                    }
                    _ => {
                        return Err(SecretError::Invalid(
                            "only ?version=<n> is supported".into(),
                        ))
                    }
                }
            }
        }
        Ok(SecretRef {
            provider: provider.to_string(),
            path: path.to_string(),
            key,
            version,
        })
    }

    /// `provider/path`, for audit and error messages (no key, no value).
    pub fn display_path(&self) -> String {
        format!("{}/{}", self.provider, self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("invalid secret reference: {0}")]
    Invalid(String),
    #[error("secret not found")]
    NotFound,
    #[error("secret access denied: {0}")]
    Denied(String),
    #[error("secret provider unavailable: {0}")]
    Unavailable(String),
    #[error("secret provider timed out")]
    Timeout,
}

impl SecretError {
    /// Stable code for audit details and API answers.
    pub fn code(&self) -> &'static str {
        match self {
            SecretError::Invalid(_) => "invalid",
            SecretError::NotFound => "not_found",
            SecretError::Denied(_) => "denied",
            SecretError::Unavailable(_) => "unavailable",
            SecretError::Timeout => "timeout",
        }
    }
}

/// Who asks for the secret. Only the host resolves in M28; a plugin or a user
/// is denied until a grant model exists for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requester {
    Core,
    Plugin(String),
    User(String),
}

impl Requester {
    pub fn label(&self) -> String {
        match self {
            Requester::Core => "core".into(),
            Requester::Plugin(id) => format!("plugin:{id}"),
            Requester::User(id) => format!("user:{id}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecretContext {
    pub requester: Requester,
    /// What the value is for (`config:auth.jwt_secret`), kept in the audit.
    pub purpose: String,
}

impl SecretContext {
    pub fn core(purpose: impl Into<String>) -> Self {
        SecretContext {
            requester: Requester::Core,
            purpose: purpose.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealth {
    pub healthy: bool,
    pub detail: Option<String>,
}

/// A source of secrets. Built-in providers read local state, so the trait is
/// synchronous and usable while the configuration loads, before the runtime
/// services exist.
pub trait SecretProvider: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> &'static str;
    fn get(&self, r: &SecretRef) -> Result<Secret, SecretError>;
    fn health(&self) -> ProviderHealth;
}

/// `secret://env/NAME`: the value of an environment variable.
pub struct EnvProvider;

impl SecretProvider for EnvProvider {
    fn id(&self) -> &str {
        "env"
    }

    fn kind(&self) -> &'static str {
        "builtin"
    }

    fn get(&self, r: &SecretRef) -> Result<Secret, SecretError> {
        if r.version.is_some() {
            return Err(SecretError::Invalid(
                "the env provider has no versions".into(),
            ));
        }
        let valid = !r.path.is_empty()
            && !r.path.starts_with(|c: char| c.is_ascii_digit())
            && r.path
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(SecretError::Invalid(
                "an environment variable name is letters, digits and _".into(),
            ));
        }
        match std::env::var(&r.path) {
            Ok(v) => select_key(Zeroizing::new(v), r.key.as_deref()),
            Err(std::env::VarError::NotPresent) => Err(SecretError::NotFound),
            Err(_) => Err(SecretError::Unavailable(
                "the variable is not valid UTF-8".into(),
            )),
        }
    }

    fn health(&self) -> ProviderHealth {
        ProviderHealth {
            healthy: true,
            detail: None,
        }
    }
}

/// `secret://file//run/secrets/x`: the content of a file without one trailing
/// newline. The path is absolute with no `..`; `allowed_dirs`, when set,
/// restricts where the resolved path may point.
pub struct FileProvider {
    allowed_dirs: Vec<PathBuf>,
}

impl FileProvider {
    pub fn new(allowed_dirs: Vec<PathBuf>) -> Self {
        FileProvider { allowed_dirs }
    }
}

impl SecretProvider for FileProvider {
    fn id(&self) -> &str {
        "file"
    }

    fn kind(&self) -> &'static str {
        "builtin"
    }

    fn get(&self, r: &SecretRef) -> Result<Secret, SecretError> {
        if r.version.is_some() {
            return Err(SecretError::Invalid(
                "the file provider has no versions".into(),
            ));
        }
        let path = Path::new(&r.path);
        if !path.is_absolute() {
            return Err(SecretError::Invalid(
                "the file path must be absolute (secret://file//path)".into(),
            ));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(SecretError::Invalid("the file path has '..'".into()));
        }
        // The symlink-resolved path is the one that is checked and read.
        let real = std::fs::canonicalize(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SecretError::NotFound,
            std::io::ErrorKind::PermissionDenied => SecretError::Denied("not readable".into()),
            _ => SecretError::Unavailable("cannot resolve the file".into()),
        })?;
        if !self.allowed_dirs.is_empty() && !self.allowed_dirs.iter().any(|d| real.starts_with(d)) {
            return Err(SecretError::Denied(
                "the file is outside secrets.file_allowed_dirs".into(),
            ));
        }
        let meta = std::fs::metadata(&real)
            .map_err(|_| SecretError::Unavailable("cannot read the file".into()))?;
        if !meta.is_file() {
            return Err(SecretError::Invalid("not a regular file".into()));
        }
        if meta.len() > MAX_VALUE_BYTES {
            return Err(SecretError::Invalid(format!(
                "the file is larger than {MAX_VALUE_BYTES} bytes"
            )));
        }
        let mut content =
            Zeroizing::new(std::fs::read_to_string(&real).map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => SecretError::Denied("not readable".into()),
                _ => SecretError::Unavailable("cannot read the file as UTF-8 text".into()),
            })?);
        if content.ends_with('\n') {
            content.pop();
            if content.ends_with('\r') {
                content.pop();
            }
        }
        select_key(content, r.key.as_deref())
    }

    fn health(&self) -> ProviderHealth {
        ProviderHealth {
            healthy: true,
            detail: None,
        }
    }
}

/// With `#key`, the raw value is a JSON object and the secret is that field.
fn select_key(raw: Zeroizing<String>, key: Option<&str>) -> Result<Secret, SecretError> {
    let Some(key) = key else {
        return Ok(Secret::new(raw.to_string()));
    };
    let parsed: Json = serde_json::from_str(&raw)
        .map_err(|_| SecretError::Invalid("#key needs a JSON object value".into()))?;
    match parsed.get(key) {
        Some(Json::String(s)) => Ok(Secret::new(s.clone())),
        Some(Json::Number(n)) => Ok(Secret::new(n.to_string())),
        Some(Json::Bool(b)) => Ok(Secret::new(b.to_string())),
        Some(_) => Err(SecretError::Invalid("the key is not a scalar".into())),
        None => Err(SecretError::NotFound),
    }
}

/// One resolution, for the audit. It never holds the value.
#[derive(Debug, Clone, Serialize)]
pub struct SecretAccess {
    pub requester: String,
    pub purpose: String,
    pub provider: String,
    pub path: String,
    pub result: String,
    pub at: DateTime<Utc>,
}

/// The `secrets:` section of the configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SecretsConfig {
    /// Directories the `file` provider may read from. Empty: any absolute path.
    #[serde(default)]
    pub file_allowed_dirs: Vec<PathBuf>,
}

pub struct SecretResolver {
    providers: HashMap<String, Arc<dyn SecretProvider>>,
}

impl SecretResolver {
    /// The resolver with the `env` and `file` providers.
    pub fn builtin(config: &SecretsConfig) -> Self {
        let mut r = SecretResolver {
            providers: HashMap::new(),
        };
        r.register(Arc::new(EnvProvider));
        r.register(Arc::new(FileProvider::new(
            config
                .file_allowed_dirs
                .iter()
                .map(|d| std::fs::canonicalize(d).unwrap_or_else(|_| d.clone()))
                .collect(),
        )));
        r
    }

    pub fn register(&mut self, provider: Arc<dyn SecretProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn providers(&self) -> Vec<(String, &'static str, ProviderHealth)> {
        let mut v: Vec<_> = self
            .providers
            .values()
            .map(|p| (p.id().to_string(), p.kind(), p.health()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// Resolves a reference. Fails closed; the access is returned for the audit.
    pub fn resolve(
        &self,
        r: &SecretRef,
        ctx: &SecretContext,
    ) -> (Result<Secret, SecretError>, SecretAccess) {
        let result = self.resolve_inner(r, ctx);
        let access = SecretAccess {
            requester: ctx.requester.label(),
            purpose: ctx.purpose.clone(),
            provider: r.provider.clone(),
            path: r.path.clone(),
            result: match &result {
                Ok(_) => "ok".into(),
                Err(e) => e.code().into(),
            },
            at: Utc::now(),
        };
        (result, access)
    }

    fn resolve_inner(&self, r: &SecretRef, ctx: &SecretContext) -> Result<Secret, SecretError> {
        if ctx.requester != Requester::Core {
            return Err(SecretError::Denied(format!(
                "{} cannot resolve secrets",
                ctx.requester.label()
            )));
        }
        let provider = self
            .providers
            .get(&r.provider)
            .ok_or_else(|| SecretError::Invalid(format!("unknown provider '{}'", r.provider)))?;
        provider.get(r)
    }

    /// Resolves every `secret://` string of a YAML tree in place and returns
    /// the audit records. The first failure names the field and the
    /// reference, never a value.
    pub fn resolve_tree(
        &self,
        tree: &mut serde_norway::Value,
    ) -> Result<Vec<SecretAccess>, String> {
        let mut accesses = Vec::new();
        self.walk(tree, String::new(), &mut accesses)?;
        Ok(accesses)
    }

    fn walk(
        &self,
        node: &mut serde_norway::Value,
        field: String,
        out: &mut Vec<SecretAccess>,
    ) -> Result<(), String> {
        use serde_norway::Value;
        match node {
            Value::String(s) if SecretRef::is_reference(s) => {
                let r = SecretRef::parse(s).map_err(|e| format!("{field}: {e}"))?;
                let (res, access) =
                    self.resolve(&r, &SecretContext::core(format!("config:{field}")));
                out.push(access);
                match res {
                    Ok(secret) => *s = secret.expose().to_string(),
                    Err(e) => {
                        return Err(format!(
                            "{field}: cannot resolve secret://{}: {e}",
                            r.display_path()
                        ))
                    }
                }
            }
            Value::Mapping(m) => {
                for (k, v) in m.iter_mut() {
                    let name = k.as_str().unwrap_or("?");
                    let child = if field.is_empty() {
                        name.to_string()
                    } else {
                        format!("{field}.{name}")
                    };
                    self.walk(v, child, out)?;
                }
            }
            Value::Sequence(seq) => {
                for (i, v) in seq.iter_mut().enumerate() {
                    self.walk(v, format!("{field}[{i}]"), out)?;
                }
            }
            Value::Tagged(t) => self.walk(&mut t.value, field, out)?,
            _ => {}
        }
        Ok(())
    }
}

/// Whether a YAML tree holds a reference, so a configuration without one is
/// never rewritten.
pub fn tree_has_reference(node: &serde_norway::Value) -> bool {
    use serde_norway::Value;
    match node {
        Value::String(s) => SecretRef::is_reference(s),
        Value::Mapping(m) => m.iter().any(|(_, v)| tree_has_reference(v)),
        Value::Sequence(s) => s.iter().any(tree_has_reference),
        Value::Tagged(t) => tree_has_reference(&t.value),
        _ => false,
    }
}

static BOOT_ACCESSES: Mutex<Vec<SecretAccess>> = Mutex::new(Vec::new());

/// Resolutions made while the configuration loaded, before the database
/// existed; the host audits them once it is up.
pub fn record_boot_accesses(mut accesses: Vec<SecretAccess>) {
    if let Ok(mut g) = BOOT_ACCESSES.lock() {
        g.append(&mut accesses);
    }
}

pub fn take_boot_accesses() -> Vec<SecretAccess> {
    BOOT_ACCESSES
        .lock()
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> SecretResolver {
        SecretResolver::builtin(&SecretsConfig::default())
    }

    #[test]
    fn parses_references() {
        let r =
            SecretRef::parse("secret://vault-prod/kv/openvox/puppetdb#token?version=3").unwrap();
        assert_eq!(r.provider, "vault-prod");
        assert_eq!(r.path, "kv/openvox/puppetdb");
        assert_eq!(r.key.as_deref(), Some("token"));
        assert_eq!(r.version, Some(3));
        let f = SecretRef::parse("secret://file//run/secrets/x").unwrap();
        assert_eq!(f.path, "/run/secrets/x");
        assert!(SecretRef::parse("secret://env").is_err());
        assert!(SecretRef::parse("secret:///x").is_err());
        assert!(SecretRef::parse("secret://e nv/x").is_err());
        assert!(SecretRef::parse("secret://env/X?foo=1").is_err());
        assert!(SecretRef::parse("secret://env/X#").is_err());
        assert!(SecretRef::parse("https://env/X").is_err());
    }

    #[test]
    fn secret_never_prints_its_value() {
        let s = Secret::new("hunter2".into());
        assert_eq!(format!("{s:?}"), "[REDACTED]");
        assert_eq!(format!("{s}"), "[REDACTED]");
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn env_provider_resolves_and_fails_closed() {
        std::env::set_var("OVX_TEST_SECRET_A", "value-a");
        std::env::set_var("OVX_TEST_SECRET_JSON", r#"{"token":"t0k","n":7,"o":{}}"#);
        let res = resolver();
        let ctx = SecretContext::core("test");
        let get = |s: &str| res.resolve(&SecretRef::parse(s).unwrap(), &ctx);

        let (v, a) = get("secret://env/OVX_TEST_SECRET_A");
        assert_eq!(v.unwrap().expose(), "value-a");
        assert_eq!(a.result, "ok");
        assert_eq!(a.provider, "env");

        let json = |s: &str| get(&format!("secret://env/OVX_TEST_SECRET_JSON{s}")).0;
        assert_eq!(json("#token").unwrap().expose(), "t0k");
        assert_eq!(json("#n").unwrap().expose(), "7");
        assert_eq!(json("#o").unwrap_err().code(), "invalid");
        assert_eq!(json("#nope").unwrap_err().code(), "not_found");
        assert_eq!(
            get("secret://env/OVX_TEST_SECRET_A#k")
                .0
                .unwrap_err()
                .code(),
            "invalid"
        );
        assert_eq!(
            get("secret://env/OVX_TEST_SECRET_MISSING").0.unwrap_err(),
            SecretError::NotFound
        );
        assert_eq!(
            get("secret://env/BAD-NAME").0.unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            get("secret://env/OVX_TEST_SECRET_A?version=1")
                .0
                .unwrap_err()
                .code(),
            "invalid"
        );
        let (e, a) = get("secret://nope/x");
        assert_eq!(e.unwrap_err().code(), "invalid");
        assert_eq!(a.result, "invalid");
    }

    #[test]
    fn file_provider_reads_and_restricts() {
        let dir = std::env::temp_dir().join(format!("ovx-secret-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pw");
        std::fs::write(&file, "s3cret\n").unwrap();
        let big = dir.join("big");
        std::fs::write(&big, vec![b'a'; (MAX_VALUE_BYTES + 1) as usize]).unwrap();
        let ctx = SecretContext::core("test");
        let get = |res: &SecretResolver, p: String| {
            res.resolve(
                &SecretRef::parse(&format!("secret://file/{p}")).unwrap(),
                &ctx,
            )
            .0
        };

        let res = resolver();
        assert_eq!(
            get(&res, file.display().to_string()).unwrap().expose(),
            "s3cret"
        );
        assert_eq!(
            get(&res, big.display().to_string()).unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            get(&res, format!("{}/missing", dir.display())).unwrap_err(),
            SecretError::NotFound
        );
        assert_eq!(
            get(&res, "relative/path".into()).unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            get(&res, format!("{}/../pw", dir.display()))
                .unwrap_err()
                .code(),
            "invalid"
        );

        let other = std::env::temp_dir().join(format!("ovx-other-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&other).unwrap();
        let restricted = SecretResolver::builtin(&SecretsConfig {
            file_allowed_dirs: vec![other.clone()],
        });
        assert_eq!(
            get(&restricted, file.display().to_string())
                .unwrap_err()
                .code(),
            "denied"
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&other).ok();
    }

    #[test]
    fn only_the_host_resolves() {
        std::env::set_var("OVX_TEST_SECRET_B", "b");
        let ctx = SecretContext {
            requester: Requester::Plugin("acme".into()),
            purpose: "test".into(),
        };
        let r = SecretRef::parse("secret://env/OVX_TEST_SECRET_B").unwrap();
        let (v, a) = resolver().resolve(&r, &ctx);
        assert_eq!(v.unwrap_err().code(), "denied");
        assert_eq!(a.requester, "plugin:acme");
    }

    #[test]
    fn resolves_a_yaml_tree_and_audits_without_values() {
        std::env::set_var("OVX_TEST_SECRET_C", "c-value");
        let mut tree: serde_norway::Value = serde_norway::from_str(
            "auth:\n  jwt_secret: secret://env/OVX_TEST_SECRET_C\nlist:\n  - plain\n  - secret://env/OVX_TEST_SECRET_C\n",
        )
        .unwrap();
        assert!(tree_has_reference(&tree));
        let accesses = resolver().resolve_tree(&mut tree).unwrap();
        assert!(!tree_has_reference(&tree));
        assert_eq!(tree["auth"]["jwt_secret"].as_str(), Some("c-value"));
        assert_eq!(tree["list"][1].as_str(), Some("c-value"));
        assert_eq!(accesses.len(), 2);
        assert_eq!(accesses[0].purpose, "config:auth.jwt_secret");
        assert_eq!(accesses[1].purpose, "config:list[1]");
        assert!(!serde_json::to_string(&accesses)
            .unwrap()
            .contains("c-value"));
    }

    #[test]
    fn tree_failure_names_the_field_not_the_value() {
        let mut tree: serde_norway::Value =
            serde_norway::from_str("database:\n  url: secret://env/OVX_TEST_SECRET_NOPE\n")
                .unwrap();
        let err = resolver().resolve_tree(&mut tree).unwrap_err();
        assert!(err.contains("database.url"), "{err}");
        assert!(err.contains("env/OVX_TEST_SECRET_NOPE"), "{err}");
    }
}
