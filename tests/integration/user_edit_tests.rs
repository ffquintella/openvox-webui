//! Integration tests for editing users (username, e-mail, forced password
//! change): the administrator gate, validation, the audit entry and the
//! `user.updated` event.

use axum::{body::Body, http::Request};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::common::*;

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";

async fn call(app: &TestApp, token: &str, method: &str, uri: &str, body: Value) -> TestResponse {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    app.request(req).await
}

async fn admin_token(app: &TestApp) -> String {
    generate_test_token_with_session(
        app,
        Uuid::parse_str(ADMIN_ID).unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await
}

async fn insert_user(app: &TestApp, name: &str, provider: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role, auth_provider, created_at, updated_at) \
         VALUES (?, ?, ?, 'x', 'viewer', ?, datetime('now'), datetime('now'))",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(format!("{name}@example.com"))
    .bind(provider)
    .execute(&app.state.db)
    .await
    .unwrap();
    id
}

async fn flag(app: &TestApp, id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT force_password_change FROM users WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&app.state.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_user_without_users_admin_cannot_edit_users() {
    let app = TestApp::new().await;
    let target = insert_user(&app, "target", "local").await;
    let vic = insert_user(&app, "vic", "local").await;
    let viewer_role: String = sqlx::query_scalar("SELECT id FROM roles WHERE name = 'viewer'")
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id, created_at) VALUES (?, ?, ?, datetime('now'))")
        .bind(Uuid::new_v4().to_string())
        .bind(vic.to_string())
        .bind(viewer_role)
        .execute(&app.state.db)
        .await
        .unwrap();
    let token = generate_test_token_with_session(&app, vic, "vic", vec!["viewer".into()]).await;
    call(
        &app,
        &token,
        "PUT",
        &format!("/api/v1/users/{target}"),
        json!({"email": "evil@example.com"}),
    )
    .await
    .assert_forbidden();
}

#[tokio::test]
async fn an_administrator_edits_username_and_email_and_it_is_audited() {
    let app = TestApp::new().await;
    let admin = admin_token(&app).await;
    let target = insert_user(&app, "target", "local").await;
    let r = call(
        &app,
        &admin,
        "PUT",
        &format!("/api/v1/users/{target}"),
        json!({"username": "renamed", "email": "renamed@example.com"}),
    )
    .await;
    r.assert_ok();
    let body: Value = r.json();
    assert_eq!(body["username"], "renamed");
    assert_eq!(body["email"], "renamed@example.com");

    let details: String = sqlx::query_scalar(
        "SELECT details FROM audit_log WHERE action = 'user.updated' AND resource_id = ?",
    )
    .bind(target.to_string())
    .fetch_one(&app.state.db)
    .await
    .unwrap();
    assert!(details.contains("username") && details.contains("email"));
    assert!(!details.contains("password"));

    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_outbox WHERE type = 'user.updated' AND payload LIKE '%\"source\":\"admin\"%'",
    )
    .fetch_one(&app.state.db)
    .await
    .unwrap();
    assert_eq!(events, 1);
}

#[tokio::test]
async fn forcing_a_password_change_sets_the_flag_and_can_clear_it() {
    let app = TestApp::new().await;
    let admin = admin_token(&app).await;
    let target = insert_user(&app, "target", "local").await;
    let url = format!("/api/v1/users/{target}");

    call(
        &app,
        &admin,
        "PUT",
        &url,
        json!({"force_password_change": true}),
    )
    .await
    .assert_ok();
    assert!(flag(&app, target).await);

    call(
        &app,
        &admin,
        "PUT",
        &url,
        json!({"force_password_change": false}),
    )
    .await
    .assert_ok();
    assert!(!flag(&app, target).await);
}

#[tokio::test]
async fn a_temporary_password_forces_a_change_and_must_be_long_enough() {
    let app = TestApp::new().await;
    let admin = admin_token(&app).await;
    let target = insert_user(&app, "target", "local").await;
    let url = format!("/api/v1/users/{target}");

    call(&app, &admin, "PUT", &url, json!({"password": "short"}))
        .await
        .assert_bad_request();
    assert!(!flag(&app, target).await);

    call(
        &app,
        &admin,
        "PUT",
        &url,
        json!({"password": "temporary-pass-1"}),
    )
    .await
    .assert_ok();
    assert!(flag(&app, target).await);
}

#[tokio::test]
async fn sso_only_users_have_no_local_password_to_force() {
    let app = TestApp::new().await;
    let admin = admin_token(&app).await;
    let target = insert_user(&app, "sso-user", "saml").await;
    call(
        &app,
        &admin,
        "PUT",
        &format!("/api/v1/users/{target}"),
        json!({"force_password_change": true}),
    )
    .await
    .assert_bad_request();
    assert!(!flag(&app, target).await);
}

#[tokio::test]
async fn a_duplicate_email_is_refused() {
    let app = TestApp::new().await;
    let admin = admin_token(&app).await;
    let a = insert_user(&app, "alpha", "local").await;
    insert_user(&app, "beta", "local").await;
    let r = call(
        &app,
        &admin,
        "PUT",
        &format!("/api/v1/users/{a}"),
        json!({"email": "beta@example.com"}),
    )
    .await;
    r.assert_status(axum::http::StatusCode::CONFLICT);
}
