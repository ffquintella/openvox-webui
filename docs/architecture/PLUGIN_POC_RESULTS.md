# Plugin Platform: Proof-of-Concept Results (M18)

Records the outcome of the three proofs of concept required by phase 0 of [PLUGIN_PLATFORM.md](PLUGIN_PLATFORM.md) §17 (tasks T65, T66, T67, spec S21). The code lives in `poc/plugin-poc/`, a standalone crate (its own `[workspace]`), so it never adds wasmtime to the shipped binary.

```bash
cd poc/plugin-poc
cargo test --test plugin_db                  # T65
cargo test --features wasm --test wasm       # T66
```

## T65: per-plugin SQLite with enforced limits (sqlx 0.9)

**Decision: accepted.** `plugin_db.rs` opens one database file per plugin and applies:

| Limit | Mechanism | Proven by |
|-------|-----------|-----------|
| Size cap | `PRAGMA max_page_count` set at connect time | inserts fail once the cap is reached |
| Per-query time | `LockedSqliteHandle::set_progress_handler` (built into sqlx 0.9) aborting the statement at a deadline | an endless recursive CTE is aborted well under 5 s |
| No `ATTACH` / `DETACH` | `sqlite3_set_authorizer` on the raw handle (`lock_handle().as_raw_handle()`) | `ATTACH` is denied |
| No `load_extension` | same authorizer, `SQLITE_FUNCTION` hook | denied |
| No limit tampering | authorizer allows only a read-only `PRAGMA` allowlist (`table_info`, `index_list`, ...) | `PRAGMA max_page_count`, `trusted_schema=ON` denied |
| Cross-plugin isolation | one file per plugin | plugin B cannot see plugin A's table |

Findings for the implementation (M20, T75):
- `libsqlite3-sys` must be a direct dependency with a version inside sqlx-sqlite's range (`>=0.30.1, <0.38`); the main project already locks 0.30.1.
- sqlx 0.9 requires `AssertSqlSafe` for dynamic SQL. That is intentional here: plugin SQL is dynamic by design and the authorizer is the safety layer.
- The authorizer and progress handler are connection-scoped, so the host installs them around every plugin call and removes them afterwards (the PoC does this in `run_sandboxed`); a dedicated single-connection pool per plugin keeps this simple.
- Set `max_page_count` in the connect options so every pooled connection gets it; the page size is 4096 bytes.

## T66: wasmtime fuel and memory limits inside Axum

**Decision: accepted, wasmtime directly (not Extism).** `wasm.rs` runs a guest module from an Axum handler with a new `Store` per call:

| Limit | Mechanism | Proven by |
|-------|-----------|-----------|
| CPU | `consume_fuel` + `Store::set_fuel` | an infinite loop is aborted (HTTP 429), well under 5 s |
| Memory | `ResourceLimiter::memory_growing` | `memory.grow` beyond the cap returns -1 |
| Core isolation | guest runs in `spawn_blocking` | `/health` answers within 200 ms while a guest spins |

Findings for the implementation (M20, T73 and T76):
- wasmtime 50 uses its own `wasmtime::Error`, which does not convert to `anyhow::Error` with `?`; use `wasmtime::Result` at the boundary.
- Fuel bounds CPU but not time spent blocked in host calls, so a wall-clock deadline is still needed (epoch interruption) once host functions exist. Not covered by this PoC.
- Component Model + WIT, the pooling allocator and concurrency bulkheads are left to T73/T76; the PoC uses a core module to isolate the limit mechanics.

## T67: build and binary size impact

Measured on macOS arm64, release profile of the main project (`lto = true`, `codegen-units = 1`, `panic = "abort"`), with a probe binary that links the same Axum + sqlx baseline, with and without wasmtime (`runtime`, `cranelift`, `std` features only):

| | Without wasmtime | With wasmtime | Delta |
|---|---|---|---|
| Stripped binary | 2.61 MB | 9.04 MB | +6.4 MB |
| gzip -9 | 1.32 MB | 4.01 MB | +2.7 MB |
| xz -9 | 0.98 MB | 2.70 MB | +1.7 MB |
| Clean release build | 31 s | 134 s | +103 s |

Context: the current RPM (0.40.7) is 14.6 MB, so the compressed cost is roughly +1.7 to +2.7 MB (about 12 to 18%). No RPM/DEB build constraint prevents this; the extra build time is paid once per clean build because dependencies are cached.

**Decision: accepted.** This closes open question 1 in PLUGIN_PLATFORM.md §19. Revisit only if package size becomes a constraint; a smaller footprint would be possible by shipping the runtime as a separate optional package, at the cost of a second artifact.
