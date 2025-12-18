//! OpenVox WebUI - Web interface for OpenVox infrastructure management
//!
//! This application provides a modern web interface for managing and monitoring
//! OpenVox infrastructure, including PuppetDB queries, node classification,
//! and facter generation.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::{info, Level};

use config::LogFormat;
use openvox_webui::{
    api, config, db, middleware, services, AppConfig, AppState, DbRbacService, RbacService,
};
use services::puppetdb::PuppetDbClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration first (before logging, so we know log format)
    let config = AppConfig::load().context("Failed to load configuration")?;

    // Initialize logging based on configuration
    init_logging(&config);

    info!("OpenVox WebUI starting up");
    info!("Configuration loaded successfully");

    // Ensure data directory exists
    ensure_data_directory(&config)?;

    // Initialize database connection pool
    info!("Initializing database connection");
    let db = db::init_pool(&config.database)
        .await
        .context("Failed to initialize database")?;

    // Initialize PuppetDB client if configured
    let puppetdb = if let Some(ref puppetdb_config) = config.puppetdb {
        info!("Initializing PuppetDB client: {}", puppetdb_config.url);
        Some(Arc::new(
            PuppetDbClient::new(puppetdb_config).context("Failed to initialize PuppetDB client")?,
        ))
    } else {
        info!("PuppetDB not configured, skipping client initialization");
        None
    };

    // Initialize Puppet CA client if configured
    let puppet_ca = if let Some(ref ca_config) = config.puppet_ca {
        info!("Initializing Puppet CA client: {}", ca_config.url);
        Some(Arc::new(
            services::PuppetCAService::new(ca_config)
                .context("Failed to initialize Puppet CA client")?,
        ))
    } else {
        info!("Puppet CA not configured, skipping client initialization");
        None
    };

    info!("Initializing RBAC service");
    let rbac = Arc::new(RbacService::new());
    info!(
        "RBAC initialized with {} system roles",
        rbac.get_roles().len()
    );

    // Initialize database-backed RBAC service
    info!("Initializing database-backed RBAC service");
    let rbac_db = Arc::new(DbRbacService::new(db.clone()));

    // Create application state
    let state = AppState {
        config: config.clone(),
        db,
        puppetdb,
        puppet_ca,
        rbac,
        rbac_db,
    };

    // Build the router
    let app = create_router(state);

    // Start the server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .context("Invalid server address configuration")?;

    info!("Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind to address")?;

    info!("Server is ready to accept connections");

    axum::serve(listener, app).await.context("Server error")?;

    Ok(())
}

/// Initialize the logging/tracing infrastructure
fn init_logging(config: &AppConfig) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    let subscriber = tracing_subscriber::registry().with(env_filter);

    match config.logging.format {
        LogFormat::Json => {
            subscriber
                .with(fmt::layer().json().with_target(true))
                .init();
        }
        LogFormat::Compact => {
            subscriber
                .with(fmt::layer().compact().with_target(false))
                .init();
        }
        LogFormat::Pretty => {
            subscriber
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false),
                )
                .init();
        }
    }
}

/// Ensure the data directory exists
fn ensure_data_directory(config: &AppConfig) -> Result<()> {
    // Extract directory from database URL
    if let Some(path) = config.database.url.strip_prefix("sqlite://") {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).context("Failed to create data directory")?;
                info!("Created data directory: {:?}", parent);
            }
        }
    }
    Ok(())
}

/// Create the application router with all routes and middleware
fn create_router(state: AppState) -> Router {
    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Configure tracing for HTTP requests
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    // Build the router
    //
    // Note: Authentication must not be applied globally, otherwise public endpoints like
    // `/api/v1/auth/login` become unusable. We keep public routes unauthenticated and apply
    // auth middleware only to protected routes.
    Router::new()
        .nest("/api/v1", api::public_routes())
        .nest(
            "/api/v1",
            api::protected_routes().layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::auth::auth_middleware,
            )),
        )
        .with_state(state.clone())
        .layer(CompressionLayer::new())
        .layer(trace_layer)
        .layer(cors)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ensure_data_directory_parsing() {
        // Test that we correctly parse the database URL
        let url = "sqlite://./data/test.db";
        let path = url.strip_prefix("sqlite://").unwrap();
        let parent = std::path::Path::new(path).parent().unwrap();
        assert_eq!(parent, std::path::Path::new("./data"));
    }
}
