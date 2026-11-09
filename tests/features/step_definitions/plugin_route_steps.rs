//! Step definitions for plugin API routes (H-7). Real HTTP requests go
//! through the API router and the authentication middleware of the
//! integration tests' `TestApp`, to a WASM plugin built from a WAT guest
//! ("keys") and to a first-party probe plugin that counts its calls.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use axum::{body::Body, http::Request};
use cucumber::{given, then, when};
use ed25519_dalek::SigningKey;
use openvox_webui::models::ApproveCapabilitiesRequest;
use openvox_webui::services::auth_pipeline::AuthPipeline;
use openvox_webui::services::plugins::host::{HostApiImpl, PackageModuleSource};
use openvox_webui::services::plugins::invoke::{HandlerCall, HandlerKind};
use openvox_webui::services::plugins::manifest::parse_and_validate;
use openvox_webui::services::plugins::package::build_signed_package;
use openvox_webui::services::plugins::storage::{PluginStorage, StorageLimits};
use openvox_webui::services::plugins::wasm::{WasmConfig, WasmInvoker};
use openvox_webui::services::plugins::BuiltinPlugin;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::features::support::{TestResponse as WorldResponse, TestWorld};

#[path = "../../common/test_app.rs"]
#[allow(dead_code, unused_imports)]
mod test_app;

use test_app::{generate_test_token_with_session, TestApp, TestResponse};

const SEED: [u8; 32] = [81; 32];
const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const COOKIE_SECRET: &str = "cookie-secret-value";

// ---- the probe plugin ----------------------------------------------------------

/// A first-party plugin that records the route handlers it ran.
#[derive(Default)]
pub struct Probe {
    calls: Mutex<Vec<String>>,
}

impl BuiltinPlugin for Probe {
    fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
        if call.kind != HandlerKind::Route {
            return Ok(json!({}));
        }
        self.calls.lock().unwrap().push(call.handler.clone());
        Ok(json!({"status": 200, "body": {"handled": call.handler}}))
    }
}

const PROBE_ROUTES: &str = "    - { method: GET, path: /admin/summary, requires: { resource: plugin.probe.admin, action: read } }\n    \
    - { method: POST, path: /sensitive, step_up: true }\n    \
    - { method: GET, path: /limited, rate_limit: 2/min }\n    \
    - { method: POST, path: /items }\n    \
    - { method: GET, path: \"/me/{x}\" }\n";

// ---- the WASM guest ------------------------------------------------------------

