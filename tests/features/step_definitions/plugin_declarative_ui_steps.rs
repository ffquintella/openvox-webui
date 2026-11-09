//! Step definitions for declarative plugin panels in the authentication
//! slots (H-6 / OQ-8). Real HTTP requests go through the API router and the
//! authentication middleware to a first-party plugin that mirrors the slots
//! and routes of the mfa-fido2 module with its example specs.

use std::sync::Arc;

use axum::{body::Body, http::Request};
use cucumber::{given, then, when};
use ed25519_dalek::SigningKey;
use openvox_webui::models::ApproveCapabilitiesRequest;
use openvox_webui::services::plugins::invoke::{HandlerCall, HandlerKind};
use openvox_webui::services::plugins::package::build_signed_package;
use openvox_webui::services::plugins::BuiltinPlugin;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::features::support::{TestResponse as WorldResponse, TestWorld};

// Loaded privately like in the other HTTP step files, hence `duplicate_mod`.
#[path = "../../common/test_app.rs"]
#[allow(dead_code, unused_imports, clippy::duplicate_mod)]
mod test_app;

use test_app::{generate_test_token_with_session, TestApp, TestResponse};

const SEED: [u8; 32] = [83; 32];
const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const PLUGIN: &str = "mfa-fido2";
const PROFILE: &str = include_str!("../../fixtures/plugins/mfa-fido2-ui/profile-security.json");
const ADMIN: &str = include_str!("../../fixtures/plugins/mfa-fido2-ui/admin-auth.json");

const ADMIN_READ: &str = "requires: { resource: plugin.mfa-fido2.admin, action: read }";
const ADMIN_WRITE: &str = "requires: { resource: plugin.mfa-fido2.admin, action: write }";

/// The slots and routes of the mfa-fido2 manifest (SPEC 11.2 and 12.2),
/// with `unsuspend` behind step-up (SPEC phase 2).
fn manifest() -> String {
    format!(
        "api_version: 2\nid: {PLUGIN}\nname: FIDO2 Security Keys\nversion: 1.0.0\npublisher: OpenVox\n\
         runtime: {{ kind: builtin, entry: {PLUGIN} }}\n\
         capabilities:\n  - api:routes\n  - ui:slot:profile.security\n  - ui:slot:admin.auth\n\
         contributes:\n  slots:\n    \
         - {{ slot: profile.security, id: security-keys, render: declarative, spec: ui/profile-security.json, title: Security keys, order: 100 }}\n    \
         - {{ slot: admin.auth, id: security-key-admin, render: declarative, spec: ui/admin-auth.json, title: Security keys (FIDO2), order: 100, {ADMIN_READ} }}\n  \
         api_routes:\n    \
         - {{ method: GET, path: /me/credentials }}\n    \
         - {{ method: PATCH, path: \"/me/credentials/{{credential_ref}}\" }}\n    \
         - {{ method: GET, path: /admin/credentials, {ADMIN_READ} }}\n    \
         - {{ method: GET, path: /admin/summary, {ADMIN_READ} }}\n    \
         - {{ method: GET, path: /admin/authenticators, {ADMIN_READ} }}\n    \
         - {{ method: POST, path: \"/admin/credentials/{{credential_ref}}/suspend\", {ADMIN_WRITE} }}\n    \
         - {{ method: POST, path: \"/admin/credentials/{{credential_ref}}/unsuspend\", {ADMIN_WRITE}, step_up: true }}\n    \
         - {{ method: POST, path: /admin/policy/preview, {ADMIN_READ} }}\n"
    )
}

/// A first-party plugin whose routes answer an empty list.
struct Keys;

impl BuiltinPlugin for Keys {
    fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
        if call.kind != HandlerKind::Route {
            return Ok(json!({}));
        }
        Ok(json!({"status": 200, "body": {"credentials": [], "items": []}}))
    }
}

/// What one scenario built and observed.
pub struct DeclarativeScenario {
    app: TestApp,
    alice: String,
    admin: String,
    last: Option<TestResponse>,
}

impl std::fmt::Debug for DeclarativeScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeclarativeScenario")
            .field("status", &self.last.as_ref().map(|r| r.status))
            .finish_non_exhaustive()
    }
}

fn scenario(world: &mut TestWorld) -> &mut DeclarativeScenario {
    world
        .declarative_ui
        .as_mut()
        .expect("the background builds the plugin host")
}

fn last_json(world: &mut TestWorld) -> Value {
    let r = scenario(world).last.as_ref().expect("a request was sent");
    serde_json::from_slice(&r.body).unwrap_or(Value::Null)
}

fn record(world: &mut TestWorld) {
    let r = scenario(world).last.as_ref().expect("a request was sent");
    let response = WorldResponse {
        status: r.status.as_u16(),
        body: serde_json::from_slice(&r.body).unwrap_or(Value::Null),
    };
    world.last_response = Some(response);
}

