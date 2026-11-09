//! Integration tests for the authentication pipeline (M29): challenges, TOTP,
//! recovery codes, MFA policies and step-up.

use axum::{body::Body, http::Request};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use openvox_webui::services::{
    auth_pipeline::{totp, MfaConfig},
    AuthService,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::common::*;

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const PASSWORD: &str = "correct horse battery";

async fn prepare_admin(app: &TestApp) -> String {
    let hash = AuthService::hash_password(PASSWORD).unwrap();
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(ADMIN_ID)
        .execute(&app.state.db)
        .await
        .unwrap();
    generate_test_token_with_session(
        app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await
}

async fn add_viewer(app: &TestApp) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let hash = AuthService::hash_password(PASSWORD).unwrap();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', ?, 'viewer')")
        .bind(id.to_string())
        .bind(hash)
        .execute(&app.state.db)
        .await
        .unwrap();
    let token = generate_test_token_with_session(app, id, "viewer", vec!["viewer".into()]).await;
    (id, token)
}

async fn call(
    app: &TestApp,
    token: Option<&str>,
    ua: &str,
    method: &str,
    uri: &str,
    body: Value,
) -> TestResponse {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("User-Agent", ua);
    if let Some(t) = token {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    app.request(b.body(Body::from(body.to_string())).unwrap())
        .await
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

/// Enrolls TOTP for the token's user and returns the seed and recovery codes.
async fn enroll_totp(app: &TestApp, token: &str) -> (Vec<u8>, Vec<String>) {
    let begin = call(
        app,
        Some(token),
        "t",
        "POST",
        "/api/v1/users/me/factors/totp/enroll/begin",
        json!({}),
    )
    .await;
    begin.assert_ok();
    let b: Value = begin.json();
    let secret = totp::secret_from_base32(b["enrollment"]["secret"].as_str().unwrap()).unwrap();
    assert!(b["enrollment"]["otpauth_uri"]
        .as_str()
        .unwrap()
        .starts_with("otpauth://totp/"));
    let code = totp::hotp(&secret, totp::step_at(now()));
    let done = call(
        app,
        Some(token),
        "t",
        "POST",
        "/api/v1/users/me/factors/totp/enroll/complete",
        json!({ "code": code }),
    )
    .await;
    done.assert_ok();
    let d: Value = done.json();
    let codes = d["recovery"]["codes"]
        .as_array()
        .expect("first factor also returns recovery codes")
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    (secret, codes)
}

async fn require_mfa_for_everyone(app: &TestApp, token: &str) {
    call(
        app,
        Some(token),
        "t",
        "PUT",
        "/api/v1/auth/policies",
        json!({"policies": [{"name": "everyone", "conditions": {}, "required_acr": 2}]}),
    )
    .await
    .assert_ok();
}

async fn login(app: &TestApp, ua: &str) -> TestResponse {
    call(
        app,
        None,
        ua,
        "POST",
        "/api/v1/auth/login",
        json!({"username": "admin", "password": PASSWORD}),
    )
    .await
}

async fn audit_count(app: &TestApp, action: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action = ?")
        .bind(action)
        .fetch_one(&app.state.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn login_without_a_policy_still_returns_a_session() {
    let app = TestApp::new().await;
    prepare_admin(&app).await;
    let r = login(&app, "ua").await;
    r.assert_ok();
    let body: Value = r.json();
    let claims = claims_of(body["access_token"].as_str().unwrap());
    assert_eq!(claims["acr"], 1);
    assert_eq!(claims["amr"], json!(["pwd"]));
}

#[tokio::test]
async fn providers_endpoint_is_public_and_describes_factors() {
    let app = TestApp::new().await;
    let r = app.get("/api/v1/auth/providers").await;
    r.assert_ok();
    let body: Value = r.json();
    assert_eq!(body["providers"][0]["id"], "local");
    let ids: Vec<&str> = body["factors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"totp") && ids.contains(&"recovery"));
    assert!(
        !ids.contains(&"webauthn"),
        "needs a relying party configured"
    );
}

#[tokio::test]
async fn policy_turns_login_into_a_challenge_that_totp_satisfies() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    require_mfa_for_everyone(&app, &admin).await;
    let (secret, _) = enroll_totp(&app, &admin).await;

    let r = login(&app, "ua").await;
    r.assert_status(axum::http::StatusCode::ACCEPTED);
    let body: Value = r.json();
    assert!(
        body.get("access_token").is_none(),
        "no session before the factor"
    );
    let id = body["challenge_id"].as_str().unwrap().to_string();
    assert_eq!(body["factors"][0]["id"], "totp");

    // The enrollment code is spent; the next step is inside the window.
    let code = totp::hotp(&secret, totp::step_at(now()) + 1);
    let verify = call(
        &app,
        None,
        "ua",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"factor_id": "totp", "response": {"code": code}}),
    )
    .await;
    verify.assert_ok();
    let session: Value = verify.json();
    let claims = claims_of(session["access_token"].as_str().unwrap());
    assert_eq!(claims["acr"], 2);
    assert_eq!(claims["amr"], json!(["pwd", "otp"]));

    // A challenge is single use.
    call(
        &app,
        None,
        "ua",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"factor_id": "totp", "response": {"code": code}}),
    )
    .await
    .assert_unauthorized();
    assert!(audit_count(&app, "auth.challenge_verified").await >= 1);
}

#[tokio::test]
async fn a_totp_code_cannot_be_replayed() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    require_mfa_for_everyone(&app, &admin).await;
    let (secret, _) = enroll_totp(&app, &admin).await;
    let code = totp::hotp(&secret, totp::step_at(now()) + 1);

    for expected_ok in [true, false] {
        let c: Value = login(&app, "ua").await.json();
        let id = c["challenge_id"].as_str().unwrap();
        let r = call(
            &app,
            None,
            "ua",
            "POST",
            &format!("/api/v1/auth/challenge/{id}/verify"),
            json!({"factor_id": "totp", "response": {"code": code}}),
        )
        .await;
        if expected_ok {
            r.assert_ok();
        } else {
            r.assert_unauthorized();
        }
    }
}

