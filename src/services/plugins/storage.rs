//! Plugin storage (M20, T75): per-plugin key/value store and private database.
//!
//! `kv` lives in the main database (`plugin_kv`) under quotas. `db:own` is one
//! SQLite file per plugin (`<data_dir>/db/<id>.db`) with an authorizer, a size
//! cap and a per-query deadline, as proven in the M18 PoC.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use chrono::Utc;
use futures::TryStreamExt;
use libsqlite3_sys as ffi;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqliteRow};
use sqlx::{AssertSqlSafe, Column, Connection, Row, SqlitePool, TypeInfo, ValueRef};
use tokio::sync::Mutex;

const PAGE_SIZE: u64 = 4096;
const MAX_MIGRATION_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct StorageLimits {
    pub kv_max_keys: u64,
    /// Total bytes (keys plus values) per plugin.
    pub kv_max_bytes: u64,
    pub kv_max_value_bytes: u64,
    pub kv_max_key_len: usize,
    pub db_max_bytes: u64,
    pub db_query_timeout: Duration,
    pub db_max_rows: usize,
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            kv_max_keys: 10_000,
            kv_max_bytes: 10 * 1024 * 1024,
            kv_max_value_bytes: 256 * 1024,
            kv_max_key_len: 256,
            db_max_bytes: 64 * 1024 * 1024,
            db_query_timeout: Duration::from_secs(2),
            db_max_rows: 1000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("quota exceeded: {0}")]
    Quota(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("denied: {0}")]
    Denied(String),
    #[error("the query exceeded its time limit")]
    Timeout,
    #[error("storage failure: {0}")]
    Failed(String),
}

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        let msg = e.to_string();
        let lower = msg.to_lowercase();
        if lower.contains("interrupt") {
            StorageError::Timeout
        } else if lower.contains("is full") {
            StorageError::Quota("the plugin database reached its size limit".into())
        } else if lower.contains("authoriz") {
            StorageError::Denied("this statement is not allowed in a plugin database".into())
        } else {
            StorageError::Failed(msg)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecResult {
    pub rows_affected: u64,
    pub last_insert_rowid: i64,
}

#[derive(Debug, Clone)]
pub struct MigrationFile {
    pub number: u32,
    pub name: String,
    pub up: String,
    pub down: Option<String>,
}

type Conn = Arc<Mutex<SqliteConnection>>;

/// kv and `db:own` storage of all plugins.
pub struct PluginStorage {
    pool: SqlitePool,
    data_dir: PathBuf,
    limits: StorageLimits,
    conns: Mutex<HashMap<String, Conn>>,
}

fn valid_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 63
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn check_plugin(id: &str) -> Result<(), StorageError> {
    if valid_plugin_id(id) {
        Ok(())
    } else {
        Err(StorageError::Invalid("invalid plugin id".into()))
    }
}

impl PluginStorage {
    pub fn new(pool: SqlitePool, data_dir: PathBuf, limits: StorageLimits) -> Self {
        Self {
            pool,
            data_dir,
            limits,
            conns: Mutex::new(HashMap::new()),
        }
    }

    // ---- kv -----------------------------------------------------------------

    fn check_key(&self, key: &str) -> Result<(), StorageError> {
        if key.is_empty() || key.len() > self.limits.kv_max_key_len {
            return Err(StorageError::Invalid(format!(
                "keys must be 1 to {} bytes",
                self.limits.kv_max_key_len
            )));
        }
        Ok(())
    }

    pub async fn kv_get(&self, plugin: &str, key: &str) -> Result<Option<Value>, StorageError> {
        check_plugin(plugin)?;
        let row: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT value FROM plugin_kv WHERE plugin_id = ? AND key = ?")
                .bind(plugin)
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|b| serde_json::from_slice(&b).map_err(|e| StorageError::Failed(e.to_string())))
            .transpose()
    }

    pub async fn kv_put(&self, plugin: &str, key: &str, value: &Value) -> Result<(), StorageError> {
        check_plugin(plugin)?;
        self.check_key(key)?;
        let bytes = serde_json::to_vec(value).map_err(|e| StorageError::Invalid(e.to_string()))?;
        if bytes.len() as u64 > self.limits.kv_max_value_bytes {
            return Err(StorageError::Quota(format!(
                "values are limited to {} bytes",
                self.limits.kv_max_value_bytes
            )));
        }
        let mut tx = self.pool.begin().await?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT length(key) + length(value) FROM plugin_kv WHERE plugin_id = ? AND key = ?",
        )
        .bind(plugin)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?;
        let (keys, total): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(SUM(length(key) + length(value)), 0) FROM plugin_kv WHERE plugin_id = ?",
        )
        .bind(plugin)
        .fetch_one(&mut *tx)
        .await?;
        let new_keys = keys as u64 + u64::from(existing.is_none());
        let new_total = (total - existing.unwrap_or(0)) as u64 + (key.len() + bytes.len()) as u64;
        if new_keys > self.limits.kv_max_keys {
            return Err(StorageError::Quota(format!(
                "at most {} keys are allowed",
                self.limits.kv_max_keys
            )));
        }
        if new_total > self.limits.kv_max_bytes {
            return Err(StorageError::Quota(format!(
                "at most {} bytes are allowed",
                self.limits.kv_max_bytes
            )));
        }
        sqlx::query(
            "INSERT INTO plugin_kv (plugin_id, key, value, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(plugin_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(plugin)
        .bind(key)
        .bind(&bytes)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn kv_delete(&self, plugin: &str, key: &str) -> Result<bool, StorageError> {
        check_plugin(plugin)?;
        let r = sqlx::query("DELETE FROM plugin_kv WHERE plugin_id = ? AND key = ?")
            .bind(plugin)
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn kv_list(
        &self,
        plugin: &str,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        check_plugin(plugin)?;
        // `substr` compares literally, so `%` and `_` in the prefix are safe.
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
            "SELECT key, value FROM plugin_kv WHERE plugin_id = ? AND substr(key, 1, length(?)) = ?
             ORDER BY key LIMIT ?",
        )
        .bind(plugin)
        .bind(prefix)
        .bind(prefix)
        .bind(limit.min(10_000) as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(k, v)| {
                serde_json::from_slice(&v)
                    .map(|v| (k, v))
                    .map_err(|e| StorageError::Failed(e.to_string()))
            })
            .collect()
    }

    /// Number of keys and total bytes stored by the plugin.
    pub async fn kv_usage(&self, plugin: &str) -> Result<(u64, u64), StorageError> {
        check_plugin(plugin)?;
        let (keys, bytes): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(SUM(length(key) + length(value)), 0) FROM plugin_kv WHERE plugin_id = ?",
        )
        .bind(plugin)
        .fetch_one(&self.pool)
        .await?;
        Ok((keys as u64, bytes as u64))
    }

    // ---- db:own -------------------------------------------------------------

    fn db_path(&self, plugin: &str) -> PathBuf {
        self.data_dir.join("db").join(format!("{}.db", plugin))
    }

    async fn conn(&self, plugin: &str) -> Result<Conn, StorageError> {
        check_plugin(plugin)?;
        let mut map = self.conns.lock().await;
        if let Some(c) = map.get(plugin) {
            return Ok(c.clone());
        }
        let path = self.db_path(plugin);
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| StorageError::Failed(e.to_string()))?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .pragma("trusted_schema", "OFF")
            .pragma(
                "max_page_count",
                (self.limits.db_max_bytes / PAGE_SIZE).max(1).to_string(),
            );
        let conn = Arc::new(Mutex::new(SqliteConnection::connect_with(&opts).await?));
        map.insert(plugin.to_string(), conn.clone());
        Ok(conn)
    }

    /// Runs `f` on the plugin's connection with the sandbox installed.
    async fn sandboxed<T, F>(&self, plugin: &str, read_only: bool, f: F) -> Result<T, StorageError>
    where
        F: for<'c> FnOnce(
            &'c mut SqliteConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, StorageError>> + Send + 'c>,
        >,
    {
        let conn = self.conn(plugin).await?;
        let mut guard = conn.lock().await;
        let c: &mut SqliteConnection = &mut guard;
        // Host-side settings are applied before the authorizer exists.
        let qo = if read_only {
            "PRAGMA query_only = ON"
        } else {
            "PRAGMA query_only = OFF"
        };
        sqlx::query(qo).execute(&mut *c).await?;
        {
            let mut h = c.lock_handle().await?;
            let raw = h.as_raw_handle().as_ptr();
            // SAFETY: `raw` is the live connection handle owned by `h`.
            unsafe { ffi::sqlite3_set_authorizer(raw, Some(authorizer), std::ptr::null_mut()) };
            let started = Instant::now();
            let timeout = self.limits.db_query_timeout;
            h.set_progress_handler(1000, move || started.elapsed() < timeout);
        }
        let res = f(&mut *c).await;
        if let Ok(mut h) = c.lock_handle().await {
            h.remove_progress_handler();
            let raw = h.as_raw_handle().as_ptr();
            // SAFETY: as above; removes the authorizer again.
            unsafe { ffi::sqlite3_set_authorizer(raw, None, std::ptr::null_mut()) };
        }
        let _ = sqlx::query("PRAGMA query_only = OFF")
            .execute(&mut *c)
            .await;
        res
    }

    /// Runs a read-only statement on the plugin's database.
    pub async fn db_query(
        &self,
        plugin: &str,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<Map<String, Value>>, StorageError> {
        let sql = sql.to_string();
        let params = params.to_vec();
        let max = self.limits.db_max_rows;
        self.sandboxed(plugin, true, move |c| {
            Box::pin(async move {
                let mut q = sqlx::query(AssertSqlSafe(sql));
                for p in &params {
                    q = bind_json(q, p);
                }
                let mut rows = q.fetch(&mut *c);
                let mut out = Vec::new();
                while let Some(r) = rows.try_next().await? {
                    if out.len() >= max {
                        break;
                    }
                    out.push(row_to_json(&r));
                }
                Ok(out)
            })
        })
        .await
    }

    /// Runs a statement that may change the plugin's database.
    pub async fn db_exec(
        &self,
        plugin: &str,
        sql: &str,
        params: &[Value],
    ) -> Result<ExecResult, StorageError> {
        let sql = sql.to_string();
        let params = params.to_vec();
        self.sandboxed(plugin, false, move |c| {
            Box::pin(async move {
                let mut q = sqlx::query(AssertSqlSafe(sql));
                for p in &params {
                    q = bind_json(q, p);
                }
                let r = q.execute(&mut *c).await?;
                Ok(ExecResult {
                    rows_affected: r.rows_affected(),
                    last_insert_rowid: r.last_insert_rowid(),
                })
            })
        })
        .await
    }

    // ---- migrations ---------------------------------------------------------

    /// Highest applied migration number (0 when none).
    pub async fn schema_version(&self, plugin: &str) -> Result<u32, StorageError> {
        check_plugin(plugin)?;
        let v: Option<i64> =
            sqlx::query_scalar("SELECT MAX(number) FROM plugin_migrations WHERE plugin_id = ?")
                .bind(plugin)
                .fetch_one(&self.pool)
                .await?;
        Ok(v.unwrap_or(0).max(0) as u32)
    }

    async fn applied(&self, plugin: &str) -> Result<HashMap<u32, String>, StorageError> {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT number, checksum FROM plugin_migrations WHERE plugin_id = ?")
                .bind(plugin)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(n, c)| (n as u32, c)).collect())
    }

    /// Applies the pending migrations up to `target` (default: all), each in
    /// its own transaction. Returns the resulting schema version.
    pub async fn migrate_up(
        &self,
        plugin: &str,
        files: &[MigrationFile],
        target: Option<u32>,
    ) -> Result<u32, StorageError> {
        check_plugin(plugin)?;
        let applied = self.applied(plugin).await?;
        for f in files {
            if let Some(sum) = applied.get(&f.number) {
                if *sum != checksum(&f.up) {
                    return Err(StorageError::Failed(format!(
                        "migration {} changed after it was applied",
                        f.number
                    )));
                }
            }
        }
        let target = target.unwrap_or(u32::MAX);
        let mut version = applied.keys().copied().max().unwrap_or(0);
        for f in files.iter().filter(|f| f.number <= target) {
            if applied.contains_key(&f.number) {
                continue;
            }
            self.run_migration(plugin, &f.up).await.map_err(|e| {
                StorageError::Failed(format!("migration {} ({}) failed: {}", f.number, f.name, e))
            })?;
            sqlx::query(
                "INSERT INTO plugin_migrations (plugin_id, number, name, checksum, applied_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(plugin)
            .bind(f.number as i64)
            .bind(&f.name)
            .bind(checksum(&f.up))
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
            version = version.max(f.number);
        }
        Ok(version)
    }

    /// Reverts migrations above `to`, newest first. Nothing changes unless
    /// every needed `.down` file exists.
    pub async fn migrate_down(
        &self,
        plugin: &str,
        files: &[MigrationFile],
        to: u32,
    ) -> Result<u32, StorageError> {
        check_plugin(plugin)?;
        let applied = self.applied(plugin).await?;
        let mut todo: Vec<&MigrationFile> = Vec::new();
        for n in applied.keys().filter(|n| **n > to) {
            match files.iter().find(|f| f.number == *n) {
                Some(f) if f.down.is_some() => todo.push(f),
                _ => {
                    return Err(StorageError::Failed(format!(
                        "migration {} has no .down file, so the schema cannot go back to {}",
                        n, to
                    )))
                }
            }
        }
        todo.sort_by_key(|f| std::cmp::Reverse(f.number));
        for f in todo {
            let down = f.down.as_deref().unwrap_or_default();
            self.run_migration(plugin, down).await.map_err(|e| {
                StorageError::Failed(format!("reverting {} ({}) failed: {}", f.number, f.name, e))
            })?;
            sqlx::query("DELETE FROM plugin_migrations WHERE plugin_id = ? AND number = ?")
                .bind(plugin)
                .bind(f.number as i64)
                .execute(&self.pool)
                .await?;
        }
        self.schema_version(plugin).await
    }

    async fn run_migration(&self, plugin: &str, sql: &str) -> Result<(), StorageError> {
        let sql = sql.to_string();
        self.sandboxed(plugin, false, move |c| {
            Box::pin(async move {
                let mut tx = c.begin().await?;
                sqlx::raw_sql(AssertSqlSafe(sql)).execute(&mut *tx).await?;
                tx.commit().await?;
                Ok(())
            })
        })
        .await
    }

    // ---- purge --------------------------------------------------------------

    /// Deletes kv entries, migration records and the private database file.
    pub async fn purge(&self, plugin: &str) -> Result<(), StorageError> {
        check_plugin(plugin)?;
        sqlx::query("DELETE FROM plugin_kv WHERE plugin_id = ?")
            .bind(plugin)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM plugin_migrations WHERE plugin_id = ?")
            .bind(plugin)
            .execute(&self.pool)
            .await?;
        if let Some(c) = self.conns.lock().await.remove(plugin) {
            // Wait for any running call, then close the connection.
            let conn = c.lock().await;
            drop(conn);
            if let Ok(m) = Arc::try_unwrap(c) {
                let _ = m.into_inner().close().await;
            }
        }
        let path = self.db_path(plugin);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut p = path.clone().into_os_string();
            p.push(suffix);
            match tokio::fs::remove_file(Path::new(&p)).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(StorageError::Failed(e.to_string())),
            }
        }
        Ok(())
    }
}