#[derive(Clone, Copy)]
enum Out {
    Json(&'static str),
    Echo,
    Trap,
    Spin,
    /// `audit.write` naming a forged user, then the JSON.
    Audit(&'static str),
}

const KEYS_ROUTES: &str =
    "    - { method: PATCH, path: \"/me/credentials/{credential_ref}\", handler: echo }\n    \
     - { method: POST, path: /conflict, handler: conflict }\n    \
     - { method: DELETE, path: \"/items/{id}\", handler: nocontent }\n    \
     - { method: GET, path: /redirect, handler: redirect }\n    \
     - { method: GET, path: /unauthorized, handler: unauthorized }\n    \
     - { method: GET, path: /headers, handler: headers }\n    \
     - { method: GET, path: /boom, handler: boom }\n    \
     - { method: GET, path: /spin, handler: spin }\n    \
     - { method: POST, path: /audit, handler: audit }\n";

const KEYS_HANDLERS: &[(char, Out)] = &[
    ('e', Out::Echo),
    (
        'c',
        Out::Json(r#"{"ok":{"status":409,"body":{"error":{"code":"already_suspended"}}}}"#),
    ),
    ('n', Out::Json(r#"{"ok":{"status":204}}"#)),
    (
        'r',
        Out::Json(r#"{"ok":{"status":302,"body":{"location":"https://evil.example"}}}"#),
    ),
    (
        'u',
        Out::Json(r#"{"ok":{"status":401,"body":{"code":"step_up_required"}}}"#),
    ),
    (
        'h',
        Out::Json(r#"{"ok":{"status":200,"body":{},"headers":{"Set-Cookie":"a=b"}}}"#),
    ),
    ('b', Out::Trap),
    ('s', Out::Spin),
    (
        'a',
        Out::Audit(r#"{"ok":{"status":201,"body":{"audited":true}}}"#),
    ),
];

const ECHO_PREFIX: &str = r#"{"ok":{"status":200,"body":"#;
const EMPTY_OK: &str = r#"{"ok":{}}"#;
const AUDIT_OP: &str = "audit.write";

fn wat_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn audit_request() -> String {
    format!(
        r#"{{"action":"probe","user_id":"{ADMIN_ID}","actor":"{ADMIN_ID}","details":{{"user_id":"{ADMIN_ID}"}}}}"#
    )
}

/// Dispatches on the handler's first letter, at offset 27 of
/// `{"kind":"route","handler":"`; lifecycle hooks answer `{}`. `Echo`
/// answers the whole guest request as the body.
fn route_guest(handlers: &[(char, Out)]) -> Vec<u8> {
    let mut data = String::new();
    let mut funcs = String::new();
    let mut dispatch = String::new();
    let pre = ECHO_PREFIX.len();
    let req = audit_request();
    for (i, (letter, out)) in handlers.iter().enumerate() {
        let off = 1024 + i * 4096;
        let mut fixed = |s: &str| {
            data += &format!("  (data (i32.const {off}) \"{}\")\n", wat_str(s));
            format!(
                "(i64.or (i64.shl (i64.const {off}) (i64.const 32)) (i64.const {}))",
                s.len()
            )
        };
        let body = match out {
            Out::Json(s) => fixed(s),
            Out::Trap => "(unreachable)".to_string(),
            Out::Spin => "(loop $spin (br $spin)) (unreachable)".to_string(),
            Out::Echo => format!(
                "(local.set $out (call $alloc (i32.add (local.get $l) (i32.const {n})))) \
                 (memory.copy (local.get $out) (i32.const 60000) (i32.const {pre})) \
                 (memory.copy (i32.add (local.get $out) (i32.const {pre})) (local.get $p) (local.get $l)) \
                 (memory.copy (i32.add (i32.add (local.get $out) (i32.const {pre})) (local.get $l)) (i32.const 60100) (i32.const 2)) \
                 (i64.or (i64.shl (i64.extend_i32_u (local.get $out)) (i64.const 32)) \
                         (i64.extend_i32_u (i32.add (local.get $l) (i32.const {n}))))",
                n = pre + 2
            ),
            Out::Audit(s) => format!(
                "(drop (call $hc (i32.const 60300) (i32.const {oplen}) (i32.const 60400) (i32.const {reqlen}))) {}",
                fixed(s),
                oplen = AUDIT_OP.len(),
                reqlen = req.len(),
            ),
        };
        funcs += &format!(
            "  (func $h{i} (param $p i32) (param $l i32) (result i64) (local $out i32) {body})\n"
        );
        dispatch += &format!(
            "    (if (i32.eq (local.get $c) (i32.const {})) (then (return (call $h{i} (local.get $p) (local.get $l)))))\n",
            *letter as u32
        );
    }
    let wat = format!(
        r#"(module
  (import "openvox" "host_call" (func $hc (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 4)
  (global $bump (mut i32) (i32.const 131072))
{data}  (data (i32.const 60000) "{prefix}")
  (data (i32.const 60100) "}}}}")
  (data (i32.const 60200) "{empty}")
  (data (i32.const 60300) "{op}")
  (data (i32.const 60400) "{req}")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $r i32)
    (local.set $r (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $r))
{funcs}  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    (local $c i32)
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 9))) (i32.const 108))
      (then (return (i64.or (i64.shl (i64.const 60200) (i64.const 32)) (i64.const {empty_len})))))
    (local.set $c (i32.load8_u (i32.add (local.get $p) (i32.const 27))))
{dispatch}    (unreachable)))"#,
        prefix = wat_str(ECHO_PREFIX),
        empty = wat_str(EMPTY_OK),
        empty_len = EMPTY_OK.len(),
        op = AUDIT_OP,
        req = wat_str(&req),
    );
    wat::parse_str(wat).unwrap()
}

// ---- the scenario --------------------------------------------------------------

/// What one scenario built and observed.
pub struct RouteScenario {
    app: TestApp,
    probe: Arc<Probe>,
    alice_id: Uuid,
    alice: String,
    admin: String,
    last: Option<TestResponse>,
    manifest: Option<Vec<String>>,
}

impl std::fmt::Debug for RouteScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteScenario")
            .field("status", &self.last.as_ref().map(|r| r.status))
            .field("manifest", &self.manifest)
            .finish_non_exhaustive()
    }
}

fn scenario(world: &mut TestWorld) -> &mut RouteScenario {
    world
        .plugin_routes
        .as_mut()
        .expect("the background builds the plugin host")
}

fn last(world: &mut TestWorld) -> &TestResponse {
    scenario(world).last.as_ref().expect("a request was sent")
}

fn last_json(world: &mut TestWorld) -> Value {
    serde_json::from_slice(&last(world).body).unwrap_or(Value::Null)
}

/// The input the echo handler received: the body is the whole guest request.
fn echoed_input(world: &mut TestWorld) -> Value {
    last_json(world)["input"].clone()
}

impl RouteScenario {
    fn token(&self, who: &str) -> &str {
        if who == "alice" {
            &self.alice
        } else {
            &self.admin
        }
    }