#[tokio::test]
async fn challenge_is_bound_to_the_client_and_burns_after_failures() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    require_mfa_for_everyone(&app, &admin).await;
    let (secret, _) = enroll_totp(&app, &admin).await;

    // Another client cannot use the challenge.
    let c: Value = login(&app, "browser-a").await.json();
    let id = c["challenge_id"].as_str().unwrap();
    let good = totp::hotp(&secret, totp::step_at(now()) + 1);
    let r = call(
        &app,
        None,
        "browser-b",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"factor_id": "totp", "response": {"code": good}}),
    )
    .await;
    r.assert_unauthorized();
    assert_eq!(r.json::<Value>()["code"], "invalid_challenge");

    // Wrong codes burn it, and then even the right code fails.
    let c: Value = login(&app, "browser-a").await.json();
    let id = c["challenge_id"].as_str().unwrap();
    for _ in 0..5 {
        let r = call(
            &app,
            None,
            "browser-a",
            "POST",
            &format!("/api/v1/auth/challenge/{id}/verify"),
            json!({"factor_id": "totp", "response": {"code": "000000"}}),
        )
        .await;
        r.assert_unauthorized();
    }
    call(
        &app,
        None,
        "browser-a",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"factor_id": "totp", "response": {"code": good}}),
    )
    .await
    .assert_unauthorized();
}

#[tokio::test]
async fn recovery_codes_work_once_and_are_audited() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    require_mfa_for_everyone(&app, &admin).await;
    let (_, codes) = enroll_totp(&app, &admin).await;
    assert_eq!(codes.len(), 10);

    let mut results = vec![];
    for _ in 0..2 {
        let c: Value = login(&app, "ua").await.json();
        let id = c["challenge_id"].as_str().unwrap();
        let r = call(
            &app,
            None,
            "ua",
            "POST",
            &format!("/api/v1/auth/challenge/{id}/verify"),
            json!({"factor_id": "recovery", "response": {"code": codes[0].to_lowercase()}}),
        )
        .await;
        results.push(r);
    }
    results[0].assert_ok();
    results[1].assert_unauthorized();
    let claims = claims_of(results[0].json::<Value>()["access_token"].as_str().unwrap());
    assert_eq!(claims["amr"], json!(["pwd", "rcv"]));
    assert_eq!(audit_count(&app, "auth.recovery_code_used").await, 1);

    // The stored data never contains a plaintext code.
    let sealed: String =
        sqlx::query_scalar("SELECT data_encrypted FROM user_factors WHERE kind = 'recovery'")
            .fetch_one(&app.state.db)
            .await
            .unwrap();
    assert!(!sealed.contains(&codes[1].replace('-', "")));
}

