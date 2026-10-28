use std::time::Duration;

use plugin_poc::plugin_db::{connect_raw, run_sandboxed, Limits};

const LIMITS: Limits = Limits { max_bytes: 256 * 1024, query_timeout: Duration::from_millis(200) };

async fn conn() -> (tempfile::TempDir, sqlx::SqliteConnection) {
    let dir = tempfile::tempdir().unwrap();
    let c = connect_raw(&dir.path().join("p.db"), LIMITS).await.unwrap();
    (dir, c)
}

#[tokio::test]
async fn allows_normal_ddl_and_dml() {
    let (_d, mut c) = conn().await;
    run_sandboxed(&mut c, LIMITS, "CREATE TABLE t(a TEXT)").await.unwrap();
    assert_eq!(run_sandboxed(&mut c, LIMITS, "INSERT INTO t VALUES ('x')").await.unwrap(), 1);
}

#[tokio::test]
async fn denies_attach() {
    let (d, mut c) = conn().await;
    let sql = format!("ATTACH DATABASE '{}' AS other", d.path().join("o.db").display());
    assert!(run_sandboxed(&mut c, LIMITS, &sql).await.is_err());
}

#[tokio::test]
async fn denies_load_extension_and_pragmas() {
    let (_d, mut c) = conn().await;
    assert!(run_sandboxed(&mut c, LIMITS, "SELECT load_extension('x')").await.is_err());
    assert!(run_sandboxed(&mut c, LIMITS, "PRAGMA max_page_count=1000000").await.is_err());
    assert!(run_sandboxed(&mut c, LIMITS, "PRAGMA trusted_schema=ON").await.is_err());
}

#[tokio::test]
async fn enforces_size_limit() {
    let (_d, mut c) = conn().await;
    run_sandboxed(&mut c, LIMITS, "CREATE TABLE t(a BLOB)").await.unwrap();
    let mut full = false;
    for _ in 0..200 {
        if run_sandboxed(&mut c, LIMITS, "INSERT INTO t VALUES (zeroblob(8192))").await.is_err() {
            full = true;
            break;
        }
    }
    assert!(full, "inserts must fail once max_page_count is reached");
}

#[tokio::test]
async fn enforces_query_timeout() {
    let (_d, mut c) = conn().await;
    let started = std::time::Instant::now();
    let r = run_sandboxed(
        &mut c,
        LIMITS,
        "WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT count(*) FROM r",
    )
    .await;
    assert!(r.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn files_are_isolated_per_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = connect_raw(&dir.path().join("a.db"), LIMITS).await.unwrap();
    let mut b = connect_raw(&dir.path().join("b.db"), LIMITS).await.unwrap();
    run_sandboxed(&mut a, LIMITS, "CREATE TABLE secret(v)").await.unwrap();
    assert!(run_sandboxed(&mut b, LIMITS, "SELECT * FROM secret").await.is_err());
}
