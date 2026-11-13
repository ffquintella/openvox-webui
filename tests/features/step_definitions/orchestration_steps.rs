//! Step definitions for the OpenBolt orchestration API (M32). They run the
//! real router with the authentication middleware on a temporary database.

use axum::{body::Body, http::Request};
use cucumber::{given, then, when};
use openvox_webui::services::plugins::interceptors;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::features::support::TestWorld;

// Shared with the integration tests; see enrollment_grace_steps.rs.
#[path = "../../common/test_app.rs"]
#[allow(dead_code, unused_imports, clippy::duplicate_mod)]
mod test_app;

use test_app::{generate_test_token_with_session, test_config, TestApp};

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";

pub struct OrchestrationScenario {
    app: TestApp,
    admin: String,
    viewer: String,
    last: Option<(u16, Value)>,
}

impl std::fmt::Debug for OrchestrationScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationScenario")
            .field("last", &self.last)
            .finish_non_exhaustive()
    }
}

impl OrchestrationScenario {
    async fn send(&mut self, token: &str, method: &str, uri: &str, body: Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .unwrap();
        let r = self.app.request(req).await;
        let json = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
        self.last = Some((r.status.as_u16(), json));
    }

    fn last(&self) -> &(u16, Value) {
        self.last.as_ref().expect("a request was sent")
    }
}

fn scenario(world: &mut TestWorld) -> &mut OrchestrationScenario {
    world.orchestration.as_mut().expect("a server was started")
}

async fn start(world: &mut TestWorld, enabled: bool) {
    let mut config = test_config();
    config.orchestration.bolt.enabled = enabled;
    config.orchestration.bolt.binary_path = "/nonexistent/bolt".into();
    let app = TestApp::with_config(config).await;
    let admin = generate_test_token_with_session(
        &app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await;
    let viewer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role) \
         VALUES (?, 'vera', 'vera@example.com', 'x', 'viewer')",
    )
    .bind(viewer_id.to_string())
    .execute(&app.state.db)
    .await
    .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "vera", vec!["viewer".into()]).await;
    world.orchestration = Some(OrchestrationScenario {
        app,
        admin,
        viewer,
        last: None,
    });
}

#[given("an orchestration server with OpenBolt disabled")]
async fn given_disabled(world: &mut TestWorld) {
    start(world, false).await;
}

#[given("an orchestration server with OpenBolt enabled")]
async fn given_enabled(world: &mut TestWorld) {
    start(world, true).await;
}

#[when("the administrator asks for the OpenBolt status")]
async fn when_status(world: &mut TestWorld) {
    let s = scenario(world);
    let t = s.admin.clone();
    s.send(&t, "GET", "/api/v1/orchestration/bolt/status", json!({}))
        .await;
}

#[when("the viewer asks for the OpenBolt status")]
async fn when_viewer_status(world: &mut TestWorld) {
    let s = scenario(world);
    let t = s.viewer.clone();
    s.send(&t, "GET", "/api/v1/orchestration/bolt/status", json!({}))
        .await;
}

#[when("the viewer lists the OpenBolt jobs")]
async fn when_viewer_jobs(world: &mut TestWorld) {
    let s = scenario(world);
    let t = s.viewer.clone();
    s.send(&t, "GET", "/api/v1/orchestration/bolt/jobs", json!({}))
        .await;
}

fn run_body(kind: &str, name: &str, node: &str) -> Value {
    json!({"kind": kind, "name": name, "parameters": {}, "nodes": [node]})
}

#[when(expr = "the administrator requests a task run on {string}")]
async fn when_admin_run(world: &mut TestWorld, node: String) {
    let s = scenario(world);
    let t = s.admin.clone();
    s.send(
        &t,
        "POST",
        "/api/v1/orchestration/bolt/jobs",
        run_body("task", "package", &node),
    )
    .await;
}

#[when(expr = "the administrator requests a command run on {string}")]
async fn when_admin_command(world: &mut TestWorld, node: String) {
    let s = scenario(world);
    let t = s.admin.clone();
    s.send(
        &t,
        "POST",
        "/api/v1/orchestration/bolt/jobs",
        run_body("command", "uptime", &node),
    )
    .await;
}

#[when(expr = "the viewer requests a task run on {string}")]
async fn when_viewer_run(world: &mut TestWorld, node: String) {
    let s = scenario(world);
    let t = s.viewer.clone();
    s.send(
        &t,
        "POST",
        "/api/v1/orchestration/bolt/jobs",
        run_body("task", "package", &node),
    )
    .await;
}

#[when("the administrator allows the task \"package\" requiring approval")]
async fn when_allow_package(world: &mut TestWorld) {
    let s = scenario(world);
    let t = s.admin.clone();
    s.send(
        &t,
        "PUT",
        "/api/v1/orchestration/bolt/allowlist",
        json!({"entries": [{"kind": "task", "name": "package", "requires_approval": true}]}),
    )
    .await;
}

#[when(expr = "the administrator allows the {word} {string}")]
async fn when_allow(world: &mut TestWorld, kind: String, name: String) {
    let s = scenario(world);
    let t = s.admin.clone();
    s.send(
        &t,
        "PUT",
        "/api/v1/orchestration/bolt/allowlist",
        json!({"entries": [{"kind": kind, "name": name, "requires_approval": false}]}),
    )
    .await;
}

#[then(expr = "the answer should be {int} with state {string}")]
async fn then_status_state(world: &mut TestWorld, code: u16, state: String) {
    let (status, body) = scenario(world).last().clone();
    assert_eq!(status, code, "{body}");
    assert_eq!(body["state"], state.as_str(), "{body}");
}

#[then(expr = "the answer should be {int}")]
async fn then_status(world: &mut TestWorld, code: u16) {
    let (status, body) = scenario(world).last().clone();
    assert_eq!(status, code, "{body}");
}

#[then(expr = "the answer should not be {int}")]
async fn then_not_status(world: &mut TestWorld, code: u16) {
    let (status, body) = scenario(world).last().clone();
    assert_ne!(status, code, "{body}");
}

#[then("the allowlist should contain the task \"package\" requiring approval")]
async fn then_allowlist(world: &mut TestWorld) {
    let s = scenario(world);
    let t = s.admin.clone();
    s.send(&t, "GET", "/api/v1/orchestration/bolt/allowlist", json!({}))
        .await;
    let (status, body) = s.last().clone();
    assert_eq!(status, 200, "{body}");
    let entries = body["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1, "{body}");
    assert_eq!(entries[0]["kind"], "task");
    assert_eq!(entries[0]["name"], "package");
    assert_eq!(entries[0]["requires_approval"], true);
}

#[then(expr = "the orchestration audit log should record {string}")]
async fn then_audit(world: &mut TestWorld, action: String) {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action = ?")
        .bind(&action)
        .fetch_one(&scenario(world).app.state.db)
        .await
        .unwrap();
    assert!(n >= 1, "no audit entry '{action}'");
}

#[then(expr = "the interceptor catalog should list {string} failing closed")]
async fn then_catalog(_world: &mut TestWorld, event: String) {
    let e = interceptors::find(&event).expect("the event is in the catalog");
    assert_eq!(e.fail_mode, interceptors::FailMode::Closed);
    assert!(e.can_veto);
}
