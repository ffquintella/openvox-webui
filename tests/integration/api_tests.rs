//! API integration tests
//!
//! Tests the API endpoints with real HTTP requests against a test server.

use crate::common::{generate_test_token, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let app = TestApp::new().await;
    let response = app.get("/api/v1/health").await;

    response.assert_ok();

    let json: serde_json::Value = response.json();
    assert_eq!(json["status"], "healthy");
}

#[tokio::test]
async fn test_detailed_health_endpoint() {
    let app = TestApp::new().await;
    let response = app.get("/api/v1/health/detailed").await;

    response.assert_ok();

    let json: serde_json::Value = response.json();
    assert!(json.get("status").is_some());
    assert!(json.get("components").is_some());
    assert!(json["components"].get("database").is_some());
}

#[tokio::test]
async fn test_liveness_probe() {
    let app = TestApp::new().await;
    let response = app.get("/api/v1/health/live").await;

    response.assert_ok();
}

#[tokio::test]
async fn test_readiness_probe() {
    let app = TestApp::new().await;
    let response = app.get("/api/v1/health/ready").await;

    response.assert_ok();
}

#[tokio::test]
async fn test_nodes_endpoint_without_puppetdb() {
    let app = TestApp::new().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/nodes")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    // Stub implementation returns empty list (PuppetDB integration to be implemented)
    response.assert_ok();
    let json: Vec<serde_json::Value> = response.json();
    assert!(json.is_empty());
}

#[tokio::test]
async fn test_reports_endpoint_without_puppetdb() {
    let app = TestApp::new().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/reports")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    // Stub implementation returns empty list (PuppetDB integration to be implemented)
    response.assert_ok();
    let json: Vec<serde_json::Value> = response.json();
    assert!(json.is_empty());
}

#[tokio::test]
async fn test_groups_endpoint_returns_default_group() {
    let app = TestApp::new().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/groups")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    response.assert_ok();

    let json: Vec<serde_json::Value> = response.json();
    // Should contain the default "All Nodes" group from migrations
    assert!(!json.is_empty(), "Should have at least the default group");

    // Check that "All Nodes" group exists
    let has_all_nodes = json
        .iter()
        .any(|g| g.get("name").and_then(|n| n.as_str()) == Some("All Nodes"));
    assert!(has_all_nodes, "Should have 'All Nodes' group");
}

#[tokio::test]
async fn test_roles_endpoint_returns_system_roles() {
    let app = TestApp::new().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/roles")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    response.assert_ok();

    let json: Vec<serde_json::Value> = response.json();
    // Should contain system roles initialized by database migrations
    assert!(!json.is_empty(), "Should have system roles");

    // Check that admin role exists
    let has_admin = json
        .iter()
        .any(|r| r.get("name").and_then(|n| n.as_str()) == Some("admin"));
    assert!(has_admin, "Should have admin role");
}

#[tokio::test]
async fn test_not_found_returns_404() {
    let app = TestApp::new().await;
    let response = app.get("/api/v1/nonexistent").await;

    response.assert_not_found();
}