#[tokio::test]
async fn without_a_usable_factor_policy_denies_users_but_not_break_glass() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    let (_, _viewer_token) = add_viewer(&app).await;
    require_mfa_for_everyone(&app, &admin).await;

    // Local super admin with nothing enrolled: break-glass, audited.
    login(&app, "ua").await.assert_ok();
    assert_eq!(audit_count(&app, "auth.break_glass").await, 1);

    // Anyone else is denied, never silently let through.
    let r = call(
        &app,
        None,
        "ua",
        "POST",
        "/api/v1/auth/login",
        json!({"username": "viewer", "password": PASSWORD}),
    )
    .await;
    r.assert_forbidden();
    assert_eq!(r.json::<Value>()["code"], "mfa_enrollment_required");
}

/// A record of a plugin factor whose plugin is not running (disabled, never
/// installed here): login is refused and step-up stays required, so an
/// outage can neither let a user in nor waive step-up.
#[tokio::test]
async fn an_unavailable_plugin_factor_denies_login_and_keeps_step_up() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    require_mfa_for_everyone(&app, &admin).await;
    let (viewer, viewer_token) = add_viewer(&app).await;
    let record =
        openvox_webui::services::auth_pipeline::AuthPipeline::new(&app.state.db, &app.state.config)
            .store()
            .insert(viewer, "mfa-fido2:security-key", "Key", &json!({}), true)
            .await
            .unwrap()
            .id;

    let r = call(
        &app,
        None,
        "ua",
        "POST",
        "/api/v1/auth/login",
        json!({"username": "viewer", "password": PASSWORD}),
    )
    .await;
    r.assert_forbidden();
    assert_eq!(r.json::<Value>()["code"], "factor_unavailable");
    let providers: Value = app.get("/api/v1/auth/providers").await.json();
    assert!(!providers.to_string().contains("mfa-fido2"));

    let r = call(
        &app,
        Some(&viewer_token),
        "t",
        "DELETE",
        &format!("/api/v1/users/me/factors/{record}"),
        json!({}),
    )
    .await;
    r.assert_unauthorized();
    assert_eq!(r.json::<Value>()["code"], "step_up_required");
    let r = call(
        &app,
        Some(&viewer_token),
        "t",
        "POST",
        "/api/v1/auth/step-up",
        json!({}),
    )
    .await;
    r.assert_status(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json::<Value>()["code"], "factor_unavailable");
}

/// A first-party factor plugin for the HTTP tests: accepts the credential
/// `"good"` and nothing else.
struct GoodKey;

impl openvox_webui::services::plugins::BuiltinPlugin for GoodKey {
    fn handle(
        &self,
        call: &openvox_webui::services::plugins::invoke::HandlerCall,
    ) -> anyhow::Result<Value> {
        let good = call.input["response"]["credential"] == "good";
        Ok(match call.handler.as_str() {
            "begin_enroll" => {
                json!({"client": {"publicKey": {"challenge": "c"}}, "mode": {"pending": {"s": 1}}})
            }
            "complete_enroll" if good => json!({"data": {"cred": "k1"}, "label": "Key"}),
            "begin_challenge" => json!({"client": {"publicKey": {}}, "flow_state": "f"}),
            "verify" if good && call.input["flow_state"] == "f" => {
                json!({"outcome": "verified", "amr": "hwk"})
            }
            "verify" => json!({"outcome": "failed", "retryable": true}),
            _ => json!({"error": {"kind": "invalid", "code": "bad_credential"}}),
        })
    }
}

