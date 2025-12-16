//! Health check endpoints
//!
//! Provides health check endpoints for monitoring and load balancers.

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{db, AppState};

/// Basic health response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Detailed health response with component status
#[derive(Serialize)]
pub struct DetailedHealthResponse {
    pub status: String,
    pub version: String,
    pub components: ComponentHealth,
}

/// Health status of individual components
#[derive(Serialize)]
pub struct ComponentHealth {
    pub database: ComponentStatus,
    pub puppetdb: ComponentStatus,
}

/// Status of a single component
#[derive(Serialize)]
pub struct ComponentStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ComponentStatus {
    fn healthy() -> Self {
        Self {
            status: "healthy".to_string(),
            message: None,
        }
    }

    fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: "unhealthy".to_string(),
            message: Some(message.into()),
        }
    }

    fn not_configured() -> Self {
        Self {
            status: "not_configured".to_string(),
            message: None,
        }
    }
}

/// Simple health check endpoint (for load balancers)
///
/// Returns 200 OK if the service is running.
/// Does not check component health.
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Detailed health check endpoint
///
/// Checks the health of all components (database, PuppetDB).
/// Returns 200 if all components are healthy, 503 otherwise.
pub async fn health_check_detailed(
    State(state): State<AppState>,
) -> (StatusCode, Json<DetailedHealthResponse>) {
    // Check database health
    let database_status = match db::check_health(&state.db).await {
        Ok(_) => ComponentStatus::healthy(),
        Err(e) => ComponentStatus::unhealthy(e.to_string()),
    };

    // Check PuppetDB health (if configured)
    let puppetdb_status = if let Some(ref _client) = state.puppetdb {
        // TODO: Add actual health check when PuppetDB client supports it
        ComponentStatus::healthy()
    } else {
        ComponentStatus::not_configured()
    };

    // Determine overall status
    let overall_healthy = database_status.status == "healthy"
        && (puppetdb_status.status == "healthy" || puppetdb_status.status == "not_configured");

    let status_code = if overall_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let response = DetailedHealthResponse {
        status: if overall_healthy {
            "healthy".to_string()
        } else {
            "unhealthy".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        components: ComponentHealth {
            database: database_status,
            puppetdb: puppetdb_status,
        },
    };

    (status_code, Json(response))
}

/// Liveness probe (for Kubernetes)
///
/// Returns 200 OK if the process is alive.
pub async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// Readiness probe (for Kubernetes)
///
/// Returns 200 OK if the service is ready to accept traffic.
pub async fn readiness(State(state): State<AppState>) -> StatusCode {
    // Check if database is accessible
    match db::check_health(&state.db).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_returns_healthy() {
        let response = health_check().await;
        assert_eq!(response.status, "healthy");
    }

    #[tokio::test]
    async fn test_health_check_returns_version() {
        let response = health_check().await;
        assert!(!response.version.is_empty());
    }

    #[test]
    fn test_component_status_healthy() {
        let status = ComponentStatus::healthy();
        assert_eq!(status.status, "healthy");
        assert!(status.message.is_none());
    }

    #[test]
    fn test_component_status_unhealthy() {
        let status = ComponentStatus::unhealthy("Connection failed");
        assert_eq!(status.status, "unhealthy");
        assert_eq!(status.message.unwrap(), "Connection failed");
    }
}
