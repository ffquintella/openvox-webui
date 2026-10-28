//! Integration tests for the plugin admin API (M19).

use axum::{body::Body, http::Request};
use openvox_webui::services::plugins::package::build_signed_package;
use openvox_webui::services::AuthService;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::common::*;

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const PASSWORD: &str = "correct horse battery";

fn remote_package(seed: u8) -> Vec<u8> {
    let yaml = "api_version: 2\nid: demo\nname: Demo\nversion: 1.0.0\npublisher: ACME\n\
                runtime: { kind: remote, endpoint: \"https://demo.example.com\" }\n";
    build_signed_package(
        &[("plugin.yaml", yaml.as_bytes())],
        "demo",
        "1.0.0",
        2,
        [seed; 32],
    )
}

async fn admin_token(app: &TestApp) -> String {
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

async fn send(
    app: &TestApp,
    token: &str,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    headers: &[(&str, &str)],
) -> TestResponse {
    let mut b = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    app.request_with_auth(b.body(Body::from(body)).unwrap(), token)
        .await
}

fn json_body(v: Value) -> Vec<u8> {
    v.to_string().into_bytes()
}

const JSON: &[(&str, &str)] = &[("Content-Type", "application/json")];

#[tokio::test]
async fn plugin_endpoints_require_authentication_and_permission() {
    let app = TestApp::new().await;
    app.get("/api/v1/plugins").await.assert_unauthorized();

    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    send(&app, &viewer, "GET", "/api/v1/plugins", vec![], &[])
        .await
        .assert_forbidden();
    send(
        &app,
        &viewer,
        "POST",
        "/api/v1/plugins",
        remote_package(1),
        &[],
    )
    .await
    .assert_forbidden();
    send(
        &app,
        &viewer,
        "GET",
        "/api/v1/plugins/trust/keys",
        vec![],
        &[],
    )
    .await
    .assert_forbidden();

    // Any signed-in user may read the (empty) registry.
    let r = send(
        &app,
        &viewer,
        "GET",
        "/api/v1/plugins/ui/slots",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["slots"], json!({}));
}

#[tokio::test]
async fn unsigned_and_untrusted_packages_are_refused() {
    let app = TestApp::new().await;
    let t = admin_token(&app).await;

    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins",
        b"not a zip".to_vec(),
        &[],
    )
    .await;
    assert_eq!(r.status.as_u16(), 422);
    assert_eq!(r.json::<Value>()["code"], "invalid_package");

    let pkg = remote_package(5);
    let pre = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/preflight",
        pkg.clone(),
        &[],
    )
    .await;
    pre.assert_ok();
    let pre: Value = pre.json();
    assert_eq!(pre["signature"]["status"], "untrusted_key");
    assert_eq!(pre["can_install"], true);

    let r = send(&app, &t, "POST", "/api/v1/plugins", pkg.clone(), &[]).await;
    assert_eq!(r.status.as_u16(), 422);
    assert_eq!(r.json::<Value>()["code"], "untrusted_signer");

    // Trusting during install needs the password (step-up).
    let prefix = pre["signature"]["key_id"].as_str().unwrap()[..8].to_string();
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins",
        pkg.clone(),
        &[
            ("X-Trust-Key-Confirm", &prefix),
            ("X-Reauth-Password", "wrong"),
        ],
    )
    .await;
    r.assert_forbidden();

    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins",
        pkg,
        &[
            ("X-Trust-Key-Confirm", &prefix),
            ("X-Reauth-Password", PASSWORD),
            ("X-Trust-Key-Label", "dev"),
        ],
    )
    .await;
    r.assert_created();
    let d: Value = r.json();
    assert_eq!(d["id"], "demo");
    assert_eq!(d["state"], "installed");
    assert_eq!(d["signer_label"], "dev");

    // Remote runtimes cannot run until M20.
    let r = send(&app, &t, "POST", "/api/v1/plugins/demo/enable", vec![], &[]).await;
    assert_eq!(r.status.as_u16(), 409);
    assert_eq!(r.json::<Value>()["code"], "runtime_unavailable");

    // Installing and trusting are audited.
    let logs = send(
        &app,
        &t,
        "GET",
        "/api/v1/audit-logs?resource_type=plugins",
        vec![],
        &[],
    )
    .await;
    assert!(logs.text().contains("plugin.installed"));
}

#[tokio::test]
async fn trusted_key_management_requires_password_and_blocks_removal_in_use() {
    let app = TestApp::new().await;
    let t = admin_token(&app).await;
    let pk = {
        use base64::Engine;
        let k = ed25519_dalek::SigningKey::from_bytes(&[9; 32])
            .verifying_key()
            .to_bytes();
        base64::engine::general_purpose::STANDARD.encode(k)
    };

    let body =
        |password: &str| json_body(json!({"public_key": pk, "label": "ci", "password": password}));
    send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/trust/keys",
        body("nope"),
        JSON,
    )
    .await
    .assert_forbidden();
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/trust/keys",
        body(PASSWORD),
        JSON,
    )
    .await;
    r.assert_created();
    let key: Value = r.json();
    let id = key["key_id"].as_str().unwrap().to_string();
    assert_eq!(key["state"], "active");
    assert_eq!(key["origin"], "admin");

    // Duplicate is a conflict.
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/trust/keys",
        body(PASSWORD),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 409);

    // A plugin signed by it installs without any confirmation.
    send(&app, &t, "POST", "/api/v1/plugins", remote_package(9), &[])
        .await
        .assert_created();
    let r = send(
        &app,
        &t,
        "DELETE",
        &format!("/api/v1/plugins/trust/keys/{}", id),
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status.as_u16(), 409);

    // Revoking requires a reason and the password, and blocks new installs.
    let rev = |reason: &str, pw: &str| json_body(json!({"reason": reason, "password": pw}));
    let uri = format!("/api/v1/plugins/trust/keys/{}/revoke", id);
    send(&app, &t, "POST", &uri, rev("leaked", "nope"), JSON)
        .await
        .assert_forbidden();
    send(&app, &t, "POST", &uri, rev(" ", PASSWORD), JSON)
        .await
        .assert_bad_request();
    let r = send(&app, &t, "POST", &uri, rev("leaked", PASSWORD), JSON).await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["state"], "revoked");
    send(&app, &t, "DELETE", "/api/v1/plugins/demo", vec![], &[]).await;
    let pre = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/preflight",
        remote_package(9),
        &[],
    )
    .await;
    let pre: Value = pre.json();
    assert_eq!(pre["signature"]["status"], "key_revoked");
    assert_eq!(pre["can_install"], false);
    let r = send(&app, &t, "POST", "/api/v1/plugins", remote_package(9), &[]).await;
    assert_eq!(r.status.as_u16(), 422);
    assert_eq!(r.json::<Value>()["code"], "signature_rejected");
}
