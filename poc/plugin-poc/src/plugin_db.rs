//! T65: a SQLite database dedicated to one plugin, with enforced limits.
//!
//! Enforced: no ATTACH / load_extension / writes to other schemas (authorizer),
//! `trusted_schema=OFF`, a size cap (`max_page_count`) and a per-query time cap
//! (progress handler).
use std::ffi::{c_char, c_int, c_void};
use std::path::Path;
use std::time::{Duration, Instant};

use libsqlite3_sys as ffi;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePoolOptions};
use sqlx::{Connection, Executor, SqlitePool};

#[derive(Clone, Copy)]
pub struct Limits {
    pub max_bytes: u64,
    pub query_timeout: Duration,
}

pub async fn open(path: &Path, limits: Limits) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .pragma("trusted_schema", "OFF")
        .pragma("max_page_count", (limits.max_bytes / 4096).to_string());
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    Ok(pool)
}

extern "C" fn authorizer(
    _ud: *mut c_void,
    action: c_int,
    a1: *const c_char,
    _a2: *const c_char,
    db: *const c_char,
    _trigger: *const c_char,
) -> c_int {
    match action {
        ffi::SQLITE_ATTACH | ffi::SQLITE_DETACH => ffi::SQLITE_DENY,
        // `PRAGMA` is limited to a read-only allowlist.
        ffi::SQLITE_PRAGMA => {
            let name = unsafe { std::ffi::CStr::from_ptr(a1) }.to_string_lossy().to_lowercase();
            if matches!(name.as_str(), "table_info" | "table_xinfo" | "index_list" | "foreign_key_list" | "user_version") {
                ffi::SQLITE_OK
            } else {
                ffi::SQLITE_DENY
            }
        }
        // `load_extension` shows up as a function call.
        ffi::SQLITE_FUNCTION => {
            let name = unsafe { std::ffi::CStr::from_ptr(_a2) }.to_string_lossy().to_lowercase();
            if name == "load_extension" { ffi::SQLITE_DENY } else { ffi::SQLITE_OK }
        }
        // Only the plugin's own `main` schema may be touched.
        _ if !db.is_null() && unsafe { std::ffi::CStr::from_ptr(db) }.to_bytes() != b"main" => {
            ffi::SQLITE_DENY
        }
        _ => ffi::SQLITE_OK,
    }
}

/// Runs plugin-supplied SQL on `conn` under the sandbox rules.
pub async fn run_sandboxed(
    conn: &mut SqliteConnection,
    limits: Limits,
    sql: &str,
) -> Result<u64, sqlx::Error> {
    let mut handle = conn.lock_handle().await?;
    let raw = handle.as_raw_handle().as_ptr();
    unsafe { ffi::sqlite3_set_authorizer(raw, Some(authorizer), std::ptr::null_mut()) };
    let started = Instant::now();
    let timeout = limits.query_timeout;
    // Abort the running statement once the deadline passes.
    handle.set_progress_handler(1000, move || started.elapsed() < timeout);
    drop(handle);
    let res = conn.execute(sqlx::AssertSqlSafe(sql.to_owned())).await;
    let mut handle = conn.lock_handle().await?;
    handle.remove_progress_handler();
    unsafe { ffi::sqlite3_set_authorizer(handle.as_raw_handle().as_ptr(), None, std::ptr::null_mut()) };
    Ok(res?.rows_affected())
}

pub async fn connect_raw(path: &Path, limits: Limits) -> anyhow::Result<SqliteConnection> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .pragma("trusted_schema", "OFF")
        .pragma("max_page_count", (limits.max_bytes / 4096).to_string());
    Ok(SqliteConnection::connect_with(&opts).await?)
}