impl DeclarativeScenario {
    fn token(&self, who: &str) -> String {
        if who == "alice" {
            self.alice.clone()
        } else {
            self.admin.clone()
        }
    }

    async fn send(&mut self, method: &str, uri: &str, who: &str, body: Vec<u8>) {
        let r = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", format!("Bearer {}", self.token(who)))
            .body(Body::from(body))
            .unwrap();
        self.last = Some(self.app.request(r).await);
    }
}

fn package(manifest: &str, profile: &str, admin: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut files: Vec<(&str, &[u8])> = vec![
        ("plugin.yaml", manifest.as_bytes()),
        ("ui/profile-security.json", profile.as_bytes()),
        ("ui/admin-auth.json", admin.as_bytes()),
    ];
    files.extend_from_slice(extra);
    build_signed_package(&files, PLUGIN, "1.0.0", 2, SEED)
}

fn replaced(text: &str, from: &str, to: &str) -> String {
    assert!(
        text.contains(from),
        "the fixture no longer contains {from:?}"
    );
    text.replacen(from, to, 1)
}

/// The package for one preflight example: a single change to the manifest
/// or to one of the example specs.
fn changed_package(change: &str) -> Vec<u8> {
    let (mut m, mut profile, admin) = (manifest(), PROFILE.to_string(), ADMIN.to_string());
    let link = |target: &str| {
        replaced(
            PROFILE,
            "\"blocks\": [",
            &format!("\"blocks\": [{{\"layout\": \"markdown\", \"content\": \"See [the docs]({target}).\"}},"),
        )
    };
    let mut extra: Vec<(&str, &[u8])> = Vec::new();
    match change {
        "unchanged" => {}
        "a source route the plugin does not declare" => {
            profile = replaced(
                &profile,
                "\"route\": \"/me/credentials\", \"path\"",
                "\"route\": \"/admin/secrets\", \"path\"",
            )
        }
        "an action with another method" => {
            profile = replaced(&profile, "\"method\": \"PATCH\"", "\"method\": \"PUT\"")
        }
        "an unknown key \"html\"" => {
            profile = replaced(
                &profile,
                "\"layout\": \"table\",",
                "\"layout\": \"table\", \"html\": \"<b>x</b>\",",
            )
        }
        "an external markdown link" => profile = link("https://evil.example/login"),
        "a javascript markdown link" => profile = link("javascript:alert(1)"),
        "a danger action without a confirmation" => {
            profile = replaced(
                &profile,
                "\"id\": \"rename\",",
                "\"id\": \"rename\", \"danger\": true,",
            )
        }
        "a spec of 70 KiB" => {
            profile = json!({"layout": "markdown", "content": "a".repeat(70 * 1024)}).to_string()
        }
        "an iframe in profile.security" => {
            m = replaced(
                &m,
                "render: declarative, spec: ui/profile-security.json",
                "render: iframe, src: ui/panel.html",
            );
            extra.push(("ui/panel.html", b"<html></html>"));
        }
        "an admin.auth panel without requires" => {
            m = replaced(&m, &format!("order: 100, {ADMIN_READ} }}"), "order: 100 }");
        }
        other => panic!("unknown change '{other}'"),
    }
    package(&m, &profile, &admin, &extra)
}

// ---- background ------------------------------------------------------------------

#[given(
    expr = "a plugin host for declarative panels with the viewer {string} and an administrator"
)]
async fn given_host(world: &mut TestWorld, name: String) {
    let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
    let app = TestApp::with_plugins(move |m| {
        m.with_builtin_keys(vec![("OpenVox".into(), pk)])
            .with_builtin_runtime(PLUGIN, Arc::new(Keys))
    })
    .await;
    std::fs::create_dir_all(app.state.plugin_manager.data_dir()).unwrap();
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
    world.declarative_ui = Some(DeclarativeScenario {
        app,
        alice,
        admin,
        last: None,
    });
}

#[given("the security key plugin is enabled")]
async fn given_enabled(world: &mut TestWorld) {
    let s = scenario(world);
    let pkg = package(&manifest(), PROFILE, ADMIN, &[]);
    let m = &s.app.state.plugin_manager;
    m.install(&pkg, "admin", None).await.unwrap();
    m.approve_capabilities(
        PLUGIN,
        &ApproveCapabilitiesRequest {
            version: None,
            capabilities: vec![
                "api:routes".into(),
                "ui:slot:profile.security".into(),
                "ui:slot:admin.auth".into(),
            ],
        },
        "admin",
    )
    .await
    .unwrap();
    m.enable(PLUGIN).await.unwrap();
}

// ---- requests --------------------------------------------------------------------

#[when(regex = r"^(alice|the administrator) reads the slot catalog$")]
async fn when_catalog(world: &mut TestWorld, who: String) {
    scenario(world)
        .send("GET", "/api/v1/plugins/ui/catalog", &who, Vec::new())
        .await;
    record(world);
}