fn checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

// ---- authorizer ------------------------------------------------------------------

unsafe fn cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(CStr::from_ptr(p).to_string_lossy().to_lowercase())
    }
}

extern "C" fn authorizer(
    _ud: *mut c_void,
    action: c_int,
    a1: *const c_char,
    a2: *const c_char,
    db: *const c_char,
    _trigger: *const c_char,
) -> c_int {
    // SAFETY: SQLite passes NUL-terminated strings or null pointers.
    unsafe {
        match action {
            ffi::SQLITE_ATTACH | ffi::SQLITE_DETACH => ffi::SQLITE_DENY,
            ffi::SQLITE_PRAGMA => match cstr(a1).as_deref() {
                Some(
                    "table_info" | "table_xinfo" | "table_list" | "index_list" | "index_info"
                    | "index_xinfo" | "foreign_key_list" | "user_version",
                ) => ffi::SQLITE_OK,
                _ => ffi::SQLITE_DENY,
            },
            ffi::SQLITE_FUNCTION => {
                if cstr(a2).as_deref() == Some("load_extension") {
                    ffi::SQLITE_DENY
                } else {
                    ffi::SQLITE_OK
                }
            }
            _ => match cstr(db) {
                Some(d) if d != "main" => ffi::SQLITE_DENY,
                _ => ffi::SQLITE_OK,
            },
        }
    }
}