const KEY_SEED: [u8; 32] = [51; 32];

#[tokio::test]
async fn a_plugin_factor_is_served_over_http_while_its_plugin_is_enabled() {
    use ed25519_dalek::SigningKey;
    use openvox_webui::services::plugins::package::build_signed_package;
    let app = TestApp::with_plugins(|m| {
        let pk = SigningKey::from_bytes(&KEY_SEED).verifying_key().to_bytes();
        m.with_builtin_keys(vec![("OpenVox".into(), pk)])
            .with_builtin_runtime("goodkey", std::sync::Arc::new(GoodKey))
    })
    .await;
    let yaml = "api_version: 2\nid: goodkey\nname: Good key\nversion: 1.0.0\npublisher: ACME\n\
                runtime: { kind: builtin, entry: goodkey }\ncapabilities:\n  - auth:factor\n\
                contributes:\n  auth_factors:\n    - { id: key, kind: webauthn, label: Good key, amr: hwk, enrollment: host-managed, fields: [credential] }\n";
    let pkg = build_signed_package(
        &[("plugin.yaml", yaml.as_bytes())],
        "goodkey",
        "1.0.0",
        2,
        KEY_SEED,
    );
    let pm = &app.state.plugin_manager;
    pm.install(&pkg, "admin", None).await.unwrap();
    pm.enable("goodkey").await.unwrap();

    let providers: Value = app.get("/api/v1/auth/providers").await.json();
    let factor = providers["factors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "goodkey:key")
        .expect("the plugin factor is listed")
        .clone();
    assert_eq!(factor["kind"], "webauthn");
    assert_eq!(factor["amr"], "hwk");

    let admin = prepare_admin(&app).await;
    require_mfa_for_everyone(&app, &admin).await;
    let begin = call(
        &app,
        Some(&admin),
        "t",
        "POST",
        "/api/v1/users/me/factors/goodkey:key/enroll/begin",
        json!({}),
    )
    .await;
    begin.assert_ok();
    assert_eq!(
        begin.json::<Value>()["enrollment"]["publicKey"]["challenge"],
        "c"
    );
    let bad = call(
        &app,
        Some(&admin),
        "t",
        "POST",
        "/api/v1/users/me/factors/goodkey:key/enroll/complete",
        json!({"credential": "bad"}),
    )
    .await;
    bad.assert_status(axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(bad.json::<Value>()["message"], "bad_credential");
    let done = call(
        &app,
        Some(&admin),
        "t",
        "POST",
        "/api/v1/users/me/factors/goodkey:key/enroll/complete",
        json!({"credential": "good"}),
    )
    .await;
    done.assert_ok();
    assert!(done.json::<Value>()["recovery"]["codes"].is_array());

    let r = login(&app, "ua").await;
    r.assert_status(axum::http::StatusCode::ACCEPTED);
    let body: Value = r.json();
    let ids: Vec<&str> = body["factors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["recovery", "goodkey:key"]);
    let id = body["challenge_id"].as_str().unwrap().to_string();
    call(
        &app,
        None,
        "ua",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/begin"),
        json!({"factor_id": "goodkey:key"}),
    )
    .await
    .assert_ok();
    let wrong = call(
        &app,
        None,
        "ua",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"response": {"credential": "bad"}}),
    )
    .await;
    wrong.assert_unauthorized();
    assert_eq!(wrong.json::<Value>()["code"], "factor_failed");
    let ok = call(
        &app,
        None,
        "ua",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"response": {"credential": "good"}}),
    )
    .await;
    ok.assert_ok();
    let claims = claims_of(ok.json::<Value>()["access_token"].as_str().unwrap());
    assert_eq!(claims["acr"], 2);
    assert_eq!(claims["amr"], json!(["pwd", "hwk"]));

    // Disabled: no longer listed or offered; recovery codes remain.
    pm.disable("goodkey").await.unwrap();
    let providers: Value = app.get("/api/v1/auth/providers").await.json();
    assert!(!providers.to_string().contains("goodkey"));
    let r = login(&app, "ua").await;
    r.assert_status(axum::http::StatusCode::ACCEPTED);
    assert_eq!(
        r.json::<Value>()["factors"],
        json!([
            {"id": "recovery", "kind": "recovery", "label": "Recovery code", "amr": "rcv",
             "enrollment": "host-managed", "fields": ["code"]}
        ])
    );
}

