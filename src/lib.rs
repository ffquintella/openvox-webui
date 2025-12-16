//! OpenVox WebUI Library
//!
//! This crate provides the core functionality for the OpenVox WebUI application.

use std::sync::Arc;

pub mod api;
pub mod config;
pub mod db;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;
pub mod utils;

pub use config::AppConfig;
pub use db::DbPool;
pub use middleware::{auth_middleware, AuthUser, Claims};
use services::puppetdb::PuppetDbClient;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Application configuration
    pub config: AppConfig,
    /// Database connection pool
    pub db: DbPool,
    /// PuppetDB client (optional)
    pub puppetdb: Option<Arc<PuppetDbClient>>,
}
