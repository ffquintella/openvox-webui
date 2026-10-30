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

#[tokio::test]
async fn events_jobs_and_webhooks_endpoints_enforce_permissions() {
    let app = TestApp::new().await;
    for path in [
        "/api/v1/plugins/events/stats",
        "/api/v1/plugins/events/deliveries",
        "/api/v1/plugins/jobs",
        "/api/v1/plugins/webhooks/outbound",
    ] {
        app.get(path).await.assert_unauthorized();
    }

    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    for (method, path) in [
        ("GET", "/api/v1/plugins/events/stats"),
        ("GET", "/api/v1/plugins/jobs"),
        ("GET", "/api/v1/plugins/webhooks/outbound"),
        ("POST", "/api/v1/plugins/events/deliveries/1/retry"),
        ("POST", "/api/v1/plugins/demo/jobs/j/run"),
        ("POST", "/api/v1/plugins/demo/webhooks/h/secret"),
        ("POST", "/api/v1/plugins/webhooks/outbound"),
    ] {
        let body = json_body(json!({
            "name": "x", "url": "https://hooks.example.com/a", "event_types": ["node.*"]
        }));
        send(&app, &viewer, method, path, body, JSON)
            .await
            .assert_forbidden();
    }
}

#[tokio::test]
async fn outbound_webhooks_are_managed_through_the_api_and_secrets_are_shown_once() {
    let app = TestApp::new().await;
    let token = admin_token(&app).await;
    let create = |body: Value| {
        let (app, token) = (&app, &token);
        async move {
            send(
                app,
                token,
                "POST",
                "/api/v1/plugins/webhooks/outbound",
                json_body(body),
                JSON,
            )
            .await
        }
    };

    // http, loopback, bad patterns and bad filters are refused.
    for bad in [
        json!({"name": "x", "url": "http://hooks.example.com/a", "event_types": ["node.*"]}),
        json!({"name": "x", "url": "https://169.254.169.254/a", "event_types": ["node.*"]}),
        json!({"name": "x", "url": "https://hooks.example.com/a", "event_types": ["Bad Pattern"]}),
        json!({"name": "x", "url": "https://hooks.example.com/a", "event_types": ["node.*"], "filter": "a =="}),
        json!({"name": "x", "url": "https://hooks.example.com/a", "event_types": []}),
    ] {
        assert_eq!(create(bad).await.status, 400);
    }

    let r = create(json!({
        "name": "crm", "url": "https://hooks.example.com/a",
        "event_types": ["node.*"], "filter": "certname ~ 'web'"
    }))
    .await;
    assert_eq!(r.status, 201);
    let created: Value = r.json();
    let id = created["id"].as_str().unwrap().to_string();
    assert!(created["secret"].as_str().unwrap().starts_with("whsec_"));

    let list = send(
        &app,
        &token,
        "GET",
        "/api/v1/plugins/webhooks/outbound",
        vec![],
        &[],
    )
    .await;
    list.assert_ok();
    let list: Value = list.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(list[0].get("secret").is_none());

    let r = send(
        &app,
        &token,
        "PATCH",
        &format!("/api/v1/plugins/webhooks/outbound/{id}"),
        json_body(json!({"enabled": false, "filter": null})),
        JSON,
    )
    .await;
    r.assert_ok();
    let v: Value = r.json();
    assert_eq!(
        (v["enabled"].clone(), v["filter"].clone()),
        (json!(false), Value::Null)
    );

    let r = send(
        &app,
        &token,
        "POST",
        &format!("/api/v1/plugins/webhooks/outbound/{id}/secret"),
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    assert_ne!(r.json::<Value>()["secret"], created["secret"]);

    let r = send(
        &app,
        &token,
        "DELETE",
        &format!("/api/v1/plugins/webhooks/outbound/{id}"),
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status, 204);
    let r = send(
        &app,
        &token,
        "DELETE",
        &format!("/api/v1/plugins/webhooks/outbound/{id}"),
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status, 404);

    let stats = send(
        &app,
        &token,
        "GET",
        "/api/v1/plugins/events/stats",
        vec![],
        &[],
    )
    .await;
    stats.assert_ok();
    assert_eq!(stats.json::<Value>()["dead_letters"], 0);
    let r = send(
        &app,
        &token,
        "GET",
        "/api/v1/plugins/events/deliveries?status=bogus",
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status, 400);
    let r = send(
        &app,
        &token,
        "POST",
        "/api/v1/plugins/events/deliveries/999/retry",
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status, 404);
}

#[tokio::test]
async fn inbound_webhook_route_is_public_but_reveals_nothing_for_unknown_hooks() {
    let app = TestApp::new().await;
    // No session needed; an unknown plugin or hook is a plain 404, never a 401 from the auth layer.
    let r = app
        .request(
            Request::builder()
                .method("POST")
                .uri("/api/v1/plugins/ghost/webhooks/inbound")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, 404);
}

#[tokio::test]
async fn governance_endpoints_enforce_permissions_and_report_aggregates() {
    let app = TestApp::new().await;
    for uri in [
        "/api/v1/plugins/governance",
        "/api/v1/plugins/metrics",
        "/api/v1/plugins/demo/metrics",
    ] {
        app.get(uri).await.assert_unauthorized();
    }

    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    for (method, uri) in [
        ("GET", "/api/v1/plugins/governance"),
        ("GET", "/api/v1/plugins/metrics"),
        ("GET", "/api/v1/plugins/demo/metrics"),
        ("POST", "/api/v1/plugins/demo/circuit/reset"),
    ] {
        send(&app, &viewer, method, uri, vec![], &[])
            .await
            .assert_forbidden();
    }

    let t = admin_token(&app).await;
    let r = send(&app, &t, "GET", "/api/v1/plugins/governance", vec![], &[]).await;
    r.assert_ok();
    let body = r.json::<Value>();
    assert_eq!(body["plugins"], json!([]));
    assert_eq!(body["open_circuits"], 0);

    let r = send(&app, &t, "GET", "/api/v1/plugins/metrics", vec![], &[]).await;
    r.assert_ok();
    assert!(r.headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/plain; version=0.0.4"));
    assert!(r
        .text()
        .contains("# TYPE openvox_plugin_invocations_total counter"));
    assert!(r.text().contains("openvox_plugin_events_pending 0"));

    send(&app, &t, "GET", "/api/v1/plugins/demo/metrics", vec![], &[])
        .await
        .assert_not_found();
    send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/demo/circuit/reset",
        vec![],
        &[],
    )
    .await
    .assert_not_found();
}

#[tokio::test]
async fn plugin_ui_catalog_and_sessions_need_a_signed_in_user_and_a_real_contribution() {
    let app = TestApp::new().await;
    let anon = app
        .request(
            Request::builder()
                .uri("/api/v1/plugins/ui/catalog")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(anon.status, 401);

    let token = admin_token(&app).await;
    let r = send(
        &app,
        &token,
        "GET",
        "/api/v1/plugins/ui/catalog",
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status, 200);
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["version"], 1);
    let names: Vec<&str> = v["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"node.detail.tabs") && names.contains(&"page"));

    let r = send(
        &app,
        &token,
        "POST",
        "/api/v1/plugins/ui/sessions",
        json_body(json!({"plugin_id": "ghost", "contribution_id": "x"})),
        JSON,
    )
    .await;
    assert_eq!(r.status, 404);
}

#[tokio::test]
async fn plugin_ui_assets_are_public_but_a_bad_token_is_a_plain_404() {
    let app = TestApp::new().await;
    for uri in [
        "/api/v1/plugins/ui-assets/deadbeef/ui/app.html",
        "/api/v1/plugins/ui-assets/deadbeef/..%2Fplugin.yaml",
    ] {
        let r = app
            .request(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await;
        assert_eq!(r.status, 404, "{uri}");
    }
}

fn remote_version(seed: u8, version: &str) -> Vec<u8> {
    let yaml = format!(
        "api_version: 2\nid: demo\nname: Demo\nversion: {version}\npublisher: ACME\n\
         runtime: {{ kind: remote, endpoint: \"https://demo.example.com\" }}\n"
    );
    build_signed_package(
        &[("plugin.yaml", yaml.as_bytes())],
        "demo",
        version,
        2,
        [seed; 32],
    )
}

#[tokio::test]
async fn updates_rollback_and_revocation_endpoints() {
    let app = TestApp::new().await;
    for uri in ["/api/v1/plugins/updates", "/api/v1/plugins/demo/versions"] {
        app.get(uri).await.assert_unauthorized();
    }
    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    let policy = json!({"auto_update": "off"});
    for (method, uri, body) in [
        ("GET", "/api/v1/plugins/updates", json!({})),
        (
            "POST",
            "/api/v1/plugins/updates/apply",
            json!({"plugin_ids": ["demo"]}),
        ),
        ("GET", "/api/v1/plugins/demo/versions", json!({})),
        ("POST", "/api/v1/plugins/demo/update", json!({})),
        ("POST", "/api/v1/plugins/demo/rollback", json!({})),
        ("PUT", "/api/v1/plugins/demo/update-policy", policy),
        (
            "POST",
            "/api/v1/plugins/demo/versions/1.0.0/revoke",
            json!({"reason": "x"}),
        ),
        (
            "POST",
            "/api/v1/plugins/demo/versions/1.0.0/unrevoke",
            json!({}),
        ),
    ] {
        send(&app, &viewer, method, uri, json_body(body), JSON)
            .await
            .assert_forbidden();
    }

    // Install 1.0.0 (trusting the key), then stage 1.1.0.
    let t = admin_token(&app).await;
    let pre: Value = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/preflight",
        remote_version(9, "1.0.0"),
        &[],
    )
    .await
    .json();
    let prefix = pre["signature"]["key_id"].as_str().unwrap()[..8].to_string();
    send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins",
        remote_version(9, "1.0.0"),
        &[
            ("X-Trust-Key-Confirm", &prefix),
            ("X-Reauth-Password", PASSWORD),
        ],
    )
    .await
    .assert_created();
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins?stage=true",
        remote_version(9, "1.1.0"),
        &[],
    )
    .await;
    r.assert_created();
    let d: Value = r.json();
    assert_eq!(d["active_version"], "1.0.0");
    assert_eq!(d["available_version"], "1.1.0");

    let r = send(&app, &t, "GET", "/api/v1/plugins/updates", vec![], &[]).await;
    r.assert_ok();
    let u: Value = r.json();
    assert_eq!(u[0]["available_version"], "1.1.0");
    assert_eq!(u[0]["requires_approval"], false);
    assert_eq!(u[0]["auto_update"], "off");

    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/updates/apply",
        json_body(json!({"plugin_ids": ["demo", "missing"]})),
        JSON,
    )
    .await;
    r.assert_ok();
    let res: Value = r.json();
    assert_eq!(res["results"][0]["outcome"], "updated");
    assert_eq!(res["results"][0]["to_version"], "1.1.0");
    assert_eq!(res["results"][1]["outcome"], "failed");

    let r = send(
        &app,
        &t,
        "GET",
        "/api/v1/plugins/demo/versions",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    let v: Value = r.json();
    assert_eq!(v[0]["version"], "1.1.0");
    assert_eq!(v[0]["relation"], "active");
    assert_eq!(v[1]["relation"], "older");
    assert_eq!(v[1]["can_activate"], true);

    // Rollback without a body goes to the previous version.
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/demo/rollback",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    let rb: Value = r.json();
    assert_eq!(rb["to_version"], "1.0.0");
    assert_eq!(rb["plugin"]["update_hold_version"], "1.1.0");
    // An explicit update to an older version is refused.
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/demo/update",
        json_body(json!({"version": "0.1.0"})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 404);

    // Update policy: the window must be a five-field cron expression.
    let r = send(
        &app,
        &t,
        "PUT",
        "/api/v1/plugins/demo/update-policy",
        json_body(json!({"auto_update": "patch", "update_window": "nightly"})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 400);
    let r = send(
        &app,
        &t,
        "PUT",
        "/api/v1/plugins/demo/update-policy",
        json_body(json!({"auto_update": "minor", "update_window": "* 2-3 * * 6"})),
        JSON,
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["auto_update"], "minor");

    // Revocation needs a reason and is reversible.
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/demo/versions/1.1.0/revoke",
        json_body(json!({"reason": ""})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 400);
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/demo/versions/1.1.0/revoke",
        json_body(json!({"reason": "bad build"})),
        JSON,
    )
    .await;
    r.assert_ok();
    let rv: Value = r.json();
    assert_eq!(rv["deactivated"], false);
    assert!(rv["plugin"]["available_version"].is_null());
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/demo/versions/1.1.0/unrevoke",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["available_version"], "1.1.0");

    let logs = send(
        &app,
        &t,
        "GET",
        "/api/v1/audit-logs?resource_type=plugins",
        vec![],
        &[],
    )
    .await
    .text();
    for action in [
        "plugin.update_staged",
        "plugin.upgraded",
        "plugin.rolled_back",
        "plugin.update_policy_changed",
        "plugin.version_revoked",
        "plugin.version_unrevoked",
    ] {
        assert!(logs.contains(action), "missing audit entry {action}");
    }
}

