//! Step definitions for enrollment grace windows (OQ-12). They run the real
//! router with the authentication middleware and pipeline on a temporary
//! database, through the integration tests' `TestApp`.

use axum::{body::Body, http::Request};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use cucumber::{given, then, when};
use openvox_webui::services::auth_pipeline::{totp, AuthPipeline};
use openvox_webui::services::AuthService;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::features::support::TestWorld;

// Shared with the integration tests; its own `#[cfg(test)]` module does not
// run under the cucumber runner, hence `unused_imports`. The plugin route
// steps load the same file privately, hence `duplicate_mod`.
#[path = "../../common/test_app.rs"]
#[allow(dead_code, unused_imports, clippy::duplicate_mod)]
mod test_app;

use test_app::{generate_test_token_with_session, test_config, TestApp, TestResponse};

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const PASSWORD: &str = "correct horse battery";
const UA: &str = "bdd-grace";

/// What one scenario built and observed.
pub struct GraceScenario {
    app: TestApp,
    admin: String,
    username: String,
    user_id: Uuid,
    reset: Option<Value>,
    login: Option<(u16, Value)>,
    token: Option<String>,
    call: Option<(u16, Value)>,
    enrollment: Option<Value>,
}

impl std::fmt::Debug for GraceScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraceScenario")
            .field("username", &self.username)
            .field("login", &self.login)
            .field("call", &self.call)
            .finish_non_exhaustive()
    }
}

impl GraceScenario {
    async fn send(
        &self,
        token: Option<&str>,
        method: &str,
        uri: &str,
        body: Value,
    ) -> TestResponse {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("User-Agent", UA);
        if let Some(t) = token {
            b = b.header("Authorization", format!("Bearer {t}"));
        }
        self.app
            .request(b.body(Body::from(body.to_string())).unwrap())
            .await
    }

    async fn audit(&self, action: &str) -> Vec<Value> {
        let rows: Vec<Option<String>> =
            sqlx::query_scalar("SELECT details FROM audit_log WHERE action = ?")
                .bind(action)
                .fetch_all(&self.app.state.db)
                .await
                .unwrap();
        rows.into_iter()
            .map(|d| serde_json::from_str(&d.unwrap_or_default()).unwrap_or(Value::Null))
            .collect()
    }

    async fn windows(&self) -> Vec<(Option<String>, Option<String>, Option<String>)> {
        sqlx::query_as(
            "SELECT granted_by, closed_at, closed_reason FROM auth_enrollment_grace \
             WHERE user_id = ? ORDER BY created_at, rowid",
        )
        .bind(self.user_id.to_string())
        .fetch_all(&self.app.state.db)
        .await
        .unwrap()
    }

    fn token(&self) -> &str {
        self.token
            .as_deref()
            .expect("an enrollment-only session was issued")
    }
}

fn scenario(world: &mut TestWorld) -> &mut GraceScenario {
    world
        .enrollment_grace
        .as_mut()
        .expect("a server was started")
}

fn json_of(r: &TestResponse) -> Value {
    serde_json::from_slice(&r.body).unwrap_or(Value::Null)
}

fn claims_of(token: &str) -> Value {
    let payload = token.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ---- setup -----------------------------------------------------------------

async fn start(world: &mut TestWorld, secs: u64, on_policy: bool) {
    let mut config = test_config();
    config.auth.mfa.enrollment_grace_secs = secs;
    config.auth.mfa.enrollment_grace_on_policy = on_policy;
    let app = TestApp::with_config(config).await;
    let admin = generate_test_token_with_session(
        &app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await;
    world.enrollment_grace = Some(GraceScenario {
        app,
        admin,
        username: String::new(),
        user_id: Uuid::nil(),
        reset: None,
        login: None,
        token: None,
        call: None,
        enrollment: None,
    });
}

#[given(expr = "a server with an enrollment grace window of {int} seconds")]
async fn given_server(world: &mut TestWorld, secs: u64) {
    start(world, secs, false).await;
}

#[given(expr = "a server with an enrollment grace window of {int} seconds granted once by policy")]
async fn given_server_policy(world: &mut TestWorld, secs: u64) {
    start(world, secs, true).await;
}

#[given("a policy that requires a second factor for everyone")]
async fn given_policy(world: &mut TestWorld) {
    let s = scenario(world);
    let r = s
        .send(
            Some(&s.admin.clone()),
            "PUT",
            "/api/v1/auth/policies",
            json!({"policies": [{"name": "everyone", "conditions": {}, "required_acr": 2}]}),
        )
        .await;
    r.assert_ok();
}

async fn add_user(s: &mut GraceScenario, username: &str) {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role) VALUES (?, ?, ?, ?, 'viewer')",
    )
    .bind(id.to_string())
    .bind(username)
    .bind(format!("{username}@example.com"))
    .bind(AuthService::hash_password(PASSWORD).unwrap())
    .execute(&s.app.state.db)
    .await
    .unwrap();
    s.username = username.to_string();
    s.user_id = id;
}

