//! M18 proofs of concept (S21): per-plugin SQLite storage limits (T65) and
//! wasmtime resource limits inside Axum (T66).
pub mod plugin_db;
#[cfg(feature = "wasm")]
pub mod wasm;
