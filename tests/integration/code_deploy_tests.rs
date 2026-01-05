//! Code Deploy API integration tests
//!
//! Tests the Code Deploy feature API endpoints for repositories,
//! environments, deployments, and SSH keys.

use crate::common::{generate_test_token, TestApp};
use axum::http::{Request, StatusCode};
use serde_json::json;
use uuid::Uuid;

// ============================================================================
// SSH Keys Tests
// ============================================================================

#[tokio::test]
async fn test_list_ssh_keys_empty() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/code/ssh-keys")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    response.assert_ok();
    let json: Vec<serde_json::Value> = response.json();
    assert!(json.is_empty(), "Should have no SSH keys initially");
}

#[tokio::test]
async fn test_create_ssh_key() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    // Create an SSH key
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/ssh-keys")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "test-key",
                "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;

    response.assert_status(StatusCode::CREATED);
    let json: serde_json::Value = response.json();
    assert_eq!(json["name"], "test-key");
    assert!(json.get("id").is_some());
    // Private key should NOT be in response
    assert!(json.get("private_key").is_none());
    assert!(json.get("private_key_encrypted").is_none());
}

#[tokio::test]
async fn test_create_ssh_key_duplicate_name() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    // Create first key
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/ssh-keys")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "duplicate-key",
                "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\ntest1\n-----END OPENSSH PRIVATE KEY-----"
            })
            .to_string(),
        ))
        .unwrap();
    app.request(request).await.assert_status(StatusCode::CREATED);

    // Try to create second key with same name
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/ssh-keys")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "duplicate-key",
                "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\ntest2\n-----END OPENSSH PRIVATE KEY-----"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_delete_ssh_key() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    // Create key
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/ssh-keys")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "key-to-delete",
                "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;
    let key: serde_json::Value = response.json();
    let key_id = key["id"].as_str().unwrap();

    // Delete key
    let request = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/code/ssh-keys/{}", key_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request(request).await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify deleted
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/code/ssh-keys/{}", key_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request(request).await;
    response.assert_not_found();
}

// ============================================================================
// Repositories Tests
// ============================================================================

#[tokio::test]
async fn test_list_repositories_empty() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/code/repositories")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    response.assert_ok();
    let json: Vec<serde_json::Value> = response.json();
    assert!(json.is_empty(), "Should have no repositories initially");
}

#[tokio::test]
async fn test_create_repository() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/repositories")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "control-repo",
                "url": "git@github.com:example/control-repo.git",
                "branch_pattern": "*",
                "poll_interval_seconds": 300,
                "is_control_repo": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;

    response.assert_status(StatusCode::CREATED);
    let json: serde_json::Value = response.json();
    assert_eq!(json["name"], "control-repo");
    assert_eq!(json["url"], "git@github.com:example/control-repo.git");
    assert_eq!(json["branch_pattern"], "*");
    assert_eq!(json["poll_interval_seconds"], 300);
    assert_eq!(json["is_control_repo"], true);
    assert!(json.get("id").is_some());
    assert!(json.get("webhook_url").is_some());
}

#[tokio::test]
async fn test_create_repository_with_defaults() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/repositories")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "minimal-repo",
                "url": "git@github.com:example/repo.git"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;

    response.assert_status(StatusCode::CREATED);
    let json: serde_json::Value = response.json();
    assert_eq!(json["name"], "minimal-repo");
    assert_eq!(json["branch_pattern"], "*"); // default
    assert_eq!(json["poll_interval_seconds"], 300); // default
    assert_eq!(json["is_control_repo"], false); // default
}

#[tokio::test]
async fn test_update_repository() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    // Create repository
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/repositories")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "repo-to-update",
                "url": "git@github.com:example/repo.git"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;
    let repo: serde_json::Value = response.json();
    let repo_id = repo["id"].as_str().unwrap();

    // Update repository
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/code/repositories/{}", repo_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "updated-repo",
                "poll_interval_seconds": 600,
                "is_control_repo": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;

    response.assert_ok();
    let json: serde_json::Value = response.json();
    assert_eq!(json["name"], "updated-repo");
    assert_eq!(json["poll_interval_seconds"], 600);
    assert_eq!(json["is_control_repo"], true);
}

#[tokio::test]
async fn test_delete_repository() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    // Create repository
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/repositories")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "name": "repo-to-delete",
                "url": "git@github.com:example/repo.git"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;
    let repo: serde_json::Value = response.json();
    let repo_id = repo["id"].as_str().unwrap();

    // Delete repository
    let request = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/code/repositories/{}", repo_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request(request).await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify deleted
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/code/repositories/{}", repo_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request(request).await;
    response.assert_not_found();
}

// ============================================================================
// Environments Tests
// ============================================================================

#[tokio::test]
async fn test_list_environments_empty() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/code/environments")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    response.assert_ok();
    let json: Vec<serde_json::Value> = response.json();
    assert!(json.is_empty(), "Should have no environments initially");
}

// ============================================================================
// Deployments Tests
// ============================================================================

#[tokio::test]
async fn test_list_deployments_empty() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/code/deployments")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;

    response.assert_ok();
    let json: Vec<serde_json::Value> = response.json();
    assert!(json.is_empty(), "Should have no deployments initially");
}

#[tokio::test]
async fn test_trigger_deployment_invalid_environment() {
    let app = TestApp::with_code_deploy().await;
    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );

    let fake_env_id = Uuid::new_v4();
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/code/deployments")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(axum::body::Body::from(
            json!({
                "environment_id": fake_env_id.to_string()
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.request(request).await;

    response.assert_not_found();
}

// ============================================================================
// Authentication Tests
// ============================================================================

#[tokio::test]
async fn test_code_deploy_endpoints_require_auth() {
    let app = TestApp::with_code_deploy().await;

    // Test without authentication
    let endpoints = vec![
        ("/api/v1/code/ssh-keys", "GET"),
        ("/api/v1/code/repositories", "GET"),
        ("/api/v1/code/environments", "GET"),
        ("/api/v1/code/deployments", "GET"),
    ];

    for (uri, method) in endpoints {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.request(request).await;
        response.assert_unauthorized();
    }
}
