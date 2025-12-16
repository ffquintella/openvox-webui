//! API routes and handlers
//!
//! This module defines all API endpoints and their routing.

use axum::{routing::get, Router};

use crate::AppState;

mod facts;
mod groups;
mod health;
mod nodes;
mod permissions;
mod reports;
mod roles;
mod users;

pub use health::*;

/// Create the API router with all routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Health check endpoints
        .route("/health", get(health::health_check))
        .route("/health/detailed", get(health::health_check_detailed))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        // Resource endpoints
        .nest("/nodes", nodes::routes())
        .nest("/groups", groups::routes())
        .nest("/facts", facts::routes())
        .nest("/reports", reports::routes())
        .nest("/roles", roles::routes())
        .nest("/users", users::routes())
        .nest("/permissions", permissions::routes())
}
