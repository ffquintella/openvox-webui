//! Integration tests for directory providers (M30), against the in-memory
//! LDAP server: login, provisioning, group-to-role mapping, deactivation by
//! sync, the MFA pipeline and the administration API.

use std::sync::Arc;

use axum::{body::Body, http::Request};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use openvox_webui::services::{
    auth_pipeline::totp,
    directory::{fake::FakeDirectory, DirectoryConfig, DirectoryRegistry, DirectoryTransport},
    directory_sync_scheduler, AuthService,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::common::*;

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const OPERATOR_ROLE: &str = "00000000-0000-0000-0000-000000000002";
const VIEWER_ROLE: &str = "00000000-0000-0000-0000-000000000003";
const ADMIN_PASSWORD: &str = "local admin password";
const ALICE_DN: &str = "CN=Alice Doe,OU=Users,DC=example,DC=com";
const ALICE_PW: &str = "alice directory password";
const OPS_DN: &str = "CN=WebUI Operators,OU=Groups,DC=example,DC=com";
const TEAM_DN: &str = "CN=Platform Team,OU=Groups,DC=example,DC=com";

fn directory_config(jit: bool) -> DirectoryConfig {
    serde_json::from_value(json!({
        "id": "corp",
        "label": "Corporate account",
        "kind": "active_directory",
        "servers": ["ldaps://dc1.example.com"],
        "bind_dn": "CN=svc-openvox,OU=Service,DC=example,DC=com",
        "bind_password": "service-account-secret",
        "base_dn": "DC=example,DC=com",
        "domains": ["EXAMPLE", "example.com"],
        "jit": jit,
        "sync": {"enabled": true, "interval_secs": 300},
    }))
    .unwrap()
}

struct Env {
    app: TestApp,
    fake: Arc<FakeDirectory>,
    admin: String,
}

/// Alice is in "Platform Team", which is nested in "WebUI Operators".
async fn env_with(cfg: DirectoryConfig) -> Env {
    let fake = Arc::new(FakeDirectory::new());
    fake.add_service_account(
        "CN=svc-openvox,OU=Service,DC=example,DC=com",
        "service-account-secret",
    );
    fake.add_ad_user(
        ALICE_DN,
        "alice",
        Uuid::new_v4(),
        Some("alice@example.com"),
        ALICE_PW,
    );
    fake.add_ad_group(TEAM_DN, "Platform Team", Uuid::new_v4(), &[ALICE_DN]);
    fake.add_ad_group(OPS_DN, "WebUI Operators", Uuid::new_v4(), &[TEAM_DN]);
    let mut config = test_config();
    config.auth.directories = vec![cfg.clone()];
    let transport: Arc<dyn DirectoryTransport> = fake.clone();
    let registry = Arc::new(DirectoryRegistry::with_transports(vec![(cfg, transport)]));
    let app = TestApp::with_directories(config, registry).await;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(AuthService::hash_password(ADMIN_PASSWORD).unwrap())
        .bind(ADMIN_ID)
        .execute(&app.state.db)
        .await
        .unwrap();
    let admin = generate_test_token_with_session(
        &app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await;
    Env { app, fake, admin }
}

async fn env() -> Env {
    env_with(directory_config(true)).await
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
        .header("Content-Type", "application/json")
        .header("User-Agent", "directory-tests");
    if let Some(t) = token {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    app.request(b.body(Body::from(body.to_string())).unwrap())
        .await
}

async fn sync(e: &Env) -> Value {
    let r = call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/directories/corp/sync?wait=true",
        json!({}),
    )
    .await;
    r.assert_ok();
    r.json::<Value>()["run"].clone()
}

async fn login(app: &TestApp, user: &str, password: &str) -> TestResponse {
    call(
        app,
        None,
        "POST",
        "/api/v1/auth/login",
        json!({"username": user, "password": password}),
    )
    .await
}

async fn map_group(e: &Env, group: &str, role: &str) -> Value {
    let r = call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/directories/corp/role-mappings",
        json!({"group_ref": group, "role_id": role}),
    )
    .await;
    r.assert_created();
    r.json::<Value>()["mapping"].clone()
}

fn claims_of(token: &str) -> Value {
    let payload = token.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}

async fn audit_reasons(app: &TestApp, action: &str) -> Vec<String> {
    let rows: Vec<Option<String>> =
        sqlx::query_scalar("SELECT details FROM audit_log WHERE action = ? ORDER BY created_at")
            .bind(action)
            .fetch_all(&app.state.db)
            .await
            .unwrap();
    rows.into_iter()
        .flatten()
        .filter_map(|d| serde_json::from_str::<Value>(&d).ok())
        .filter_map(|v| v["reason"].as_str().map(String::from))
        .collect()
}

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    AuthService::new(app.state.db.clone())
        .get_user_by_username(username)
        .await
        .unwrap()
        .expect("user exists")
        .id
}

