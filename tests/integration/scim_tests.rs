//! Integration tests for inbound SCIM 2.0 (M31, T106): token management,
//! the bearer gate, Users and Groups, group-to-role mapping, deactivation,
//! protection of local accounts, ETags, limits and events.

use axum::{body::Body, http::Request};
use openvox_webui::services::{scim::ScimConfig, AuthService};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::common::*;

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const OPERATOR_ROLE: &str = "00000000-0000-0000-0000-000000000002";
const SUPER_ADMIN_ROLE_NAME: &str = "super_admin";
const BASE: &str = "/api/v1/scim/v2";
const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const PATCH_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
const ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";

struct Env {
    app: TestApp,
    admin: String,
    token: String,
}

fn scim_config(f: impl FnOnce(&mut ScimConfig)) -> ScimConfig {
    let mut c = ScimConfig {
        enabled: true,
        ..Default::default()
    };
    f(&mut c);
    c
}

async fn env_with(cfg: ScimConfig) -> Env {
    let mut config = test_config();
    config.auth.scim = cfg;
    let app = TestApp::with_config(config).await;
    let admin = generate_test_token_with_session(
        &app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await;
    let r = call(
        &app,
        Some(&admin),
        "POST",
        "/api/v1/scim/tokens",
        json!({"name": "Entra ID"}),
    )
    .await;
    r.assert_created();
    let token = r.json::<Value>()["token"].as_str().unwrap().to_string();
    Env { app, admin, token }
}

async fn env() -> Env {
    env_with(scim_config(|_| {})).await
}

async fn call(
    app: &TestApp,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Value,
) -> TestResponse {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json");
    if let Some(t) = token {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        Body::from(body.to_string())
    };
    app.request(b.body(body).unwrap()).await
}

/// A SCIM request with the client's token and `application/scim+json`.
async fn scim(e: &Env, method: &str, path: &str, body: Value) -> TestResponse {
    scim_with(e, method, path, body, &[]).await
}

async fn scim_with(
    e: &Env,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> TestResponse {
    let mut b = Request::builder()
        .method(method)
        .uri(format!("{BASE}{path}"))
        .header("Authorization", format!("Bearer {}", e.token))
        .header("Content-Type", "application/scim+json");
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        Body::from(body.to_string())
    };
    e.app.request(b.body(body).unwrap()).await
}

fn alice_body() -> Value {
    json!({
        "schemas": [USER_SCHEMA],
        "userName": "alice@example.com",
        "externalId": "entra-alice",
        "name": {"givenName": "Alice", "familyName": "Doe"},
        "displayName": "Alice Doe",
        "emails": [{"value": "alice@example.com", "type": "work", "primary": true}],
        "active": true,
        "password": "must never be stored"
    })
}

async fn create_alice(e: &Env) -> String {
    let r = scim(e, "POST", "/Users", alice_body()).await;
    r.assert_created();
    r.json::<Value>()["id"].as_str().unwrap().to_string()
}

async fn role_names(app: &TestApp, id: &str) -> Vec<String> {
    let mut r = AuthService::new(app.state.db.clone())
        .get_user_roles(&Uuid::parse_str(id).unwrap())
        .await
        .unwrap();
    r.sort();
    r
}

async fn audit_count(app: &TestApp, action: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action = ?")
        .bind(action)
        .fetch_one(&app.state.db)
        .await
        .unwrap()
}

async fn events(app: &TestApp, kind: &str) -> Vec<Value> {
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT payload FROM event_outbox WHERE type = ? ORDER BY created_at")
            .bind(kind)
            .fetch_all(&app.state.db)
            .await
            .unwrap();
    rows.iter()
        .map(|p| serde_json::from_str(p).unwrap())
        .collect()
}

fn assert_scim_error(r: &TestResponse, status: u16) -> Value {
    assert_eq!(r.status.as_u16(), status, "{}", r.text());
    assert!(r.headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("application/scim+json"));
    let v: Value = r.json();
    assert_eq!(v["schemas"][0], ERROR_SCHEMA);
    assert_eq!(v["status"], status.to_string());
    v
}

#[tokio::test]
async fn scim_is_off_by_default_and_needs_a_bearer_token() {
    let app = TestApp::new().await;
    let r = call(&app, None, "GET", &format!("{BASE}/Users"), Value::Null).await;
    assert_scim_error(&r, 404);

    let e = env().await;
    let r = call(&e.app, None, "GET", &format!("{BASE}/Users"), Value::Null).await;
    assert_scim_error(&r, 401);
    assert!(r.headers["www-authenticate"]
        .to_str()
        .unwrap()
        .starts_with("Bearer"));
    for bad in [
        "ovscim_00000000000000000000000000000000_nope",
        "not-a-token",
        e.admin.as_str(),
    ] {
        let r = call(
            &e.app,
            Some(bad),
            "GET",
            &format!("{BASE}/Users"),
            Value::Null,
        )
        .await;
        assert_scim_error(&r, 401);
    }
    // A session cookie or JWT never opens SCIM; a SCIM token never opens the API.
    let r = call(&e.app, Some(&e.token), "GET", "/api/v1/users", Value::Null).await;
    r.assert_unauthorized();
    assert_eq!(audit_count(&e.app, "scim.auth_failed").await, 4);

    let r = scim(&e, "GET", "/ServiceProviderConfig", Value::Null).await;
    r.assert_ok();
    let spc: Value = r.json();
    assert_eq!(spc["patch"]["supported"], true);
    assert_eq!(spc["etag"]["supported"], true);
    let r = scim(&e, "GET", "/ResourceTypes", Value::Null).await;
    assert_eq!(r.json::<Value>()["totalResults"], 2);
    scim(&e, "GET", "/ResourceTypes/User", Value::Null)
        .await
        .assert_ok();
    let r = scim(&e, "GET", &format!("/Schemas/{USER_SCHEMA}"), Value::Null).await;
    assert_eq!(r.json::<Value>()["name"], "User");
    assert_scim_error(&scim(&e, "GET", "/Nope", Value::Null).await, 404);
}

#[tokio::test]
async fn tokens_are_shown_once_stored_hashed_and_can_be_rotated_or_revoked() {
    let e = env().await;
    let r = call(
        &e.app,
        Some(&e.admin),
        "GET",
        "/api/v1/scim/tokens",
        Value::Null,
    )
    .await;
    r.assert_ok();
    let listed = r.text();
    assert!(!listed.contains(&e.token));
    assert!(!listed.contains("token_hash"));
    let id = r.json::<Value>()["tokens"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(r.json::<Value>()["tokens"][0]["status"], "active");
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM scim_tokens WHERE id = ?")
        .bind(&id)
        .fetch_one(&e.app.state.db)
        .await
        .unwrap();
    assert!(!e.token.contains(&stored) && stored.len() == 64);

    let r = call(
        &e.app,
        Some(&e.admin),
        "POST",
        &format!("/api/v1/scim/tokens/{id}/rotate"),
        Value::Null,
    )
    .await;
    r.assert_ok();
    let rotated = r.json::<Value>()["token"].as_str().unwrap().to_string();
    assert_ne!(rotated, e.token);
    assert_scim_error(&scim(&e, "GET", "/Users", Value::Null).await, 401);
    let e2 = Env {
        token: rotated,
        ..e
    };
    scim(&e2, "GET", "/Users", Value::Null).await.assert_ok();
    let last_used: Option<String> =
        sqlx::query_scalar("SELECT last_used_at FROM scim_tokens WHERE id = ?")
            .bind(&id)
            .fetch_one(&e2.app.state.db)
            .await
            .unwrap();
    assert!(last_used.is_some());

    call(
        &e2.app,
        Some(&e2.admin),
        "DELETE",
        &format!("/api/v1/scim/tokens/{id}"),
        Value::Null,
    )
    .await
    .assert_status(axum::http::StatusCode::NO_CONTENT);
    assert_scim_error(&scim(&e2, "GET", "/Users", Value::Null).await, 401);
    for action in [
        "scim.token_created",
        "scim.token_rotated",
        "scim.token_revoked",
    ] {
        assert_eq!(audit_count(&e2.app, action).await, 1, "{action}");
    }

    // Expired tokens and names are checked.
    let r = call(
        &e2.app,
        Some(&e2.admin),
        "POST",
        "/api/v1/scim/tokens",
        json!({"name": "short", "expires_in_days": 1}),
    )
    .await;
    r.assert_created();
    let short = r.json::<Value>();
    sqlx::query("UPDATE scim_tokens SET expires_at = '2000-01-01T00:00:00+00:00' WHERE id = ?")
        .bind(short["info"]["id"].as_str().unwrap())
        .execute(&e2.app.state.db)
        .await
        .unwrap();
    let e3 = Env {
        token: short["token"].as_str().unwrap().to_string(),
        ..e2
    };
    assert_scim_error(&scim(&e3, "GET", "/Users", Value::Null).await, 401);
    call(
        &e3.app,
        Some(&e3.admin),
        "POST",
        "/api/v1/scim/tokens",
        json!({"name": ""}),
    )
    .await
    .assert_bad_request();
}

#[tokio::test]
async fn token_lifetime_can_be_capped() {
    let mut config = test_config();
    config.auth.scim = scim_config(|c| c.max_token_lifetime_days = Some(30));
    let app = TestApp::with_config(config).await;
    let admin = generate_test_token_with_session(
        &app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await;
    for body in [
        json!({"name": "x"}),
        json!({"name": "x", "expires_in_days": 31}),
    ] {
        call(&app, Some(&admin), "POST", "/api/v1/scim/tokens", body)
            .await
            .assert_bad_request();
    }
    let created = call(
        &app,
        Some(&admin),
        "POST",
        "/api/v1/scim/tokens",
        json!({"name": "x", "expires_in_days": 30}),
    )
    .await;
    created.assert_created();
    let info = created.json::<Value>()["info"].clone();
    // Rotation without a new lifetime keeps the expiry.
    let r = call(
        &app,
        Some(&admin),
        "POST",
        &format!(
            "/api/v1/scim/tokens/{}/rotate",
            info["id"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["info"]["expires_at"], info["expires_at"]);
}

#[tokio::test]
async fn users_are_provisioned_found_and_protected_from_local_login() {
    let e = env().await;
    let r = scim(&e, "POST", "/Users", alice_body()).await;
    r.assert_created();
    assert!(r.headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("application/scim+json"));
    let created: Value = r.json();
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(
        r.headers["location"].to_str().unwrap(),
        format!("{BASE}/Users/{id}")
    );
    assert_eq!(r.headers["etag"].to_str().unwrap(), "W/\"1\"");
    assert_eq!(created["meta"]["version"], "W/\"1\"");
    assert_eq!(created["name"]["givenName"], "Alice");
    assert!(created.get("password").is_none());

    let (dir, hash, provider): (Option<String>, String, String) =
        sqlx::query_as("SELECT directory_id, password_hash, auth_provider FROM users WHERE id = ?")
            .bind(&id)
            .fetch_one(&e.app.state.db)
            .await
            .unwrap();
    assert_eq!(dir.as_deref(), Some("scim"));
    assert!(!hash.contains("never stored") && !hash.starts_with("$argon2"));
    assert_eq!(provider, "local");
    // No local password: login fails whatever the password.
    for pw in ["must never be stored", "", "!DIRECTORY!"] {
        call(
            &e.app,
            None,
            "POST",
            "/api/v1/auth/login",
            json!({"username": "alice@example.com", "password": pw}),
        )
        .await
        .assert_unauthorized();
    }
    // An administrator cannot give it a local password either.
    let r = call(
        &e.app,
        Some(&e.admin),
        "PUT",
        &format!("/api/v1/users/{id}"),
        json!({"password": "a new local password"}),
    )
    .await;
    r.assert_bad_request();

    let r = scim(
        &e,
        "GET",
        "/Users?filter=userName%20eq%20%22ALICE%40example.com%22",
        Value::Null,
    )
    .await;
    r.assert_ok();
    let list: Value = r.json();
    assert_eq!(list["totalResults"], 1);
    assert_eq!(list["Resources"][0]["id"], id.as_str());
    let r = scim(
        &e,
        "GET",
        "/Users?filter=externalId%20eq%20%22entra-alice%22&attributes=userName",
        Value::Null,
    )
    .await;
    let list: Value = r.json();
    assert_eq!(list["totalResults"], 1);
    assert!(list["Resources"][0].get("emails").is_none());
    let r = scim(
        &e,
        "GET",
        "/Users?filter=userName%20xx%20%22a%22",
        Value::Null,
    )
    .await;
    assert_eq!(assert_scim_error(&r, 400)["scimType"], "invalidFilter");

    // Duplicate userName or externalId.
    let r = scim(&e, "POST", "/Users", alice_body()).await;
    assert_eq!(assert_scim_error(&r, 409)["scimType"], "uniqueness");
    let mut other = alice_body();
    other["userName"] = json!("someone@example.com");
    other["emails"] = json!([]);
    let r = scim(&e, "POST", "/Users", other).await;
    assert_eq!(assert_scim_error(&r, 409)["scimType"], "uniqueness");

    assert_eq!(events(&e.app, "user.created").await.len(), 1);
    assert_eq!(
        events(&e.app, "user.created").await[0]["user_id"],
        id.as_str()
    );
    assert_eq!(audit_count(&e.app, "scim.user_created").await, 1);
}

#[tokio::test]
async fn the_local_administrator_is_invisible_and_untouchable() {
    let e = env().await;
    let r = scim(
        &e,
        "GET",
        "/Users?filter=userName%20eq%20%22admin%22",
        Value::Null,
    )
    .await;
    assert_eq!(r.json::<Value>()["totalResults"], 0);
    let r = scim(&e, "GET", "/Users", Value::Null).await;
    assert_eq!(r.json::<Value>()["totalResults"], 0);
    for (method, body) in [
        ("GET", Value::Null),
        ("PUT", json!({"userName": "admin", "active": false})),
        (
            "PATCH",
            json!({"schemas": [PATCH_SCHEMA], "Operations": [{"op": "replace", "path": "active", "value": false}]}),
        ),
        ("DELETE", Value::Null),
    ] {
        let r = scim(&e, method, &format!("/Users/{ADMIN_ID}"), body).await;
        assert_scim_error(&r, 404);
    }
    for name in ["admin", "ADMIN", "Admin"] {
        let r = scim(&e, "POST", "/Users", json!({"userName": name})).await;
        assert_eq!(assert_scim_error(&r, 409)["scimType"], "uniqueness");
    }
    let r = scim(
        &e,
        "POST",
        "/Groups",
        json!({"displayName": "Admins", "members": [{"value": ADMIN_ID}]}),
    )
    .await;
    assert_eq!(assert_scim_error(&r, 400)["scimType"], "invalidValue");
    let admin = AuthService::new(e.app.state.db.clone())
        .get_user_by_id(&Uuid::parse_str(ADMIN_ID).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(admin.is_active() && admin.directory_id.is_none());
    // super_admin can never be mapped.
    let super_id: String = sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
        .bind(SUPER_ADMIN_ROLE_NAME)
        .fetch_one(&e.app.state.db)
        .await
        .unwrap();
    call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/scim/role-mappings",
        json!({"group_ref": "Admins", "role_id": super_id}),
    )
    .await
    .assert_bad_request();
}

#[tokio::test]
async fn groups_map_to_roles_and_follow_membership() {
    let e = env().await;
    let alice = create_alice(&e).await;
    // Mapping first (by display name, any case), then the group is pushed.
    let r = call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/scim/role-mappings",
        json!({"group_ref": "webui operators", "group_ref_kind": "display_name", "role_id": OPERATOR_ROLE}),
    )
    .await;
    r.assert_created();
    let r = scim(
        &e,
        "POST",
        "/Groups",
        json!({"displayName": "WebUI Operators", "externalId": "g-ops", "members": [{"value": alice}]}),
    )
    .await;
    r.assert_created();
    let group: Value = r.json();
    let gid = group["id"].as_str().unwrap().to_string();
    assert_eq!(group["members"][0]["value"], alice.as_str());
    assert_eq!(role_names(&e.app, &alice).await, vec!["operator"]);

    let user: Value = scim(&e, "GET", &format!("/Users/{alice}"), Value::Null)
        .await
        .json();
    assert_eq!(user["groups"][0]["display"], "WebUI Operators");
    let r = scim(
        &e,
        "GET",
        "/Groups?filter=displayName%20eq%20%22webui%20operators%22&excludedAttributes=members",
        Value::Null,
    )
    .await;
    let list: Value = r.json();
    assert_eq!(list["totalResults"], 1);
    assert!(list["Resources"][0].get("members").is_none());

    // Entra-style removal of the member: the role goes away.
    let r = scim(
        &e,
        "PATCH",
        &format!("/Groups/{gid}"),
        json!({"schemas": [PATCH_SCHEMA], "Operations": [
            {"op": "Remove", "path": "members", "value": [{"value": alice}]}
        ]}),
    )
    .await;
    r.assert_status(axum::http::StatusCode::NO_CONTENT);
    assert!(r.headers.contains_key("etag"));
    assert!(role_names(&e.app, &alice).await.is_empty());

    // Back in through a filter path, then the group is deleted.
    scim(
        &e,
        "PATCH",
        &format!("/Groups/{gid}"),
        json!({"schemas": [PATCH_SCHEMA], "Operations": [
            {"op": "add", "path": "members", "value": [{"value": alice}]}
        ]}),
    )
    .await
    .assert_status(axum::http::StatusCode::NO_CONTENT);
    assert_eq!(role_names(&e.app, &alice).await, vec!["operator"]);
    // A role assigned by hand survives SCIM reconciliation.
    let viewer: String = sqlx::query_scalar("SELECT id FROM roles WHERE name = 'viewer'")
        .fetch_one(&e.app.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id, created_at) VALUES (?, ?, ?, ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(&alice)
        .bind(&viewer)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&e.app.state.db)
        .await
        .unwrap();
    scim(&e, "DELETE", &format!("/Groups/{gid}"), Value::Null)
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);
    assert_eq!(role_names(&e.app, &alice).await, vec!["viewer"]);
    assert_scim_error(
        &scim(&e, "GET", &format!("/Groups/{gid}"), Value::Null).await,
        404,
    );

    // Admin view of groups and mappings.
    let r = call(
        &e.app,
        Some(&e.admin),
        "GET",
        "/api/v1/scim/role-mappings",
        Value::Null,
    )
    .await;
    let mapping_id = r.json::<Value>()["mappings"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &e.app,
        Some(&e.admin),
        "DELETE",
        &format!("/api/v1/scim/role-mappings/{mapping_id}"),
        Value::Null,
    )
    .await
    .assert_status(axum::http::StatusCode::NO_CONTENT);
    let r = call(&e.app, Some(&e.admin), "GET", "/api/v1/scim", Value::Null).await;
    r.assert_ok();
    let status: Value = r.json();
    assert_eq!(status["enabled"], true);
    assert_eq!(status["endpoint"], BASE);
    assert_eq!(status["counts"]["users_active"], 1);
    assert_eq!(status["tokens"]["active"], 1);
    assert!(!status["recent"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn deactivation_revokes_sessions_and_api_keys_and_delete_keeps_the_account() {
    let e = env().await;
    let alice = create_alice(&e).await;
    let uid = Uuid::parse_str(&alice).unwrap();
    let session = generate_test_token_with_session(&e.app, uid, "alice@example.com", vec![]).await;
    call(
        &e.app,
        Some(&session),
        "GET",
        "/api/v1/auth/me",
        Value::Null,
    )
    .await
    .assert_ok();
    sqlx::query(
        "INSERT INTO api_keys (id, user_id, name, key_hash, created_at) VALUES (?, ?, 'k', 'x', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&alice)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&e.app.state.db)
    .await
    .unwrap();

    // ETags: a stale If-Match is refused, If-None-Match answers 304.
    let r = scim_with(
        &e,
        "PATCH",
        &format!("/Users/{alice}"),
        json!({"schemas": [PATCH_SCHEMA], "Operations": [{"op": "Replace", "path": "active", "value": "False"}]}),
        &[("If-Match", "W/\"7\"")],
    )
    .await;
    assert_scim_error(&r, 412);
    let r = scim_with(
        &e,
        "GET",
        &format!("/Users/{alice}"),
        Value::Null,
        &[("If-None-Match", "W/\"1\"")],
    )
    .await;
    assert_eq!(r.status.as_u16(), 304);

    // Entra deactivates with a string boolean.
    let r = scim_with(
        &e,
        "PATCH",
        &format!("/Users/{alice}"),
        json!({"schemas": [PATCH_SCHEMA], "Operations": [{"op": "Replace", "path": "active", "value": "False"}]}),
        &[("If-Match", "W/\"1\"")],
    )
    .await;
    r.assert_ok();
    let user: Value = r.json();
    assert_eq!(user["active"], false);
    assert_eq!(user["meta"]["version"], "W/\"2\"");
    call(
        &e.app,
        Some(&session),
        "GET",
        "/api/v1/auth/me",
        Value::Null,
    )
    .await
    .assert_unauthorized();
    let live_keys: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM api_keys WHERE user_id = ? AND revoked_at IS NULL",
    )
    .bind(&alice)
    .fetch_one(&e.app.state.db)
    .await
    .unwrap();
    assert_eq!(live_keys, 0);
    assert_eq!(events(&e.app, "user.deactivated").await.len(), 1);
    assert_eq!(audit_count(&e.app, "scim.user_deactivated").await, 1);

    // Okta-style reactivation and an update by PUT.
    let r = scim(
        &e,
        "PATCH",
        &format!("/Users/{alice}"),
        json!({"schemas": [PATCH_SCHEMA], "Operations": [{"op": "replace", "value": {"active": true, "name.givenName": "Alicia"}}]}),
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["active"], true);
    assert_eq!(r.json::<Value>()["name"]["givenName"], "Alicia");
    let mut put = alice_body();
    put["displayName"] = json!("Alicia Doe");
    put["userName"] = json!("alicia@example.com");
    let r = scim(&e, "PUT", &format!("/Users/{alice}"), put).await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["userName"], "alicia@example.com");
    let updated = events(&e.app, "user.updated").await;
    assert!(updated.len() >= 2);

    // DELETE deactivates and hides the resource; the account stays.
    scim(&e, "DELETE", &format!("/Users/{alice}"), Value::Null)
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);
    assert_scim_error(
        &scim(&e, "GET", &format!("/Users/{alice}"), Value::Null).await,
        404,
    );
    let kept = AuthService::new(e.app.state.db.clone())
        .get_user_by_id(&uid)
        .await
        .unwrap()
        .expect("the account is kept");
    assert!(!kept.is_active());
    assert_eq!(events(&e.app, "user.deactivated").await.len(), 2);

    // Provisioning the same identity again brings the account back.
    let mut again = alice_body();
    again["userName"] = json!("alicia@example.com");
    let r = scim(&e, "POST", "/Users", again).await;
    r.assert_created();
    assert_eq!(r.json::<Value>()["id"], alice.as_str());
    assert_eq!(r.json::<Value>()["active"], true);
}

#[tokio::test]
async fn invalid_requests_get_scim_errors() {
    let e = env().await;
    let alice = create_alice(&e).await;
    let r = scim(&e, "POST", "/Users", json!({"displayName": "no user name"})).await;
    assert_eq!(assert_scim_error(&r, 400)["scimType"], "invalidValue");
    let r = e
        .app
        .request(
            Request::builder()
                .method("POST")
                .uri(format!("{BASE}/Users"))
                .header("Authorization", format!("Bearer {}", e.token))
                .header("Content-Type", "application/scim+json")
                .body(Body::from("{not json"))
                .unwrap(),
        )
        .await;
    assert_eq!(assert_scim_error(&r, 400)["scimType"], "invalidSyntax");
    let r = e
        .app
        .request(
            Request::builder()
                .method("POST")
                .uri(format!("{BASE}/Users"))
                .header("Authorization", format!("Bearer {}", e.token))
                .header("Content-Type", "text/plain")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_scim_error(&r, 415);
    let r = scim(
        &e,
        "PATCH",
        &format!("/Users/{alice}"),
        json!({"schemas": [PATCH_SCHEMA], "Operations": [{"op": "remove", "path": "userName"}]}),
    )
    .await;
    assert_eq!(assert_scim_error(&r, 400)["scimType"], "mutability");
    let r = scim(
        &e,
        "PATCH",
        &format!("/Users/{alice}"),
        json!({"schemas": [PATCH_SCHEMA], "Operations": [{"op": "add", "path": "groups", "value": []}]}),
    )
    .await;
    assert_eq!(assert_scim_error(&r, 400)["scimType"], "mutability");
    let r = scim(&e, "GET", "/Users/not-a-uuid", Value::Null).await;
    assert_scim_error(&r, 404);
}

#[tokio::test]
async fn rate_and_body_limits_apply() {
    let e = env_with(scim_config(|c| {
        c.rate_limit_per_minute = 3;
        c.max_body_bytes = 4096;
    }))
    .await;
    let big = json!({"userName": "x".repeat(5000)});
    let r = scim(&e, "POST", "/Users", big).await;
    assert_scim_error(&r, 413);
    scim(&e, "GET", "/Users", Value::Null).await.assert_ok();
    scim(&e, "GET", "/Users", Value::Null).await.assert_ok();
    let r = scim(&e, "GET", "/Users", Value::Null).await;
    assert_scim_error(&r, 429);
    assert!(r.headers.contains_key("retry-after"));
}

#[tokio::test]
async fn failed_authentications_are_limited_without_locking_out_valid_tokens() {
    let e = env_with(scim_config(|c| c.max_auth_failures_per_minute = 2)).await;
    let bad = "ovscim_00000000000000000000000000000000_wrong";
    for _ in 0..2 {
        let r = call(
            &e.app,
            Some(bad),
            "GET",
            &format!("{BASE}/Users"),
            Value::Null,
        )
        .await;
        assert_scim_error(&r, 401);
    }
    let r = call(
        &e.app,
        Some(bad),
        "GET",
        &format!("{BASE}/Users"),
        Value::Null,
    )
    .await;
    assert_scim_error(&r, 429);
    assert_eq!(audit_count(&e.app, "scim.auth_failed").await, 2);
    scim(&e, "GET", "/Users", Value::Null).await.assert_ok();
}

#[tokio::test]
async fn scim_users_are_linked_to_their_sso_identity() {
    let e = env_with(scim_config(|c| c.sso_provider = Some("corp-oidc".into()))).await;
    let alice = create_alice(&e).await;
    let linked: Option<String> = sqlx::query_scalar(
        "SELECT external_id FROM user_identities WHERE provider_id = 'corp-oidc' AND user_id = ?",
    )
    .bind(&alice)
    .fetch_optional(&e.app.state.db)
    .await
    .unwrap();
    assert_eq!(linked.as_deref(), Some("alice@example.com"));
    scim(&e, "DELETE", &format!("/Users/{alice}"), Value::Null)
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_identities WHERE provider_id = 'corp-oidc'")
            .fetch_one(&e.app.state.db)
            .await
            .unwrap();
    assert_eq!(left, 0);

    let saml = env_with(scim_config(|c| c.sso_provider = Some("saml".into()))).await;
    let bob = scim(
        &saml,
        "POST",
        "/Users",
        json!({"userName": "bob@example.com", "externalId": "b-1"}),
    )
    .await;
    bob.assert_created();
    let (provider, ext): (String, Option<String>) = sqlx::query_as(
        "SELECT auth_provider, external_id FROM users WHERE username = 'bob@example.com'",
    )
    .fetch_one(&saml.app.state.db)
    .await
    .unwrap();
    assert_eq!(provider, "saml");
    assert_eq!(ext.as_deref(), Some("bob@example.com"));
}

#[tokio::test]
async fn scim_administration_needs_users_admin() {
    let e = env().await;
    let viewer_id: Uuid =
        sqlx::query_scalar::<_, String>("SELECT id FROM roles WHERE name = 'viewer'")
            .fetch_one(&e.app.state.db)
            .await
            .map(|s| Uuid::parse_str(&s).unwrap())
            .unwrap();
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role, created_at, updated_at) \
         VALUES (?, 'vic', 'vic@example.com', 'x', 'viewer', datetime('now'), datetime('now'))",
    )
    .bind(uid.to_string())
    .execute(&e.app.state.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id, created_at) VALUES (?, ?, ?, datetime('now'))")
        .bind(Uuid::new_v4().to_string())
        .bind(uid.to_string())
        .bind(viewer_id.to_string())
        .execute(&e.app.state.db)
        .await
        .unwrap();
    let vic = generate_test_token_with_session(&e.app, uid, "vic", vec!["viewer".into()]).await;
    for (method, path, body) in [
        ("GET", "/api/v1/scim", Value::Null),
        ("GET", "/api/v1/scim/tokens", Value::Null),
        ("POST", "/api/v1/scim/tokens", json!({"name": "x"})),
        ("GET", "/api/v1/scim/role-mappings", Value::Null),
    ] {
        call(&e.app, Some(&vic), method, path, body)
            .await
            .assert_forbidden();
    }
}
