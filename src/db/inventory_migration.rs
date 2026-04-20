//! One-shot migrator that moves Phase-10 inventory data out of the main
//! application database and into the dedicated inventory database on first
//! startup of the new release.
//!
//! ## Why
//!
//! Older releases stored inventory snapshots, packages, applications,
//! container/user inventory, update-job state, and per-host/fleet repository
//! configs in the same SQLite file as everything else. Under heavy Puppet
//! agent posting, this caused write-lock starvation that blocked UI reads
//! (see the incident report that motivated this release).
//!
//! ## How
//!
//! Uses SQLite's `ATTACH DATABASE` so the copy happens entirely inside the
//! engine and we inherit SQLite's native column type coercion — no need to
//! hand-marshal every column type through Rust.
//!
//!   1. Check the migration marker in `schema_meta` on the **inventory**
//!      database. If `inventory_migrated_from_main == "done"` → skip.
//!   2. Detect whether the main DB still holds any rows in any legacy
//!      inventory table.
//!   3. Acquire a single connection from the main pool, `ATTACH` the
//!      inventory file as `inv`, and for each legacy table run
//!      `INSERT OR IGNORE INTO inv.<table> (<shared cols>)
//!       SELECT <shared cols> FROM main.<table>` in FK-topological order.
//!   4. Once every copy succeeds, `DELETE FROM` each legacy table in the
//!      main DB in reverse FK order (do NOT `DROP TABLE` — SQLx migration
//!      state still references those schema objects).
//!   5. `DETACH` and write the marker.
//!   6. Spawn a background task that runs `VACUUM` on the main DB after a
//!      short delay to return reclaimed space to the filesystem.
//!
//! ## Recovery
//!
//! `INSERT OR IGNORE` makes the copy phase idempotent. If the process
//! crashes mid-migration, simply restart. Operators can also reset the
//! marker via the `--reset-inventory-migration` CLI flag to force a re-run.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use sqlx::Row;
use tracing::{info, warn};

use crate::db::DbPool;

const MARKER_KEY: &str = "inventory_migrated_from_main";
const MARKER_DONE: &str = "done";

/// Every legacy inventory table that lives in the main DB, in topological
/// order for FK-safe copy. `host_inventory_snapshots` must come first because
/// many others reference `snapshot_id`. Update jobs have their own FK chain.
const LEGACY_INVENTORY_TABLES: &[&str] = &[
    "host_inventory_snapshots",
    "host_os_inventory",
    "host_package_inventory",
    "host_application_inventory",
    "host_web_inventory",
    "host_runtime_inventory",
    "host_container_inventory",
    "host_user_inventory",
    "host_update_status",
    "repository_version_catalog",
    "node_repository_configs",
    "fleet_repository_configs",
    "update_jobs",
    "update_job_targets",
    "update_job_results",
    "group_update_schedules",
];

/// Summary of work done by a single migration run.
#[derive(Debug, Default, Clone)]
pub struct MigrationReport {
    pub skipped: bool,
    pub per_table: Vec<(String, u64)>,
    pub total_rows_copied: u64,
    pub total_rows_deleted: u64,
}

/// Reset the migration marker on the inventory DB. Intended for operator
/// recovery via the `--reset-inventory-migration` CLI flag.
pub async fn reset_marker(inventory: &DbPool) -> Result<bool> {
    let res = sqlx::query("DELETE FROM schema_meta WHERE key = ?1")
        .bind(MARKER_KEY)
        .execute(inventory)
        .await
        .context("Failed to reset inventory migration marker")?;
    Ok(res.rows_affected() > 0)
}