    async fn send(
        &mut self,
        method: &str,
        uri: &str,
        token: &str,
        headers: &[(&str, &str)],
        body: Vec<u8>,
    ) {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"));
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        let r = self.app.request(b.body(Body::from(body)).unwrap()).await;
        self.last = Some(r);
    }

    async fn install(&self, id: &str, runtime: &str, routes: &str, files: &[(&str, &[u8])]) {
        let y = format!(
            "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
             runtime: {runtime}\ncapabilities:\n  - api:routes\n  - audit:write\n\
             contributes:\n  api_routes:\n{routes}"
        );
        let mut all: Vec<(&str, &[u8])> = vec![("plugin.yaml", y.as_bytes())];
        all.extend_from_slice(files);
        let pkg = build_signed_package(&all, id, "1.0.0", 2, SEED);
        let m = &self.app.state.plugin_manager;
        m.install(&pkg, "admin", None).await.unwrap();
        m.approve_capabilities(
            id,
            &ApproveCapabilitiesRequest {
                version: None,
                capabilities: vec!["api:routes".into(), "audit:write".into()],
            },
            "admin",
        )
        .await
        .unwrap();
        m.enable(id).await.unwrap();
    }
}

fn record(world: &mut TestWorld) {
    let r = last(world);
    let response = WorldResponse {
        status: r.status.as_u16(),
        body: serde_json::from_slice(&r.body).unwrap_or(Value::Null),
    };
    world.last_response = Some(response);
}

// ---- background and plugins ----------------------------------------------------

#[given(expr = "a plugin host serving API routes with the viewer {string} and an administrator")]
async fn given_host(world: &mut TestWorld, name: String) {
    let probe = Arc::new(Probe::default());
    let runtime = probe.clone();
    let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
    let app = TestApp::with_plugins(move |m| {
        m.with_builtin_keys(vec![("OpenVox".into(), pk)])
            .with_builtin_runtime("probe", runtime)
    })
    .await;
    let manager = app.state.plugin_manager.clone();
    let dir = manager.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(PluginStorage::new(
        app.state.db.clone(),
        dir.clone(),
        StorageLimits::default(),
    ));
    let host = Arc::new(HostApiImpl::new(
        &manager,
        Default::default(),
        storage.clone(),
        None,
    ));
    let source = Arc::new(PackageModuleSource {
        data_dir: dir,
        max_package_bytes: 8 * 1024 * 1024,
    });
    let wasm = Arc::new(WasmInvoker::new(WasmConfig::default(), source, host).unwrap());
    manager.attach_wasm(storage, wasm);

    let alice_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role) \
         VALUES (?, ?, 'alice@example.com', 'x', 'viewer')",
    )
    .bind(alice_id.to_string())
    .bind(&name)
    .execute(&app.state.db)
    .await
    .unwrap();
    let alice =
        generate_test_token_with_session(&app, alice_id, &name, vec!["viewer".into()]).await;
    let admin = generate_test_token_with_session(
        &app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await;
    world.plugin_routes = Some(RouteScenario {
        app,
        probe,
        alice_id,
        alice,
        admin,
        last: None,
        manifest: None,
    });
}