#[given(expr = "the user {string} who never enrolled a factor")]
async fn given_user(world: &mut TestWorld, username: String) {
    add_user(scenario(world), &username).await;
}

#[given(expr = "the user {string} whose authenticator an administrator reset")]
async fn given_reset_user(world: &mut TestWorld, username: String) {
    let s = scenario(world);
    add_user(s, &username).await;
    AuthPipeline::new(&s.app.state.db, &s.app.state.config)
        .store()
        .insert(
            s.user_id,
            "totp",
            "Phone",
            &json!({"secret": "JBSWY3DPEHPK3PXP"}),
            true,
        )
        .await
        .unwrap();
    let r = s
        .send(
            Some(&s.admin.clone()),
            "POST",
            &format!("/api/v1/users/{}/factors/reset", s.user_id),
            json!({}),
        )
        .await;
    r.assert_ok();
    s.reset = Some(json_of(&r));
}

#[given(regex = r"^[a-z]+'s grace window has expired$")]
async fn given_expired(world: &mut TestWorld) {
    let s = scenario(world);
    sqlx::query(
        "UPDATE auth_enrollment_grace SET expires_at = '2000-01-01T00:00:00.000Z' WHERE user_id = ?",
    )
    .bind(s.user_id.to_string())
    .execute(&s.app.state.db)
    .await
    .unwrap();
}

// ---- actions ---------------------------------------------------------------

async fn log_in(s: &mut GraceScenario) {
    let r = s
        .send(
            None,
            "POST",
            "/api/v1/auth/login",
            json!({"username": s.username, "password": PASSWORD}),
        )
        .await;
    let body = json_of(&r);
    if body["status"] == "enrollment_required" {
        s.token = body["access_token"].as_str().map(String::from);
    }
    s.login = Some((r.status.as_u16(), body));
}

#[when(regex = r"^[a-z]+ logs in with (?:her|his|their) password$")]
async fn when_login(world: &mut TestWorld) {
    log_in(scenario(world)).await;
}

#[when(regex = r"^[a-z]+ logs in with (?:her|his|their) password (\d+) times$")]
async fn when_login_times(world: &mut TestWorld, times: usize) {
    let s = scenario(world);
    for _ in 0..times {
        log_in(s).await;
        assert_eq!(s.login.as_ref().unwrap().0, 202, "{:?}", s.login);
    }
}

#[when(regex = r#"^[a-z]+ calls "([A-Z]+) ([^"]+)" with that session$"#)]
async fn when_call(world: &mut TestWorld, method: String, uri: String) {
    let s = scenario(world);
    let token = s.token().to_string();
    let r = s.send(Some(&token), &method, &uri, json!({})).await;
    s.call = Some((r.status.as_u16(), json_of(&r)));
}

#[when(regex = r"^[a-z]+ enrolls an authenticator app with that session$")]
async fn when_enroll(world: &mut TestWorld) {
    let s = scenario(world);
    let token = s.token().to_string();
    let begin = s
        .send(
            Some(&token),
            "POST",
            "/api/v1/users/me/factors/totp/enroll/begin",
            json!({}),
        )
        .await;
    begin.assert_ok();
    let secret =
        totp::secret_from_base32(json_of(&begin)["enrollment"]["secret"].as_str().unwrap())
            .unwrap();
    let done = s
        .send(
            Some(&token),
            "POST",
            "/api/v1/users/me/factors/totp/enroll/complete",
            json!({ "code": totp::hotp(&secret, totp::step_at(now())) }),
        )
        .await;
    done.assert_ok();
    s.enrollment = Some(json_of(&done));
}

// ---- outcomes --------------------------------------------------------------

#[then("the reset answer should name the grace window expiry")]
async fn then_reset_expiry(world: &mut TestWorld) {
    let reset = scenario(world).reset.clone().unwrap();
    assert!(
        reset["enrollment_grace"]["expires_at"].is_string(),
        "{reset}"
    );
}

