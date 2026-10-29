//! WASM runtime of the plugin platform (M20, T73 and T76). Documented
//! deviation from PLUGIN_PLATFORM.md §3.1: plain wasm core modules with a JSON
//! over linear memory ABI instead of the Component Model, and no WASI.
//! Every call gets a new `Store` with fuel, memory and deadline limits.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{OnceCell, Semaphore};
use wasmtime::{
    Caller, Config, Engine, Extern, ExternType, FuncType, Linker, Module, ResourceLimiter, Store,
    Trap, ValType,
};

use super::invoke::{
    HandlerCall, HandlerInvoker, HandlerKind, InvokeError, DEFAULT_HANDLER_TIMEOUT_MS,
    MAX_HANDLER_TIMEOUT_MS,
};

/// Name of the only module the guest may import from.
const HOST_MODULE: &str = "openvox";
/// Name of the only function the guest may import.
const HOST_FN: &str = "host_call";
/// Milliseconds between two engine epoch increments.
const EPOCH_TICK_MS: u64 = 10;
/// Extra wall time the async side waits for a guest stuck in a host call.
const DEADLINE_GRACE_MS: u64 = 2_000;
/// Longest operation name a guest may pass to `host_call`.
const MAX_OP_LEN: usize = 256;
/// Largest table a guest may grow to (elements).
const MAX_TABLE_ELEMENTS: usize = 100_000;
/// Longest guest error message that is passed on.
const MAX_ERROR_LEN: usize = 1000;

/// Who is calling the host: the plugin version and the handler being run.
#[derive(Debug, Clone)]
pub struct HostContext {
    pub plugin_id: String,
    pub version: String,
    pub kind: HandlerKind,
    pub handler: String,
}

/// A failed host call, returned to the guest as an error envelope.
#[derive(Debug, Clone)]
pub struct HostError {
    pub code: String,
    pub message: String,
}

/// Host side of `openvox.host_call`. Synchronous: it runs on a blocking
/// thread, so implementations may block on the async runtime.
pub trait HostApi: Send + Sync {
    fn call(&self, ctx: &HostContext, op: &str, request: Value) -> Result<Value, HostError>;
}

/// Supplies the module bytes of a plugin version.
#[async_trait]
pub trait ModuleSource: Send + Sync {
    async fn load(&self, plugin_id: &str, version: &str, entry: &str) -> Result<Vec<u8>, String>;
}

/// Host ceilings and defaults of the WASM runtime.
#[derive(Debug, Clone)]
pub struct WasmConfig {
    pub default_memory_mb: u64,
    pub max_memory_mb: u64,
    pub default_fuel: u64,
    pub max_fuel: u64,
    /// Largest request, response or host message in bytes.
    pub max_message_bytes: usize,
    /// Host calls allowed in one handler call.
    pub max_host_calls: u32,
    /// Handler calls of one plugin that may run at the same time.
    pub per_plugin_concurrency: u32,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            default_memory_mb: 32,
            max_memory_mb: 256,
            default_fuel: 50_000_000,
            max_fuel: 5_000_000_000,
            max_message_bytes: 1024 * 1024,
            max_host_calls: 1000,
            per_plugin_concurrency: 4,
        }
    }
}

fn failed(msg: impl Into<String>) -> InvokeError {
    InvokeError::Failed(msg.into())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}

fn first_line(s: &str) -> String {
    truncate(s.lines().next().unwrap_or(""), 300)
}

// ---- engine, limiter and module checks ---------------------------------------

fn build_engine() -> wasmtime::Result<Engine> {
    let mut cfg = Config::new();
    cfg.consume_fuel(true);
    cfg.epoch_interruption(true);
    cfg.wasm_memory64(false);
    Engine::new(&cfg)
}

/// Enforces the memory, table and instance ceilings of one call.
struct GuestLimiter {
    max_memory: usize,
    denied: bool,
}

impl ResourceLimiter for GuestLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.max_memory {
            self.denied = true;
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= MAX_TABLE_ELEMENTS)
    }

    fn instances(&self) -> usize {
        1
    }

    fn tables(&self) -> usize {
        4
    }

    fn memories(&self) -> usize {
        1
    }
}

fn kinds(it: impl Iterator<Item = ValType>) -> String {
    it.map(|v| match v {
        ValType::I32 => 'i',
        ValType::I64 => 'l',
        _ => '?',
    })
    .collect()
}

fn has_sig(f: &FuncType, params: &str, results: &str) -> bool {
    kinds(f.params()) == params && kinds(f.results()) == results
}

