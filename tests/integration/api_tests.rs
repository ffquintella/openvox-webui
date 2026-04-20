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

#[tokio::test]
async fn test_inventory_update_job_api_lifecycle() {
    let app = TestApp::new().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "operator",
        vec!["operator".to_string()],
    );

    let create_request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/v1/inventory/updates")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "operation_type": "package_update",
                "package_names": ["nginx"],
                "certnames": ["node1.example.com"],
                "requires_approval": true
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = app.request_with_auth(create_request, &token).await;
    create_response.assert_created();
    let created: serde_json::Value = create_response.json();
    let job_id = created["id"].as_str().expect("job id").to_string();

    let approve_request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/v1/inventory/updates/{}/approve", job_id))
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "approved": true,
                "notes": "go ahead"
            })
            .to_string(),
        ))
        .unwrap();
    let approve_response = app.request_with_auth(approve_request, &token).await;
    approve_response.assert_ok();

    let poll_request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/nodes/node1.example.com/update-jobs")
        .header("X-SSL-Client-Verify", "SUCCESS")
        .header("X-SSL-Client-CN", "node1.example.com")
        .body(axum::body::Body::empty())
        .unwrap();
    let poll_response = app.request(poll_request).await;
    poll_response.assert_ok();
    let polled: Vec<serde_json::Value> = poll_response.json();
    assert_eq!(polled.len(), 1);
    let target_id = polled[0]["target_id"]
        .as_str()
        .expect("target id")
        .to_string();

    let result_request = axum::http::Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/nodes/node1.example.com/update-jobs/{}/targets/{}/results",
            job_id, target_id
        ))
        .header("Content-Type", "application/json")
        .header("X-SSL-Client-Verify", "SUCCESS")
        .header("X-SSL-Client-CN", "node1.example.com")
        .body(axum::body::Body::from(
            serde_json::json!({
                "status": "succeeded",
                "summary": "updated nginx",
                "output": "ok"
            })
            .to_string(),
        ))
        .unwrap();
    let result_response = app.request(result_request).await;
    result_response.assert_ok();
    let completed: serde_json::Value = result_response.json();
    assert_eq!(completed["status"], "completed");

    let get_request = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/api/v1/inventory/updates/{}", job_id))
        .body(axum::body::Body::empty())
        .unwrap();
    let get_response = app.request_with_auth(get_request, &token).await;
    get_response.assert_ok();
    let fetched: serde_json::Value = get_response.json();
    assert_eq!(fetched["results"].as_array().map(|v| v.len()), Some(1));
}
