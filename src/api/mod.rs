//! API routes and handlers

use axum::{routing::get, Router};

use crate::AppState;

mod health;
mod nodes;
mod groups;
mod facts;
mod reports;
mod roles;
mod users;
mod permissions;

pub use health::*;

/// Create the API router with all routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health_check))
        .nest("/nodes", nodes::routes())
        .nest("/groups", groups::routes())
        .nest("/facts", facts::routes())
        .nest("/reports", reports::routes())
        .nest("/roles", roles::routes())
        .nest("/users", users::routes())
        .nest("/permissions", permissions::routes())
}