async fn role_names(app: &TestApp, id: Uuid) -> Vec<String> {
    let mut r = AuthService::new(app.state.db.clone())
        .get_user_roles(&id)
        .await
        .unwrap();
    r.sort();
    r
}

#[tokio::test]
async fn directory_login_provisions_the_user_with_nested_group_roles() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;

    for form in ["alice", "EXAMPLE\\alice", "alice@example.com"] {
        let r = login(&e.app, form, ALICE_PW).await;
        r.assert_ok();
        let body: Value = r.json();
        let claims = claims_of(body["access_token"].as_str().unwrap());
        assert_eq!(claims["roles"], json!(["operator"]), "{form}");
        assert_eq!(claims["amr"], json!(["pwd"]));
        assert_eq!(body["user"]["directory_id"], "corp");
        assert_eq!(body["user"]["username"], "alice");
    }

    let users: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT username, directory_id, password_hash FROM users WHERE username = 'alice'",
    )
    .fetch_all(&e.app.state.db)
    .await
    .unwrap();
    assert_eq!(users.len(), 1, "provisioned once, linked afterwards");
    assert_eq!(users[0].1.as_deref(), Some("corp"));
    assert!(
        AuthService::verify_password(ALICE_PW, &users[0].2).is_err(),
        "no local password"
    );
    let link: String =
        sqlx::query_scalar("SELECT external_id FROM user_identities WHERE provider_id = 'corp'")
            .fetch_one(&e.app.state.db)
            .await
            .unwrap();
    assert!(
        Uuid::parse_str(&link).is_ok(),
        "linked by objectGUID, not by name"
    );
    // The nested group was resolved with LDAP_MATCHING_RULE_IN_CHAIN.
    assert!(e
        .fake
        .filters()
        .iter()
        .any(|f| f.contains("member:1.2.840.113556.1.4.1941:=")));
}

#[tokio::test]
async fn wrong_password_and_unknown_user_look_the_same() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;

    let binds = e.fake.bind_count();
    let wrong = login(&e.app, "alice", "not the password").await;
    assert_eq!(e.fake.bind_count(), binds + 1);
    let binds = e.fake.bind_count();
    let unknown = login(&e.app, "mallory", "whatever password").await;
    assert_eq!(
        e.fake.bind_count(),
        binds + 1,
        "dummy bind for an unknown user"
    );

    wrong.assert_unauthorized();
    unknown.assert_unauthorized();
    assert_eq!(wrong.text(), unknown.text());
    // The dummy bind never carries the user's DN.
    assert!(!e.fake.bound_dns().last().unwrap().contains("mallory"));
    // No password ends up in the audit log.
    let dump: Vec<Option<String>> = sqlx::query_scalar("SELECT details FROM audit_log")
        .fetch_all(&e.app.state.db)
        .await
        .unwrap();
    let dump = dump.into_iter().flatten().collect::<Vec<_>>().join("\n");
    assert!(!dump.contains("not the password") && !dump.contains("whatever password"));
    assert!(audit_reasons(&e.app, "auth.login_denied")
        .await
        .contains(&"invalid_credentials".to_string()));
}

#[tokio::test]
async fn an_empty_password_never_reaches_the_directory() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    let (searches, binds) = (e.fake.search_count(), e.fake.bind_count());
    // The fake accepts an empty password as an anonymous bind, like many
    // servers; the host must refuse it first.
    login(&e.app, "alice", "").await.assert_unauthorized();
    assert_eq!(e.fake.bind_count(), binds);
    assert_eq!(e.fake.search_count(), searches);
}

