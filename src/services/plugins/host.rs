//! Host API of the WASM runtime and its capability gate (M20).
//!
//! Every call a plugin makes arrives here with the `(plugin, version)`
//! context; the plugin never chooses whose data it touches. The gate applies
//! `effective = declared (manifest) ∩ approved (plugin_grants) ∩ policy`.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::runtime::Handle;

use super::events::EventBus;
use super::filter::pattern_matches;
use super::manager::PluginManager;
use super::manifest::capabilities;
use super::package;
use super::storage::{PluginStorage, StorageError};
use super::wasm::{HostApi, HostContext, HostError, ModuleSource};
use crate::config::PluginPolicyConfig;
use crate::db::{AuditRepository, PluginRepository};
use crate::models::{Capability, PluginManifest};
use crate::services::puppetdb::PuppetDbClient;

pub const MAX_HTTP_BODY: usize = 1024 * 1024;
const HTTP_TIMEOUT_SECS: u64 = 10;
const MAX_REDIRECTS: usize = 3;
const MAX_LOG_LEN: usize = 2000;

/// Capabilities the plugin may use right now: declared by its active
/// version, approved by an administrator with the same scope, and not
/// denied by the policy ceiling.
pub async fn effective_capabilities(
    pool: &SqlitePool,
    policy: &PluginPolicyConfig,
    plugin_id: &str,
) -> Vec<Capability> {
    let repo = PluginRepository::new(pool);
    let Ok(Some(rec)) = repo.get_plugin(plugin_id).await else {
        return Vec::new();
    };
    let Some(version) = rec.active_version else {
        return Vec::new();
    };
    let manifest = match repo.get_version(plugin_id, &version).await {
        Ok(Some(v)) => serde_json::from_str::<PluginManifest>(&v.manifest_json).ok(),
        _ => None,
    };
    let Some(manifest) = manifest else {
        return Vec::new();
    };
    let grants = repo.list_grants(plugin_id).await.unwrap_or_default();
    capabilities(&manifest)
        .into_iter()
        .filter(|c| !policy.denies(&c.name))
        .filter(|c| {
            grants
                .iter()
                .any(|g| g.capability == c.name && g.scope == c.scope)
        })
        .collect()
}

fn denied(cap: &str) -> HostError {
    HostError {
        code: "forbidden".into(),
        message: format!("the capability '{}' is not approved for this plugin", cap),
    }
}

fn invalid(msg: impl Into<String>) -> HostError {
    HostError {
        code: "invalid".into(),
        message: msg.into(),
    }
}

fn failed(msg: impl Into<String>) -> HostError {
    HostError {
        code: "failed".into(),
        message: msg.into(),
    }
}

impl From<StorageError> for HostError {
    fn from(e: StorageError) -> Self {
        let code = match &e {
            StorageError::Quota(_) => "quota",
            StorageError::Invalid(_) => "invalid",
            StorageError::Denied(_) => "forbidden",
            StorageError::Timeout => "timeout",
            StorageError::Failed(_) => "failed",
        };
        HostError {
            code: code.into(),
            message: e.to_string(),
        }
    }
}

fn str_field<'a>(req: &'a Value, name: &str) -> Result<&'a str, HostError> {
    req.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("'{}' must be a string", name)))
}

/// Host-side implementation of the plugin host API.
pub struct HostApiImpl {
    pool: SqlitePool,
    policy: PluginPolicyConfig,
    storage: Arc<PluginStorage>,
    manager: std::sync::Weak<PluginManager>,
    puppetdb: Option<Arc<PuppetDbClient>>,
    handle: Handle,
}

impl HostApiImpl {
    /// Must be created inside the tokio runtime: calls from guest threads
    /// are driven on that runtime.
    pub fn new(
        manager: &Arc<PluginManager>,
        policy: PluginPolicyConfig,
        storage: Arc<PluginStorage>,
        puppetdb: Option<Arc<PuppetDbClient>>,
    ) -> Self {
        Self {
            pool: manager.pool().clone(),
            policy,
            storage,
            manager: Arc::downgrade(manager),
            puppetdb,
            handle: Handle::current(),
        }
    }