fn facts_package(seed: u8) -> Vec<u8> {
    let yaml = "api_version: 2\nid: cmdb\nname: CMDB\nversion: 1.0.0\npublisher: ACME\n\
                runtime: { kind: remote, endpoint: \"https://cmdb.example.com\" }\n\
                capabilities:\n  - facts:provide\n  - facts:export:external: { optional: true }\n\
                contributes:\n  facts:\n    namespace: acme_cmdb\n    mode: push\n    items:\n\
                \x20     - { name: owner, type: string, sensitivity: pii, exportable: true }\n\
                \x20     - { name: tier, type: integer, usable_in_rules: true,\n\
                \x20         requires: { resource: plugin.cmdb.facts, action: read } }\n";
    build_signed_package(
        &[("plugin.yaml", yaml.as_bytes())],
        "cmdb",
        "1.0.0",
        2,
        [seed; 32],
    )
}

#[tokio::test]
async fn plugin_facts_catalog_settings_preview_export_and_merged_views() {
    let app = TestApp::new().await;
    app.get("/api/v1/plugins/facts").await.assert_unauthorized();
    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    for (method, uri, body) in [
        ("GET", "/api/v1/plugins/facts", json!({})),
        ("GET", "/api/v1/plugins/cmdb/facts", json!({})),
        (
            "PUT",
            "/api/v1/plugins/cmdb/facts/acme_cmdb.tier/settings",
            json!({"usable_in_rules": true}),
        ),
        (
            "GET",
            "/api/v1/plugins/cmdb/facts/acme_cmdb.owner/preview",
            json!({}),
        ),
    ] {
        send(&app, &viewer, method, uri, json_body(body), JSON)
            .await
            .assert_forbidden();
    }

    // Install the provider (trusting its key).
    let t = admin_token(&app).await;
    let pre: Value = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/preflight",
        facts_package(21),
        &[],
    )
    .await
    .json();
    assert!(pre["declared_pii_fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str().unwrap().contains("acme_cmdb.owner")));
    let prefix = pre["signature"]["key_id"].as_str().unwrap()[..8].to_string();
    send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins",
        facts_package(21),
        &[
            ("X-Trust-Key-Confirm", &prefix),
            ("X-Reauth-Password", PASSWORD),
        ],
    )
    .await
    .assert_created();

    let r = send(&app, &t, "GET", "/api/v1/plugins/facts", vec![], &[]).await;
    r.assert_ok();
    let cat: Value = r.json();
    assert_eq!(cat[0]["namespace"], "acme_cmdb");
    assert_eq!(cat[0]["export_capability"], "not_approved");
    assert_eq!(cat[0]["puppetdb_checked"], false);
    assert_eq!(cat[0]["facts"][0]["name"], "acme_cmdb.owner");
    assert_eq!(cat[0]["facts"][0]["usable_in_rules"], false);
    assert_eq!(cat[0]["facts"][0]["exportable"], false);

    let settings = |name: &str| format!("/api/v1/plugins/cmdb/facts/{name}/settings");
    let r = send(
        &app,
        &t,
        "PUT",
        &settings("acme_cmdb.tier"),
        json_body(json!({"usable_in_rules": true})),
        JSON,
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["usable_in_rules"], true);
    for (name, body, code) in [
        (
            "acme_cmdb.owner",
            json!({"usable_in_rules": true}),
            "not_offered",
        ),
        (
            "acme_cmdb.owner",
            json!({"exportable": true, "confirm": "acme_cmdb.owner"}),
            "export_capability_missing",
        ),
    ] {
        let r = send(&app, &t, "PUT", &settings(name), json_body(body), JSON).await;
        assert_eq!(r.status.as_u16(), 422);
        assert_eq!(r.json::<Value>()["code"], code);
    }
    send(
        &app,
        &t,
        "PUT",
        "/api/v1/plugins/cmdb/capabilities",
        json_body(json!({"capabilities": ["facts:provide", "facts:export:external"]})),
        JSON,
    )
    .await
    .assert_ok();
    let r = send(
        &app,
        &t,
        "PUT",
        &settings("acme_cmdb.owner"),
        json_body(json!({"exportable": true})),
        JSON,
    )
    .await;
    assert_eq!(r.json::<Value>()["code"], "confirmation_required");

    // The remote runtime cannot run yet: put the provider in play and write
    // values as its host API would.
    sqlx::query("UPDATE plugins SET state = 'enabled' WHERE id = 'cmdb'")
        .execute(&app.state.db)
        .await
        .unwrap();
    app.state.plugin_manager.reload().await.unwrap();
    let values = json!({"owner": "ops team", "tier": 1});
    app.state
        .plugin_manager
        .put_plugin_facts(
            "cmdb",
            "n1.example.com",
            values.as_object().unwrap(),
            openvox_webui::models::FactMode::Push,
        )
        .await
        .unwrap();

    // Preview before enabling the export, then enable it with the confirmation.
    let r = send(
        &app,
        &t,
        "GET",
        "/api/v1/plugins/cmdb/facts/acme_cmdb.owner/preview?limit=10",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    let p: Value = r.json();
    assert_eq!(p["total"], 1);
    assert_eq!(p["sensitivity"], "pii");
    assert_eq!(p["rows"][0]["certname"], "n1.example.com");
    assert_eq!(p["rows"][0]["value"], "ops team");
    let r = send(
        &app,
        &t,
        "PUT",
        &settings("acme_cmdb.owner"),
        json_body(json!({"exportable": true, "confirm": "acme_cmdb.owner"})),
        JSON,
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["exportable"], true);

    // Merged catalogs: `requires` hides the tier from the viewer.
    let names: Value = send(
        &app,
        &t,
        "GET",
        "/api/v1/facts/names?source=plugins",
        vec![],
        &[],
    )
    .await
    .json();
    assert_eq!(names, json!(["acme_cmdb.owner", "acme_cmdb.tier"]));
    let names: Value = send(
        &app,
        &viewer,
        "GET",
        "/api/v1/facts/names?source=plugin:cmdb",
        vec![],
        &[],
    )
    .await
    .json();
    assert_eq!(names, json!(["acme_cmdb.owner"]));
    let paths: Value = send(
        &app,
        &t,
        "GET",
        "/api/v1/facts/paths?source=plugins",
        vec![],
        &[],
    )
    .await
    .json();
    assert_eq!(paths[0]["path"], json!(["acme_cmdb", "owner"]));
    assert_eq!(paths[0]["source"], "plugin:cmdb");
    assert_eq!(paths[1]["type"], "integer");
    let r = send(
        &app,
        &t,
        "GET",
        "/api/v1/facts?source=plugin:cmdb&certname=n1.example.com",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    let f: Value = r.json();
    assert_eq!(f["total"], 2);
    assert_eq!(f["facts"][0]["source"], "plugin:cmdb");
    assert_eq!(f["facts"][0]["stale"], false);
    assert!(f["facts"][0]["computed_at"].is_string());
    let r = send(&app, &t, "GET", "/api/v1/facts?source=nope", vec![], &[]).await;
    assert_eq!(r.status.as_u16(), 400);
    // Without PuppetDB the default (merged) catalog keeps answering 503.
    let r = send(&app, &t, "GET", "/api/v1/facts/names", vec![], &[]).await;
    assert_eq!(r.status.as_u16(), 503);

    // Export to agents: only the enabled fact, single-quoted for the shell.
    let r = send(
        &app,
        &t,
        "GET",
        "/api/v1/facter/export/n1.example.com?format=json",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    let exported: Value = serde_json::from_str(&r.text()).unwrap();
    assert_eq!(exported, json!({"acme_cmdb.owner": "ops team"}));
    let shell = send(
        &app,
        &t,
        "GET",
        "/api/v1/facter/export/n1.example.com?format=shell",
        vec![],
        &[],
    )
    .await
    .text();
    assert_eq!(shell, "export FACTER_ACME_CMDB_OWNER='ops team'\n");

    let logs = send(&app, &t, "GET", "/api/v1/audit-logs", vec![], &[])
        .await
        .text();
    for action in ["plugin.fact.settings_changed", "plugin.fact.exported"] {
        assert!(logs.contains(action), "missing audit entry {action}");
    }
}

fn reports_package(seed: u8) -> Vec<u8> {
    let yaml = "api_version: 2\nid: acme\nname: ACME\nversion: 1.0.0\npublisher: ACME\n\
                runtime: { kind: remote, endpoint: \"https://acme.example.com\" }\n\
                capabilities:\n  - reports:provide\n  - reports:annotate\n  - reports:export:format\n\
                contributes:\n  report_types:\n\
                \x20   - { id: assets, label: Assets, handler: generate_assets,\n\
                \x20       params_schema: { type: object, properties: { since: { type: string, format: date } }, required: [since] } }\n\
                \x20   - { id: hidden, label: Hidden, handler: generate_assets,\n\
                \x20       requires: { resource: plugin.acme.reports, action: read } }\n\
                \x20 export_formats:\n\
                \x20   - { id: sarif, label: SARIF, content_type: application/sarif+json, extension: sarif, handler: render_sarif }\n\
                \x20 report_annotations:\n\
                \x20   - { key: risk, type: integer, label: Risk, show_in: [list_column, detail_panel], sortable: true }\n\
                \x20 datasets:\n\
                \x20   - { id: assets, label: Assets, schema: { columns: [ { name: certname, type: string }, { name: tier, type: integer } ] } }\n";
    build_signed_package(
        &[("plugin.yaml", yaml.as_bytes())],
        "acme",
        "1.0.0",
        2,
        [seed; 32],
    )
}

#[tokio::test]
async fn plugin_report_types_formats_annotations_and_datasets() {
    let app = TestApp::new().await;
    app.get("/api/v1/analytics/report-types")
        .await
        .assert_unauthorized();
    let t = admin_token(&app).await;
    let pre: Value = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/preflight",
        reports_package(31),
        &[],
    )
    .await
    .json();
    let prefix = pre["signature"]["key_id"].as_str().unwrap()[..8].to_string();
    send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins",
        reports_package(31),
        &[
            ("X-Trust-Key-Confirm", &prefix),
            ("X-Reauth-Password", PASSWORD),
        ],
    )
    .await
    .assert_created();
    send(
        &app,
        &t,
        "PUT",
        "/api/v1/plugins/acme/capabilities",
        json_body(json!({"capabilities": ["reports:provide", "reports:annotate", "reports:export:format"]})),
        JSON,
    )
    .await
    .assert_ok();
    // The remote runtime cannot run yet: put the plugin in play directly.
    sqlx::query("UPDATE plugins SET state = 'enabled' WHERE id = 'acme'")
        .execute(&app.state.db)
        .await
        .unwrap();
    app.state.plugin_manager.reload().await.unwrap();

    // Catalogs: `requires` hides a type from a viewer.
    let types: Value = send(
        &app,
        &t,
        "GET",
        "/api/v1/analytics/report-types",
        vec![],
        &[],
    )
    .await
    .json();
    let ids: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"node_health") && ids.contains(&"plugin:acme:assets"));
    assert!(ids.contains(&"plugin:acme:hidden"));
    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    let vt = send(
        &app,
        &viewer,
        "GET",
        "/api/v1/analytics/report-types",
        vec![],
        &[],
    )
    .await
    .text();
    assert!(vt.contains("plugin:acme:assets") && !vt.contains("plugin:acme:hidden"));
    let formats: Value = send(
        &app,
        &t,
        "GET",
        "/api/v1/analytics/export-formats",
        vec![],
        &[],
    )
    .await
    .json();
    assert!(formats
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["id"] == "plugin:acme:sarif" && f["content_type"] == "application/sarif+json"));

    // Saved plugin reports: parameters are checked against the schema.
    let saved = |params: Value| {
        json_body(json!({
            "name": "assets", "report_type": "plugin:acme:assets",
            "query_config": {"custom_params": params}, "is_public": false
        }))
    };
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/analytics/saved-reports",
        saved(json!({})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 400, "{}", r.text());
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/analytics/saved-reports",
        saved(json!({"since": "2026-01-01"})),
        JSON,
    )
    .await;
    r.assert_ok();
    let report: Value = r.json();
    assert_eq!(report["report_type"], "plugin:acme:assets");
    let id = report["id"].as_str().unwrap().to_string();
    // Executions run as a background job; the remote runtime makes it fail.
    let r = send(
        &app,
        &t,
        "POST",
        &format!("/api/v1/analytics/saved-reports/{id}/execute"),
        json_body(json!({"output_format": "json"})),
        JSON,
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["status"], "running");
    let mut status = String::new();
    for _ in 0..100 {
        let list: Value = send(
            &app,
            &t,
            "GET",
            &format!("/api/v1/analytics/saved-reports/{id}/executions"),
            vec![],
            &[],
        )
        .await
        .json();
        status = list[0]["status"].as_str().unwrap_or("").to_string();
        if status == "failed" || status == "completed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(status, "failed");
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/analytics/generate",
        json_body(json!({"report_type": "plugin:acme:assets", "config": {"custom_params": {"since": "2026-01-01"}}})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 503, "{}", r.text());

    // Annotations on a Puppet run report.
    let hash = "0123456789abcdef0123456789abcdef01234567";
    app.state
        .plugin_manager
        .annotate_report("acme", hash, "risk", &json!(8), Some("high"))
        .await
        .unwrap();
    let cols: Value = send(
        &app,
        &t,
        "GET",
        "/api/v1/reports/annotation-columns",
        vec![],
        &[],
    )
    .await
    .json();
    assert_eq!(cols[0]["key"], "risk");
    assert_eq!(cols[0]["sortable"], true);
    let a: Value = send(
        &app,
        &t,
        "GET",
        &format!("/api/v1/reports/{hash}/annotations"),
        vec![],
        &[],
    )
    .await
    .json();
    assert_eq!(a[0]["value"], 8);
    assert_eq!(a[0]["severity"], "high");
    let l: Value = send(
        &app,
        &t,
        "POST",
        "/api/v1/reports/annotations/lookup",
        json_body(json!({"hashes": [hash, "f".repeat(40)]})),
        JSON,
    )
    .await
    .json();
    assert_eq!(l[hash][0]["label"], "Risk");
    assert!(l.get("f".repeat(40)).is_none());

    // Datasets feed the custom report builder.
    app.state
        .plugin_manager
        .put_dataset_rows(
            "acme",
            "assets",
            &[
                json!({"certname": "a", "tier": 1}),
                json!({"certname": "b", "tier": 2}),
            ],
            true,
            &[],
        )
        .await
        .unwrap();
    let ds: Value = send(&app, &t, "GET", "/api/v1/analytics/datasets", vec![], &[])
        .await
        .json();
    assert_eq!(ds[0]["id"], "plugin:acme:assets");
    assert_eq!(ds[0]["rows"], 2);
    let q: Value = send(
        &app,
        &t,
        "POST",
        "/api/v1/analytics/datasets/query",
        json_body(json!({"dataset": "plugin:acme:assets", "filter": {"tier": 2}})),
        JSON,
    )
    .await
    .json();
    assert_eq!(q["total"], 1);
    assert_eq!(q["rows"][0]["certname"], "b");
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/analytics/saved-reports",
        json_body(json!({
            "name": "custom", "report_type": "custom",
            "query_config": {"custom_params": {"dataset": "plugin:acme:assets"}}, "is_public": false
        })),
        JSON,
    )
    .await;
    r.assert_ok();
    let cid = r.json::<Value>()["id"].as_str().unwrap().to_string();
    let exec: Value = send(
        &app,
        &t,
        "POST",
        &format!("/api/v1/analytics/saved-reports/{cid}/execute"),
        json_body(json!({"output_format": "csv"})),
        JSON,
    )
    .await
    .json();
    assert_eq!(exec["status"], "completed");
    assert_eq!(exec["output_data"]["report_type"], "custom");
    assert_eq!(exec["output_data"]["total"], 2);
    let eid = exec["id"].as_str().unwrap();
    let r = send(
        &app,
        &t,
        "GET",
        &format!("/api/v1/analytics/executions/{eid}/export?format=json"),
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    assert!(r.headers["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment"));
    // The plugin format reaches the (unavailable) remote runtime.
    let r = send(
        &app,
        &t,
        "GET",
        &format!("/api/v1/analytics/executions/{eid}/export?format=plugin:acme:sarif"),
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status.as_u16(), 503, "{}", r.text());
    let r = send(
        &app,
        &t,
        "GET",
        &format!("/api/v1/analytics/executions/{eid}/export?format=plugin:acme:nope"),
        vec![],
        &[],
    )
    .await;
    assert_eq!(r.status.as_u16(), 404);
}