#[tokio::test]
async fn filter_injection_in_the_username_matches_nothing() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    for evil in ["*", "*)(sAMAccountName=*", "alice)(|(objectClass=*", "a*"] {
        login(&e.app, evil, ALICE_PW).await.assert_unauthorized();
    }
    assert!(e.fake.filters().iter().any(|f| f.contains("\\2a\\29\\28")));
    // Never authenticated as Alice through a wildcard.
    assert!(!e.fake.bound_dns().iter().any(|d| d == ALICE_DN));
}

#[tokio::test]
async fn disabled_expired_and_locked_ad_accounts_are_refused() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    for (attr, value, reason) in [
        ("userAccountControl", "514", "account_disabled"),
        ("accountExpires", "116444736000000000", "account_expired"),
        ("msDS-User-Account-Control-Computed", "16", "account_locked"),
    ] {
        e.fake.set_attr(ALICE_DN, attr, &[value]);
        login(&e.app, "alice", ALICE_PW).await.assert_unauthorized();
        assert!(audit_reasons(&e.app, "auth.login_denied")
            .await
            .contains(&reason.to_string()));
        // Restore.
        let ok = match attr {
            "userAccountControl" => "512",
            _ => "0",
        };
        e.fake.set_attr(ALICE_DN, attr, &[ok]);
    }
    assert!(!e.fake.bound_dns().iter().any(|d| d == ALICE_DN));
    login(&e.app, "alice", ALICE_PW).await.assert_ok();
}

#[tokio::test]
async fn without_jit_or_without_a_mapping_nobody_is_created() {
    let e = env_with(directory_config(false)).await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    login(&e.app, "alice", ALICE_PW).await.assert_unauthorized();
    assert!(audit_reasons(&e.app, "auth.login_denied")
        .await
        .contains(&"identity_not_provisioned".to_string()));

    let e = env().await;
    login(&e.app, "alice", ALICE_PW).await.assert_unauthorized();
    assert!(audit_reasons(&e.app, "auth.login_denied")
        .await
        .contains(&"no_mapped_role".to_string()));
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = 'alice'")
        .fetch_one(&e.app.state.db)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn mapped_roles_follow_membership_and_manual_roles_stay() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    login(&e.app, "alice", ALICE_PW).await.assert_ok();
    let alice = user_id(&e.app, "alice").await;
    assert_eq!(role_names(&e.app, alice).await, vec!["operator"]);

    // An administrator also gives her viewer by hand.
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(alice.to_string())
        .bind(VIEWER_ROLE)
        .execute(&e.app.state.db)
        .await
        .unwrap();

    // She leaves the team: the mapped role goes, the manual one stays.
    e.fake.set_members(TEAM_DN, &[]);
    login(&e.app, "alice", ALICE_PW).await.assert_ok();
    assert_eq!(role_names(&e.app, alice).await, vec!["viewer"]);

    // Back in the team, mapped by SID-less objectGUID this time: still
    // only mapped roles are added.
    e.fake.set_members(TEAM_DN, &[ALICE_DN]);
    login(&e.app, "alice", ALICE_PW).await.assert_ok();
    assert_eq!(role_names(&e.app, alice).await, vec!["operator", "viewer"]);

    // Without any role at all, the login is refused.
    e.fake.set_members(TEAM_DN, &[]);
    sqlx::query("DELETE FROM user_roles WHERE user_id = ?")
        .bind(alice.to_string())
        .execute(&e.app.state.db)
        .await
        .unwrap();
    login(&e.app, "alice", ALICE_PW).await.assert_unauthorized();
    assert!(audit_reasons(&e.app, "auth.login_denied")
        .await
        .contains(&"no_role".to_string()));
}