/// Checks the imports and exports of a compiled module against the ABI.
fn check_shape(module: &Module) -> Result<(), String> {
    for imp in module.imports() {
        if imp.module() != HOST_MODULE || imp.name() != HOST_FN {
            return Err(format!(
                "import '{}.{}' is not allowed; the only import is {}.{}",
                imp.module(),
                imp.name(),
                HOST_MODULE,
                HOST_FN
            ));
        }
        match imp.ty() {
            ExternType::Func(f) if has_sig(&f, "iiii", "l") => {}
            _ => {
                return Err(format!(
                    "{}.{} must have the type (i32, i32, i32, i32) -> i64",
                    HOST_MODULE, HOST_FN
                ))
            }
        }
    }
    match module.get_export("memory") {
        Some(ExternType::Memory(m)) if !m.is_64() && !m.is_shared() => {}
        _ => return Err("the module must export a 32-bit, non-shared memory 'memory'".into()),
    }
    let funcs: [(&str, &str, &str, bool); 3] = [
        ("alloc", "i", "i", true),
        ("handle", "ii", "l", true),
        ("dealloc", "ii", "", false),
    ];
    for (name, params, results, required) in funcs {
        match module.get_export(name) {
            Some(ExternType::Func(f)) if has_sig(&f, params, results) => {}
            Some(_) => return Err(format!("export '{}' has the wrong type", name)),
            None if required => return Err(format!("the module must export '{}'", name)),
            None => {}
        }
    }
    Ok(())
}

fn compile(engine: &Engine, bytes: &[u8]) -> Result<Module, String> {
    let module = Module::new(engine, bytes).map_err(|e| first_line(&e.to_string()))?;
    check_shape(&module)?;
    Ok(module)
}

fn validation_engine() -> Result<&'static Engine, String> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| build_engine().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| e.clone())
}

// ---- per-call state and the host function ------------------------------------

/// Why the host stopped the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Abort {
    HostCalls,
    Deadline,
    Message,
}

struct GuestState {
    limiter: GuestLimiter,
    host: Arc<dyn HostApi>,
    ctx: HostContext,
    host_calls: u32,
    max_host_calls: u32,
    max_message: usize,
    deadline: Instant,
    abort: Option<Abort>,
}

fn pack(ptr: u32, len: u32) -> i64 {
    (((ptr as u64) << 32) | len as u64) as i64
}

fn unpack(v: i64) -> (usize, usize) {
    let v = v as u64;
    ((v >> 32) as usize, (v & 0xffff_ffff) as usize)
}

fn error_envelope(code: &str, message: &str) -> Value {
    json!({"error": {"code": code, "message": message}})
}

/// Reads `len` bytes at `ptr` of guest memory. Both values are untrusted:
/// `max` bounds the allocation and the read itself is bounds-checked.
fn read_guest(
    caller: &Caller<'_, GuestState>,
    memory: &wasmtime::Memory,
    ptr: i32,
    len: i32,
    max: usize,
) -> wasmtime::Result<Vec<u8>> {
    let len = len as u32 as usize;
    if len > max {
        wasmtime::bail!(
            "guest message of {} bytes exceeds the limit of {}",
            len,
            max
        );
    }
    let start = ptr as u32 as usize;
    let mut buf = vec![0u8; len];
    memory
        .read(caller, start, &mut buf)
        .map_err(|_| wasmtime::format_err!("guest pointer out of bounds"))?;
    Ok(buf)
}

/// `openvox.host_call(op_ptr, op_len, req_ptr, req_len) -> i64`: forwards one
/// request to the [`HostApi`] and answers with an `{"ok": ..}` or
/// `{"error": {"code","message"}}` envelope allocated through the guest.
fn host_call(
    mut caller: Caller<'_, GuestState>,
    op_ptr: i32,
    op_len: i32,
    req_ptr: i32,
    req_len: i32,
) -> wasmtime::Result<i64> {
    let st = caller.data_mut();
    st.host_calls = st.host_calls.saturating_add(1);
    if st.host_calls > st.max_host_calls {
        st.abort = Some(Abort::HostCalls);
        wasmtime::bail!("host call limit exceeded");
    }
    if Instant::now() >= st.deadline {
        st.abort = Some(Abort::Deadline);
        wasmtime::bail!("deadline exceeded");
    }
    let max = st.max_message;
    let memory = match caller.get_export("memory") {
        Some(Extern::Memory(m)) => m,
        _ => wasmtime::bail!("the guest exports no memory"),
    };
    let op = read_guest(&caller, &memory, op_ptr, op_len, MAX_OP_LEN).inspect_err(|_| {
        caller.data_mut().abort = Some(Abort::Message);
    })?;
    let req = read_guest(&caller, &memory, req_ptr, req_len, max).inspect_err(|_| {
        caller.data_mut().abort = Some(Abort::Message);
    })?;
    let op = String::from_utf8(op).map_err(|_| wasmtime::format_err!("operation is not UTF-8"))?;

    let envelope = match serde_json::from_slice::<Value>(&req) {
        Err(e) => error_envelope("invalid", &format!("request is not valid JSON: {}", e)),
        Ok(request) => {
            let (host, ctx) = {
                let st = caller.data();
                (st.host.clone(), st.ctx.clone())
            };
            match catch_unwind(AssertUnwindSafe(|| host.call(&ctx, &op, request))) {
                Ok(Ok(v)) => json!({"ok": v}),
                Ok(Err(e)) => error_envelope(&e.code, &e.message),
                Err(_) => error_envelope("internal", "the host call panicked"),
            }
        }
    };
    if Instant::now() >= caller.data().deadline {
        caller.data_mut().abort = Some(Abort::Deadline);
        wasmtime::bail!("deadline exceeded");
    }
    let mut out = serde_json::to_vec(&envelope)
        .map_err(|e| wasmtime::format_err!("host response could not be encoded: {}", e))?;
    if out.len() > max {
        out = serde_json::to_vec(&error_envelope(
            "too_large",
            "the host response exceeds the message limit",
        ))
        .map_err(|e| wasmtime::format_err!("host response could not be encoded: {}", e))?;
    }
    let len = i32::try_from(out.len()).map_err(|_| wasmtime::format_err!("response too large"))?;
    let alloc = match caller.get_export("alloc") {
        Some(Extern::Func(f)) => f.typed::<i32, i32>(&caller)?,
        _ => wasmtime::bail!("the guest exports no alloc"),
    };
    let ptr = alloc.call(&mut caller, len)?;
    memory
        .write(&mut caller, ptr as u32 as usize, &out)
        .map_err(|_| wasmtime::format_err!("alloc returned a pointer out of bounds"))?;
    Ok(pack(ptr as u32, len as u32))
}

