//! OpenVox WebUI Library
//!
//! This crate provides the core functionality for the OpenVox WebUI application.

use std::sync::Arc;

pub mod api;
pub mod config;
pub mod db;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;
pub mod utils;

use std::sync::atomic::{AtomicBool, Ordering};

pub use config::AppConfig;
use config::{BackupConfig, InventoryConfig};
pub use db::DbPool;
use db::InventoryRepository;
pub use middleware::{
    auth_middleware, check_permission, optional_auth_middleware, require_permission_middleware,
    AuthUser, Claims, RbacError, RequirePermission,
};
use services::backup::BackupService;
use services::code_deploy::{CodeDeployConfig, CodeDeployService};
use services::notification::NotificationService;
use services::puppet_ca::PuppetCAService;
use services::puppetdb::PuppetDbClient;
pub use services::{DbRbacService, RbacService};
use utils::AppError;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Application configuration
    pub config: AppConfig,
    /// Database connection pool (main application DB)
    pub db: DbPool,
    /// Database connection pool for the dedicated inventory database.
    /// All Phase-10 inventory reads/writes go through this pool so that the
    /// main DB's UI readers are not blocked by inventory ingestion writers.
    pub inventory_db: DbPool,
    /// Inventory configuration. Used to build `InventoryRepository` with the
    /// correct `keep_raw_payload` setting on each request path.
    pub inventory_config: InventoryConfig,
    /// Becomes `true` once the one-shot inventory-migration has completed.
    /// Inventory endpoints gate on this so we never serve stale/partial state
    /// during the initial data-migration window on upgrade.
    pub inventory_ready: Arc<AtomicBool>,
    /// PuppetDB client (optional)
    pub puppetdb: Option<Arc<PuppetDbClient>>,
    /// Puppet CA client (optional)
    pub puppet_ca: Option<Arc<PuppetCAService>>,
    /// RBAC service for permission checking (in-memory, for middleware)
    pub rbac: Arc<RbacService>,
    /// Database-backed RBAC service (for API operations)
    pub rbac_db: Arc<DbRbacService>,
    /// Code Deploy service configuration (optional)
    pub code_deploy_config: Option<CodeDeployConfig>,
    /// Backup service configuration (optional)
    pub backup_config: Option<BackupConfig>,
    /// Notification service
    pub notification_service: Arc<NotificationService>,
}

impl AppState {
    /// Get a Code Deploy service instance
    ///
    /// Returns an error if code deploy is not enabled in configuration.
    pub fn code_deploy_service(&self) -> Result<CodeDeployService, AppError> {
        let config = self
            .code_deploy_config
            .clone()
            .ok_or_else(|| AppError::service_unavailable("Code deploy feature is not enabled"))?;

        if !config.enabled {
            return Err(AppError::service_unavailable(
                "Code deploy feature is not enabled",
            ));
        }

        Ok(CodeDeployService::new(self.db.clone(), config))
    }

    /// Get a Backup service instance
    ///
    /// Returns an error if backup is not enabled in configuration.
    pub fn backup_service(&self) -> Result<BackupService, AppError> {
        let config = self
            .backup_config
            .clone()
            .ok_or_else(|| AppError::service_unavailable("Backup feature is not enabled"))?;

        if !config.enabled {
            return Err(AppError::service_unavailable(
                "Backup feature is not enabled",
            ));
        }

        Ok(BackupService::new(self.db.clone(), config))
    }

    /// Construct an `InventoryRepository` bound to the dedicated inventory
    /// pool and configured with the current `keep_raw_payload` flag. Every
    /// inventory code path should call this helper rather than
    /// `InventoryRepository::new(state.db.clone())` — which would target the
    /// main DB and recreate the pre-release write-contention problem.
    pub fn inventory_repository(&self) -> InventoryRepository {
        InventoryRepository::new(self.inventory_db.clone())
            .with_keep_raw_payload(self.inventory_config.keep_raw_payload)
    }

    /// Whether the startup inventory-migration has completed. Handlers that
    /// read or write inventory should return 503 when this is false.
    pub fn is_inventory_ready(&self) -> bool {
        self.inventory_ready.load(Ordering::Acquire)
    }
}
