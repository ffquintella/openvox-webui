//! OpenVox WebUI - Web interface for OpenVox infrastructure management
//!
//! This application provides a modern web interface for managing and monitoring
//! OpenVox infrastructure, including PuppetDB queries, node classification,
//! and facter generation.

use std::env;
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
use services::notification::NotificationService;
use services::puppetdb::PuppetDbClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    // Check for --fix-database flag
    if args.iter().any(|arg| arg == "--fix-database") {
        return fix_database().await;
    }

    // Check for --check-r10k-permissions flag
    if args.iter().any(|arg| arg == "--check-r10k-permissions") {
        return check_r10k_permissions();
    }

    // Check for --help flag
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    // Check for --version flag
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("OpenVox WebUI {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

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

    // Initialize Code Deploy config if enabled
    let code_deploy_config = config.code_deploy.as_ref().and_then(|cd| {
        if cd.enabled {
            info!("Initializing Code Deploy service");
            Some(services::code_deploy::CodeDeployConfig {
                git: services::git::GitServiceConfig {
                    repos_base_dir: cd.repos_base_dir.clone(),
                    ssh_keys_dir: cd.ssh_keys_dir.clone(),
                },
                r10k: services::r10k::R10kConfig {
                    binary_path: cd.r10k_binary_path.clone(),
                    config_path: cd.r10k_config_path.clone(),
                    basedir: cd.environments_basedir.clone(),
                    cachedir: cd.r10k_cachedir.clone(),
                    ..Default::default()
                },
                enabled: true,
                encryption_key: cd.encryption_key.clone(),
                webhook_base_url: cd.webhook_base_url.clone(),
                retain_history_days: cd.retain_history_days,
            })
        } else {
            info!("Code Deploy feature is disabled");
            None
        }
    });

    // Start Code Deploy scheduler if enabled
    let _code_deploy_scheduler = if let Some(ref cd_config) = code_deploy_config {
        info!("Starting Code Deploy scheduler");
        Some(services::start_code_deploy_scheduler(db.clone(), cd_config.clone()))
    } else {
        None
    };

    // Initialize Backup config if enabled
    let backup_config = config.backup.clone().and_then(|backup| {
        if backup.enabled {
            info!("Backup feature enabled, backup dir: {:?}", backup.backup_dir);
            Some(backup)
        } else {
            info!("Backup feature is disabled");
            None
        }
    });

    // Start Backup scheduler if enabled
    let _backup_scheduler = if let Some(ref backup_cfg) = backup_config {
        info!("Starting Backup scheduler");
        Some(services::start_backup_scheduler(db.clone(), backup_cfg.clone()))
    } else {
        None
    };

    // Initialize notification service
    info!("Initializing notification service");
    let notification_service = Arc::new(NotificationService::new(db.clone()));

    // Create application state
    let state = AppState {
        config: config.clone(),
        db,
        puppetdb,
        puppet_ca,
        rbac,
        rbac_db,
        code_deploy_config,
        backup_config,
        notification_service,
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
        axum_server::from_tcp_rustls(listener.into_std()?, rustls_config)?
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
    use rustls::crypto::aws_lc_rs::default_provider;
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

    // Get the crypto provider (using aws-lc-rs as the default provider)
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

/// Print help message
fn print_help() {
    println!(
        r#"OpenVox WebUI {}

USAGE:
    openvox-webui [OPTIONS]

OPTIONS:
    -h, --help              Print this help message
    -V, --version           Print version information
    --fix-database          Fix database by running all migrations and ensuring
                            all required tables exist. This is useful when
                            upgrading from an older version or recovering from
                            migration failures.
    --check-r10k-permissions
                            Check r10k directory permissions and show commands
                            to fix them. Run this if Code Deploy fails with
                            "Read-only file system" errors.

ENVIRONMENT:
    OPENVOX_CONFIG      Path to configuration file (default: config.yaml)

CONFIGURATION:
    The application looks for configuration files in the following order:
    1. Path specified by OPENVOX_CONFIG environment variable
    2. ./config.yaml
    3. /etc/openvox-webui/config.yaml

For more information, see: https://github.com/openvoxproject/openvox-webui"#,
        env!("CARGO_PKG_VERSION")
    );
}

/// Fix database by running migrations without the integrity check,
/// then verify all tables exist.
async fn fix_database() -> Result<()> {
    use sqlx::{
        sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
        Row,
    };
    use std::time::Duration;

    println!("OpenVox WebUI Database Repair Tool v{}", env!("CARGO_PKG_VERSION"));
    println!();

    // Load configuration
    let config = AppConfig::load().context("Failed to load configuration")?;

    // Extract database path and ensure directory exists
    if let Some(path) = config.database.url.strip_prefix("sqlite://") {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).context("Failed to create data directory")?;
                println!("Created data directory: {:?}", parent);
            }
        }
    }

    println!("Database URL: {}", config.database.url);
    println!();

    // Parse the database URL and configure SQLite options
    let connect_options = config
        .database
        .url
        .parse::<SqliteConnectOptions>()
        .context("Failed to parse database URL")?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(config.database.connect_timeout_secs))
        .create_if_missing(true);

    println!("Connecting to database...");

    // Create a minimal pool for repair operations
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(connect_options)
        .await
        .context("Failed to connect to database")?;

    println!("Running database migrations...");

    // Run migrations
    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(_) => {
            println!("Migrations completed successfully.");
        }
        Err(e) => {
            eprintln!("Migration error: {}", e);
            return Err(e).context("Failed to run database migrations");
        }
    }

    println!();
    println!("Verifying database tables...");

    // Query SQLite for all existing tables
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%'",
    )
    .fetch_all(&pool)
    .await
    .context("Failed to query database tables")?;

    let existing_tables: Vec<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    println!("Found {} tables:", existing_tables.len());
    for table in &existing_tables {
        println!("  - {}", table);
    }

    // Required tables list (same as db/mod.rs)
    let required_tables: &[&str] = &[
        "users",
        "organizations",
        "node_groups",
        "classification_rules",
        "pinned_nodes",
        "fact_templates",
        "roles",
        "permissions",
        "user_roles",
        "alert_rules",
        "alert_silences",
        "alerts",
        "notification_channels",
        "notification_history",
        "report_schedules",
        "saved_reports",
        "report_executions",
        "code_ssh_keys",
        "code_repositories",
        "code_environments",
        "code_deployments",
        "code_pat_tokens",
        "notifications",
        "server_backups",
        "backup_schedules",
        "backup_restores",
    ];

    // Check for missing tables
    let missing_tables: Vec<&str> = required_tables
        .iter()
        .filter(|&&table| !existing_tables.iter().any(|t| t == table))
        .copied()
        .collect();

    println!();

    if missing_tables.is_empty() {
        println!("Database repair completed successfully!");
        println!("All {} required tables are present.", required_tables.len());
        println!();
        println!("You can now start the application normally.");
    } else {
        eprintln!("WARNING: {} missing table(s) after migrations:", missing_tables.len());
        for table in &missing_tables {
            eprintln!("  - {}", table);
        }
        eprintln!();
        eprintln!("This may indicate a problem with the migration files.");
        eprintln!("Please ensure you are running the latest version of the application.");
        return Err(anyhow::anyhow!(
            "Database repair incomplete: {} missing tables",
            missing_tables.len()
        ));
    }

    Ok(())
}