/// Everything one guest run needs, resolved from the call and the config.
struct RunParams {
    request: Vec<u8>,
    ctx: HostContext,
    fuel: u64,
    memory_bytes: usize,
    timeout_ms: u64,
    max_message: usize,
    max_host_calls: u32,
}

/// Maps an error raised while running the guest to an [`InvokeError`].
fn map_wasm_error(e: wasmtime::Error, st: &GuestState, timeout_ms: u64) -> InvokeError {
    if let Some(t) = e.downcast_ref::<Trap>() {
        match t {
            Trap::OutOfFuel => return failed("fuel exhausted"),
            Trap::Interrupt => return InvokeError::Timeout(timeout_ms),
            _ => {}
        }
    }
    match st.abort {
        Some(Abort::Deadline) => return InvokeError::Timeout(timeout_ms),
        Some(Abort::HostCalls) => return failed("host call limit exceeded"),
        Some(Abort::Message) => {
            return failed(format!(
                "invalid guest message: {}",
                first_line(&e.to_string())
            ))
        }
        None => {}
    }
    if st.limiter.denied {
        return failed("memory limit exceeded");
    }
    failed(format!("guest trapped: {}", first_line(&e.to_string())))
}

/// Runs one handler call to completion on the current (blocking) thread.
fn run_guest(
    engine: &Engine,
    linker: &Linker<GuestState>,
    module: &Module,
    host: Arc<dyn HostApi>,
    p: RunParams,
) -> Result<Value, InvokeError> {
    let state = GuestState {
        limiter: GuestLimiter {
            max_memory: p.memory_bytes,
            denied: false,
        },
        host,
        ctx: p.ctx,
        host_calls: 0,
        max_host_calls: p.max_host_calls,
        max_message: p.max_message,
        deadline: Instant::now() + Duration::from_millis(p.timeout_ms),
        abort: None,
    };
    let mut store = Store::new(engine, state);
    store.limiter(|s| &mut s.limiter);
    store
        .set_fuel(p.fuel)
        .map_err(|e| failed(first_line(&e.to_string())))?;
    store.set_epoch_deadline(p.timeout_ms / EPOCH_TICK_MS + 1);
    store.epoch_deadline_trap();

    macro_rules! tr {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Err(map_wasm_error(e, store.data(), p.timeout_ms)),
            }
        };
    }

    let instance = tr!(linker.instantiate(&mut store, module));
    let memory = match instance.get_memory(&mut store, "memory") {
        Some(m) => m,
        None => return Err(failed("the module exports no memory")),
    };
    let alloc = tr!(instance.get_typed_func::<i32, i32>(&mut store, "alloc"));
    let handle = tr!(instance.get_typed_func::<(i32, i32), i64>(&mut store, "handle"));

    let req_len = i32::try_from(p.request.len()).map_err(|_| failed("request too large"))?;
    let req_ptr = tr!(alloc.call(&mut store, req_len));
    if memory
        .write(&mut store, req_ptr as u32 as usize, &p.request)
        .is_err()
    {
        return Err(failed("alloc returned a pointer out of bounds"));
    }
    let packed = tr!(handle.call(&mut store, (req_ptr, req_len)));

    let (out_ptr, out_len) = unpack(packed);
    if out_len > p.max_message {
        return Err(failed(format!(
            "guest output of {} bytes exceeds the limit of {}",
            out_len, p.max_message
        )));
    }
    let mut out = vec![0u8; out_len];
    if memory.read(&store, out_ptr, &mut out).is_err() {
        return Err(failed("guest output is out of bounds"));
    }
    parse_output(&out)
}