// ---- value conversion --------------------------------------------------------------

fn bind_json<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    v: &Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    match v {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => q.bind(i),
            None => q.bind(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => q.bind(s.clone()),
        other => q.bind(other.to_string()),
    }
}

fn row_to_json(row: &SqliteRow) -> Map<String, Value> {
    let mut out = Map::new();
    for (i, col) in row.columns().iter().enumerate() {
        let ty = row
            .try_get_raw(i)
            .map(|r| r.type_info().name().to_string())
            .unwrap_or_else(|_| "NULL".into());
        let v = match ty.as_str() {
            "NULL" => Value::Null,
            "INTEGER" | "BOOLEAN" | "INT" | "INT8" => row
                .try_get::<i64, _>(i)
                .map(Value::from)
                .unwrap_or(Value::Null),
            "REAL" | "FLOAT" | "DOUBLE" => row
                .try_get::<f64, _>(i)
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            "BLOB" => row
                .try_get::<Vec<u8>, _>(i)
                .map(|b| Value::String(base64::engine::general_purpose::STANDARD.encode(b)))
                .unwrap_or(Value::Null),
            _ => row
                .try_get::<String, _>(i)
                .map(Value::String)
                .unwrap_or(Value::Null),
        };
        out.insert(col.name().to_string(), v);
    }
    out
}

