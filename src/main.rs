//! OpenVox WebUI - Web interface for OpenVox infrastructure management
//!
//! This application provides a modern web interface for managing and monitoring
//! OpenVox infrastructure, including PuppetDB queries, node classification,
//! and facter generation.

use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::{info, warn, Level};

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
    // The guard must be kept alive for the duration of the program
    // to ensure log messages are flushed to files
    let _log_guard = init_logging(&config);

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
    let app = create_router(state, &config);

    // Start the server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .context("Invalid server address configuration")?;

    // Check if TLS is configured
    if let Some(ref tls_config) = config.server.tls {
        info!("Starting HTTPS server on https://{}", addr);
        info!("TLS certificate: {:?}", tls_config.cert_file);
        info!("TLS minimum version: {}", tls_config.min_version);

        let rustls_config = create_rustls_config(tls_config).await?;
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .context("Failed to bind to address")?;

        info!("HTTPS server is ready to accept connections");

        // Use axum-server for TLS with ConnectInfo support
        axum_server::from_tcp_rustls(listener.into_std()?, rustls_config)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .context("HTTPS server error")?;
    } else {
        info!("Starting HTTP server on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .context("Failed to bind to address")?;

        info!("HTTP server is ready to accept connections");

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .context("HTTP server error")?;
    }

    Ok(())
}

/// Create RusTLS configuration from TLS config
async fn create_rustls_config(tls_config: &config::TlsConfig) -> Result<axum_server::tls_rustls::RustlsConfig> {
    use axum_server::tls_rustls::RustlsConfig;
    use rustls::crypto::ring::default_provider;
    use rustls::ServerConfig;

    // Load certificate chain
    let cert_file = std::fs::File::open(&tls_config.cert_file)
        .with_context(|| format!("Failed to open certificate file: {:?}", tls_config.cert_file))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<_> = rustls_pemfile::certs(&mut cert_reader)
        .filter_map(|r| r.ok())
        .collect();

    if certs.is_empty() {
        anyhow::bail!("No certificates found in {:?}", tls_config.cert_file);
    }

    // Load private key
    let key_file = std::fs::File::open(&tls_config.key_file)
        .with_context(|| format!("Failed to open key file: {:?}", tls_config.key_file))?;
    let mut key_reader = BufReader::new(key_file);
    let key = rustls_pemfile::private_key(&mut key_reader)
        .with_context(|| format!("Failed to read private key: {:?}", tls_config.key_file))?
        .ok_or_else(|| anyhow::anyhow!("No private key found in {:?}", tls_config.key_file))?;

    // Get the crypto provider
    let provider = default_provider();

    // Determine minimum TLS version from config
    let versions: Vec<&'static rustls::SupportedProtocolVersion> = match tls_config.min_version.as_str() {
        "1.3" => vec![&rustls::version::TLS13],
        "1.2" | _ => vec![&rustls::version::TLS12, &rustls::version::TLS13],
    };

    info!("TLS configured with minimum version: {}", tls_config.min_version);
    info!("Enabled TLS versions: {:?}", versions.iter().map(|v| format!("{:?}", v.version)).collect::<Vec<_>>());

    // Build ServerConfig with specified TLS versions
    let mut server_config = ServerConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&versions)
        .context("Failed to set TLS protocol versions")?
        .with_no_client_auth()
        .with_single_cert(certs, key.into())
        .context("Failed to build TLS server config")?;

    // Enable ALPN for HTTP/1.1 and HTTP/2
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    // Build RustlsConfig from ServerConfig
    let config = RustlsConfig::from_config(Arc::new(server_config));

    Ok(config)
}

/// Initialize the logging/tracing infrastructure
fn init_logging(config: &AppConfig) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use config::LogTarget;
    use tracing_subscriber::{prelude::*, EnvFilter};

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    let log_config = &config.logging;

    match &log_config.target {
        LogTarget::Console => {
            // Console-only logging (development mode)
            let subscriber = tracing_subscriber::registry().with(env_filter);
            init_console_logging(subscriber, &log_config.format);
            None
        }
        LogTarget::File => {
            // File-only logging (production mode)
            let (writer, guard) = create_file_writer(log_config);
            let subscriber = tracing_subscriber::registry().with(env_filter);
            init_file_logging(subscriber, &log_config.format, writer);
            Some(guard)
        }
        LogTarget::Both => {
            // Both console and file logging
            let (writer, guard) = create_file_writer(log_config);
            let subscriber = tracing_subscriber::registry().with(env_filter);
            init_both_logging(subscriber, &log_config.format, writer);
            Some(guard)
        }
    }
}