#[tokio::test]
async fn sensitive_changes_need_step_up_once_a_factor_exists() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    enroll_totp(&app, &admin).await;

    // The session predates the factor, so it is still level 1.
    let factors: Value = call(
        &app,
        Some(&admin),
        "t",
        "GET",
        "/api/v1/users/me/factors",
        json!({}),
    )
    .await
    .json();
    let record = factors["factors"][0]["id"].as_str().unwrap().to_string();
    let r = call(
        &app,
        Some(&admin),
        "t",
        "DELETE",
        &format!("/api/v1/users/me/factors/{record}"),
        json!({}),
    )
    .await;
    r.assert_unauthorized();
    let body: Value = r.json();
    assert_eq!(body["code"], "step_up_required");
    assert_eq!(body["details"]["required_acr"], 2);

    // Step up with the authenticator, then the removal goes through.
    let secret = {
        let sealed: String =
            sqlx::query_scalar("SELECT data_encrypted FROM user_factors WHERE kind = 'totp'")
                .fetch_one(&app.state.db)
                .await
                .unwrap();
        let data: Value = openvox_webui::services::auth_pipeline::crypto::open_json(
            &app.state.config.auth.jwt_secret,
            &sealed,
        )
        .unwrap();
        totp::secret_from_base32(data["secret"].as_str().unwrap()).unwrap()
    };
    let r = call(
        &app,
        Some(&admin),
        "ua",
        "POST",
        "/api/v1/auth/step-up",
        json!({}),
    )
    .await;
    r.assert_status(axum::http::StatusCode::ACCEPTED);
    let id = r.json::<Value>()["challenge_id"]
        .as_str()
        .unwrap()
        .to_string();
    let v = call(
        &app,
        None,
        "ua",
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"factor_id": "totp", "response": {"code": totp::hotp(&secret, totp::step_at(now()) + 1)}}),
    )
    .await;
    v.assert_ok();
    let raised = v.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(claims_of(&raised)["acr"], 2);

    // The old session was revoked: no fixation.
    call(
        &app,
        Some(&admin),
        "t",
        "GET",
        "/api/v1/users/me/factors",
        json!({}),
    )
    .await
    .assert_unauthorized();
    call(
        &app,
        Some(&raised),
        "t",
        "DELETE",
        &format!("/api/v1/users/me/factors/{record}"),
        json!({}),
    )
    .await
    .assert_ok();
    assert_eq!(audit_count(&app, "auth.factor_removed").await, 1);
}

#[tokio::test]
async fn policy_administration_requires_permission() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    let (_, viewer) = add_viewer(&app).await;
    call(
        &app,
        Some(&viewer),
        "t",
        "GET",
        "/api/v1/auth/policies",
        json!({}),
    )
    .await
    .assert_forbidden();
    call(&app, None, "t", "GET", "/api/v1/auth/policies", json!({}))
        .await
        .assert_unauthorized();
    let bad = call(
        &app,
        Some(&admin),
        "t",
        "PUT",
        "/api/v1/auth/policies",
        json!({"policies": [{"name": "x", "required_acr": 3}]}),
    )
    .await;
    bad.assert_bad_request();
    require_mfa_for_everyone(&app, &admin).await;
    let r: Value = call(
        &app,
        Some(&admin),
        "t",
        "GET",
        "/api/v1/auth/policies",
        json!({}),
    )
    .await
    .json();
    assert_eq!(r["policies"][0]["name"], "everyone");
}

#[tokio::test]
async fn admin_can_reset_a_users_factors() {
    let app = TestApp::new().await;
    let admin = prepare_admin(&app).await;
    let (viewer_id, viewer) = add_viewer(&app).await;
    enroll_totp(&app, &viewer).await;
    let r = call(
        &app,
        Some(&admin),
        "t",
        "POST",
        &format!("/api/v1/users/{viewer_id}/factors/reset"),
        json!({}),
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["removed"], 2, "totp and recovery codes");
    call(
        &app,
        Some(&viewer),
        "t",
        "POST",
        &format!("/api/v1/users/{ADMIN_ID}/factors/reset"),
        json!({}),
    )
    .await
    .assert_forbidden();
    assert_eq!(audit_count(&app, "auth.factors_reset").await, 1);
}

