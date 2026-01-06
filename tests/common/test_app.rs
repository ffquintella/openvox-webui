//! Test application setup utilities
//!
//! Provides utilities for setting up test instances of the application
//! with in-memory databases and mock services.

use std::sync::Arc;

use axum::{body::Body, http::Request, Router};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use tower::ServiceExt;
use uuid::Uuid;

use openvox_webui::{
    api,
    config::{
        AppConfig, AuthConfig, CacheConfig, CodeDeployYamlConfig, DashboardConfig, DatabaseConfig,
        LoggingConfig, RbacConfig, ServerConfig,
    },
    db,
    middleware::auth::{Claims, TokenType},
    models::default_organization_uuid,
    AppState, DbRbacService, RbacService,
};

/// Test application wrapper for integration testing
pub struct TestApp {
    pub router: Router,
    pub state: AppState,
}

impl TestApp {
    /// Create a new test application with in-memory SQLite database
    pub async fn new() -> Self {
        Self::with_config(test_config()).await
    }

    /// Create a new test application with code deploy feature enabled
    pub async fn with_code_deploy() -> Self {
        Self::with_config(test_config_with_code_deploy()).await
    }

    /// Create a new test application with custom configuration
    pub async fn with_config(config: AppConfig) -> Self {
        // Initialize in-memory database
        let db = db::init_pool(&config.database)
            .await
            .expect("Failed to initialize test database");

        // Initialize RBAC services
        let rbac = Arc::new(RbacService::new());
        let rbac_db = Arc::new(DbRbacService::new(db.clone()));

        // Convert code_deploy yaml config to runtime config if enabled
        let code_deploy_config = config.code_deploy.as_ref().and_then(|c| {
            if c.enabled {
                Some(openvox_webui::services::code_deploy::CodeDeployConfig {
                    enabled: c.enabled,
                    encryption_key: c.encryption_key.clone(),
                    webhook_base_url: c.webhook_base_url.clone(),
                    retain_history_days: c.retain_history_days,
                    git: openvox_webui::services::git::GitServiceConfig {
                        repos_base_dir: c.repos_base_dir.clone(),
                        ssh_keys_dir: c.ssh_keys_dir.clone(),
                    },
                    r10k: openvox_webui::services::r10k::R10kConfig {
                        binary_path: c.r10k_binary_path.clone(),
                        config_path: c.r10k_config_path.clone(),
                        cachedir: c.r10k_cachedir.clone(),
                        basedir: c.environments_basedir.clone(),
                        ..Default::default()
                    },
                })
            } else {
                None
            }
        });

        // Create application state
        let state = AppState {
            config,
            db,
            puppetdb: None,
            puppet_ca: None,
            rbac,
            rbac_db,
            code_deploy_config,
        };

        // Build the router
        let router = Router::new()
            .nest("/api/v1", api::public_routes())
            .nest(
                "/api/v1",
                api::protected_routes().layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    openvox_webui::middleware::auth::auth_middleware,
                )),
            )
            .with_state(state.clone());

        Self { router, state }
    }

    /// Make a GET request to the test application
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a POST request with JSON body
    pub async fn post_json(&self, uri: &str, body: serde_json::Value) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    /// Make a PUT request with JSON body
    pub async fn put_json(&self, uri: &str, body: serde_json::Value) -> TestResponse {
        self.request(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    /// Make a DELETE request
    pub async fn delete(&self, uri: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a request with authentication
    pub async fn request_with_auth(&self, request: Request<Body>, token: &str) -> TestResponse {
        let (mut parts, body) = request.into_parts();
        parts.headers.insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );
        self.request(Request::from_parts(parts, body)).await
    }

    /// Make an arbitrary request
    pub async fn request(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("Failed to execute request");

        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("Failed to read response body");

        TestResponse {
            status,
            headers,
            body,
        }
    }
}

/// Response from a test request
#[derive(Debug)]
pub struct TestResponse {
    pub status: axum::http::StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: bytes::Bytes,
}

impl TestResponse {
    /// Get the response body as a string
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Parse the response body as JSON
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("Failed to parse response as JSON")
    }

    /// Check if the response status is successful (2xx)
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Assert the response status
    pub fn assert_status(&self, expected: axum::http::StatusCode) -> &Self {
        assert_eq!(
            self.status,
            expected,
            "Expected status {}, got {}. Body: {}",
            expected,
            self.status,
            self.text()
        );
        self
    }

    /// Assert the response status is OK (200)
    pub fn assert_ok(&self) -> &Self {
        self.assert_status(axum::http::StatusCode::OK)
    }

    /// Assert the response status is Created (201)
    pub fn assert_created(&self) -> &Self {
        self.assert_status(axum::http::StatusCode::CREATED)
    }

    /// Assert the response status is Bad Request (400)
    pub fn assert_bad_request(&self) -> &Self {
        self.assert_status(axum::http::StatusCode::BAD_REQUEST)
    }

    /// Assert the response status is Unauthorized (401)
    pub fn assert_unauthorized(&self) -> &Self {
        self.assert_status(axum::http::StatusCode::UNAUTHORIZED)
    }

    /// Assert the response status is Forbidden (403)
    pub fn assert_forbidden(&self) -> &Self {
        self.assert_status(axum::http::StatusCode::FORBIDDEN)
    }

    /// Assert the response status is Not Found (404)
    pub fn assert_not_found(&self) -> &Self {
        self.assert_status(axum::http::StatusCode::NOT_FOUND)
    }
}