    async fn caps(&self, plugin: &str) -> Vec<Capability> {
        effective_capabilities(&self.pool, &self.policy, plugin).await
    }

    async fn require(&self, plugin: &str, cap: &str) -> Result<Capability, HostError> {
        self.caps(plugin)
            .await
            .into_iter()
            .find(|c| c.name == cap)
            .ok_or_else(|| denied(cap))
    }

    async fn dispatch(&self, ctx: &HostContext, op: &str, req: Value) -> Result<Value, HostError> {
        let plugin = ctx.plugin_id.as_str();
        match op {
            "kv.get" => {
                self.require(plugin, "kv:rw").await?;
                let v = self.storage.kv_get(plugin, str_field(&req, "key")?).await?;
                Ok(json!({ "value": v }))
            }
            "kv.put" => {
                self.require(plugin, "kv:rw").await?;
                let value = req
                    .get("value")
                    .ok_or_else(|| invalid("'value' is required"))?;
                self.storage
                    .kv_put(plugin, str_field(&req, "key")?, value)
                    .await?;
                Ok(json!({}))
            }
            "kv.delete" => {
                self.require(plugin, "kv:rw").await?;
                let d = self
                    .storage
                    .kv_delete(plugin, str_field(&req, "key")?)
                    .await?;
                Ok(json!({ "deleted": d }))
            }
            "kv.list" => {
                self.require(plugin, "kv:rw").await?;
                let prefix = req.get("prefix").and_then(Value::as_str).unwrap_or("");
                let limit = req
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(100)
                    .min(1000) as usize;
                let items = self.storage.kv_list(plugin, prefix, limit).await?;
                Ok(
                    json!({ "items": items.into_iter().map(|(k, v)| json!({"key": k, "value": v})).collect::<Vec<_>>() }),
                )
            }
            "db.query" | "db.exec" => {
                self.require(plugin, "db:own").await?;
                let sql = str_field(&req, "sql")?;
                let params = req
                    .get("params")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if op == "db.query" {
                    let rows = self.storage.db_query(plugin, sql, &params).await?;
                    Ok(json!({ "rows": rows }))
                } else {
                    let r = self.storage.db_exec(plugin, sql, &params).await?;
                    Ok(
                        json!({ "rows_affected": r.rows_affected, "last_insert_rowid": r.last_insert_rowid }),
                    )
                }
            }
            "nodes.list" => {
                self.require(plugin, "nodes:read").await?;
                let db = self.puppetdb()?;
                let limit = req
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(500)
                    .min(5000) as usize;
                let nodes = db.get_nodes().await.map_err(|e| failed(e.to_string()))?;
                Ok(json!({ "nodes": nodes.into_iter().take(limit).collect::<Vec<_>>() }))
            }
            "nodes.get" => {
                self.require(plugin, "nodes:read").await?;
                let n = self
                    .puppetdb()?
                    .get_node(str_field(&req, "certname")?)
                    .await
                    .map_err(|e| failed(e.to_string()))?;
                Ok(json!({ "node": n }))
            }
            "facts.get" => {
                self.require(plugin, "facts:read").await?;
                let certname = str_field(&req, "certname")?;
                let db = self.puppetdb()?;
                if let Some(name) = req.get("name").and_then(Value::as_str) {
                    let f = db
                        .get_node_fact(certname, name)
                        .await
                        .map_err(|e| failed(e.to_string()))?;
                    Ok(json!({ "fact": f }))
                } else {
                    let f = db
                        .get_node_facts(certname)
                        .await
                        .map_err(|e| failed(e.to_string()))?;
                    Ok(json!({ "facts": f }))
                }
            }
            "reports.query" => {
                self.require(plugin, "reports:read").await?;
                let certname = str_field(&req, "certname")?;
                let limit = req
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .min(200) as u32;
                let r = self
                    .puppetdb()?
                    .get_node_reports(certname, Some(limit))
                    .await
                    .map_err(|e| failed(e.to_string()))?;
                Ok(json!({ "reports": r }))
            }
            "events.publish" => {
                let event = str_field(&req, "type")?;
                let allowed = self.caps(plugin).await.iter().any(|c| {
                    c.name
                        .strip_prefix("events:publish:")
                        .is_some_and(|p| pattern_matches(p, event))
                });
                if !allowed {
                    return Err(denied(&format!("events:publish:{}", event)));
                }
                let manager = self
                    .manager
                    .upgrade()
                    .ok_or_else(|| failed("host is shutting down"))?;
                let bus = EventBus::new(self.pool.clone());
                let env = bus
                    .publish_from_plugin(
                        &manager.hooks(),
                        plugin,
                        event,
                        req.get("payload").cloned().unwrap_or(Value::Null),
                    )
                    .await
                    .map_err(|e| invalid(e.to_string()))?;
                Ok(json!({ "id": env.id }))
            }
            "audit.write" => {
                self.require(plugin, "audit:write").await?;
                let action = str_field(&req, "action")?;
                if action.is_empty() || action.len() > 64 {
                    return Err(invalid("'action' must be 1 to 64 characters"));
                }
                AuditRepository::new(&self.pool)
                    .insert(
                        crate::models::default_organization_uuid(),
                        None,
                        &format!("plugin.{}.{}", plugin, action),
                        "plugin_runtime",
                        Some(plugin),
                        req.get("details"),
                        None,
                    )
                    .await
                    .map_err(|e| failed(e.to_string()))?;
                Ok(json!({}))
            }
            "log.write" => {
                let level = req.get("level").and_then(Value::as_str).unwrap_or("info");
                let msg: String = str_field(&req, "message")?
                    .chars()
                    .take(MAX_LOG_LEN)
                    .collect();
                match level {
                    "error" => tracing::error!(plugin = plugin, "{}", msg),
                    "warn" => tracing::warn!(plugin = plugin, "{}", msg),
                    "debug" => tracing::debug!(plugin = plugin, "{}", msg),
                    _ => tracing::info!(plugin = plugin, "{}", msg),
                }
                Ok(json!({}))
            }
            "config.get" => {
                let cfg = PluginRepository::new(&self.pool)
                    .get_config(plugin)
                    .await
                    .map_err(|e| failed(e.to_string()))?;
                Ok(match req.get("key").and_then(Value::as_str) {
                    Some(k) => json!({ "value": cfg.get(k) }),
                    None => json!({ "values": cfg }),
                })
            }
            "http.fetch" => {
                let cap = self.require(plugin, "http:egress").await?;
                http_fetch(&cap, &req).await
            }
            "email.send" | "notify.send" | "secrets.resolve" => Err(HostError {
                code: "unavailable".into(),
                message: format!("'{}' is not available in this version", op),
            }),
            _ => Err(HostError {
                code: "unknown_op".into(),
                message: format!("unknown host operation '{}'", op),
            }),
        }
    }