#[tokio::test]
async fn sync_deactivates_a_removed_user_and_revokes_sessions_and_api_keys() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    let session: Value = login(&e.app, "alice", ALICE_PW).await.json();
    let access = session["access_token"].as_str().unwrap().to_string();
    let refresh = session["refresh_token"].as_str().unwrap().to_string();
    call(&e.app, Some(&access), "GET", "/api/v1/auth/me", json!({}))
        .await
        .assert_ok();
    let key = call(
        &e.app,
        Some(&access),
        "POST",
        "/api/v1/api-keys",
        json!({"name": "ci"}),
    )
    .await;
    key.assert_created();
    let key = key.json::<Value>()["key"].as_str().unwrap().to_string();
    let with_key = |app: &TestApp| {
        let req = Request::builder()
            .uri("/api/v1/auth/me")
            .header("X-API-Key", key.clone())
            .body(Body::empty())
            .unwrap();
        let app = app.router.clone();
        async move {
            use tower::ServiceExt;
            app.oneshot(req).await.unwrap().status()
        }
    };
    assert_eq!(with_key(&e.app).await, 200);

    // Someone else stays: an empty directory would be refused as suspicious.
    e.fake.add_ad_user(
        "CN=Bob,OU=Users,DC=example,DC=com",
        "bob",
        Uuid::new_v4(),
        None,
        "bob pw",
    );
    e.fake.remove(ALICE_DN);
    let r = call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/directories/corp/sync?wait=true",
        json!({}),
    )
    .await;
    r.assert_ok();
    let run = r.json::<Value>()["run"].clone();
    assert_eq!(run["status"], "ok");
    assert_eq!(run["deactivated"], 1);

    // Every way in is closed, and the account still exists.
    call(&e.app, Some(&access), "GET", "/api/v1/auth/me", json!({}))
        .await
        .assert_unauthorized();
    assert_eq!(with_key(&e.app).await, 401);
    call(
        &e.app,
        None,
        "POST",
        "/api/v1/auth/refresh",
        json!({"refresh_token": refresh}),
    )
    .await
    .assert_unauthorized();
    let alice = user_id(&e.app, "alice").await;
    let u = call(
        &e.app,
        Some(&e.admin),
        "GET",
        &format!("/api/v1/users/{alice}"),
        json!({}),
    )
    .await;
    u.assert_ok();
    assert_eq!(u.json::<Value>()["active"], false);

    // Audit, history and the event.
    let runs = call(
        &e.app,
        Some(&e.admin),
        "GET",
        "/api/v1/directories/corp/sync-runs",
        json!({}),
    )
    .await;
    assert_eq!(runs.json::<Value>()["runs"][0]["deactivated"], 1);
    assert!(!audit_reasons(&e.app, "directory.user_deactivated")
        .await
        .is_empty());
    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_outbox WHERE type = 'directory.sync.finished'",
    )
    .fetch_one(&e.app.state.db)
    .await
    .unwrap();
    assert_eq!(events, 1);
}

#[tokio::test]
async fn sync_follows_the_account_state_and_provisions_mapped_users() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    // Bob never logged in; he is in the mapped group, so sync creates him.
    let bob_dn = "CN=Bob,OU=Users,DC=example,DC=com";
    e.fake
        .add_ad_user(bob_dn, "bob", Uuid::new_v4(), None, "bob pw");
    e.fake.set_members(TEAM_DN, &[ALICE_DN, bob_dn]);
    // Carol is in no mapped group: never created.
    e.fake.add_ad_user(
        "CN=Carol,OU=Users,DC=example,DC=com",
        "carol",
        Uuid::new_v4(),
        None,
        "c",
    );
    login(&e.app, "alice", ALICE_PW).await.assert_ok();

    let ran =
        directory_sync_scheduler::run_due(&e.app.state.db, &e.app.state.directories, None).await;
    assert_eq!(ran, 1);
    let bob = user_id(&e.app, "bob").await;
    assert_eq!(role_names(&e.app, bob).await, vec!["operator"]);
    let carol: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = 'carol'")
        .fetch_one(&e.app.state.db)
        .await
        .unwrap();
    assert_eq!(carol, 0);
    // Not due again before the interval.
    assert_eq!(
        directory_sync_scheduler::run_due(&e.app.state.db, &e.app.state.directories, None).await,
        0
    );

    // Disabled in AD: deactivated by the next run; enabled again: reactivated.
    e.fake.set_attr(ALICE_DN, "userAccountControl", &["514"]);
    assert_eq!(sync(&e).await["deactivated"], 1);
    login(&e.app, "alice", ALICE_PW).await.assert_unauthorized();
    e.fake.set_attr(ALICE_DN, "userAccountControl", &["512"]);
    assert_eq!(sync(&e).await["reactivated"], 1);
    login(&e.app, "alice", ALICE_PW).await.assert_ok();

    // An empty answer never deactivates everyone.
    e.fake.remove(ALICE_DN);
    e.fake.remove(bob_dn);
    e.fake.remove("CN=Carol,OU=Users,DC=example,DC=com");
    let run = sync(&e).await;
    assert_eq!(run["status"], "failed");
    assert_eq!(run["deactivated"], 0);
}

