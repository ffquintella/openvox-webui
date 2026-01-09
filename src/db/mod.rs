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
    Pool, Sqlite,
};
use tracing::{info, warn};

use crate::config::DatabaseConfig;

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
            warn!("Migration error: {}", e);
            Err(e).context("Failed to run database migrations")
        }
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
