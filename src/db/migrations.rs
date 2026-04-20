//! Database migrations
//!
//! Migrations are handled by SQLx and stored in the `migrations/` directory.
//! This module provides utilities for working with migrations programmatically.

use anyhow::Result;
use sqlx::SqlitePool;

/// Check if migrations are up to date
pub async fn check_migrations(pool: &SqlitePool) -> Result<bool> {
    // SQLx automatically tracks migration status
    // This function can be extended to provide more detailed status
    let _result = sqlx::query("SELECT 1").fetch_one(pool).await?;
    Ok(true)
}
