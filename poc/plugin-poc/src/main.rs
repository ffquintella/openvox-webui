//! Size probe for T67: the same Axum + sqlx baseline, with or without the
//! wasmtime runtime (feature `wasm`).
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await?;
    sqlx::query("SELECT 1").execute(&pool).await?;
    #[allow(unused_mut)]
    let mut app = axum::Router::new().route("/health", axum::routing::get(|| async { "ok" }));
    #[cfg(feature = "wasm")]
    {
        use std::sync::Arc;
        use plugin_poc::wasm::*;
        let st = AppState { engine: engine().map_err(|e| anyhow::anyhow!("{e}"))?, wasm: Arc::new(wat::parse_str(GUEST_WAT)?), fuel: 50_000_000, max_memory: 4 << 20 };
        app = app.merge(router(st));
    }
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    axum::serve(l, app).await?;
    Ok(())
}