    fn puppetdb(&self) -> Result<&Arc<PuppetDbClient>, HostError> {
        self.puppetdb.as_ref().ok_or_else(|| HostError {
            code: "unavailable".into(),
            message: "PuppetDB is not configured".into(),
        })
    }
}

impl HostApi for HostApiImpl {
    fn call(&self, ctx: &HostContext, op: &str, request: Value) -> Result<Value, HostError> {
        // Runs on a blocking thread dedicated to the guest.
        self.handle.block_on(self.dispatch(ctx, op, request))
    }
}

// ---- http.fetch ------------------------------------------------------------

/// Public unicast addresses only: loopback, private, link-local (cloud
/// metadata), CGNAT, multicast and unspecified ranges are refused.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xC0) == 64)
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xFFC0) == 0xFE80
                || (s[0] & 0xFE00) == 0xFC00)
        }
    }
}

/// Exact host or `*.domain` allowlist match.
pub fn host_allowed(allowlist: &[String], host: &str) -> bool {
    allowlist.iter().any(|a| match a.strip_prefix("*.") {
        Some(domain) => host.ends_with(&format!(".{}", domain)),
        None => a == host,
    })
}

fn allowlist_of(cap: &Capability) -> Vec<String> {
    cap.scope
        .as_ref()
        .and_then(|s| s.get("hosts"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|h| h.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

async fn http_fetch(cap: &Capability, req: &Value) -> Result<Value, HostError> {
    let allow = allowlist_of(cap);
    let mut url =
        reqwest::Url::parse(str_field(req, "url")?).map_err(|e| invalid(e.to_string()))?;
    let method = req.get("method").and_then(Value::as_str).unwrap_or("GET");
    let mut method = reqwest::Method::from_bytes(method.to_ascii_uppercase().as_bytes())
        .map_err(|_| invalid("invalid method"))?;
    if !matches!(
        method.as_str(),
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE"
    ) {
        return Err(invalid("method not allowed"));
    }
    let body = req.get("body").and_then(Value::as_str).map(str::to_string);
    if body.as_ref().is_some_and(|b| b.len() > MAX_HTTP_BODY) {
        return Err(invalid("request body is too large"));
    }
    let mut body = body;
    for _ in 0..=MAX_REDIRECTS {
        if url.scheme() != "https" {
            return Err(invalid("only https is allowed"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| invalid("missing host"))?
            .to_ascii_lowercase();
        if !host_allowed(&allow, &host) {
            return Err(denied(&format!("http:egress to {}", host)));
        }
        let port = url.port_or_known_default().unwrap_or(443);
        // Resolve once, validate every address, then connect to exactly
        // those addresses (no second lookup an attacker could change).
        let addrs: Vec<_> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| failed(format!("dns: {}", e)))?
            .collect();
        if addrs.is_empty() || addrs.iter().any(|a| !is_public_ip(a.ip())) {
            return Err(denied("the host resolves to a non-public address"));
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .resolve_to_addrs(&host, &addrs)
            .build()
            .map_err(|e| failed(e.to_string()))?;
        let mut rb = client.request(method.clone(), url.clone());
        if let Some(h) = req.get("headers").and_then(Value::as_object) {
            for (k, v) in h {
                let lk = k.to_ascii_lowercase();
                if matches!(
                    lk.as_str(),
                    "host" | "content-length" | "connection" | "transfer-encoding"
                ) {
                    continue;
                }
                if let Some(v) = v.as_str() {
                    rb = rb.header(k.as_str(), v);
                }
            }
        }
        if let Some(b) = &body {
            rb = rb.body(b.clone());
        }
        let resp = rb.send().await.map_err(|e| failed(e.to_string()))?;
        let status = resp.status();
        if status.is_redirection() {
            let loc = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| failed("redirect without location"))?;
            url = url.join(loc).map_err(|e| failed(e.to_string()))?;
            if status != reqwest::StatusCode::TEMPORARY_REDIRECT
                && status != reqwest::StatusCode::PERMANENT_REDIRECT
            {
                method = reqwest::Method::GET;
                body = None;
            }
            continue;
        }
        let headers: serde_json::Map<String, Value> = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), json!(v.to_str().ok()?))))
            .collect();
        let mut buf = Vec::new();
        let mut resp = resp;
        while let Some(chunk) = resp.chunk().await.map_err(|e| failed(e.to_string()))? {
            if buf.len() + chunk.len() > MAX_HTTP_BODY {
                return Err(HostError {
                    code: "quota".into(),
                    message: "the response is larger than 1 MiB".into(),
                });
            }
            buf.extend_from_slice(&chunk);
        }
        return Ok(json!({
            "status": status.as_u16(),
            "headers": headers,
            "body": String::from_utf8_lossy(&buf),
        }));
    }
    Err(failed("too many redirects"))
}

