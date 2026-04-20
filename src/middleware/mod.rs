//! Middleware components
//!
//! This module contains middleware for:
//! - Authentication (JWT)
//! - Authorization (RBAC)
//! - Rate limiting
//! - Security headers
//! - Client certificate authentication (mTLS)

pub mod auth;
pub mod client_cert;
pub mod rate_limit;
pub mod rbac;
pub mod security_headers;

pub use auth::{auth_middleware, optional_auth_middleware, AuthUser, Claims, TokenType};
pub use client_cert::{ClientCert, ClientCertError, OptionalClientCert};
pub use rate_limit::{
    api_rate_limit_config, auth_rate_limit_config, create_rate_limit_state, rate_limit_middleware,
    spawn_rate_limit_cleanup, RateLimitConfig, RateLimitState,
};
pub use rbac::{check_permission, require_permission_middleware, RbacError, RequirePermission};
pub use security_headers::{api_cache_control_middleware, security_headers_middleware};
