//! Middleware components
//!
//! This module contains middleware for:
//! - Authentication (JWT)
//! - Authorization (RBAC)
//! - Request logging
//! - Rate limiting

pub mod auth;
pub mod rbac;

pub use auth::{auth_middleware, optional_auth_middleware, AuthUser, Claims, TokenType};
pub use rbac::{check_permission, require_permission_middleware, RbacError, RequirePermission};