/// Check r10k directory permissions and provide fix commands
fn check_r10k_permissions() -> Result<()> {
    use std::fs;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    println!("OpenVox WebUI r10k Permissions Checker v{}", env!("CARGO_PKG_VERSION"));
    println!();

    // Get current user info
    let current_uid = unsafe { libc::getuid() };
    let current_gid = unsafe { libc::getgid() };
    let current_user = std::env::var("USER").unwrap_or_else(|_| format!("uid:{}", current_uid));

    println!("Current user: {} (uid={}, gid={})", current_user, current_uid, current_gid);
    println!();

    // Load configuration to get r10k paths
    let config = AppConfig::load().context("Failed to load configuration")?;

    // Collect directories to check
    let mut dirs_to_check: Vec<(&str, String)> = Vec::new();

    // Default r10k directories
    dirs_to_check.push(("r10k cache", "/opt/puppetlabs/puppet/cache/r10k".to_string()));
    dirs_to_check.push(("Puppet environments", "/etc/puppetlabs/code/environments".to_string()));

    // Add configured directories if Code Deploy is enabled
    if let Some(ref cd) = config.code_deploy {
        if cd.enabled {
            dirs_to_check.push(("repos_base_dir", cd.repos_base_dir.to_string_lossy().to_string()));
            dirs_to_check.push(("ssh_keys_dir", cd.ssh_keys_dir.to_string_lossy().to_string()));
            dirs_to_check.push(("environments_basedir", cd.environments_basedir.to_string_lossy().to_string()));
            dirs_to_check.push(("r10k_cachedir (config)", cd.r10k_cachedir.to_string_lossy().to_string()));
        }
    }

    println!("Checking directory permissions:");
    println!("{}", "=".repeat(80));

    let mut issues_found = false;
    let mut fix_commands: Vec<String> = Vec::new();

    for (name, path_str) in &dirs_to_check {
        let path = Path::new(path_str);
        print!("\n{}: {}\n", name, path_str);

        if !path.exists() {
            println!("  Status: DOES NOT EXIST");
            fix_commands.push(format!("sudo mkdir -p {}", path_str));
            fix_commands.push(format!("sudo chown -R openvox-webui:openvox-webui {}", path_str));
            issues_found = true;
            continue;
        }

        match fs::metadata(path) {
            Ok(meta) => {
                let owner_uid = meta.uid();
                let owner_gid = meta.gid();
                let mode = meta.mode();

                // Try to get owner name
                let owner_name = get_username(owner_uid).unwrap_or_else(|| format!("uid:{}", owner_uid));
                let group_name = get_groupname(owner_gid).unwrap_or_else(|| format!("gid:{}", owner_gid));

                println!("  Owner: {}:{} ({}:{})", owner_name, group_name, owner_uid, owner_gid);
                println!("  Mode: {:o}", mode & 0o7777);

                // Check if current user can write
                let can_write = if current_uid == 0 {
                    true // root can write anywhere
                } else if current_uid == owner_uid {
                    (mode & 0o200) != 0 // owner write bit
                } else if current_gid == owner_gid {
                    (mode & 0o020) != 0 // group write bit
                } else {
                    (mode & 0o002) != 0 // other write bit
                };

                // Also try to actually create a test file
                let test_file = path.join(".openvox-permission-test");
                let actual_write = fs::write(&test_file, "test").is_ok();
                if actual_write {
                    let _ = fs::remove_file(&test_file);
                }

                if can_write && actual_write {
                    println!("  Status: OK (writable)");
                } else {
                    println!("  Status: NOT WRITABLE");
                    issues_found = true;

                    // Check subdirectories recursively for ownership issues
                    if path.is_dir() {
                        check_subdirs_ownership(path, current_uid);
                    }

                    if owner_uid != current_uid {
                        fix_commands.push(format!("sudo chown -R openvox-webui:openvox-webui {}", path_str));
                    }
                }
            }
            Err(e) => {
                println!("  Status: ERROR reading metadata: {}", e);
                issues_found = true;
            }
        }
    }

    println!("\n{}", "=".repeat(80));

    if issues_found {
        // Deduplicate fix commands
        fix_commands.sort();
        fix_commands.dedup();

        println!("\nISSUES FOUND! Run the following commands to fix permissions:\n");
        println!("# Run these commands as root on the server:");
        println!();
        for cmd in &fix_commands {
            println!("{}", cmd);
        }
        println!();
        println!("# Or run this single command to fix all r10k directories:");
        println!("sudo chown -R openvox-webui:openvox-webui /opt/puppetlabs/puppet/cache/r10k /etc/puppetlabs/code/environments");
        println!();

        // Also output a script that can be copied
        println!("# Copy-paste friendly script:");
        println!("cat << 'EOF' | sudo bash");
        println!("#!/bin/bash");
        println!("set -e");
        println!("echo 'Fixing r10k directory permissions for openvox-webui...'");
        println!("mkdir -p /opt/puppetlabs/puppet/cache/r10k");
        println!("mkdir -p /etc/puppetlabs/code/environments");
        println!("chown -R openvox-webui:openvox-webui /opt/puppetlabs/puppet/cache/r10k");
        println!("chown -R openvox-webui:openvox-webui /etc/puppetlabs/code/environments");
        println!("echo 'Done! Permissions fixed.'");
        println!("EOF");
    } else {
        println!("\nAll directories have correct permissions.");
        println!("If you're still seeing errors, check that:");
        println!("  1. The service is running as the 'openvox-webui' user");
        println!("  2. SELinux/AppArmor isn't blocking access");
        println!("  3. The filesystem isn't mounted read-only");
    }

    Ok(())
}

/// Check subdirectories for ownership mismatches
fn check_subdirs_ownership(path: &std::path::Path, expected_uid: u32) {
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if let Ok(meta) = entry_path.metadata() {
                if meta.uid() != expected_uid {
                    let owner = get_username(meta.uid()).unwrap_or_else(|| format!("uid:{}", meta.uid()));
                    println!("    - {} owned by {} (not current user)", entry_path.display(), owner);
                }
            }
        }
    }
}

/// Get username from uid (Unix-specific)
fn get_username(uid: u32) -> Option<String> {
    use std::process::Command;
    let output = Command::new("id")
        .args(["-nu", &uid.to_string()])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Get group name from gid (Unix-specific)
fn get_groupname(gid: u32) -> Option<String> {
    use std::process::Command;
    // Use getent to get group name
    let output = Command::new("getent")
        .args(["group", &gid.to_string()])
        .output()
        .ok()?;
    if output.status.success() {
        let line = String::from_utf8_lossy(&output.stdout);
        Some(line.split(':').next()?.to_string())
    } else {
        None
    }
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