#[when(regex = r"^(alice|the administrator) lists the UI slots$")]
async fn when_slots(world: &mut TestWorld, who: String) {
    scenario(world)
        .send("GET", "/api/v1/plugins/ui/slots", &who, Vec::new())
        .await;
    record(world);
}

#[when(regex = r#"^(alice|the administrator) fetches the contribution "([^"]+)"$"#)]
async fn when_contribution(world: &mut TestWorld, who: String, id: String) {
    let uri = format!("/api/v1/plugins/ui/contributions/{PLUGIN}/{id}");
    scenario(world).send("GET", &uri, &who, Vec::new()).await;
    record(world);
}

#[when(regex = r#"^(alice|the administrator) requests ([A-Z]+) "([^"]+)" from the plugin$"#)]
async fn when_route(world: &mut TestWorld, who: String, method: String, uri: String) {
    scenario(world).send(&method, &uri, &who, Vec::new()).await;
    record(world);
}

#[when(regex = r"^the administrator preflights the security key plugin (?:with )?(.+)$")]
async fn when_preflight(world: &mut TestWorld, change: String) {
    let pkg = changed_package(&change);
    scenario(world)
        .send("POST", "/api/v1/plugins/preflight", "admin", pkg)
        .await;
    record(world);
}

// ---- outcomes --------------------------------------------------------------------

fn catalog_slot(world: &mut TestWorld, name: &str) -> Value {
    last_json(world)["slots"]
        .as_array()
        .and_then(|a| a.iter().find(|s| s["name"] == name).cloned())
        .unwrap_or_else(|| panic!("slot {name} is not in the catalog"))
}

#[then(expr = "the slot {string} should be mounted and declarative only")]
async fn then_mounted(world: &mut TestWorld, name: String) {
    let s = catalog_slot(world, &name);
    assert_eq!(s["mounted"], true, "{s}");
    assert_eq!(s["declarative_only"], true, "{s}");
    assert_eq!(s["requires_permission"], false, "{s}");
}

#[then(expr = "the slot {string} should be mounted, declarative only and gated by a permission")]
async fn then_mounted_gated(world: &mut TestWorld, name: String) {
    let s = catalog_slot(world, &name);
    assert_eq!(s["mounted"], true, "{s}");
    assert_eq!(s["declarative_only"], true, "{s}");
    assert_eq!(s["requires_permission"], true, "{s}");
}

#[then("the preflight should be accepted")]
async fn then_accepted(world: &mut TestWorld) {
    let status = scenario(world).last.as_ref().unwrap().status.as_u16();
    let body = last_json(world);
    assert_eq!(status, 200, "{body}");
}

#[then(expr = "the preflight should be refused with {string}")]
async fn then_refused(world: &mut TestWorld, problem: String) {
    let status = scenario(world).last.as_ref().unwrap().status.as_u16();
    let body = last_json(world);
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["code"], "invalid_manifest", "{body}");
    let found = body["details"].as_array().is_some_and(|d| {
        d.iter()
            .any(|p| p.as_str().is_some_and(|p| p.contains(&problem)))
    });
    assert!(found, "no problem contains {problem:?}: {body}");
}

#[then(regex = r#"^the slot "([^"]+)" should list "([^"]+)"$"#)]
async fn then_lists(world: &mut TestWorld, slot: String, id: String) {
    let body = last_json(world);
    let ids: Vec<&str> = body["slots"][&slot]
        .as_array()
        .map(|a| a.iter().filter_map(|c| c["id"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(ids, vec![id.as_str()], "{body}");
}

#[then(regex = r#"^the slot "([^"]+)" should list nothing$"#)]
async fn then_lists_nothing(world: &mut TestWorld, slot: String) {
    let body = last_json(world);
    let n = body["slots"][&slot].as_array().map_or(0, Vec::len);
    assert_eq!(n, 0, "{body}");
}

#[then(regex = r#"^the contribution spec should be the example "([a-z-]+)"$"#)]
async fn then_spec(world: &mut TestWorld, name: String) {
    let want: Value = serde_json::from_str(if name == "profile-security" {
        PROFILE
    } else {
        ADMIN
    })
    .unwrap();
    assert_eq!(last_json(world)["spec"], want);
}

#[then(regex = r#"^the route "([A-Z]+) ([^"]+)" should be (allowed|refused)( and need step-up)?$"#)]
async fn then_route(
    world: &mut TestWorld,
    method: String,
    path: String,
    verdict: String,
    step_up: String,
) {
    let body = last_json(world);
    let route = body["routes"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|r| r["method"] == method.as_str() && r["path"] == path.as_str())
                .cloned()
        })
        .unwrap_or_else(|| panic!("route {method} {path} is not listed: {body}"));
    assert_eq!(route["allowed"], verdict == "allowed", "{route}");
    assert_eq!(route["step_up"], !step_up.is_empty(), "{route}");
    assert!(route.get("requires").is_none(), "{route}");
}
