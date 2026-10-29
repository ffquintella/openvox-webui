//! Builds the plugin manager with the WASM runtime and plugin storage, for
//! the server and for the scheduled report runner (which needs it to run
//! plugin report types, M25).

use std::sync::Arc;

use sqlx::SqlitePool;
use tracing::warn;

use super::host::{HostApiImpl, PackageModuleSource};
use super::storage::{PluginStorage, StorageLimits};
use super::wasm::{WasmConfig, WasmInvoker};
use super::PluginManager;
use crate::config::AppConfig;
use crate::services::puppetdb::PuppetDbClient;

/// Creates the manager from `plugins.*`, attaches the WASM runtime and
/// re-verifies the enabled plugins. Must run inside the tokio runtime.
pub async fn build_plugin_manager(
    config: &AppConfig,
    db: SqlitePool,
    puppetdb: Option<Arc<PuppetDbClient>>,
) -> Arc<PluginManager> {
    let plugin_manager = Arc::new(
        PluginManager::new(
            db.clone(),
            config.plugins.data_dir.clone(),
            config.plugins.max_package_mb,
        )
        .with_policy(config.plugins.policy.clone())
        .with_governance(config.plugins.governance.clone())
        .with_facts(config.plugins.facts.clone())
        .with_reports(config.plugins.reports.clone())
        .with_blocked_hashes(&config.plugins.blocked_hashes),
    );
    // WASM runtime: sandboxed handlers with kv and private-database storage
    let w = &config.plugins.wasm;
    let storage = Arc::new(PluginStorage::new(
        db.clone(),
        config.plugins.data_dir.clone(),
        StorageLimits {
            kv_max_keys: w.kv_max_keys,
            kv_max_bytes: w.kv_max_mb * 1024 * 1024,
            db_max_bytes: w.db_max_mb * 1024 * 1024,
            ..Default::default()
        },
    ));
    let host = Arc::new(HostApiImpl::new(
        &plugin_manager,
        config.plugins.policy.clone(),
        storage.clone(),
        puppetdb,
    ));
    let source = Arc::new(PackageModuleSource {
        data_dir: config.plugins.data_dir.clone(),
        max_package_bytes: config.plugins.max_package_mb.max(1) * 1024 * 1024,
    });
    let wasm_cfg = WasmConfig {
        default_memory_mb: w.default_memory_mb,
        max_memory_mb: w.max_memory_mb,
        default_fuel: w.default_fuel,
        max_fuel: w.max_fuel,
        per_plugin_concurrency: config.plugins.events.per_plugin_concurrency,
        ..Default::default()
    };
    match WasmInvoker::new(wasm_cfg, source, host) {
        Ok(inv) => plugin_manager.attach_wasm(storage, Arc::new(inv)),
        Err(e) => warn!("WASM plugin runtime unavailable: {}", e),
    }
    match plugin_manager.reload().await {
        Ok(failed) if failed.is_empty() => {}
        Ok(failed) => warn!(
            "{} plugin(s) failed verification at startup: {:?}",
            failed.len(),
            failed
        ),
        Err(e) => warn!("Plugin manager startup failed: {}", e),
    }
    plugin_manager
}