#[given(expr = "the WASM route plugin {string} is enabled")]
async fn given_wasm_plugin(world: &mut TestWorld, id: String) {
    let wasm = route_guest(KEYS_HANDLERS);
    scenario(world)
        .install(
            &id,
            "{ kind: wasm, entry: plugin.wasm, limits: { timeout_ms: 300 } }",
            KEYS_ROUTES,
            &[("plugin.wasm", &wasm)],
        )
        .await;
}

#[given(expr = "the builtin route plugin {string} is enabled")]
async fn given_builtin_plugin(world: &mut TestWorld, id: String) {
    scenario(world)
        .install(
            &id,
            &format!("{{ kind: builtin, entry: {id} }}"),
            PROBE_ROUTES,
            &[],
        )
        .await;
}

#[given("alice has enrolled an authenticator app")]
async fn given_totp(world: &mut TestWorld) {
    let s = scenario(world);
    AuthPipeline::new(&s.app.state.db, &s.app.state.config)
        .store()
        .insert(
            s.alice_id,
            "totp",
            "Phone",
            &json!({"secret": "JBSWY3DP"}),
            true,
        )
        .await
        .unwrap();
}

#[when("alice's session reaches the MFA level")]
async fn when_mfa_level(world: &mut TestWorld) {
    let s = scenario(world);
    sqlx::query("UPDATE auth_sessions SET acr = 2, amr = '[\"pwd\",\"otp\"]' WHERE user_id = ?")
        .bind(s.alice_id.to_string())
        .execute(&s.app.state.db)
        .await
        .unwrap();
}

#[when(expr = "the route plugin {string} is disabled")]
async fn when_disabled(world: &mut TestWorld, id: String) {
    scenario(world)
        .app
        .state
        .plugin_manager
        .disable(&id)
        .await
        .unwrap();
}

#[when(expr = "the route plugin {string} is uninstalled")]
async fn when_uninstalled(world: &mut TestWorld, id: String) {
    scenario(world)
        .app
        .state
        .plugin_manager
        .uninstall(&id, true)
        .await
        .unwrap();
}

#[when(expr = "the package of {string} fails its integrity check")]
async fn when_tampered(world: &mut TestWorld, id: String) {
    let m = scenario(world).app.state.plugin_manager.clone();
    let path = m.data_dir().join("packages").join(&id).join("1.0.0.ovxp");
    std::fs::write(path, b"tampered").unwrap();
    assert!(m.verify(&id).await.is_err());
}

// ---- requests ------------------------------------------------------------------

#[when(regex = r#"^(alice|the administrator) calls ([A-Z]+) "([^"]*)"$"#)]
async fn when_calls(world: &mut TestWorld, who: String, method: String, uri: String) {
    let s = scenario(world);
    let token = s.token(&who).to_string();
    s.send(&method, &uri, &token, &[], Vec::new()).await;
    record(world);
}

#[when(regex = r#"^(alice|the administrator) calls ([A-Z]+) "([^"]*)" (\d+) times$"#)]
async fn when_calls_n(world: &mut TestWorld, who: String, method: String, uri: String, n: usize) {
    let s = scenario(world);
    let token = s.token(&who).to_string();
    for _ in 0..n {
        s.send(&method, &uri, &token, &[], Vec::new()).await;
        assert_eq!(s.last.as_ref().unwrap().status.as_u16(), 200);
    }
    record(world);
}

