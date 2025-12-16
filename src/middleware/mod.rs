//! Middleware components
//!
//! This module contains middleware for:
//! - Authentication (JWT)
//! - Authorization (RBAC)
//! - Request logging
//! - Rate limiting

pub mod auth;

pub use auth::{auth_middleware, AuthUser, Claims};