#[tokio::test]
async fn mfa_policies_apply_to_directory_logins() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    let session: Value = login(&e.app, "alice", ALICE_PW).await.json();
    let token = session["access_token"].as_str().unwrap().to_string();

    // Policy for the directory provider only.
    call(
        &e.app,
        Some(&e.admin),
        "PUT",
        "/api/v1/auth/policies",
        json!({"policies": [{"name": "corp", "conditions": {"providers": ["corp"]}, "required_acr": 2}]}),
    )
    .await
    .assert_ok();
    // No factor enrolled: denied, and there is no break-glass for directory users.
    let r = login(&e.app, "alice", ALICE_PW).await;
    r.assert_forbidden();
    assert_eq!(r.json::<Value>()["code"], "mfa_enrollment_required");

    // With TOTP enrolled, the login becomes a challenge.
    let begin = call(
        &e.app,
        Some(&token),
        "POST",
        "/api/v1/users/me/factors/totp/enroll/begin",
        json!({}),
    )
    .await;
    begin.assert_ok();
    let secret = totp::secret_from_base32(
        begin.json::<Value>()["enrollment"]["secret"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let code = totp::hotp(&secret, totp::step_at(now));
    call(
        &e.app,
        Some(&token),
        "POST",
        "/api/v1/users/me/factors/totp/enroll/complete",
        json!({"code": code}),
    )
    .await
    .assert_ok();
    let r = login(&e.app, "alice", ALICE_PW).await;
    r.assert_status(axum::http::StatusCode::ACCEPTED);
    assert!(r.json::<Value>().get("access_token").is_none());

    // The local administrator is not affected by the directory policy.
    login(&e.app, "admin", ADMIN_PASSWORD).await.assert_ok();
}

#[tokio::test]
async fn local_login_never_depends_on_the_directory() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    e.fake.set_down(true);
    login(&e.app, "admin", ADMIN_PASSWORD).await.assert_ok();
    let r = login(&e.app, "alice", ALICE_PW).await;
    r.assert_unauthorized();
    assert!(audit_reasons(&e.app, "auth.login_denied")
        .await
        .contains(&"directory_unavailable".to_string()));

    // Break-glass still works for the local super administrator.
    e.fake.set_down(false);
    call(
        &e.app,
        Some(&e.admin),
        "PUT",
        "/api/v1/auth/policies",
        json!({"policies": [{"name": "all", "conditions": {}, "required_acr": 2}]}),
    )
    .await
    .assert_ok();
    login(&e.app, "admin", ADMIN_PASSWORD).await.assert_ok();
}

#[tokio::test]
async fn a_directory_identity_never_takes_over_a_local_account() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    AuthService::new(e.app.state.db.clone())
        .create_user(
            "alice",
            "alice.local@example.org",
            "local alice password",
            "viewer",
        )
        .await
        .unwrap();
    // The directory password does not open the local account, and the
    // directory identity is not attached to it by name.
    login(&e.app, "alice", ALICE_PW).await.assert_unauthorized();
    assert!(audit_reasons(&e.app, "auth.login_denied")
        .await
        .contains(&"username_conflict".to_string()));
    let linked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_identities WHERE provider_id = 'corp'")
            .fetch_one(&e.app.state.db)
            .await
            .unwrap();
    assert_eq!(linked, 0);
    login(&e.app, "alice", "local alice password")
        .await
        .assert_ok();
}

#[tokio::test]
async fn directory_users_have_read_only_identity_fields() {
    let e = env().await;
    map_group(&e, OPS_DN, OPERATOR_ROLE).await;
    let token = login(&e.app, "alice", ALICE_PW).await.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let alice = user_id(&e.app, "alice").await;
    let uri = format!("/api/v1/users/{alice}");
    for body in [
        json!({"email": "other@example.com"}),
        json!({"username": "alice2"}),
        json!({"password": "a new local password"}),
    ] {
        let r = call(&e.app, Some(&e.admin), "PUT", &uri, body).await;
        r.assert_bad_request();
        assert_eq!(r.json::<Value>()["code"], "directory_managed");
    }
    // Unchanged values and roles are fine.
    call(
        &e.app,
        Some(&e.admin),
        "PUT",
        &uri,
        json!({"email": "alice@example.com", "role": "viewer"}),
    )
    .await
    .assert_ok();
    let r = call(
        &e.app,
        Some(&token),
        "POST",
        "/api/v1/auth/change-password",
        json!({"current_password": ALICE_PW, "new_password": "something else entirely"}),
    )
    .await;
    r.assert_bad_request();
    // Password reset by e-mail issues no token for a directory account.
    let r = call(
        &e.app,
        None,
        "POST",
        "/api/v1/auth/forgot-password",
        json!({"email": "alice@example.com"}),
    )
    .await;
    r.assert_ok();
    assert!(r.json::<Value>().get("reset_token").is_none());
}