// ---- module source -----------------------------------------------------------

/// Reads WASM modules from the stored, hash-pinned `.ovxp` packages.
pub struct PackageModuleSource {
    pub data_dir: PathBuf,
    pub max_package_bytes: u64,
}

#[async_trait]
impl ModuleSource for PackageModuleSource {
    async fn load(&self, plugin_id: &str, version: &str, entry: &str) -> Result<Vec<u8>, String> {
        if plugin_id.contains(['/', '\\']) || version.contains(['/', '\\']) {
            return Err("invalid plugin reference".into());
        }
        let path = self
            .data_dir
            .join("packages")
            .join(plugin_id)
            .join(format!("{}.ovxp", version));
        let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
        let pkg =
            package::read_package(&bytes, self.max_package_bytes).map_err(|e| e.to_string())?;
        pkg.files
            .get(entry)
            .cloned()
            .ok_or_else(|| format!("'{}' is not in the package", entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_public_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.1",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(is_public_ip("93.184.216.34".parse().unwrap()));
        assert!(is_public_ip("2606:4700::1111".parse().unwrap()));
    }

    #[test]
    fn egress_allowlist_matches_exact_and_wildcard() {
        let a = vec![
            "api.example.com".to_string(),
            "*.corp.example.org".to_string(),
        ];
        assert!(host_allowed(&a, "api.example.com"));
        assert!(host_allowed(&a, "x.corp.example.org"));
        assert!(!host_allowed(&a, "corp.example.org"));
        assert!(!host_allowed(&a, "evil-api.example.com"));
        assert!(!host_allowed(&a, "api.example.com.evil.net"));
    }

    // ---- end to end: manager, grants, host API and a real WASM guest ----------

    use crate::models::ApproveCapabilitiesRequest;
    use crate::services::plugins::invoke::{HandlerInvoker, HandlerKind};
    use crate::services::plugins::package::build_signed_package;
    use crate::services::plugins::wasm::{WasmConfig, WasmInvoker};
    use ed25519_dalek::SigningKey;
    use sqlx::sqlite::SqlitePoolOptions;

    const SEED: [u8; 32] = [5; 32];

    /// A guest whose `handle` makes one fixed host call and returns the host
    /// envelope as its result: `{"ok": <envelope>}`.
    fn guest(op: &str, req: &str) -> Vec<u8> {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let wat = format!(
            r#"(module
  (import "openvox" "host_call" (func $hc (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 2)
  (global $bump (mut i32) (i32.const 8192))
  (data (i32.const 256) "{op}")
  (data (i32.const 1024) "{req}")
  (data (i32.const 512) "{{\"ok\":")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $p))
  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    (local $packed i64) (local $rp i32) (local $rl i32) (local $out i32)
    (local.set $packed (call $hc (i32.const 256) (i32.const {oplen}) (i32.const 1024) (i32.const {reqlen})))
    (local.set $rp (i32.wrap_i64 (i64.shr_u (local.get $packed) (i64.const 32))))
    (local.set $rl (i32.wrap_i64 (local.get $packed)))
    (local.set $out (call $alloc (i32.add (local.get $rl) (i32.const 7))))
    (memory.copy (local.get $out) (i32.const 512) (i32.const 6))
    (memory.copy (i32.add (local.get $out) (i32.const 6)) (local.get $rp) (local.get $rl))
    (i32.store8 (i32.add (i32.add (local.get $out) (i32.const 6)) (local.get $rl)) (i32.const 125))
    (i64.or (i64.shl (i64.extend_i32_u (local.get $out)) (i64.const 32))
            (i64.extend_i32_u (i32.add (local.get $rl) (i32.const 7))))))"#,
            op = esc(op),
            req = esc(req),
            oplen = op.len(),
            reqlen = req.len(),
        );
        wat::parse_str(wat).unwrap()
    }

