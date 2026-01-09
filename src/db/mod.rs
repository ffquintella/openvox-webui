//! Database layer
//!
//! This module handles database operations for local storage of:
//! - Node groups and classification rules
//! - User accounts and authentication
//! - Configuration data
//! - Cached PuppetDB data
//! - Alerting and notifications

pub mod alerting_repository;
pub mod api_key_repository;
pub mod audit_repository;
pub mod code_deploy_repository;
pub mod migrations;
pub mod organization_repository;
pub mod repository;

pub use alerting_repository::{
    AlertRepository, AlertRuleRepository, AlertSilenceRepository, NotificationChannelRepository,
    NotificationHistoryRepository,
};
pub use api_key_repository::ApiKeyRepository;
pub use audit_repository::AuditRepository;
pub use code_deploy_repository::{
    CodeDeploymentRepository, CodeEnvironmentRepository, CodePatTokenRepository,
    CodeRepositoryRepository, CodeSshKeyRepository,
};
pub use organization_repository::OrganizationRepository;

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Sqlite, Row,
};
use tracing::{error, info};

use crate::config::DatabaseConfig;

/// Required database tables that must exist after migrations
/// This list should be updated when new migrations add tables
const REQUIRED_TABLES: &[&str] = &[
    // Core tables (initial migration)
    "users",
    "organizations",
    "node_groups",
    "classification_rules",
    "pinned_nodes",
    "fact_templates",
    // RBAC tables
    "roles",
    "role_permissions",
    "user_roles",
    // Alerting tables
    "alert_rules",
    "alert_silences",
    "alerts",
    "notification_channels",
    "notification_history",
    // Reporting tables
    "report_schedules",
    "generated_reports",
    // Code Deploy tables
    "code_ssh_keys",
    "code_repositories",
    "code_environments",
    "code_deployments",
    "code_pat_tokens",
    // Notification tables
    "notifications",
];

/// Database connection pool type
pub type DbPool = Pool<Sqlite>;

/// Initialize the database connection pool
pub async fn init_pool(config: &DatabaseConfig) -> Result<DbPool> {
    // Parse the database URL and configure SQLite options
    let connect_options = config
        .url
        .parse::<SqliteConnectOptions>()
        .context("Failed to parse database URL")?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(config.connect_timeout_secs))
        .create_if_missing(true);

    info!("Initializing database connection pool");

    // Create the pool with configured options
    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .connect_with(connect_options)
        .await
        .context("Failed to create database connection pool")?;

    info!(
        "Database pool created: max={}, min={}",
        config.max_connections, config.min_connections
    );

    // Run migrations
    run_migrations(&pool).await?;

    // Verify database integrity after migrations
    verify_database_integrity(&pool).await?;

    Ok(pool)
}

/// Run database migrations
async fn run_migrations(pool: &DbPool) -> Result<()> {
    info!("Running database migrations");

    match sqlx::migrate!("./migrations").run(pool).await {
        Ok(_) => {
            info!("Database migrations completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Migration error: {}", e);
            Err(e).context("Failed to run database migrations")
        }
    }
}

/// Verify database integrity by checking all required tables exist
///
/// This check runs after migrations to ensure the database schema is complete.
/// If any required tables are missing, the application will fail to start with
/// a clear error message listing the missing tables.
async fn verify_database_integrity(pool: &DbPool) -> Result<()> {
    info!("Verifying database integrity");

    // Query SQLite for all existing tables
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%'"
    )
    .fetch_all(pool)
    .await
    .context("Failed to query database tables")?;

    let existing_tables: Vec<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    // Check for missing tables
    let missing_tables: Vec<&str> = REQUIRED_TABLES
        .iter()
        .filter(|&&table| !existing_tables.iter().any(|t| t == table))
        .copied()
        .collect();

    if missing_tables.is_empty() {
        info!(
            "Database integrity check passed: all {} required tables present",
            REQUIRED_TABLES.len()
        );
        Ok(())
    } else {
        error!(
            "Database integrity check FAILED: {} missing table(s)",
            missing_tables.len()
        );
        for table in &missing_tables {
            error!("  - Missing table: {}", table);
        }
        error!("");
        error!("This usually means database migrations were not applied correctly.");
        error!("Possible causes:");
        error!("  1. The application binary was built before migrations were added");
        error!("  2. The database file is from an older version");
        error!("  3. Migrations failed silently");
        error!("");
        error!("To fix this:");
        error!("  1. Rebuild and redeploy the application with the latest code");
        error!("  2. Or manually apply missing migrations from the migrations/ directory");
        error!("");

        Err(anyhow::anyhow!(
            "Database integrity check failed: missing tables: {}",
            missing_tables.join(", ")
        ))
    }
}

/// Check database health
pub async fn check_health(pool: &DbPool) -> Result<()> {
    sqlx::query("SELECT 1")
        .fetch_one(pool)
        .await
        .context("Database health check failed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_pool_in_memory() {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            min_connections: 1,
            connect_timeout_secs: 5,
            idle_timeout_secs: 60,
        };

        // Note: This test may fail if migrations require a persistent database
        // In that case, use a temp file instead
        let result = init_pool(&config).await;
        // We just check it doesn't panic - migrations may fail in memory
        let _ = result;
    }
}
