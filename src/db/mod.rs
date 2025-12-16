//! Database layer
//!
//! This module handles database operations for local storage of:
//! - Node groups and classification rules
//! - User accounts and authentication
//! - Configuration data
//! - Cached PuppetDB data

pub mod migrations;
pub mod repository;

use anyhow::Result;
use sqlx::{Pool, Sqlite};

/// Database connection pool type
pub type DbPool = Pool<Sqlite>;

/// Initialize the database connection pool
pub async fn init_pool(database_url: &str) -> Result<DbPool> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