#[when(
    regex = r#"^alice calls ([A-Z]+) "([^"]*)" with her token in the query, a cookie and the body:$"#
)]
async fn when_calls_with_credentials(
    world: &mut TestWorld,
    step: &cucumber::gherkin::Step,
    method: String,
    uri: String,
) {
    let body = step.docstring.clone().unwrap_or_default();
    let s = scenario(world);
    let token = s.alice.clone();
    let sep = if uri.contains('?') { '&' } else { '?' };
    let uri = format!("{uri}{sep}token={token}");
    let cookie = format!("ovx_session={token}; secret={COOKIE_SECRET}");
    let headers = [
        ("Content-Type", "application/json"),
        ("Cookie", cookie.as_str()),
        ("X-Forwarded-For", "203.0.113.9"),
    ];
    s.send(
        &method,
        &uri,
        &token,
        &headers,
        body.trim().as_bytes().to_vec(),
    )
    .await;
    record(world);
}

#[when(regex = r#"^alice calls ([A-Z]+) "([^"]*)" with the body:$"#)]
async fn when_calls_with_body(
    world: &mut TestWorld,
    step: &cucumber::gherkin::Step,
    method: String,
    uri: String,
) {
    let body = step.docstring.clone().unwrap_or_default();
    let s = scenario(world);
    let token = s.alice.clone();
    let headers = [("Content-Type", "application/json")];
    s.send(
        &method,
        &uri,
        &token,
        &headers,
        body.trim().as_bytes().to_vec(),
    )
    .await;
    record(world);
}

#[when(regex = r#"^alice calls POST "([^"]*)" with a body of (\d+) KiB$"#)]
async fn when_calls_large(world: &mut TestWorld, uri: String, kib: usize) {
    let s = scenario(world);
    let token = s.alice.clone();
    let body = format!("{{\"x\":\"{}\"}}", "a".repeat(kib * 1024));
    let headers = [("Content-Type", "application/json")];
    s.send("POST", &uri, &token, &headers, body.into_bytes())
        .await;
    record(world);
}

#[when(regex = r#"^alice calls POST "([^"]*)" with a text/plain body$"#)]
async fn when_calls_text(world: &mut TestWorld, uri: String) {
    let s = scenario(world);
    let token = s.alice.clone();
    let headers = [("Content-Type", "text/plain")];
    s.send("POST", &uri, &token, &headers, b"hello".to_vec())
        .await;
    record(world);
}

// ---- manifests -----------------------------------------------------------------

const MFA_FIDO2_ROUTES: &str = "    - { method: GET,   path: /me/credentials, rate_limit: 60/min }\n    \
    - { method: PATCH, path: \"/me/credentials/{credential_ref}\", rate_limit: 10/min }\n    \
    - { method: GET, path: /admin/credentials, requires: { resource: plugin.mfa-fido2.admin, action: read }, rate_limit: 60/min }\n    \
    - { method: GET, path: /admin/summary, requires: { resource: plugin.mfa-fido2.admin, action: read }, rate_limit: 60/min }\n    \
    - { method: GET, path: /admin/authenticators, requires: { resource: plugin.mfa-fido2.admin, action: read }, rate_limit: 60/min }\n    \
    - { method: POST, path: \"/admin/credentials/{credential_ref}/suspend\", requires: { resource: plugin.mfa-fido2.admin, action: write }, rate_limit: 20/min }\n    \
    - { method: POST, path: \"/admin/credentials/{credential_ref}/unsuspend\", requires: { resource: plugin.mfa-fido2.admin, action: write }, rate_limit: 20/min, step_up: true }\n    \
    - { method: POST, path: /admin/policy/preview, requires: { resource: plugin.mfa-fido2.admin, action: read }, rate_limit: 10/min }\n";

fn quoted(s: &str) -> String {
    s.split('"').nth(1).unwrap_or_default().to_string()
}