/// `{"ok": v}` yields `v`, `{"error": m}` a failure; anything else is malformed.
fn parse_output(bytes: &[u8]) -> Result<Value, InvokeError> {
    let v: Value =
        serde_json::from_slice(bytes).map_err(|_| failed("handler output is not valid JSON"))?;
    let Value::Object(mut map) = v else {
        return Err(failed("handler output must be a JSON object"));
    };
    if map.len() != 1 {
        return Err(failed(
            "handler output must have exactly one of 'ok' or 'error'",
        ));
    }
    if let Some(ok) = map.remove("ok") {
        return Ok(ok);
    }
    match map.remove("error") {
        Some(Value::String(m)) => Err(failed(truncate(&m, MAX_ERROR_LEN))),
        Some(other) => Err(failed(truncate(&other.to_string(), MAX_ERROR_LEN))),
        None => Err(failed(
            "handler output must have exactly one of 'ok' or 'error'",
        )),
    }
}

/// The JSON the guest receives; field order is part of the ABI.
#[derive(Serialize)]
struct GuestRequest<'a> {
    kind: &'a str,
    handler: &'a str,
    input: &'a Value,
}

// ---- the invoker --------------------------------------------------------------

type ModuleKey = (String, String);

/// Runs plugin handlers in sandboxed wasm modules.
///
/// The guest exports `memory`, `alloc(len) -> ptr` and `handle(ptr, len) ->
/// (out_ptr << 32) | out_len`; `dealloc` is optional and never called.
pub struct WasmInvoker {
    cfg: WasmConfig,
    engine: Engine,
    linker: Arc<Linker<GuestState>>,
    source: Arc<dyn ModuleSource>,
    host: Arc<dyn HostApi>,
    modules: Mutex<HashMap<ModuleKey, Arc<OnceCell<Arc<Module>>>>>,
    bulkheads: Mutex<HashMap<String, Arc<Semaphore>>>,
    stop_ticker: Arc<AtomicBool>,
}

impl WasmInvoker {
    /// Builds the engine, the host linker and the epoch ticker thread.
    pub fn new(
        cfg: WasmConfig,
        source: Arc<dyn ModuleSource>,
        host: Arc<dyn HostApi>,
    ) -> anyhow::Result<Self> {
        let engine = build_engine().map_err(|e| anyhow::anyhow!("wasm engine: {}", e))?;
        let mut linker: Linker<GuestState> = Linker::new(&engine);
        linker
            .func_wrap(HOST_MODULE, HOST_FN, host_call)
            .map_err(|e| anyhow::anyhow!("wasm linker: {}", e))?;
        let stop = Arc::new(AtomicBool::new(false));
        let (ticker_engine, ticker_stop) = (engine.clone(), stop.clone());
        std::thread::Builder::new()
            .name("wasm-epoch".into())
            .spawn(move || {
                while !ticker_stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(EPOCH_TICK_MS));
                    ticker_engine.increment_epoch();
                }
            })?;
        Ok(Self {
            cfg,
            engine,
            linker: Arc::new(linker),
            source,
            host,
            modules: Mutex::new(HashMap::new()),
            bulkheads: Mutex::new(HashMap::new()),
            stop_ticker: stop,
        })
    }

    /// Checks that the bytes are a wasm module that follows the guest ABI:
    /// the required exports exist and the only import is `openvox.host_call`.
    pub fn validate_module(bytes: &[u8]) -> Result<(), String> {
        let engine = validation_engine()?;
        compile(engine, bytes).map(|_| ())
    }

    /// Drops the compiled modules of a plugin so the next call reloads them
    /// (after an upgrade, rollback, disable or uninstall).
    pub fn forget(&self, plugin_id: &str) {
        self.modules
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(id, _), _| id != plugin_id);
    }

    fn bulkhead(&self, plugin_id: &str) -> Arc<Semaphore> {
        let permits = self.cfg.per_plugin_concurrency.max(1) as usize;
        self.bulkheads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(plugin_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(permits)))
            .clone()
    }

    async fn module(
        &self,
        plugin_id: &str,
        version: &str,
        entry: &str,
    ) -> Result<Arc<Module>, InvokeError> {
        let cell = self
            .modules
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry((plugin_id.to_string(), version.to_string()))
            .or_default()
            .clone();
        let module = cell
            .get_or_try_init(|| async {
                let bytes = self
                    .source
                    .load(plugin_id, version, entry)
                    .await
                    .map_err(|e| {
                        InvokeError::Unavailable(format!("module could not be loaded: {}", e))
                    })?;
                let engine = self.engine.clone();
                let compiled = tokio::task::spawn_blocking(move || compile(&engine, &bytes))
                    .await
                    .map_err(|_| failed("module compilation was aborted"))?
                    .map_err(|e| failed(format!("invalid module: {}", e)))?;
                Ok::<_, InvokeError>(Arc::new(compiled))
            })
            .await?;
        Ok(module.clone())
    }
}

impl Drop for WasmInvoker {
    fn drop(&mut self) {
        self.stop_ticker.store(true, Ordering::Relaxed);
    }
}

