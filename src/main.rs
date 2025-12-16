//! OpenVox WebUI - Web interface for OpenVox infrastructure management
//!
//! This application provides a modern web interface for managing and monitoring
//! OpenVox infrastructure, including PuppetDB queries, node classification,
//! and facter generation.

use std::net::SocketAddr;

use anyhow::Result;
use axum::Router;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

mod api;
mod config;
mod db;
mod handlers;
mod models;
mod services;
mod utils;

use crate::config::AppConfig;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    // TODO: Add database pool
    // TODO: Add PuppetDB client
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openvox_webui=debug,tower_http=debug".into()),
        )
        .init();

    // Load configuration
    let config = AppConfig::load()?;
    info!("Configuration loaded successfully");

    // Create application state
    let state = AppState {
        config: config.clone(),
    };

    // Build the router
    let app = Router::new()
        .nest("/api/v1", api::routes())
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], config.server.port));
    info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