#[test]
fn mfa_config_defaults_are_safe() {
    let c = MfaConfig::default();
    assert_eq!(c.max_attempts, 5);
    assert!(c.webauthn.is_none() && c.oidc.is_empty());
}

mod oidc_linking {
    use super::*;
    use openvox_webui::services::auth_pipeline::oidc::{resolve_user, IdentityClaims, LinkResult};
    use openvox_webui::services::auth_pipeline::OidcProviderConfig;

    fn provider(jit: bool, trust_email: bool) -> OidcProviderConfig {
        serde_json::from_value(json!({
            "id": "corp", "issuer": "https://idp.example.com", "client_id": "c",
            "redirect_uri": "https://ovx.example.com/api/v1/auth/providers/corp/callback",
            "jit": jit, "trust_email": trust_email, "default_role": "viewer",
        }))
        .unwrap()
    }

    fn claims(sub: &str, email: &str, verified: bool) -> IdentityClaims {
        IdentityClaims {
            subject: sub.into(),
            email: Some(email.into()),
            email_verified: verified,
            name: None,
            preferred_username: Some("sso-user".into()),
            amr: vec![],
            acr: None,
            raw: Value::Null,
        }
    }

    #[tokio::test]
    async fn unknown_identity_is_denied_unless_jit_is_enabled() {
        let app = TestApp::new().await;
        let r = resolve_user(
            &app.state.db,
            &provider(false, true),
            &claims("s1", "n@example.com", true),
        )
        .await
        .unwrap();
        assert!(matches!(r, LinkResult::Denied("identity_not_provisioned")));
    }

    #[tokio::test]
    async fn jit_creates_a_linked_user_with_the_configured_role_only() {
        let app = TestApp::new().await;
        let cfg = provider(true, true);
        let c = claims("s1", "new@example.com", true);
        let LinkResult::User(u) = resolve_user(&app.state.db, &cfg, &c).await.unwrap() else {
            panic!("expected a user");
        };
        assert_eq!(u.role, "viewer");
        assert_eq!(u.username, "sso-user");
        // Second login links by subject, not by e-mail.
        let mut c2 = c.clone();
        c2.email = Some("changed@example.com".into());
        let LinkResult::User(again) = resolve_user(&app.state.db, &cfg, &c2).await.unwrap() else {
            panic!("expected the linked user");
        };
        assert_eq!(again.id, u.id);
        // SSO-only: no local password can match.
        assert!(AuthService::verify_password("anything", &u.password_hash).is_err());
    }

    #[tokio::test]
    async fn jit_never_attaches_to_an_existing_account_by_email() {
        let app = TestApp::new().await;
        let existing: String = sqlx::query_scalar("SELECT email FROM users WHERE id = ?")
            .bind(ADMIN_ID)
            .fetch_one(&app.state.db)
            .await
            .unwrap();
        let r = resolve_user(
            &app.state.db,
            &provider(true, true),
            &claims("evil", &existing, true),
        )
        .await
        .unwrap();
        assert!(matches!(r, LinkResult::Denied("email_already_registered")));
    }

    #[tokio::test]
    async fn jit_needs_a_trusted_verified_email() {
        let app = TestApp::new().await;
        let unverified = resolve_user(
            &app.state.db,
            &provider(true, true),
            &claims("a", "a@example.com", false),
        )
        .await
        .unwrap();
        assert!(matches!(
            unverified,
            LinkResult::Denied("email_not_verified")
        ));
        let untrusted = resolve_user(
            &app.state.db,
            &provider(true, false),
            &claims("b", "b@example.com", true),
        )
        .await
        .unwrap();
        assert!(matches!(
            untrusted,
            LinkResult::Denied("email_not_verified")
        ));
    }
}