#[then("the reset answer should name no grace window")]
async fn then_reset_none(world: &mut TestWorld) {
    let reset = scenario(world).reset.clone().unwrap();
    assert!(reset["enrollment_grace"].is_null(), "{reset}");
}

#[then(regex = r"^[a-z]+ should have an open grace window granted by the administrator$")]
async fn then_open_window(world: &mut TestWorld) {
    let w = scenario(world).windows().await;
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].0.as_deref(), Some(ADMIN_ID));
    assert!(w[0].1.is_none(), "open");
}

#[then(regex = r"^[a-z]+ should have no grace window$")]
async fn then_no_window(world: &mut TestWorld) {
    assert!(scenario(world).windows().await.is_empty());
}

#[then(regex = r"^[a-z]+ should have (\d+) grace windows? in total$")]
async fn then_window_count(world: &mut TestWorld, count: usize) {
    assert_eq!(scenario(world).windows().await.len(), count);
}

#[then(regex = r#"^[a-z]+'s grace window should be closed as "([a-z]+)"$"#)]
async fn then_closed(world: &mut TestWorld, reason: String) {
    let w = scenario(world).windows().await;
    let last = w.last().expect("a window");
    assert!(last.1.is_some(), "closed");
    assert_eq!(last.2.as_deref(), Some(reason.as_str()));
}

#[then(expr = "the audit log should record {string} with the reason {string}")]
async fn then_audit_reason(world: &mut TestWorld, action: String, reason: String) {
    let entries = scenario(world).audit(&action).await;
    assert!(
        entries.iter().any(|d| d["reason"] == reason.as_str()),
        "{action}: {entries:?}"
    );
}

#[then(expr = "the audit log should record {string}")]
async fn then_audit(world: &mut TestWorld, action: String) {
    assert!(!scenario(world).audit(&action).await.is_empty(), "{action}");
}

#[then(expr = "the audit log should not record {string}")]
async fn then_no_audit(world: &mut TestWorld, action: String) {
    assert!(scenario(world).audit(&action).await.is_empty(), "{action}");
}

#[then(
    regex = r"^[a-z]+ should get an enrollment-only session at level 1 without a refresh token$"
)]
async fn then_enrollment_session(world: &mut TestWorld) {
    let s = scenario(world);
    let (status, body) = s.login.clone().unwrap();
    assert_eq!(status, 202, "{body}");
    assert_eq!(body["status"], "enrollment_required");
    assert_eq!(body["session_scope"], "enrollment");
    assert!(body.get("refresh_token").is_none());
    let claims = claims_of(s.token());
    assert_eq!(claims["acr"], 1);
    assert_eq!(claims["roles"], json!([]));
}

#[then(regex = r"^[a-z]+ should be offered a second factor challenge$")]
async fn then_challenge(world: &mut TestWorld) {
    let (status, body) = scenario(world).login.clone().unwrap();
    assert_eq!(status, 202, "{body}");
    assert_eq!(body["status"], "mfa_required");
    assert_eq!(body["factors"][0]["id"], "totp");
}

#[then(expr = "the login should be refused with {string}")]
async fn then_login_refused(world: &mut TestWorld, code: String) {
    let (status, body) = scenario(world).login.clone().unwrap();
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], code.as_str());
}

#[then("the call should succeed")]
async fn then_call_ok(world: &mut TestWorld) {
    let (status, body) = scenario(world).call.clone().unwrap();
    assert_eq!(status, 200, "{body}");
}

#[then(expr = "the call should be refused with {string}")]
async fn then_call_refused(world: &mut TestWorld, code: String) {
    let (status, body) = scenario(world).call.clone().unwrap();
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], code.as_str());
}

#[then(regex = r"^the enrollment should ask [a-z]+ to sign in again$")]
async fn then_reauthenticate(world: &mut TestWorld) {
    let e = scenario(world).enrollment.clone().unwrap();
    assert_eq!(e["reauthenticate"], true, "{e}");
    assert!(
        e["recovery"]["codes"].is_array(),
        "recovery codes come with it"
    );
}

#[then(regex = r"^[a-z]+'s enrollment-only session should be revoked$")]
async fn then_revoked(world: &mut TestWorld) {
    let s = scenario(world);
    let token = s.token().to_string();
    let r = s
        .send(Some(&token), "GET", "/api/v1/users/me/factors", json!({}))
        .await;
    assert_eq!(r.status.as_u16(), 401, "{}", r.text());
}