/// Create a test configuration with temporary SQLite database
pub fn test_config() -> AppConfig {
    // Use a unique temp file for each test to avoid conflicts
    let db_path = format!(
        "/tmp/openvox_test_{}.db",
        Uuid::new_v4().to_string().replace('-', "")
    );

    AppConfig {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000, // Test port
            workers: 1,
            request_timeout_secs: None,
            tls: None,
            static_dir: None,
            serve_frontend: false,
        },
        database: DatabaseConfig {
            url: format!("sqlite://{}?mode=rwc", db_path),
            max_connections: 1,
            min_connections: 1,
            connect_timeout_secs: 30,
            idle_timeout_secs: 600,
        },
        auth: AuthConfig {
            jwt_secret: "test_secret_key_that_is_at_least_32_bytes_long".to_string(),
            token_expiry_hours: 24,
            refresh_token_expiry_days: 7,
            bcrypt_cost: 4, // Lower cost for faster tests
            password_min_length: 8,
        },
        puppetdb: None,
        puppet_ca: None,
        logging: LoggingConfig::default(),
        cache: CacheConfig {
            enabled: false, // Disable cache in tests
            ..CacheConfig::default()
        },
        dashboard: DashboardConfig::default(),
        rbac: RbacConfig::default(),
        groups_config_path: None,
        code_deploy: None,
        saml: None,
    }
}

/// Create a test configuration with code deploy enabled
pub fn test_config_with_code_deploy() -> AppConfig {
    let mut config = test_config();
    config.code_deploy = Some(CodeDeployYamlConfig {
        enabled: true,
        encryption_key: "test_encryption_key_32_bytes___!".to_string(),
        webhook_base_url: Some("http://localhost:3000".to_string()),
        ..CodeDeployYamlConfig::default()
    });
    config
}

/// Generate a test JWT token for authentication
pub fn generate_test_token(
    config: &AppConfig,
    user_id: Uuid,
    username: &str,
    roles: Vec<String>,
) -> String {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        email: format!("{}@example.com", username),
        roles,
        iat: now,
        exp: now + 3600,
        nbf: now,
        jti: Uuid::new_v4().to_string(),
        token_type: TokenType::Access,
        organization_id: Some(default_organization_uuid().to_string()),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.auth.jwt_secret.as_bytes()),
    )
    .expect("Failed to generate test token")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_creation() {
        let app = TestApp::new().await;
        assert!(app.state.puppetdb.is_none());
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = TestApp::new().await;
        let response = app.get("/api/v1/health").await;
        response.assert_ok();
    }

    #[tokio::test]
    async fn test_response_json_parsing() {
        let app = TestApp::new().await;
        let response = app.get("/api/v1/health").await;
        let json: serde_json::Value = response.json();
        assert!(json.get("status").is_some());
    }
}