#[when(regex = r"^I validate a route plugin manifest with (.+)$")]
async fn when_validate(world: &mut TestWorld, change: String) {
    let mut id = "keys".to_string();
    let mut runtime = "{ kind: builtin, entry: keys }".to_string();
    let mut caps = "  - api:routes\n".to_string();
    let mut routes = "    - { method: GET, path: /me/credentials }\n".to_string();
    let route = |extra: &str| format!("    - {{ method: GET, path: /x{extra} }}\n");
    match change.as_str() {
        "no api:routes capability" => caps = "  - kv:rw\n".into(),
        "an optional api:routes" => caps = "  - api:routes: { optional: true }\n".into(),
        "the remote runtime" => {
            runtime = "{ kind: remote, endpoint: \"https://x.example\" }".into()
        }
        "a headers key" => routes = route(", headers: { X-A: b }"),
        "the mfa-fido2 routes" => {
            id = "mfa-fido2".into();
            routes = MFA_FIDO2_ROUTES.into();
        }
        c if c.starts_with("the path ") => {
            routes = format!("    - {{ method: GET, path: \"{}\" }}\n", quoted(c))
        }
        c if c.starts_with("the plugin id ") => id = quoted(c),
        c if c.starts_with("the method ") => {
            routes = format!("    - {{ method: {}, path: /x }}\n", quoted(c))
        }
        c if c.starts_with("the rate limit ") => {
            routes = route(&format!(", rate_limit: \"{}\"", quoted(c)))
        }
        c if c.starts_with("the permission ") => {
            let q = quoted(c);
            let (res, act) = q.split_once(':').unwrap();
            routes = route(&format!(", requires: {{ resource: {res}, action: {act} }}"))
        }
        other => panic!("unknown change: {other}"),
    }
    let y = format!(
        "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
         runtime: {runtime}\ncapabilities:\n{caps}contributes:\n  api_routes:\n{routes}"
    );
    let problems = parse_and_validate(y.as_bytes(), &BTreeSet::new())
        .err()
        .unwrap_or_default();
    scenario(world).manifest = Some(problems);
}

#[then(expr = "the route manifest should be refused with {string}")]
async fn then_refused(world: &mut TestWorld, problem: String) {
    let p = scenario(world).manifest.clone().expect("a manifest");
    assert!(
        p.iter().any(|m| m.contains(&problem)),
        "want {problem:?} in {p:?}"
    );
}

#[then("the route manifest should be accepted")]
async fn then_accepted(world: &mut TestWorld) {
    let p = scenario(world).manifest.clone().expect("a manifest");
    assert!(p.is_empty(), "{p:?}");
}

// ---- responses -----------------------------------------------------------------

#[then(expr = "the route response code should be {string}")]
async fn then_code(world: &mut TestWorld, code: String) {
    let body = last_json(world);
    assert_eq!(body["code"], code, "{body}");
}

#[then("the route response should not be cached")]
async fn then_no_store(world: &mut TestWorld) {
    let r = last(world);
    assert_eq!(r.headers["cache-control"], "no-store");
    assert_eq!(r.headers["x-content-type-options"], "nosniff");
}