// ---- restricted interceptors (M31, T104) -------------------------------------

const GATE_SEED: [u8; 32] = [31; 32];

/// A built-in change gate: vetoes groups named `frozen-*` and answers pins
/// with a change, which a veto-only event treats as a failure.
struct GroupGate;

impl openvox_webui::services::plugins::BuiltinPlugin for GroupGate {
    fn handle(
        &self,
        call: &openvox_webui::services::plugins::invoke::HandlerCall,
    ) -> anyhow::Result<Value> {
        let p = &call.input["payload"];
        Ok(
            if p["group_name"]
                .as_str()
                .unwrap_or("")
                .starts_with("frozen-")
            {
                json!({"decision": "veto", "reason": "change freeze"})
            } else if p["action"] == "pin" {
                json!({"decision": "alter", "changes": {"change": {}}})
            } else {
                json!({"decision": "allow"})
            },
        )
    }
}

#[tokio::test]
async fn group_changes_go_through_interceptors_and_the_admin_api_lists_them() {
    use ed25519_dalek::SigningKey;
    let app = TestApp::with_plugins(|m| {
        let pk = SigningKey::from_bytes(&GATE_SEED)
            .verifying_key()
            .to_bytes();
        m.with_builtin_keys(vec![("OpenVox".into(), pk)])
            .with_builtin_runtime("gate", std::sync::Arc::new(GroupGate))
    })
    .await;
    app.get("/api/v1/plugins/interceptors")
        .await
        .assert_unauthorized();
    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    send(
        &app,
        &viewer,
        "GET",
        "/api/v1/plugins/interceptors",
        vec![],
        &[],
    )
    .await
    .assert_forbidden();

    let t = admin_token(&app).await;
    let r = send(&app, &t, "GET", "/api/v1/plugins/interceptors", vec![], &[]).await;
    r.assert_ok();
    let body = r.json::<Value>();
    let events = body["events"].as_array().unwrap();
    assert_eq!(events[0]["type"], "before.node.classify");
    assert_eq!(events[0]["fail_mode"], "open");
    assert_eq!(events[0]["timeout_ms"], 50);
    assert_eq!(events[0]["max_interceptors"], 3);
    assert_eq!(events[0]["alterable"], json!(["facts"]));
    assert_eq!(events[1]["type"], "before.group.change");
    assert_eq!(events[1]["fail_mode"], "closed");
    assert_eq!(events[1]["interceptors"], json!([]));

    // Install and enable the gate (signed by the project key).
    let yaml = "api_version: 2\nid: gate\nname: Gate\nversion: 1.0.0\npublisher: ACME\n\
                runtime: { kind: builtin, entry: gate }\n\
                capabilities:\n  - events:intercept:before.group.change\n\
                events:\n  intercept:\n    - { type: before.group.change, priority: 5 }\n";
    let pkg = build_signed_package(
        &[("plugin.yaml", yaml.as_bytes())],
        "gate",
        "1.0.0",
        2,
        GATE_SEED,
    );
    let r = send(&app, &t, "POST", "/api/v1/plugins", pkg, &[]).await;
    assert_eq!(r.status.as_u16(), 201, "{}", r.text());
    let caps = r.json::<Value>()["capabilities"].clone();
    assert_eq!(caps[0]["name"], "events:intercept:before.group.change");
    assert_eq!(caps[0]["risk"], "high");
    send(&app, &t, "POST", "/api/v1/plugins/gate/enable", vec![], &[])
        .await
        .assert_ok();

    let r = send(
        &app,
        &t,
        "GET",
        "/api/v1/plugins/interceptors/before.group.change",
        vec![],
        &[],
    )
    .await;
    r.assert_ok();
    let ev = r.json::<Value>();
    assert_eq!(ev["capability"], "events:intercept:before.group.change");
    let gate = &ev["interceptors"][0];
    assert_eq!(
        (&gate["plugin_id"], &gate["priority"], &gate["handler"]),
        (&json!("gate"), &json!(5), &json!("on_intercept"))
    );
    assert_eq!(
        (&gate["active"], &gate["granted"]),
        (&json!(true), &json!(true))
    );
    send(
        &app,
        &t,
        "GET",
        "/api/v1/plugins/interceptors/node.classified",
        vec![],
        &[],
    )
    .await
    .assert_not_found();

    // A veto answers 409 and nothing is stored.
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/groups",
        json_body(json!({"name": "frozen-web"})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 409, "{}", r.text());
    assert!(r.text().contains("change freeze"), "{}", r.text());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_groups WHERE name = 'frozen-web'")
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    assert_eq!(n, 0);

    // Allowed changes go ahead.
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/groups",
        json_body(json!({"name": "web"})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 201, "{}", r.text());
    let id = r.json::<Value>()["id"].as_str().unwrap().to_string();

    // An answer the event does not allow fails closed: 503, without the
    // plugin's error in the message.
    let r = send(
        &app,
        &t,
        "POST",
        &format!("/api/v1/groups/{id}/pinned"),
        json_body(json!({"certname": "n1.example.com"})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 503, "{}", r.text());
    assert!(!r.text().contains("cannot be changed"), "{}", r.text());

    // Both refusals are audited with the acting user.
    let actions: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT action, user_id FROM audit_log WHERE action LIKE 'plugin.interceptor.%' ORDER BY action",
    )
    .fetch_all(&app.state.db)
    .await
    .unwrap();
    assert_eq!(
        actions,
        vec![
            (
                "plugin.interceptor.refused".to_string(),
                Some(ADMIN_ID.to_string())
            ),
            (
                "plugin.interceptor.vetoed".to_string(),
                Some(ADMIN_ID.to_string())
            ),
        ]
    );

    // Dedicated telemetry, also in the Prometheus exposition.
    let r = send(&app, &t, "GET", "/api/v1/plugins/interceptors", vec![], &[]).await;
    let stats = &r.json::<Value>()["events"][1]["interceptors"][0]["stats"];
    assert_eq!(
        (&stats["allowed"], &stats["vetoed"], &stats["failed_closed"]),
        (&json!(1), &json!(1), &json!(1))
    );
    let r = send(&app, &t, "GET", "/api/v1/plugins/metrics", vec![], &[]).await;
    r.assert_ok();
    assert!(r.text().contains(
        "openvox_plugin_interceptor_decisions_total{event=\"before.group.change\",plugin=\"gate\",decision=\"veto\"} 1"
    ));
    assert!(r
        .text()
        .contains("openvox_plugin_invocations_total{plugin=\"gate\",version=\"1.0.0\",kind=\"interceptor\",outcome=\"ok\"} 3"));
}

