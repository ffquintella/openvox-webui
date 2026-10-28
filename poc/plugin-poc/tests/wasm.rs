#![cfg(feature = "wasm")]
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{body::Body, http::{Request, StatusCode}};
use plugin_poc::wasm::*;
use tower::ServiceExt;

fn state(fuel: u64, mem: usize) -> AppState {
    AppState { engine: engine().unwrap(), wasm: Arc::new(wat::parse_str(GUEST_WAT).unwrap()), fuel, max_memory: mem }
}

async fn call(app: axum::Router, arg: i32) -> StatusCode {
    app.oneshot(Request::post(format!("/plugin/run/{arg}")).body(Body::empty()).unwrap()).await.unwrap().status()
}

#[tokio::test]
async fn normal_call_succeeds() {
    assert_eq!(call(router(state(1_000_000, 4 << 20)), 0).await, StatusCode::OK);
}

#[tokio::test]
async fn infinite_loop_is_aborted_by_fuel() {
    let t = Instant::now();
    assert_eq!(call(router(state(50_000_000, 4 << 20)), 1).await, StatusCode::TOO_MANY_REQUESTS);
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn memory_growth_beyond_limit_is_denied() {
    // memory.grow returns -1 when the limiter refuses, the call itself does not trap.
    let e = engine().unwrap();
    let wasm = wat::parse_str(GUEST_WAT).unwrap();
    let out = invoke(&e, &wasm, 2, &Limits { fuel: 1_000_000, max_memory_bytes: 4 << 20 }).unwrap();
    assert_eq!(out, Outcome::Ok(-1));
}

#[tokio::test]
async fn core_stays_responsive_while_plugin_spins() {
    let app = router(state(2_000_000_000, 4 << 20));
    let spin = tokio::spawn(call(app.clone(), 1));
    tokio::time::sleep(Duration::from_millis(50)).await;
    let t = Instant::now();
    let r = app.oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(t.elapsed() < Duration::from_millis(200));
    spin.await.unwrap();
}
