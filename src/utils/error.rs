//! Error types and handling
//!
//! This module provides a comprehensive error handling framework for the application.
//! All errors are converted to a consistent JSON response format.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use tracing::error;

/// Application error types
#[derive(Debug, Error)]
pub enum AppError {
    /// Resource not found (404)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Bad request - invalid input (400)
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Unauthorized - authentication required (401)
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden - insufficient permissions (403)
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Conflict - resource already exists or state conflict (409)
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Unprocessable entity - validation failed (422)
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Internal server error (500)
    #[error("Internal error: {0}")]
    Internal(String),

    /// PuppetDB communication error (502)
    #[error("PuppetDB error: {0}")]
    PuppetDb(String),

    /// Database error (500)
    #[error("Database error: {0}")]
    Database(String),

    /// Configuration error (500)
    #[error("Configuration error: {0}")]
    Config(String),

    /// Service unavailable (503)
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

/// Error response body
#[derive(Serialize, Debug)]
pub struct ErrorResponse {
    /// Error type identifier
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// Additional error details (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Error code for programmatic handling (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
            details: None,
            code: None,
        }
    }

    /// Add details to the error response
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Add an error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type, should_log) = match &self {
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", false),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request", false),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized", false),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden", true),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict", false),
            AppError::ValidationError(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error", false),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", true),
            AppError::PuppetDb(_) => (StatusCode::BAD_GATEWAY, "puppetdb_error", true),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database_error", true),
            AppError::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, "config_error", true),
            AppError::ServiceUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", true),
        };

        // Log server errors
        if should_log {
            error!(error = %self, error_type = error_type, "Request error");
        }

        let body = ErrorResponse::new(error_type, self.to_string());

        (status, Json(body)).into_response()
    }
}

// Implement From for common error types

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("Record not found".to_string()),
            sqlx::Error::Database(db_err) => {
                // Check for unique constraint violations
                if db_err.message().contains("UNIQUE constraint failed") {
                    AppError::Conflict("Resource already exists".to_string())
                } else {
                    AppError::Database(db_err.to_string())
                }
            }
            _ => AppError::Database(err.to_string()),
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            AppError::PuppetDb("PuppetDB request timed out".to_string())
        } else if err.is_connect() {
            AppError::PuppetDb("Failed to connect to PuppetDB".to_string())
        } else {
            AppError::PuppetDb(err.to_string())
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::BadRequest(format!("JSON parsing error: {}", err))
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        AppError::ValidationError(err.to_string())
    }
}

/// Result type alias for handlers
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AppError::NotFound("Node not found".to_string());
        assert_eq!(err.to_string(), "Not found: Node not found");
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse::new("not_found", "Resource not found");

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("not_found"));
        assert!(json.contains("Resource not found"));
    }

    #[test]
    fn test_error_response_with_details() {
        let response = ErrorResponse::new("validation_error", "Invalid input")
            .with_details(serde_json::json!({"field": "email", "reason": "invalid format"}));

        assert!(response.details.is_some());
    }

    #[test]
    fn test_sqlx_not_found_conversion() {
        let err: AppError = sqlx::Error::RowNotFound.into();
        matches!(err, AppError::NotFound(_));
    }

    #[test]
    fn test_app_result_type() {
        fn example_handler() -> AppResult<String> {
            Ok("success".to_string())
        }

        assert!(example_handler().is_ok());
    }
}
