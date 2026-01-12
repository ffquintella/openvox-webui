//! API routes and handlers
//!
//! This module defines all API endpoints and their routing.

use axum::{routing::get, Router};

use crate::AppState;

mod alerting;
mod analytics;
mod api_keys;
mod audit_logs;
mod auth;
mod backup;
mod ca;
mod code_deploy;
mod facter;
mod facts;
mod groups;
mod health;
mod nodes;
mod notifications;
mod organizations;
mod permissions;
mod query;
mod reports;
mod roles;
mod saml;
mod settings;
mod users;

pub use health::*;

/// Public API routes (no authentication required)
pub fn public_routes() -> Router<AppState> {
    Router::new()
        // Health check endpoints
        .route("/health", get(health::health_check))
        .route("/health/detailed", get(health::health_check_detailed))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        // Authentication endpoints (no auth required)
        .nest("/auth", auth::public_routes())
        // SAML SSO endpoints (no auth required)
        .nest("/auth/saml", saml::public_routes())
        // Node classification endpoint for Puppet agents (uses client cert auth)
        .nest("/nodes", nodes::public_routes())
        // Webhook endpoints (use signature verification instead of auth)
        .nest("/webhooks", code_deploy::webhook_routes())
        // Server info endpoint (needed for login page to detect SAML)
        .nest("/settings", settings::public_routes())
}

/// Protected API routes (authentication required)
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        // Protected auth endpoints (change-password, me)
        .nest("/auth", auth::protected_routes())
        // Resource endpoints
        .nest("/nodes", nodes::routes())
        .nest("/groups", groups::routes())
        .nest("/facts", facts::routes())
        .nest("/facter", facter::routes())
        .nest("/reports", reports::routes())
        .nest("/api-keys", api_keys::routes())
        .nest("/audit-logs", audit_logs::routes())
        .nest("/roles", roles::routes())
        .nest("/users", users::routes())
        .nest("/organizations", organizations::routes())
        .nest("/permissions", permissions::routes())
        .nest("/settings", settings::routes())
        // Analytics and reporting endpoints
        .nest("/analytics", analytics::routes())
        // Alerting endpoints
        .nest("/alerting", alerting::routes())
        // PQL query endpoint
        .nest("/query", query::routes())
        // CA management endpoints
        .merge(ca::routes())
        // Code deploy endpoints
        .nest("/code", code_deploy::routes())
        // Backup endpoints
        .nest("/backup", backup::routes())
        // Notification endpoints
        .nest("/notifications", notifications::routes())
}

/// Create the full API router (public + protected; useful for tests)
pub fn routes() -> Router<AppState> {
    public_routes().merge(protected_routes())
}