    fn manifest(id: &str, caps: &str, migrations: bool) -> String {
        format!(
            "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {{ kind: wasm, entry: plugin.wasm, limits: {{ timeout_ms: 5000 }} }}\n\
             capabilities:\n{caps}{}",
            if migrations {
                "migrations: { dir: migrations }\n"
            } else {
                ""
            }
        )
    }

    fn package(id: &str, caps: &str, op: &str, req: &str, migrations: bool) -> Vec<u8> {
        let y = manifest(id, caps, migrations);
        let wasm = guest(op, req);
        let up = b"CREATE TABLE notes(v TEXT)";
        let down = b"DROP TABLE notes";
        let mut files: Vec<(&str, &[u8])> =
            vec![("plugin.yaml", y.as_bytes()), ("plugin.wasm", &wasm)];
        if migrations {
            files.push(("migrations/0001_init.up.sql", up));
            files.push(("migrations/0001_init.down.sql", down));
        }
        build_signed_package(&files, id, "1.0.0", 2, SEED)
    }

    struct Env {
        manager: Arc<PluginManager>,
        storage: Arc<PluginStorage>,
        dir: std::path::PathBuf,
    }

    async fn env(policy: PluginPolicyConfig) -> Env {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = std::env::temp_dir().join(format!("ovx_m20_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let manager = Arc::new(
            PluginManager::new(pool.clone(), dir.clone(), 8)
                .with_builtin_keys(vec![("T".into(), pk)])
                .with_policy(policy.clone()),
        );
        manager.reload().await.unwrap();
        let storage = Arc::new(PluginStorage::new(
            pool,
            dir.clone(),
            crate::services::plugins::storage::StorageLimits::default(),
        ));
        let host = Arc::new(HostApiImpl::new(&manager, policy, storage.clone(), None));
        let source = Arc::new(PackageModuleSource {
            data_dir: dir.clone(),
            max_package_bytes: 8 * 1024 * 1024,
        });
        let wasm = Arc::new(WasmInvoker::new(WasmConfig::default(), source, host).unwrap());
        manager.attach_wasm(storage.clone(), wasm);
        Env {
            manager,
            storage,
            dir,
        }
    }

    fn approve(caps: &[&str]) -> ApproveCapabilitiesRequest {
        ApproveCapabilitiesRequest {
            version: None,
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
        }
    }

    async fn run(e: &Env, id: &str) -> Value {
        let call =
            e.manager
                .hooks()
                .get(id)
                .unwrap()
                .call(HandlerKind::Event, "on_event", json!({}));
        e.manager.invoke(call).await.unwrap()
    }

    #[tokio::test]
    async fn enabling_needs_every_required_capability_approved() {
        let e = env(Default::default()).await;
        let pkg = package(
            "wdemo",
            "  - kv:rw\n  - db:own\n",
            "kv.put",
            r#"{"key":"k","value":{"n":1}}"#,
            true,
        );
        e.manager.install(&pkg, "admin", None).await.unwrap();
        let err = e.manager.enable("wdemo").await.err().unwrap();
        assert!(err.to_string().contains("kv:rw"), "{err}");
        e.manager
            .approve_capabilities("wdemo", &approve(&["kv:rw"]), "admin")
            .await
            .unwrap();
        let err = e.manager.enable("wdemo").await.err().unwrap();
        assert!(err.to_string().contains("db:own"), "{err}");
        assert!(e.manager.hooks().get("wdemo").is_none());
        e.manager
            .approve_capabilities("wdemo", &approve(&["kv:rw", "db:own"]), "admin")
            .await
            .unwrap();
        e.manager.enable("wdemo").await.unwrap();
        // Migrations ran when the plugin was enabled.
        assert_eq!(e.storage.schema_version("wdemo").await.unwrap(), 1);
        assert_eq!(
            e.manager.get("wdemo").await.unwrap().summary.state,
            crate::models::PluginState::Enabled
        );
        let out = run(&e, "wdemo").await;
        assert_eq!(out["ok"], json!({}));
        assert_eq!(
            e.storage.kv_get("wdemo", "k").await.unwrap(),
            Some(json!({"n": 1}))
        );
    }

    #[tokio::test]
    async fn a_call_without_an_approved_capability_is_forbidden() {
        let e = env(Default::default()).await;
        // db:own is declared but optional, and never approved.
        let pkg = package(
            "nodb",
            "  - kv:rw\n  - db:own: { optional: true }\n",
            "db.query",
            r#"{"sql":"SELECT 1"}"#,
            false,
        );
        e.manager.install(&pkg, "admin", None).await.unwrap();
        e.manager
            .approve_capabilities("nodb", &approve(&["kv:rw"]), "admin")
            .await
            .unwrap();
        e.manager.enable("nodb").await.unwrap();
        let out = run(&e, "nodb").await;
        assert_eq!(out["error"]["code"], "forbidden");
        // An operation for a capability that was never declared is refused too.
        let pkg = package(
            "noegress",
            "  - kv:rw\n",
            "http.fetch",
            r#"{"url":"https://example.com/"}"#,
            false,
        );
        e.manager.install(&pkg, "admin", None).await.unwrap();
        e.manager
            .approve_capabilities("noegress", &approve(&["kv:rw"]), "admin")
            .await
            .unwrap();
        e.manager.enable("noegress").await.unwrap();
        assert_eq!(run(&e, "noegress").await["error"]["code"], "forbidden");
    }

    #[tokio::test]
    async fn plugins_cannot_see_each_others_data() {
        let e = env(Default::default()).await;
        for (id, op, req) in [
            ("writer", "kv.put", r#"{"key":"secret","value":"a"}"#),
            ("reader", "kv.get", r#"{"key":"secret"}"#),
        ] {
            e.manager
                .install(&package(id, "  - kv:rw\n", op, req, false), "admin", None)
                .await
                .unwrap();
            e.manager
                .approve_capabilities(id, &approve(&["kv:rw"]), "admin")
                .await
                .unwrap();
            e.manager.enable(id).await.unwrap();
        }
        run(&e, "writer").await;
        assert_eq!(
            e.storage.kv_get("writer", "secret").await.unwrap(),
            Some(json!("a"))
        );
        let out = run(&e, "reader").await;
        assert_eq!(out["ok"]["ok"]["value"], Value::Null);
    }

    #[tokio::test]
    async fn the_policy_ceiling_cannot_be_approved_around() {
        let e = env(PluginPolicyConfig {
            denied_capabilities: vec!["db:*".into()],
        })
        .await;
        let pkg = package(
            "pol",
            "  - db:own\n",
            "log.write",
            r#"{"message":"x"}"#,
            false,
        );
        e.manager.install(&pkg, "admin", None).await.unwrap();
        let err = e
            .manager
            .approve_capabilities("pol", &approve(&["db:own"]), "admin")
            .await
            .err()
            .unwrap();
        assert!(err.to_string().contains("policy"), "{err}");
        assert!(e.manager.enable("pol").await.is_err());
    }

    #[tokio::test]
    async fn uninstall_with_purge_removes_data_and_without_keeps_it() {
        let e = env(Default::default()).await;
        let caps = "  - kv:rw\n  - db:own\n";
        let pkg = package("keep", caps, "kv.put", r#"{"key":"k","value":1}"#, true);
        e.manager.install(&pkg, "admin", None).await.unwrap();
        e.manager
            .approve_capabilities("keep", &approve(&["kv:rw", "db:own"]), "admin")
            .await
            .unwrap();
        e.manager.enable("keep").await.unwrap();
        run(&e, "keep").await;
        let db_file = e.dir.join("db").join("keep.db");
        assert!(db_file.exists());
        e.manager.uninstall("keep", false).await.unwrap();
        assert_eq!(e.storage.kv_get("keep", "k").await.unwrap(), Some(json!(1)));
        assert!(db_file.exists());
        // Reinstalling finds the data again; approval starts from scratch.
        e.manager.install(&pkg, "admin", None).await.unwrap();
        assert!(e
            .manager
            .get("keep")
            .await
            .unwrap()
            .approvals
            .iter()
            .all(|a| !a.granted));
        e.manager
            .approve_capabilities("keep", &approve(&["kv:rw", "db:own"]), "admin")
            .await
            .unwrap();
        e.manager.enable("keep").await.unwrap();
        e.manager.uninstall("keep", true).await.unwrap();
        assert_eq!(e.storage.kv_get("keep", "k").await.unwrap(), None);
        assert!(!db_file.exists());
        assert_eq!(e.storage.schema_version("keep").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn an_invalid_module_cannot_be_enabled() {
        let e = env(Default::default()).await;
        let y = manifest("bad", "  - kv:rw\n", false);
        let pkg = build_signed_package(
            &[
                ("plugin.yaml", y.as_bytes()),
                ("plugin.wasm", b"\0asm\x01\0\0\0"),
            ],
            "bad",
            "1.0.0",
            2,
            SEED,
        );
        e.manager.install(&pkg, "admin", None).await.unwrap();
        e.manager
            .approve_capabilities("bad", &approve(&["kv:rw"]), "admin")
            .await
            .unwrap();
        let err = e.manager.enable("bad").await.err().unwrap();
        assert!(err.to_string().to_lowercase().contains("export"), "{err}");
    }
}
