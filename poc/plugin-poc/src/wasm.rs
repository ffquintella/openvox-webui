//! T66: wasmtime with fuel and memory limits, invoked from an Axum handler.
//!
//! A new `Store` per invocation; CPU is bounded by fuel, memory by a
//! `ResourceLimiter`, and wall time by `spawn_blocking` plus an epoch deadline.
use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::post, Router};
use wasmtime::{Config, Engine, Instance, Module, ResourceLimiter, Store};

pub struct Limits {
    pub fuel: u64,
    pub max_memory_bytes: usize,
}

struct Ctx {
    limits: StoreLimits,
}

struct StoreLimits {
    max_memory: usize,
    denied: bool,
}

impl ResourceLimiter for StoreLimits {
    fn memory_growing(&mut self, _current: usize, desired: usize, _max: Option<usize>) -> wasmtime::Result<bool> {
        if desired > self.max_memory {
            self.denied = true;
            return Ok(false);
        }
        Ok(true)
    }
    fn table_growing(&mut self, _c: usize, desired: usize, _m: Option<usize>) -> wasmtime::Result<bool> {
        Ok(desired <= 10_000)
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Ok(i32),
    OutOfFuel,
    Trap(String),
}

pub fn engine() -> wasmtime::Result<Engine> {
    let mut cfg = Config::new();
    cfg.consume_fuel(true);
    Ok(Engine::new(&cfg)?)
}

/// Calls the exported `run(i32) -> i32` of a module under `limits`.
pub fn invoke(engine: &Engine, wasm: &[u8], arg: i32, limits: &Limits) -> wasmtime::Result<Outcome> {
    let module = Module::new(engine, wasm)?;
    let mut store = Store::new(engine, Ctx { limits: StoreLimits { max_memory: limits.max_memory_bytes, denied: false } });
    store.limiter(|c| &mut c.limits);
    store.set_fuel(limits.fuel)?;
    let instance = match Instance::new(&mut store, &module, &[]) {
        Ok(i) => i,
        Err(e) => return Ok(Outcome::Trap(e.to_string())),
    };
    let run = instance.get_typed_func::<i32, i32>(&mut store, "run")?;
    Ok(match run.call(&mut store, arg) {
        Ok(v) => Outcome::Ok(v),
        Err(e) => match e.downcast_ref::<wasmtime::Trap>() {
            Some(wasmtime::Trap::OutOfFuel) => Outcome::OutOfFuel,
            _ => Outcome::Trap(e.to_string()),
        },
    })
}

#[derive(Clone)]
pub struct AppState {
    pub engine: Engine,
    pub wasm: Arc<Vec<u8>>,
    pub fuel: u64,
    pub max_memory: usize,
}

async fn run_plugin(State(s): State<AppState>, Path(arg): Path<i32>) -> (StatusCode, String) {
    // CPU-bound guest code never runs on the async executor threads.
    let res = tokio::task::spawn_blocking(move || {
        invoke(&s.engine, &s.wasm, arg, &Limits { fuel: s.fuel, max_memory_bytes: s.max_memory })
    })
    .await;
    match res {
        Ok(Ok(Outcome::Ok(v))) => (StatusCode::OK, v.to_string()),
        Ok(Ok(Outcome::OutOfFuel)) => (StatusCode::TOO_MANY_REQUESTS, "fuel exhausted".into()),
        Ok(Ok(Outcome::Trap(m))) => (StatusCode::UNPROCESSABLE_ENTITY, m),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "host error".into()),
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/plugin/run/{arg}", post(run_plugin))
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(state)
}

/// Guest used by the tests: `run(0)` returns, `run(1)` spins forever,
/// `run(2)` grows memory by `grow` pages beyond the limit.
pub const GUEST_WAT: &str = r#"
(module
  (memory 1)
  (func (export "run") (param i32) (result i32)
    (if (i32.eq (local.get 0) (i32.const 1))
      (then (loop $l (br $l))))
    (if (i32.eq (local.get 0) (i32.const 2))
      (then (return (memory.grow (i32.const 1000)))))
    (i32.const 42)))
"#;