// ---- Trusted module pages (M31, T105) --------------------------------------

const MODULE_SEED: [u8; 32] = [41; 32];
const MODULE_JS: &[u8] = b"export async function init() {}\nexport async function get() { return () => ({ default: () => null }); }\n";

struct NoHandlers;
impl openvox_webui::services::plugins::BuiltinPlugin for NoHandlers {}

fn module_package(version: &str) -> Vec<u8> {
    use openvox_webui::services::plugins::modules::{sri_of, SriAlgorithm};
    let sri = sri_of(SriAlgorithm::Sha384, MODULE_JS);
    let yaml = format!(
        "api_version: 2\nid: console\nname: Console\nversion: {version}\npublisher: ACME\n\
         runtime: {{ kind: builtin, entry: console }}\ncapabilities: [ui:slot:page]\n\
         contributes:\n  slots:\n    - {{ slot: page, id: main, render: module, src: ui/remoteEntry.js, \
         integrity: \"{sri}\", expose: ./Console, path: /p/console/main }}\n"
    );
    build_signed_package(
        &[
            ("plugin.yaml", yaml.as_bytes()),
            ("ui/remoteEntry.js", MODULE_JS),
        ],
        "console",
        version,
        2,
        MODULE_SEED,
    )
}

async fn module_app(allow: bool) -> TestApp {
    use ed25519_dalek::SigningKey;
    TestApp::with_plugins(move |m| {
        let pk = SigningKey::from_bytes(&MODULE_SEED)
            .verifying_key()
            .to_bytes();
        m.with_builtin_keys(vec![("OpenVox".into(), pk)])
            .with_builtin_runtime("console", std::sync::Arc::new(NoHandlers))
            .with_trusted_modules(allow)
    })
    .await
}

