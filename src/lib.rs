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

pub use config::AppConfig;
pub use db::DbPool;
pub use middleware::{
    auth_middleware, check_permission, optional_auth_middleware, require_permission_middleware,
    AuthUser, Claims, RbacError, RequirePermission,
};
use config::BackupConfig;
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
    /// Database connection pool
    pub db: DbPool,
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
}
