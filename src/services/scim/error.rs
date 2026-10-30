//! SCIM errors (RFC 7644 §3.12): `application/scim+json` bodies with the
//! `urn:ietf:params:scim:api:messages:2.0:Error` schema.

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

use super::{ERROR_SCHEMA, SCIM_CONTENT_TYPE};

#[derive(Debug, Clone)]
pub struct ScimError {
    pub status: StatusCode,
    /// `scimType` of RFC 7644 Table 9 (only for 400 and 409).
    pub scim_type: Option<&'static str>,
    pub detail: String,
    /// Seconds for `Retry-After` on 429.
    pub retry_after: Option<u64>,
}

impl ScimError {
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            status,
            scim_type: None,
            detail: detail.into(),
            retry_after: None,
        }
    }

    /// 400 with a `scimType` (`invalidFilter`, `invalidSyntax`, `invalidPath`,
    /// `noTarget`, `invalidValue`, `mutability`, `tooMany`).
    pub fn bad(scim_type: &'static str, detail: impl Into<String>) -> Self {
        Self {
            scim_type: Some(scim_type),
            ..Self::new(StatusCode::BAD_REQUEST, detail)
        }
    }

    pub fn invalid_value(detail: impl Into<String>) -> Self {
        Self::bad("invalidValue", detail)
    }

    pub fn invalid_syntax(detail: impl Into<String>) -> Self {
        Self::bad("invalidSyntax", detail)
    }

    pub fn mutability(detail: impl Into<String>) -> Self {
        Self::bad("mutability", detail)
    }

    /// 409 `uniqueness`.
    pub fn uniqueness(detail: impl Into<String>) -> Self {
        Self {
            scim_type: Some("uniqueness"),
            ..Self::new(StatusCode::CONFLICT, detail)
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, detail)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "A valid SCIM bearer token is required",
        )
    }

    pub fn precondition_failed() -> Self {
        Self::new(
            StatusCode::PRECONDITION_FAILED,
            "The resource version does not match If-Match",
        )
    }

    pub fn too_many_requests(retry_after: u64) -> Self {
        Self {
            retry_after: Some(retry_after.max(1)),
            ..Self::new(StatusCode::TOO_MANY_REQUESTS, "Too many requests")
        }
    }

    /// 500 without internals: the cause is logged, never returned.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        tracing::error!("SCIM request failed: {:#}", cause);
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    }
}

impl std::fmt::Display for ScimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.status.as_u16(), self.detail)
    }
}

impl From<sqlx::Error> for ScimError {
    fn from(e: sqlx::Error) -> Self {
        Self::internal(e)
    }
}

impl From<anyhow::Error> for ScimError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(e)
    }
}

impl IntoResponse for ScimError {
    fn into_response(self) -> Response {
        let mut body = json!({
            "schemas": [ERROR_SCHEMA],
            "status": self.status.as_u16().to_string(),
            "detail": self.detail,
        });
        if let Some(t) = self.scim_type {
            body["scimType"] = json!(t);
        }
        let mut resp = (self.status, body.to_string()).into_response();
        let h = resp.headers_mut();
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(SCIM_CONTENT_TYPE),
        );
        if self.status == StatusCode::UNAUTHORIZED {
            h.insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"OpenVox SCIM\""),
            );
        }
        if let Some(s) = self.retry_after {
            if let Ok(v) = HeaderValue::from_str(&s.to_string()) {
                h.insert(header::RETRY_AFTER, v);
            }
        }
        resp
    }
}