async fn install_module_plugin(app: &TestApp, t: &str) {
    let r = send(
        app,
        t,
        "POST",
        "/api/v1/plugins",
        module_package("1.0.0"),
        &[],
    )
    .await;
    assert_eq!(r.status.as_u16(), 201, "{}", r.text());
    send(
        app,
        t,
        "POST",
        "/api/v1/plugins/console/enable",
        vec![],
        &[],
    )
    .await
    .assert_ok();
}

#[tokio::test]
async fn trusted_modules_are_refused_while_the_flag_is_off() {
    let app = module_app(false).await;
    let t = admin_token(&app).await;
    install_module_plugin(&app, &t).await;
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/console/module-trust",
        json_body(json!({"password": PASSWORD})),
        JSON,
    )
    .await;
    assert_eq!(r.status.as_u16(), 409, "{}", r.text());
    assert_eq!(r.json::<Value>()["code"], "trusted_modules_disabled");
    // The refusal is audited.
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'plugin.trust.module_trusted' AND details LIKE '%rejected%'",
    )
    .fetch_one(&app.state.db)
    .await
    .unwrap();
    assert_eq!(n, 1);
    // Nothing is listed or served.
    let r = send(&app, &t, "GET", "/api/v1/plugins/ui/slots", vec![], &[]).await;
    assert_eq!(r.json::<Value>()["slots"], json!({}));
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/ui/modules",
        json_body(json!({"plugin_id": "console", "contribution_id": "main"})),
        JSON,
    )
    .await;
    r.assert_not_found();
    let detail = send(&app, &t, "GET", "/api/v1/plugins/console", vec![], &[])
        .await
        .json::<Value>();
    assert_eq!(detail["module_trust"]["allowed_by_config"], false);
    assert_eq!(detail["module_trust"]["served"], false);
}