#[tokio::test]
async fn admin_api_hides_the_bind_password_and_guards_mappings() {
    let e = env().await;
    let r = call(
        &e.app,
        Some(&e.admin),
        "GET",
        "/api/v1/directories",
        json!({}),
    )
    .await;
    r.assert_ok();
    assert!(!r.text().contains("service-account-secret"));
    assert!(!r.text().contains("bind_password"));
    let body: Value = r.json();
    assert_eq!(body["directories"][0]["id"], "corp");
    assert_eq!(body["directories"][0]["kind"], "active_directory");

    let r = call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/directories/corp/test",
        json!({}),
    )
    .await;
    r.assert_ok();
    assert_eq!(r.json::<Value>()["ok"], true);

    let groups = call(
        &e.app,
        Some(&e.admin),
        "GET",
        "/api/v1/directories/corp/groups?q=webui",
        json!({}),
    )
    .await;
    groups.assert_ok();
    let g = groups.json::<Value>()["groups"][0].clone();
    assert_eq!(g["name"], "WebUI Operators");
    let users = call(
        &e.app,
        Some(&e.admin),
        "GET",
        "/api/v1/directories/corp/users?q=ali",
        json!({}),
    )
    .await;
    assert_eq!(users.json::<Value>()["users"][0]["username"], "alice");

    // Mapping by objectGUID works like mapping by DN.
    let m = map_group(&e, g["guid"].as_str().unwrap(), OPERATOR_ROLE).await;
    assert_eq!(m["group_ref_kind"], "guid");
    // super_admin can never be granted by a directory.
    let super_id: String = sqlx::query_scalar("SELECT id FROM roles WHERE name = 'super_admin'")
        .fetch_one(&e.app.state.db)
        .await
        .unwrap();
    call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/directories/corp/role-mappings",
        json!({"group_ref": OPS_DN, "role_id": super_id}),
    )
    .await
    .assert_bad_request();
    call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/directories/corp/role-mappings",
        json!({"group_ref": "not a group", "role_id": OPERATOR_ROLE}),
    )
    .await
    .assert_bad_request();

    // Provisioning ahead of the first login.
    let r = call(
        &e.app,
        Some(&e.admin),
        "POST",
        "/api/v1/directories/corp/users/provision",
        json!({"username": "alice"}),
    )
    .await;
    r.assert_created();
    assert_eq!(r.json::<Value>()["roles"]["added"], json!(["operator"]));

    let id = m["id"].as_str().unwrap();
    call(
        &e.app,
        Some(&e.admin),
        "DELETE",
        &format!("/api/v1/directories/corp/role-mappings/{id}"),
        json!({}),
    )
    .await
    .assert_status(axum::http::StatusCode::NO_CONTENT);

    // A viewer cannot see any of it.
    let viewer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'v', 'v@example.com', 'x', 'viewer')")
        .bind(viewer_id.to_string())
        .execute(&e.app.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(viewer_id.to_string())
        .bind(VIEWER_ROLE)
        .execute(&e.app.state.db)
        .await
        .unwrap();
    let viewer =
        generate_test_token_with_session(&e.app, viewer_id, "v", vec!["viewer".into()]).await;
    call(
        &e.app,
        Some(&viewer),
        "GET",
        "/api/v1/directories",
        json!({}),
    )
    .await
    .assert_forbidden();
    call(
        &e.app,
        Some(&viewer),
        "POST",
        "/api/v1/directories/corp/sync",
        json!({}),
    )
    .await
    .assert_forbidden();

    // The directory is offered as a credentials provider on the login page.
    let p: Value = e.app.get("/api/v1/auth/providers").await.json();
    assert!(p["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["id"] == "corp" && x["kind"] == "credentials"));
}