#[then("the route response should have a Retry-After header")]
async fn then_retry_after(world: &mut TestWorld) {
    let secs: u64 = last(world).headers["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=60).contains(&secs), "{secs}");
}

#[then(regex = r"^the route response body should be (.+)$")]
async fn then_body(world: &mut TestWorld, expected: String) {
    let r = last(world);
    assert!(r.headers.get("set-cookie").is_none());
    match expected.as_str() {
        "empty" => assert!(r.body.is_empty(), "{:?}", r.body),
        "the generic plugin error" => {
            assert_eq!(r.status.as_u16(), 503);
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(
                body,
                json!({
                    "error": "service_unavailable",
                    "message": "The plugin could not handle the request",
                    "code": "plugin_unavailable",
                })
            );
        }
        quoted_json => {
            let want: Value =
                serde_json::from_str(quoted_json.trim_matches('\'')).expect("JSON in quotes");
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(body, want);
        }
    }
}

#[then(expr = "the route response should not mention {string}")]
async fn then_not_mention(world: &mut TestWorld, text: String) {
    let body = String::from_utf8_lossy(&last(world).body).to_lowercase();
    assert!(!body.contains(&text.to_lowercase()), "{body}");
}

// ---- what the plugin received --------------------------------------------------

#[then(expr = "the plugin should have received the handler {string} of kind {string}")]
async fn then_handler(world: &mut TestWorld, handler: String, kind: String) {
    let request = last_json(world);
    assert_eq!(request["handler"], handler);
    assert_eq!(request["kind"], kind);
}

#[then(expr = "the plugin input should name alice with the roles {string}")]
async fn then_identity(world: &mut TestWorld, roles: String) {
    let alice_id = scenario(world).alice_id;
    let user = echoed_input(world)["user"].clone();
    let roles: Vec<&str> = roles.split(',').collect();
    assert_eq!(
        user,
        json!({
            "id": alice_id,
            "username": "alice",
            "organization_id": openvox_webui::models::default_organization_uuid(),
            "roles": roles,
        })
    );
}

#[then(
    regex = r"^the plugin input should have the params '(.*)', the query '(.*)' and the body '(.*)'$"
)]
async fn then_request_parts(world: &mut TestWorld, params: String, query: String, body: String) {
    let input = echoed_input(world);
    let parse = |s: &str| serde_json::from_str::<Value>(s).unwrap();
    assert_eq!(input["params"], parse(&params));
    assert_eq!(input["query"], parse(&query));
    assert_eq!(input["body"], parse(&body));
}

#[then(expr = "the plugin input should hold only {string}")]
async fn then_only_keys(world: &mut TestWorld, keys: String) {
    let input = echoed_input(world);
    let got: BTreeSet<&str> = input
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = keys.split(',').collect();
    assert_eq!(got, want);
}

#[then("the plugin input should not contain alice's token, the cookie or any header")]
async fn then_no_credentials(world: &mut TestWorld) {
    let token = scenario(world).alice.clone();
    let everything = String::from_utf8_lossy(&last(world).body).to_string();
    let lower = everything.to_lowercase();
    assert!(!everything.contains(&token), "the token reached the plugin");
    let session_id = Uuid::parse_str(
        token
            .split('.')
            .nth(1)
            .and_then(|p| {
                use base64::Engine;
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(p)
                    .ok()
            })
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|c| c["jti"].as_str().map(str::to_string))
            .unwrap()
            .as_str(),
    )
    .unwrap();
    for needle in [
        COOKIE_SECRET,
        "authorization",
        "bearer",
        "cookie",
        "x-forwarded-for",
        "203.0.113.9",
        "headers",
        &session_id.to_string(),
    ] {
        assert!(
            !lower.contains(&needle.to_lowercase()),
            "{needle:?} reached the plugin: {everything}"
        );
    }
}

#[then(expr = "the probe should have handled {int} calls")]
async fn then_probe_calls(world: &mut TestWorld, n: usize) {
    let calls = scenario(world).probe.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), n, "{calls:?}");
}

// ---- audit ---------------------------------------------------------------------

#[then(expr = "the audit entry {string} should name alice as the user")]
async fn then_audit_actor(world: &mut TestWorld, action: String) {
    let s = scenario(world);
    let users: Vec<Option<String>> =
        sqlx::query_scalar("SELECT user_id FROM audit_log WHERE action = ?")
            .bind(&action)
            .fetch_all(&s.app.state.db)
            .await
            .unwrap();
    assert_eq!(users, vec![Some(s.alice_id.to_string())]);
}

#[then(expr = "the host should have audited {string} for alice with the status {int}")]
async fn then_host_audit(world: &mut TestWorld, action: String, status: u16) {
    let s = scenario(world);
    let rows: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT user_id, details FROM audit_log WHERE action = ?")
            .bind(&action)
            .fetch_all(&s.app.state.db)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0.as_deref(), Some(s.alice_id.to_string().as_str()));
    let details: Value = serde_json::from_str(rows[0].1.as_deref().unwrap()).unwrap();
    assert_eq!(details["status"], status);
    assert_eq!(details["route"], "/audit");
}