#[tokio::test]
async fn trusted_module_pages_are_approved_served_with_sri_and_withdrawn() {
    let app = module_app(true).await;
    let t = admin_token(&app).await;
    install_module_plugin(&app, &t).await;
    let trust = |body: Value| {
        let app = &app;
        let t = t.clone();
        async move {
            send(
                app,
                &t,
                "POST",
                "/api/v1/plugins/console/module-trust",
                json_body(body),
                JSON,
            )
            .await
        }
    };

    // Untrusted: not listed.
    let r = send(&app, &t, "GET", "/api/v1/plugins/ui/slots", vec![], &[]).await;
    assert_eq!(r.json::<Value>()["slots"], json!({}));

    // A viewer cannot trust; plugins:trust also needs the password.
    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'viewer', 'viewer@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&app, viewer_id, "viewer", vec!["viewer".into()]).await;
    send(
        &app,
        &viewer,
        "POST",
        "/api/v1/plugins/console/module-trust",
        json_body(json!({"password": PASSWORD})),
        JSON,
    )
    .await
    .assert_forbidden();
    send(
        &app,
        &viewer,
        "DELETE",
        "/api/v1/plugins/console/module-trust",
        vec![],
        &[],
    )
    .await
    .assert_forbidden();
    trust(json!({"password": "wrong password"}))
        .await
        .assert_forbidden();

    let r = trust(json!({"password": PASSWORD})).await;
    r.assert_ok();
    let detail = r.json::<Value>();
    assert_eq!(detail["module_trust"]["trusted"]["version"], "1.0.0");
    assert_eq!(detail["module_trust"]["trusted"]["trusted_by"], "admin");
    assert_eq!(detail["module_trust"]["served"], true);

    // The registry lists the contribution with its SRI.
    use openvox_webui::services::plugins::modules::{sri_of, SriAlgorithm};
    let sri = sri_of(SriAlgorithm::Sha384, MODULE_JS);
    let r = send(&app, &t, "GET", "/api/v1/plugins/ui/slots", vec![], &[]).await;
    let page = &r.json::<Value>()["slots"]["page"][0];
    assert_eq!(page["render"], "module");
    assert_eq!(page["integrity"], sri.as_str());
    assert_eq!(page["expose"], "./Console");

    // Any signed-in user who can see it gets a session for the module only.
    let r = send(
        &app,
        &viewer,
        "POST",
        "/api/v1/plugins/ui/modules",
        json_body(json!({"plugin_id": "console", "contribution_id": "main"})),
        JSON,
    )
    .await;
    r.assert_ok();
    let session = r.json::<Value>();
    assert_eq!(session["integrity"], sri.as_str());
    assert_eq!(session["expose"], "./Console");
    let url = session["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("/api/v1/plugins/ui-modules/"), "{url}");
    assert!(url.ends_with("/ui/remoteEntry.js"), "{url}");

    // Served without credentials, same origin only, with strict headers.
    let r = app.get(&url).await;
    r.assert_ok();
    assert_eq!(r.body.as_ref(), MODULE_JS);
    let h = |k: &str| r.headers.get(k).unwrap().to_str().unwrap().to_string();
    assert_eq!(h("content-type"), "text/javascript; charset=utf-8");
    assert_eq!(h("x-content-type-options"), "nosniff");
    assert_eq!(h("cross-origin-resource-policy"), "same-origin");
    assert_eq!(h("x-openvox-integrity"), sri);
    assert!(h("content-security-policy").contains("sandbox"));
    assert!(r.headers.get("access-control-allow-origin").is_none());
    let cross = app
        .request(
            Request::builder()
                .uri(&url)
                .header("Sec-Fetch-Site", "cross-site")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    cross.assert_not_found();
    // Only the entry file, and a module token never opens the iframe route.
    app.get(&url.replace("ui/remoteEntry.js", "plugin.yaml"))
        .await
        .assert_not_found();
    app.get(&url.replace("ui-modules", "ui-assets"))
        .await
        .assert_not_found();

    // Disabled: neither session nor file.
    send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/console/disable",
        vec![],
        &[],
    )
    .await
    .assert_ok();
    app.get(&url).await.assert_not_found();
    send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins/console/enable",
        vec![],
        &[],
    )
    .await
    .assert_ok();
    app.get(&url).await.assert_ok();

    // An update needs a new approval.
    let r = send(
        &app,
        &t,
        "POST",
        "/api/v1/plugins",
        module_package("1.1.0"),
        &[],
    )
    .await;
    assert_eq!(r.status.as_u16(), 201, "{}", r.text());
    assert!(r.json::<Value>()["module_trust"]["trusted"].is_null());
    app.get(&url).await.assert_not_found();
    let r = send(&app, &t, "GET", "/api/v1/plugins/ui/slots", vec![], &[]).await;
    assert_eq!(r.json::<Value>()["slots"], json!({}));
    let r = trust(json!({"password": PASSWORD, "version": "1.0.0"})).await;
    assert_eq!(r.status.as_u16(), 409, "{}", r.text());
    assert_eq!(r.json::<Value>()["code"], "version_not_active");
    trust(json!({"password": PASSWORD})).await.assert_ok();

    // Withdrawn by a plugin administrator, without a password.
    send(
        &app,
        &t,
        "DELETE",
        "/api/v1/plugins/console/module-trust",
        vec![],
        &[],
    )
    .await
    .assert_ok();
    send(
        &app,
        &t,
        "DELETE",
        "/api/v1/plugins/console/module-trust",
        vec![],
        &[],
    )
    .await
    .assert_not_found();

    // Every step is audited under plugin_trust.
    let actions: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT action, user_id FROM audit_log WHERE resource_type = 'plugin_trust' AND action LIKE 'plugin.trust.module_%' ORDER BY rowid",
    )
    .fetch_all(&app.state.db)
    .await
    .unwrap();
    let names: Vec<&str> = actions.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "plugin.trust.module_trusted",
            "plugin.trust.module_untrusted",
            "plugin.trust.module_trusted",
            "plugin.trust.module_trusted",
            "plugin.trust.module_untrusted",
        ],
        "{actions:?}"
    );
    // The automatic withdrawal on update has no user.
    assert_eq!(actions[1].1, None);
}