/// Create a file writer with optional daily rotation
fn create_file_writer(
    log_config: &config::LoggingConfig,
) -> (
    tracing_appender::non_blocking::NonBlocking,
    tracing_appender::non_blocking::WorkerGuard,
) {
    // Ensure log directory exists
    if let Err(e) = std::fs::create_dir_all(&log_config.log_dir) {
        eprintln!(
            "Warning: Failed to create log directory {:?}: {}",
            log_config.log_dir, e
        );
    }

    let file_appender = if log_config.daily_rotation {
        tracing_appender::rolling::daily(&log_config.log_dir, &log_config.log_prefix)
    } else {
        tracing_appender::rolling::never(&log_config.log_dir, &log_config.log_prefix)
    };

    tracing_appender::non_blocking(file_appender)
}

/// Initialize console-only logging
fn init_console_logging<S>(subscriber: S, format: &LogFormat)
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    use tracing_subscriber::{fmt, prelude::*};

    match format {
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

/// Initialize file-only logging
fn init_file_logging<S>(
    subscriber: S,
    format: &LogFormat,
    writer: tracing_appender::non_blocking::NonBlocking,
) where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    use tracing_subscriber::{fmt, prelude::*};

    match format {
        LogFormat::Json => {
            subscriber
                .with(fmt::layer().json().with_target(true).with_writer(writer))
                .init();
        }
        LogFormat::Compact => {
            subscriber
                .with(
                    fmt::layer()
                        .compact()
                        .with_target(false)
                        .with_writer(writer),
                )
                .init();
        }
        LogFormat::Pretty => {
            subscriber
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false)
                        .with_writer(writer),
                )
                .init();
        }
    }
}

/// Initialize both console and file logging
fn init_both_logging<S>(
    subscriber: S,
    format: &LogFormat,
    writer: tracing_appender::non_blocking::NonBlocking,
) where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    use tracing_subscriber::{fmt, prelude::*};

    match format {
        LogFormat::Json => {
            subscriber
                .with(fmt::layer().json().with_target(true)) // Console
                .with(fmt::layer().json().with_target(true).with_writer(writer)) // File
                .init();
        }
        LogFormat::Compact => {
            subscriber
                .with(fmt::layer().compact().with_target(false)) // Console
                .with(
                    fmt::layer()
                        .compact()
                        .with_target(false)
                        .with_writer(writer),
                ) // File
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
                ) // Console
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false)
                        .with_writer(writer),
                ) // File
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
fn create_router(state: AppState, config: &AppConfig) -> Router {
    // Configure CORS - only needed when frontend is served separately (development)
    // When serving frontend from the same server, CORS is not needed
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Configure tracing for HTTP requests
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    // Initialize rate limiting
    let api_rate_limit = middleware::create_rate_limit_state(middleware::api_rate_limit_config());
    let auth_rate_limit =
        middleware::create_rate_limit_state(middleware::auth_rate_limit_config());

    // Spawn background cleanup task for rate limiters
    middleware::spawn_rate_limit_cleanup(api_rate_limit.clone());

    // Build the API router
    //
    // Note: Authentication must not be applied globally, otherwise public endpoints like
    // `/api/v1/auth/login` become unusable. We keep public routes unauthenticated and apply
    // auth middleware only to protected routes.
    //
    // Rate limiting is applied:
    // - Stricter limits on auth endpoints (brute force protection)
    // - Standard limits on all other API endpoints
    let api_router = Router::new()
        .nest(
            "/api/v1",
            api::public_routes().layer(axum::middleware::from_fn_with_state(
                auth_rate_limit,
                middleware::rate_limit_middleware,
            )),
        )
        .nest(
            "/api/v1",
            api::protected_routes()
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::auth::auth_middleware,
                ))
                .layer(axum::middleware::from_fn_with_state(
                    api_rate_limit,
                    middleware::rate_limit_middleware,
                )),
        )
        .layer(axum::middleware::from_fn(
            middleware::api_cache_control_middleware,
        ))
        .with_state(state.clone());

    // Optionally serve frontend static files
    let router = if config.server.serve_frontend {
        if let Some(ref static_dir) = config.server.static_dir {
            if static_dir.exists() {
                info!("Serving frontend from {:?}", static_dir);

                // Serve index.html for the root and as a fallback for SPA routing
                let index_file = static_dir.join("index.html");
                if index_file.exists() {
                    // Create a service that serves static files and falls back to index.html
                    let serve_dir = ServeDir::new(static_dir)
                        .not_found_service(ServeFile::new(&index_file));

                    api_router.fallback_service(serve_dir)
                } else {
                    warn!("index.html not found in {:?}, SPA fallback disabled", static_dir);
                    let serve_dir = ServeDir::new(static_dir);
                    api_router.fallback_service(serve_dir)
                }
            } else {
                warn!("Static directory {:?} does not exist, frontend not served", static_dir);
                api_router
            }
        } else {
            info!("No static directory configured, frontend not served");
            api_router
        }
    } else {
        info!("Frontend serving disabled by configuration");
        api_router
    };

    // Apply global middleware layers:
    // 1. Security headers (HSTS, CSP, X-Frame-Options, etc.)
    // 2. Compression
    // 3. Request tracing
    // 4. CORS
    router
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
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