#[async_trait]
impl HandlerInvoker for WasmInvoker {
    async fn invoke(&self, call: HandlerCall) -> Result<Value, InvokeError> {
        let entry = call
            .runtime
            .entry
            .clone()
            .filter(|e| !e.is_empty())
            .ok_or_else(|| InvokeError::Unavailable("the wasm plugin declares no entry".into()))?;
        let limits = call.runtime.limits.clone().unwrap_or_default();
        let fuel = limits
            .fuel_per_call
            .unwrap_or(self.cfg.default_fuel)
            .clamp(1, self.cfg.max_fuel.max(1));
        let memory_mb = limits
            .memory_mb
            .unwrap_or(self.cfg.default_memory_mb)
            .clamp(1, self.cfg.max_memory_mb.max(1));
        let memory_bytes =
            usize::try_from(memory_mb.saturating_mul(1024 * 1024)).unwrap_or(usize::MAX);
        let timeout_ms = limits
            .timeout_ms
            .unwrap_or(DEFAULT_HANDLER_TIMEOUT_MS)
            .clamp(1, MAX_HANDLER_TIMEOUT_MS);

        let request = serde_json::to_vec(&GuestRequest {
            kind: call.kind.as_str(),
            handler: &call.handler,
            input: &call.input,
        })
        .map_err(|e| failed(format!("request could not be encoded: {}", e)))?;
        if request.len() > self.cfg.max_message_bytes {
            return Err(failed(format!(
                "request of {} bytes exceeds the limit of {}",
                request.len(),
                self.cfg.max_message_bytes
            )));
        }

        let permit = self
            .bulkhead(&call.plugin_id)
            .try_acquire_owned()
            .map_err(|_| {
                InvokeError::Unavailable(format!(
                    "plugin is busy (limit of {} concurrent calls reached)",
                    self.cfg.per_plugin_concurrency.max(1)
                ))
            })?;
        let module = self.module(&call.plugin_id, &call.version, &entry).await?;

        let params = RunParams {
            request,
            ctx: HostContext {
                plugin_id: call.plugin_id,
                version: call.version,
                kind: call.kind,
                handler: call.handler,
            },
            fuel,
            memory_bytes,
            timeout_ms,
            max_message: self.cfg.max_message_bytes,
            max_host_calls: self.cfg.max_host_calls,
        };
        let (engine, linker, host) = (self.engine.clone(), self.linker.clone(), self.host.clone());
        let task = tokio::task::spawn_blocking(move || {
            // The permit lives as long as the thread does, even if the caller gave up.
            let _permit = permit;
            run_guest(&engine, &linker, &module, host, params)
        });
        let wait = Duration::from_millis(timeout_ms + DEADLINE_GRACE_MS);
        match tokio::time::timeout(wait, task).await {
            Err(_) => Err(InvokeError::Timeout(timeout_ms)),
            Ok(Err(e)) if e.is_panic() => Err(failed("the wasm runtime panicked")),
            Ok(Err(_)) => Err(InvokeError::Unavailable("the call was cancelled".into())),
            Ok(Ok(r)) => r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RuntimeKind, RuntimeLimits, RuntimeSpec};
    use std::sync::atomic::AtomicUsize;

    // Guest dispatching on the first letter of the handler name: ok (echo),
    // error, loop, trap, grow, host, fail, slow, many, Huge, xgarbage,
    // zshape, bad (out of bounds output).
    const GUEST: &str = r#"
(module
  (import "openvox" "host_call" (func $hc (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 2)
  (global $bump (mut i32) (i32.const 2048))
  (data (i32.const 256) "{\"error\":\"boom\"}")
  (data (i32.const 288) "not json")
  (data (i32.const 304) "{\"foo\":1}")
  (data (i32.const 512) "{\"ok\":")
  (data (i32.const 576) "test.op")
  (data (i32.const 592) "fail.op")
  (data (i32.const 608) "{\"a\":1}")
  (data (i32.const 640) "slow.op")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $p))
  (func $pack (param $p i32) (param $l i32) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (local.get $p)) (i64.const 32))
            (i64.extend_i32_u (local.get $l))))
  (func $wrap (param $packed i64) (result i64)
    (local $p i32) (local $l i32) (local $out i32)
    (local.set $p (i32.wrap_i64 (i64.shr_u (local.get $packed) (i64.const 32))))
    (local.set $l (i32.wrap_i64 (local.get $packed)))
    (local.set $out (call $alloc (i32.add (local.get $l) (i32.const 7))))
    (memory.copy (local.get $out) (i32.const 512) (i32.const 6))
    (memory.copy (i32.add (local.get $out) (i32.const 6)) (local.get $p) (local.get $l))
    (i32.store8 (i32.add (i32.add (local.get $out) (i32.const 6)) (local.get $l)) (i32.const 125))
    (call $pack (local.get $out) (i32.add (local.get $l) (i32.const 7))))
  (func $handler_char (param $p i32) (result i32)
    (local $i i32)
    (loop $scan
      (if (i32.and
            (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 104))
            (i32.eq (i32.load8_u (i32.add (i32.add (local.get $p) (local.get $i)) (i32.const 1))) (i32.const 97)))
        (then (return (i32.load8_u (i32.add (i32.add (local.get $p) (local.get $i)) (i32.const 10))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $scan))
    (i32.const 0))
  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    (local $c i32)
    (local.set $c (call $handler_char (local.get $p)))
    (if (i32.eq (local.get $c) (i32.const 111))
      (then (return (call $wrap (call $pack (local.get $p) (local.get $l))))))
    (if (i32.eq (local.get $c) (i32.const 101))
      (then (return (call $pack (i32.const 256) (i32.const 16)))))
    (if (i32.eq (local.get $c) (i32.const 108))
      (then (loop $f (br $f))))
    (if (i32.eq (local.get $c) (i32.const 116))
      (then unreachable))
    (if (i32.eq (local.get $c) (i32.const 103))
      (then
        (if (i32.eq (memory.grow (i32.const 1000)) (i32.const -1))
          (then unreachable))
        (return (call $pack (i32.const 256) (i32.const 16)))))
    (if (i32.eq (local.get $c) (i32.const 104))
      (then (return (call $wrap (call $hc (i32.const 576) (i32.const 7) (i32.const 608) (i32.const 7))))))
    (if (i32.eq (local.get $c) (i32.const 102))
      (then (return (call $wrap (call $hc (i32.const 592) (i32.const 7) (i32.const 608) (i32.const 7))))))
    (if (i32.eq (local.get $c) (i32.const 115))
      (then (return (call $wrap (call $hc (i32.const 640) (i32.const 7) (i32.const 608) (i32.const 7))))))
    (if (i32.eq (local.get $c) (i32.const 109))
      (then (loop $m
        (drop (call $hc (i32.const 576) (i32.const 7) (i32.const 608) (i32.const 7)))
        (br $m))))
    (if (i32.eq (local.get $c) (i32.const 72))
      (then (return (call $pack (i32.const 1024) (i32.const 100000)))))
    (if (i32.eq (local.get $c) (i32.const 120))
      (then (return (call $pack (i32.const 288) (i32.const 8)))))
    (if (i32.eq (local.get $c) (i32.const 122))
      (then (return (call $pack (i32.const 304) (i32.const 9)))))
    (if (i32.eq (local.get $c) (i32.const 98))
      (then (return (call $pack (i32.const 4294967040) (i32.const 100)))))
    (call $pack (i32.const 256) (i32.const 16)))
)
"#;

    #[derive(Default)]
    struct FakeHost {
        calls: Mutex<Vec<(HostContext, String, Value)>>,
    }

    impl HostApi for FakeHost {
        fn call(&self, ctx: &HostContext, op: &str, request: Value) -> Result<Value, HostError> {
            self.calls
                .lock()
                .unwrap()
                .push((ctx.clone(), op.to_string(), request.clone()));
            match op {
                "fail.op" => Err(HostError {
                    code: "denied".into(),
                    message: "nope".into(),
                }),
                "slow.op" => {
                    std::thread::sleep(Duration::from_millis(300));
                    Ok(Value::Null)
                }
                _ => Ok(json!({"echo": request})),
            }
        }
    }

    #[derive(Default)]
    struct FakeSource {
        modules: Mutex<HashMap<(String, String), Vec<u8>>>,
        loads: AtomicUsize,
    }

    impl FakeSource {
        fn put(&self, id: &str, version: &str, bytes: Vec<u8>) {
            self.modules
                .lock()
                .unwrap()
                .insert((id.to_string(), version.to_string()), bytes);
        }
    }

    #[async_trait]
    impl ModuleSource for FakeSource {
        async fn load(
            &self,
            plugin_id: &str,
            version: &str,
            _entry: &str,
        ) -> Result<Vec<u8>, String> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            self.modules
                .lock()
                .unwrap()
                .get(&(plugin_id.to_string(), version.to_string()))
                .cloned()
                .ok_or_else(|| "not found".to_string())
        }
    }

    struct Rig {
        inv: WasmInvoker,
        host: Arc<FakeHost>,
        source: Arc<FakeSource>,
    }

    fn test_cfg() -> WasmConfig {
        WasmConfig {
            max_message_bytes: 64 * 1024,
            max_host_calls: 5,
            per_plugin_concurrency: 2,
            max_fuel: 1_000_000_000_000,
            ..Default::default()
        }
    }

    fn rig_with(cfg: WasmConfig, wat_src: &str) -> Rig {
        let source = Arc::new(FakeSource::default());
        source.put("acme", "1.0.0", wat::parse_str(wat_src).unwrap());
        let host = Arc::new(FakeHost::default());
        let inv = WasmInvoker::new(cfg, source.clone(), host.clone()).unwrap();
        Rig { inv, host, source }
    }

    fn rig() -> Rig {
        rig_with(test_cfg(), GUEST)
    }

    fn call_with(
        handler: &str,
        kind: HandlerKind,
        limits: Option<RuntimeLimits>,
        input: Value,
    ) -> HandlerCall {
        HandlerCall {
            plugin_id: "acme".into(),
            version: "1.0.0".into(),
            runtime: RuntimeSpec {
                kind: RuntimeKind::Wasm,
                entry: Some("plugin.wasm".into()),
                endpoint: None,
                limits,
            },
            handler: handler.into(),
            kind,
            input,
        }
    }

    fn call(handler: &str) -> HandlerCall {
        call_with(handler, HandlerKind::Event, None, json!({"n": 1}))
    }

    fn limits(
        memory_mb: Option<u64>,
        fuel: Option<u64>,
        timeout_ms: Option<u64>,
    ) -> Option<RuntimeLimits> {
        Some(RuntimeLimits {
            memory_mb,
            fuel_per_call: fuel,
            timeout_ms,
            invocations_per_minute: None,
        })
    }

    fn failed_msg(r: Result<Value, InvokeError>) -> String {
        match r {
            Err(InvokeError::Failed(m)) => m,
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn echo_returns_the_request_envelope() {
        let r = rig();
        let out = r
            .inv
            .invoke(call_with(
                "ok",
                HandlerKind::Webhook,
                None,
                json!({"x": [1, 2]}),
            ))
            .await
            .unwrap();
        assert_eq!(
            out,
            json!({"kind": "webhook", "handler": "ok", "input": {"x": [1, 2]}})
        );
    }

    #[tokio::test]
    async fn error_envelope_is_a_failure_with_the_message() {
        assert_eq!(failed_msg(rig().inv.invoke(call("error")).await), "boom");
    }

    #[tokio::test]
    async fn infinite_loop_is_stopped_by_fuel() {
        let t = Instant::now();
        let c = call_with(
            "loop",
            HandlerKind::Event,
            limits(None, Some(1_000_000), None),
            json!(null),
        );
        assert_eq!(failed_msg(rig().inv.invoke(c).await), "fuel exhausted");
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn long_loop_is_stopped_by_the_epoch_deadline() {
        let t = Instant::now();
        let c = call_with(
            "loop",
            HandlerKind::Event,
            limits(None, Some(900_000_000_000), Some(100)),
            json!(null),
        );
        let r = rig().inv.invoke(c).await;
        assert!(matches!(r, Err(InvokeError::Timeout(100))), "{:?}", r);
        let e = t.elapsed();
        assert!(
            e >= Duration::from_millis(80) && e < Duration::from_secs(2),
            "{:?}",
            e
        );
    }

    #[tokio::test]
    async fn memory_growth_beyond_the_limit_is_denied() {
        let c = call_with(
            "grow",
            HandlerKind::Event,
            limits(Some(1), None, None),
            json!(null),
        );
        assert_eq!(
            failed_msg(rig().inv.invoke(c).await),
            "memory limit exceeded"
        );
    }

    #[tokio::test]
    async fn initial_memory_over_the_limit_is_rejected() {
        let big = GUEST.replace(
            "(memory (export \"memory\") 2)",
            "(memory (export \"memory\") 100)",
        );
        let r = rig_with(test_cfg(), &big);
        let c = call_with(
            "ok",
            HandlerKind::Event,
            limits(Some(1), None, None),
            json!(null),
        );
        assert_eq!(failed_msg(r.inv.invoke(c).await), "memory limit exceeded");
    }

    #[tokio::test]
    async fn trap_is_contained_and_a_later_call_works() {
        let r = rig();
        let m = failed_msg(r.inv.invoke(call("trap")).await);
        assert!(m.starts_with("guest trapped"), "{}", m);
        assert!(r.inv.invoke(call("ok")).await.is_ok());
    }

    #[test]
    fn validate_module_rejects_foreign_imports_and_missing_exports() {
        assert!(WasmInvoker::validate_module(&wat::parse_str(GUEST).unwrap()).is_ok());
        let wasi = r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) (i32.const 0))
            (func (export "handle") (param i32 i32) (result i64) (i64.const 0)))"#;
        let e = WasmInvoker::validate_module(&wat::parse_str(wasi).unwrap()).unwrap_err();
        assert!(e.contains("not allowed"), "{}", e);
        let no_handle = r#"(module
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) (i32.const 0)))"#;
        let e = WasmInvoker::validate_module(&wat::parse_str(no_handle).unwrap()).unwrap_err();
        assert!(e.contains("handle"), "{}", e);
        let bad_sig = r#"(module
            (import "openvox" "host_call" (func (param i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) (i32.const 0))
            (func (export "handle") (param i32 i32) (result i64) (i64.const 0)))"#;
        assert!(WasmInvoker::validate_module(&wat::parse_str(bad_sig).unwrap()).is_err());
        assert!(WasmInvoker::validate_module(b"not wasm").is_err());
    }

    #[tokio::test]
    async fn host_call_reaches_the_host_api_and_the_guest_reads_the_response() {
        let r = rig();
        let out = r
            .inv
            .invoke(call_with("host", HandlerKind::Job, None, json!(null)))
            .await
            .unwrap();
        assert_eq!(out, json!({"ok": {"echo": {"a": 1}}}));
        let calls = r.host.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let (ctx, op, req) = &calls[0];
        assert_eq!(ctx.plugin_id, "acme");
        assert_eq!(ctx.version, "1.0.0");
        assert_eq!(ctx.kind, HandlerKind::Job);
        assert_eq!(ctx.handler, "host");
        assert_eq!(op, "test.op");
        assert_eq!(req, &json!({"a": 1}));
    }

    #[tokio::test]
    async fn host_error_code_is_visible_to_the_guest() {
        let out = rig().inv.invoke(call("fail")).await.unwrap();
        assert_eq!(out, json!({"error": {"code": "denied", "message": "nope"}}));
    }

    #[tokio::test]
    async fn too_many_host_calls_trap() {
        let r = rig();
        let m = failed_msg(r.inv.invoke(call("many")).await);
        assert_eq!(m, "host call limit exceeded");
        assert_eq!(r.host.calls.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn host_call_that_outlives_the_deadline_times_out() {
        let c = call_with(
            "slow",
            HandlerKind::Event,
            limits(None, None, Some(100)),
            json!(null),
        );
        let r = rig().inv.invoke(c).await;
        assert!(matches!(r, Err(InvokeError::Timeout(100))), "{:?}", r);
    }

    #[tokio::test]
    async fn oversized_or_malformed_guest_output_is_rejected() {
        let r = rig();
        let m = failed_msg(r.inv.invoke(call("Huge")).await);
        assert!(m.contains("exceeds the limit"), "{}", m);
        let m = failed_msg(r.inv.invoke(call("bad")).await);
        assert!(m.contains("out of bounds"), "{}", m);
        let m = failed_msg(r.inv.invoke(call("xgarbage")).await);
        assert!(m.contains("not valid JSON"), "{}", m);
        let m = failed_msg(r.inv.invoke(call("zshape")).await);
        assert!(m.contains("exactly one"), "{}", m);
    }

    #[tokio::test]
    async fn oversized_request_is_rejected() {
        let c = call_with(
            "ok",
            HandlerKind::Event,
            None,
            json!({"s": "x".repeat(70 * 1024)}),
        );
        let m = failed_msg(rig().inv.invoke(c).await);
        assert!(m.contains("request"), "{}", m);
    }

    #[tokio::test]
    async fn bulkhead_reports_unavailable_when_full() {
        let cfg = WasmConfig {
            per_plugin_concurrency: 1,
            ..test_cfg()
        };
        let r = Arc::new(rig_with(cfg, GUEST));
        let busy = {
            let r = r.clone();
            tokio::spawn(async move {
                let c = call_with(
                    "loop",
                    HandlerKind::Event,
                    limits(None, Some(900_000_000_000), Some(300)),
                    json!(null),
                );
                r.inv.invoke(c).await
            })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        let t = Instant::now();
        match r.inv.invoke(call("ok")).await {
            Err(InvokeError::Unavailable(m)) => assert!(m.contains("busy"), "{}", m),
            other => panic!("expected Unavailable, got {:?}", other),
        }
        assert!(t.elapsed() < Duration::from_millis(100));
        assert!(matches!(
            busy.await.unwrap(),
            Err(InvokeError::Timeout(300))
        ));
        assert!(r.inv.invoke(call("ok")).await.is_ok());
    }

    #[tokio::test]
    async fn spinning_guest_does_not_block_other_tasks() {
        let r = Arc::new(rig());
        let spin = {
            let r = r.clone();
            tokio::spawn(async move {
                let c = call_with(
                    "loop",
                    HandlerKind::Event,
                    limits(None, Some(900_000_000_000), Some(400)),
                    json!(null),
                );
                r.inv.invoke(c).await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        let t = Instant::now();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            t.elapsed() < Duration::from_millis(150),
            "{:?}",
            t.elapsed()
        );
        assert!(matches!(
            spin.await.unwrap(),
            Err(InvokeError::Timeout(400))
        ));
    }

    #[tokio::test]
    async fn modules_are_cached_until_forgotten() {
        let r = rig();
        r.inv.invoke(call("ok")).await.unwrap();
        r.inv.invoke(call("ok")).await.unwrap();
        assert_eq!(r.source.loads.load(Ordering::SeqCst), 1);
        r.inv.forget("acme");
        r.inv.invoke(call("ok")).await.unwrap();
        assert_eq!(r.source.loads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn missing_module_or_entry_is_unavailable() {
        let r = rig();
        let mut c = call("ok");
        c.version = "9.9.9".into();
        assert!(matches!(
            r.inv.invoke(c).await,
            Err(InvokeError::Unavailable(_))
        ));
        let mut c = call("ok");
        c.runtime.entry = None;
        assert!(matches!(
            r.inv.invoke(c).await,
            Err(InvokeError::Unavailable(_))
        ));
    }
}