// ---- migration files -----------------------------------------------------------------

/// Reads `<dir>/NNNN_name.up.sql` and `.down.sql` files from a package.
pub fn collect_migrations(
    files: &[(String, Vec<u8>)],
    dir: &str,
) -> Result<Vec<MigrationFile>, String> {
    let prefix = format!("{}/", dir.trim_end_matches('/'));
    let mut by_number: std::collections::BTreeMap<u32, MigrationFile> = Default::default();
    let mut downs: Vec<(u32, String)> = Vec::new();
    for (path, bytes) in files {
        let Some(name) = path.strip_prefix(&prefix) else {
            continue;
        };
        if name.contains('/') || !name.ends_with(".sql") {
            continue;
        }
        let (stem, is_up) = if let Some(s) = name.strip_suffix(".up.sql") {
            (s, true)
        } else if let Some(s) = name.strip_suffix(".down.sql") {
            (s, false)
        } else {
            return Err(format!("'{}' must end in .up.sql or .down.sql", path));
        };
        let (num, label) = stem
            .split_once('_')
            .ok_or_else(|| format!("'{}' must be named NNNN_name", path))?;
        let number: u32 = num
            .parse()
            .map_err(|_| format!("'{}' does not start with a number", path))?;
        if number == 0 {
            return Err(format!("'{}': numbering starts at 1", path));
        }
        if bytes.len() > MAX_MIGRATION_BYTES {
            return Err(format!("'{}' is larger than 256 KB", path));
        }
        let sql =
            String::from_utf8(bytes.clone()).map_err(|_| format!("'{}' is not UTF-8", path))?;
        if is_up {
            let m = MigrationFile {
                number,
                name: label.to_string(),
                up: sql,
                down: None,
            };
            if by_number.insert(number, m).is_some() {
                return Err(format!("migration {} is defined twice", number));
            }
        } else {
            downs.push((number, sql));
        }
    }
    for (number, sql) in downs {
        match by_number.get_mut(&number) {
            Some(m) if m.down.is_none() => m.down = Some(sql),
            Some(_) => return Err(format!("migration {} has two .down files", number)),
            None => {
                return Err(format!(
                    "migration {} has a .down file but no .up file",
                    number
                ))
            }
        }
    }
    for (i, n) in by_number.keys().enumerate() {
        if *n != i as u32 + 1 {
            return Err(format!(
                "migration numbers must be contiguous from 1 (found {})",
                n
            ));
        }
    }
    Ok(by_number.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn store(limits: StorageLimits) -> (PluginStorage, tempfile::TempDir) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        (
            PluginStorage::new(pool, dir.path().to_path_buf(), limits),
            dir,
        )
    }

    #[tokio::test]
    async fn kv_roundtrip_overwrite_delete_and_prefix_list() {
        let (s, _d) = store(Default::default()).await;
        assert_eq!(s.kv_get("a", "k").await.unwrap(), None);
        s.kv_put("a", "k", &json!({"n": 1})).await.unwrap();
        s.kv_put("a", "k", &json!({"n": 2})).await.unwrap();
        assert_eq!(s.kv_get("a", "k").await.unwrap(), Some(json!({"n": 2})));
        s.kv_put("a", "x%1", &json!(1)).await.unwrap();
        s.kv_put("a", "xz1", &json!(2)).await.unwrap();
        s.kv_put("a", "x_2", &json!(3)).await.unwrap();
        let l = s.kv_list("a", "x%", 10).await.unwrap();
        assert_eq!(l, vec![("x%1".to_string(), json!(1))]);
        assert_eq!(s.kv_list("a", "x_", 10).await.unwrap().len(), 1);
        assert_eq!(s.kv_list("a", "", 10).await.unwrap().len(), 4);
        assert!(s.kv_delete("a", "k").await.unwrap());
        assert!(!s.kv_delete("a", "k").await.unwrap());
        assert_eq!(s.kv_get("b", "xz1").await.unwrap(), None);
        assert!(s.kv_list("b", "", 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn kv_quotas_apply() {
        let (s, _d) = store(StorageLimits {
            kv_max_keys: 2,
            kv_max_bytes: 100,
            kv_max_value_bytes: 20,
            ..Default::default()
        })
        .await;
        s.kv_put("a", "1", &json!("x")).await.unwrap();
        s.kv_put("a", "2", &json!("y")).await.unwrap();
        assert!(matches!(
            s.kv_put("a", "3", &json!("z")).await,
            Err(StorageError::Quota(_))
        ));
        s.kv_put("a", "1", &json!("w")).await.unwrap();
        assert!(matches!(
            s.kv_put("a", "1", &json!("a very long value that is too big"))
                .await,
            Err(StorageError::Quota(_))
        ));
        assert!(matches!(
            s.kv_put("a", "", &json!(1)).await,
            Err(StorageError::Invalid(_))
        ));
        s.kv_put("b", "1", &json!(1)).await.unwrap();
        assert_eq!(s.kv_usage("a").await.unwrap().0, 2);
        let (s2, _d2) = store(StorageLimits {
            kv_max_bytes: 12,
            ..Default::default()
        })
        .await;
        s2.kv_put("a", "k", &json!("12345")).await.unwrap();
        assert!(matches!(
            s2.kv_put("a", "k2", &json!("123456")).await,
            Err(StorageError::Quota(_))
        ));
    }

    #[tokio::test]
    async fn db_is_private_per_plugin_and_works() {
        let (s, _d) = store(Default::default()).await;
        s.db_exec(
            "a",
            "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT, score REAL, data BLOB)",
            &[],
        )
        .await
        .unwrap();
        let r = s
            .db_exec(
                "a",
                "INSERT INTO t(name, score) VALUES (?, ?)",
                &[json!("x"), json!(1.5)],
            )
            .await
            .unwrap();
        assert_eq!((r.rows_affected, r.last_insert_rowid), (1, 1));
        let rows = s
            .db_query(
                "a",
                "SELECT id, name, score FROM t WHERE name = ?",
                &[json!("x")],
            )
            .await
            .unwrap();
        assert_eq!(rows[0]["name"], "x");
        assert_eq!(rows[0]["score"], 1.5);
        assert_eq!(rows[0]["id"], 1);
        assert!(s.db_query("b", "SELECT * FROM t", &[]).await.is_err());
    }

    #[tokio::test]
    async fn dangerous_statements_are_denied() {
        let (s, d) = store(Default::default()).await;
        s.db_exec("a", "CREATE TABLE t(v)", &[]).await.unwrap();
        let other = d.path().join("other.db");
        for sql in [
            format!("ATTACH DATABASE '{}' AS o", other.display()),
            "SELECT load_extension('x')".to_string(),
            "PRAGMA max_page_count = 100000".to_string(),
            "PRAGMA trusted_schema = ON".to_string(),
            "PRAGMA journal_mode = OFF".to_string(),
            "CREATE TEMP TABLE x(v)".to_string(),
        ] {
            let r = s.db_exec("a", &sql, &[]).await;
            assert!(r.is_err(), "{sql} should be denied");
        }
        assert!(!other.exists());
        let rows = s.db_query("a", "PRAGMA table_info(t)", &[]).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn db_query_refuses_writes() {
        let (s, _d) = store(Default::default()).await;
        s.db_exec("a", "CREATE TABLE t(v)", &[]).await.unwrap();
        assert!(s
            .db_query("a", "INSERT INTO t VALUES (1)", &[])
            .await
            .is_err());
        assert!(s.db_query("a", "DROP TABLE t", &[]).await.is_err());
        s.db_exec("a", "INSERT INTO t VALUES (1)", &[])
            .await
            .unwrap();
        assert_eq!(
            s.db_query("a", "SELECT * FROM t", &[]).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn size_cap_and_deadline_are_enforced() {
        let (s, _d) = store(StorageLimits {
            db_max_bytes: 64 * 1024,
            db_query_timeout: Duration::from_millis(300),
            ..Default::default()
        })
        .await;
        s.db_exec("a", "CREATE TABLE t(v TEXT)", &[]).await.unwrap();
        let big = "x".repeat(10_000);
        let mut hit = None;
        for _ in 0..100 {
            if let Err(e) = s
                .db_exec("a", "INSERT INTO t VALUES (?)", &[json!(big)])
                .await
            {
                hit = Some(e);
                break;
            }
        }
        assert!(matches!(hit, Some(StorageError::Quota(_))), "{hit:?}");
        let start = Instant::now();
        let e = s
            .db_query(
                "b",
                "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c) SELECT count(*) FROM c",
                &[],
            )
            .await
            .unwrap_err();
        assert!(matches!(e, StorageError::Timeout), "{e:?}");
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(s.db_query("b", "SELECT 1 AS one", &[]).await.is_ok());
    }

    fn mig(n: u32, up: &str, down: Option<&str>) -> MigrationFile {
        MigrationFile {
            number: n,
            name: format!("m{n}"),
            up: up.into(),
            down: down.map(Into::into),
        }
    }

    #[tokio::test]
    async fn migrations_apply_revert_and_guard_their_checksums() {
        let (s, _d) = store(Default::default()).await;
        let files = vec![
            mig(1, "CREATE TABLE a(v)", Some("DROP TABLE a")),
            mig(2, "CREATE TABLE b(v)", Some("DROP TABLE b")),
            mig(3, "CREATE TABLE c(v)", None),
        ];
        assert_eq!(s.migrate_up("p", &files, Some(2)).await.unwrap(), 2);
        assert_eq!(s.schema_version("p").await.unwrap(), 2);
        assert!(s.db_query("p", "SELECT * FROM b", &[]).await.is_ok());
        assert!(s.db_query("p", "SELECT * FROM c", &[]).await.is_err());
        assert_eq!(s.migrate_up("p", &files, None).await.unwrap(), 3);
        assert!(s.migrate_down("p", &files, 1).await.is_err());
        assert_eq!(s.schema_version("p").await.unwrap(), 3);
        assert!(s.db_query("p", "SELECT * FROM b", &[]).await.is_ok());
        let mut tampered = files.clone();
        tampered[0].up = "CREATE TABLE a(v, extra)".into();
        assert!(s.migrate_up("p", &tampered, None).await.is_err());
        let mut with_down = files.clone();
        with_down[2].down = Some("DROP TABLE c".into());
        assert_eq!(s.migrate_down("p", &with_down, 1).await.unwrap(), 1);
        assert!(s.db_query("p", "SELECT * FROM b", &[]).await.is_err());
    }

    #[tokio::test]
    async fn a_failed_migration_leaves_no_partial_state() {
        let (s, _d) = store(Default::default()).await;
        let files = vec![
            mig(1, "CREATE TABLE a(v)", None),
            mig(
                2,
                "CREATE TABLE b(v); INSERT INTO missing VALUES (1);",
                None,
            ),
        ];
        let e = s.migrate_up("p", &files, None).await.unwrap_err();
        assert!(e.to_string().contains("migration 2"));
        assert_eq!(s.schema_version("p").await.unwrap(), 1);
        assert!(s.db_query("p", "SELECT * FROM b", &[]).await.is_err());
    }

    fn f(path: &str, body: &str) -> (String, Vec<u8>) {
        (path.to_string(), body.as_bytes().to_vec())
    }

    #[test]
    fn migration_files_are_parsed_and_checked() {
        let ok = collect_migrations(
            &[
                f("migrations/0002_b.up.sql", "B"),
                f("migrations/0001_a.up.sql", "A"),
                f("migrations/0001_a.down.sql", "a"),
                f("other/ignored.sql", "x"),
            ],
            "migrations",
        )
        .unwrap();
        assert_eq!(ok.len(), 2);
        assert_eq!((ok[0].number, ok[0].down.as_deref()), (1, Some("a")));
        assert!(ok[1].down.is_none());
        let gap = collect_migrations(&[f("m/0001_a.up.sql", "A"), f("m/0003_c.up.sql", "C")], "m");
        assert!(gap.unwrap_err().contains("contiguous"));
        let dup = collect_migrations(&[f("m/0001_a.up.sql", "A"), f("m/0001_b.up.sql", "B")], "m");
        assert!(dup.unwrap_err().contains("twice"));
        let orphan = collect_migrations(&[f("m/0001_a.down.sql", "A")], "m");
        assert!(orphan.unwrap_err().contains("no .up"));
        assert!(collect_migrations(&[f("m/x_a.up.sql", "A")], "m").is_err());
        assert!(collect_migrations(&[f("m/0001_a.sql", "A")], "m").is_err());
    }

    #[tokio::test]
    async fn purge_removes_rows_and_file() {
        let (s, _d) = store(Default::default()).await;
        s.kv_put("p", "k", &json!(1)).await.unwrap();
        s.migrate_up("p", &[mig(1, "CREATE TABLE a(v)", None)], None)
            .await
            .unwrap();
        let path = s.db_path("p");
        assert!(path.exists());
        s.purge("p").await.unwrap();
        assert!(!path.exists());
        assert_eq!(s.kv_get("p", "k").await.unwrap(), None);
        assert_eq!(s.schema_version("p").await.unwrap(), 0);
        assert!(s.db_query("p", "SELECT * FROM a", &[]).await.is_err());
        assert!(check_plugin("../x").is_err());
    }
}