/// Top-level entry point. Safe to call on every startup — returns a report
/// with `skipped = true` when the marker is already set.
///
/// `inventory_url` is the filesystem path / URL used for the dedicated
/// inventory DB, expressed the way the operator set it in config. The
/// migrator passes the extracted path to `ATTACH DATABASE`.
pub async fn migrate_if_needed(
    main: &DbPool,
    inventory: &DbPool,
    inventory_url: &str,
) -> Result<MigrationReport> {
    if marker_is_set(inventory).await? {
        return Ok(MigrationReport {
            skipped: true,
            ..Default::default()
        });
    }

    if !main_has_inventory_rows(main).await? {
        write_marker(inventory).await?;
        info!("No legacy inventory data found in main DB; marker written");
        return Ok(MigrationReport::default());
    }

    info!("Legacy inventory data detected in main DB; starting migration");

    let inventory_path = sqlite_url_to_path(inventory_url)?;

    // Single connection handles ATTACH + all copy statements so the attached
    // database is visible for the whole run and statistics / error context
    // stay together.
    let mut conn = main
        .acquire()
        .await
        .context("Failed to acquire main DB connection for inventory migration")?;

    sqlx::query(&format!(
        "ATTACH DATABASE '{}' AS inv",
        inventory_path.replace('\'', "''")
    ))
    .execute(&mut *conn)
    .await
    .context("Failed to ATTACH inventory database")?;

    let mut report = MigrationReport::default();

    let copy_result = async {
        for table in LEGACY_INVENTORY_TABLES {
            if !table_exists_on_conn(&mut *conn, "main", table).await? {
                continue;
            }
            if !table_exists_on_conn(&mut *conn, "inv", table).await? {
                warn!(
                    "Inventory DB missing expected table '{}'; skipping copy",
                    table
                );
                continue;
            }

            let main_cols = fetch_columns(&mut *conn, "main", table).await?;
            let inv_cols = fetch_columns(&mut *conn, "inv", table).await?;
            let shared: Vec<String> = main_cols
                .into_iter()
                .filter(|c| inv_cols.contains(c))
                .collect();
            if shared.is_empty() {
                warn!(
                    "No common columns between main.{} and inv.{}; skipping",
                    table, table
                );
                continue;
            }
            let col_list = shared
                .iter()
                .map(|c| format!("\"{}\"", c))
                .collect::<Vec<_>>()
                .join(", ");

            let sql = format!(
                "INSERT OR IGNORE INTO inv.{t} ({cols}) SELECT {cols} FROM main.{t}",
                t = table,
                cols = col_list
            );
            let copied = sqlx::query(&sql)
                .execute(&mut *conn)
                .await
                .with_context(|| format!("Failed to copy main.{} → inv.{}", table, table))?
                .rows_affected();

            report.total_rows_copied = report.total_rows_copied.saturating_add(copied);
            report.per_table.push(((*table).to_string(), copied));
            if copied > 0 {
                info!("Copied {} rows from main.{} → inv.{}", copied, table, table);
            }
        }

        // Truncate legacy tables in reverse FK order so FK constraints stay
        // satisfied through the deletes.
        for table in LEGACY_INVENTORY_TABLES.iter().rev() {
            if !table_exists_on_conn(&mut *conn, "main", table).await? {
                continue;
            }
            let deleted = sqlx::query(&format!("DELETE FROM main.{}", table))
                .execute(&mut *conn)
                .await
                .with_context(|| format!("Failed to truncate main.{}", table))?
                .rows_affected();
            report.total_rows_deleted = report.total_rows_deleted.saturating_add(deleted);
        }

        Ok::<(), anyhow::Error>(())
    }
    .await;

    // Always detach before returning.
    let _ = sqlx::query("DETACH DATABASE inv").execute(&mut *conn).await;

    copy_result?;

    write_marker(inventory).await?;
    info!(
        "Inventory migration complete: copied {} rows, truncated {} from main DB",
        report.total_rows_copied, report.total_rows_deleted
    );

    // Schedule a background VACUUM so the main DB shrinks on disk. On a
    // multi-GB DB this blocks writers for minutes, so we delay it and run it
    // off the request path. Never run it synchronously at startup.
    let main_clone = main.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        info!("Starting background VACUUM of main DB to reclaim space after inventory migration");
        if let Err(e) = sqlx::query("VACUUM").execute(&main_clone).await {
            warn!("Background VACUUM of main DB failed: {}", e);
        } else {
            info!("Background VACUUM of main DB completed");
        }
    });

    Ok(report)
}

async fn marker_is_set(inventory: &DbPool) -> Result<bool> {
    let row = sqlx::query("SELECT value FROM schema_meta WHERE key = ?1")
        .bind(MARKER_KEY)
        .fetch_optional(inventory)
        .await
        .context("Failed to read inventory migration marker")?;
    Ok(row
        .and_then(|r| r.try_get::<String, _>("value").ok())
        .map(|v| v == MARKER_DONE)
        .unwrap_or(false))
}

async fn write_marker(inventory: &DbPool) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO schema_meta (key, value, updated_at) VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
        "#,
    )
    .bind(MARKER_KEY)
    .bind(MARKER_DONE)
    .bind(now)
    .execute(inventory)
    .await
    .context("Failed to write inventory migration marker")?;
    Ok(())
}

async fn main_has_inventory_rows(main: &DbPool) -> Result<bool> {
    for table in LEGACY_INVENTORY_TABLES {
        let exists_row =
            sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name = ?1 LIMIT 1")
                .bind(table)
                .fetch_optional(main)
                .await
                .with_context(|| format!("Failed to probe table {}", table))?;
        if exists_row.is_none() {
            continue;
        }
        let row = sqlx::query(&format!("SELECT COUNT(*) AS c FROM {}", table))
            .fetch_one(main)
            .await
            .with_context(|| format!("Failed to count rows in main.{}", table))?;
        let count: i64 = row.try_get("c").unwrap_or(0);
        if count > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn table_exists_on_conn<'e, E>(exec: E, schema: &str, table: &str) -> Result<bool>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let sql =
        format!("SELECT name FROM {schema}.sqlite_master WHERE type='table' AND name = ?1 LIMIT 1");
    let row = sqlx::query(&sql)
        .bind(table)
        .fetch_optional(exec)
        .await
        .with_context(|| format!("Failed to probe {}.{}", schema, table))?;
    Ok(row.is_some())
}

async fn fetch_columns<'e, E>(exec: E, schema: &str, table: &str) -> Result<Vec<String>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let rows = sqlx::query(&format!("PRAGMA {}.table_info({})", schema, table))
        .fetch_all(exec)
        .await
        .with_context(|| format!("Failed to read schema for {}.{}", schema, table))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if let Ok(name) = row.try_get::<String, _>("name") {
            out.push(name);
        }
    }
    Ok(out)
}

/// Turn a SQLx SQLite URL into the bare filesystem path that SQLite's
/// `ATTACH DATABASE` accepts. Accepts `sqlite://`, `sqlite:`, absolute paths,
/// and bare paths.
fn sqlite_url_to_path(url: &str) -> Result<String> {
    let trimmed = url.trim();
    if let Some(rest) = trimmed.strip_prefix("sqlite://") {
        // sqlite:///path/to/file.db → /path/to/file.db
        return Ok(rest.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("sqlite:") {
        return Ok(rest.to_string());
    }
    if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
        return Ok(trimmed.to_string());
    }
    bail!(
        "Unsupported inventory database URL format '{}'; expected a sqlite:// URL or a filesystem path",
        url
    )
}
