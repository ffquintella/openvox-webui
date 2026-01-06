//! RBAC (Role-Based Access Control) Middleware
//!
//! This module provides middleware for enforcing permission checks on API routes.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use uuid::Uuid;

use crate::{
    models::{Action, Resource, SystemRole},
    services::RbacService,
    utils::error::ErrorResponse,
    AppState,
};

use super::auth::AuthUser;

/// Permission requirement for a route
#[derive(Debug, Clone)]
pub struct RequirePermission {
    pub resource: Resource,
    pub action: Action,
}

impl RequirePermission {
    pub fn new(resource: Resource, action: Action) -> Self {
        Self { resource, action }
    }

    pub fn read(resource: Resource) -> Self {
        Self::new(resource, Action::Read)
    }

    pub fn create(resource: Resource) -> Self {
        Self::new(resource, Action::Create)
    }

    pub fn update(resource: Resource) -> Self {
        Self::new(resource, Action::Update)
    }

    pub fn delete(resource: Resource) -> Self {
        Self::new(resource, Action::Delete)
    }

    pub fn admin(resource: Resource) -> Self {
        Self::new(resource, Action::Admin)
    }
}

/// RBAC error types
#[derive(Debug)]
pub enum RbacError {
    /// User is not authenticated
    NotAuthenticated,
    /// User lacks required permission
    PermissionDenied {
        resource: Resource,
        action: Action,
        reason: String,
    },
    /// Role not found
    RoleNotFound(String),
}

impl std::fmt::Display for RbacError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RbacError::NotAuthenticated => write!(f, "Authentication required"),
            RbacError::PermissionDenied {
                resource,
                action,
                reason,
            } => write!(
                f,
                "Permission denied: {} {} - {}",
                action.as_str(),
                resource.as_str(),
                reason
            ),
            RbacError::RoleNotFound(name) => write!(f, "Role not found: {}", name),
        }
    }
}

impl IntoResponse for RbacError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            RbacError::NotAuthenticated => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Authentication required".to_string(),
            ),
            RbacError::PermissionDenied {
                resource,
                action,
                reason,
            } => (
                StatusCode::FORBIDDEN,
                "forbidden",
                format!(
                    "Permission denied: {} {} on {}",
                    action.as_str(),
                    resource.as_str(),
                    reason
                ),
            ),
            RbacError::RoleNotFound(name) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                format!("Role not found: {}", name),
            ),
        };

        let body = ErrorResponse {
            error: error_type.to_string(),
            message,
            details: None,
            code: None,
        };

        (status, Json(body)).into_response()
    }
}

/// Check if the authenticated user has the required permission
pub fn check_permission(
    rbac_service: &RbacService,
    auth_user: &AuthUser,
    resource: Resource,
    action: Action,
    resource_id: Option<Uuid>,
    environment: Option<&str>,
) -> Result<(), RbacError> {
    // Convert role names to UUIDs
    let role_ids: Vec<Uuid> = auth_user.role_ids.to_vec();

    // SuperAdmin always has all permissions
    if role_ids.contains(&SystemRole::SuperAdmin.uuid()) {
        return Ok(());
    }

    // Check permission
    let check =
        rbac_service.check_permission(&role_ids, resource, action, resource_id, environment);

    if check.allowed {
        Ok(())
    } else {
        Err(RbacError::PermissionDenied {
            resource,
            action,
            reason: check
                .reason
                .unwrap_or_else(|| "No matching permission".to_string()),
        })
    }
}

/// Middleware factory for requiring a specific permission
///
/// Usage:
/// ```
/// use std::sync::Arc;
/// use axum::{Router, routing::get};
/// use openvox_webui::{AppState, require_permission_middleware};
/// use openvox_webui::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig, CacheConfig, LoggingConfig, DashboardConfig, RbacConfig};
/// use openvox_webui::models::Resource;
/// use openvox_webui::middleware::rbac::RequirePermission;
/// use openvox_webui::{DbRbacService, RbacService};
///
/// # tokio_test::block_on(async {
/// async fn list_nodes() -> &'static str { "ok" }
///
/// // Create minimal in-memory database config for the example
/// let config = AppConfig {
///     server: ServerConfig { host: "127.0.0.1".into(), port: 3000, workers: 1, request_timeout_secs: None, tls: None, static_dir: None, serve_frontend: false },
///     database: DatabaseConfig {
///         url: "sqlite::memory:".into(),
///         max_connections: 1, min_connections: 1,
///         connect_timeout_secs: 30, idle_timeout_secs: 600,
///     },
///     auth: AuthConfig {
///         jwt_secret: "test_secret_at_least_32_chars_long".into(),
///         token_expiry_hours: 24, refresh_token_expiry_days: 7,
///         bcrypt_cost: 4, password_min_length: 8,
///     },
///     puppetdb: None,
///     puppet_ca: None,
///     logging: LoggingConfig::default(),
///     cache: CacheConfig::default(),
///     dashboard: DashboardConfig::default(),
///     rbac: RbacConfig::default(),
///     groups_config_path: None,
///     code_deploy: None,
/// };
///
/// let db = openvox_webui::db::init_pool(&config.database).await.unwrap();
/// let state = AppState {
///     config,
///     db,
///     puppetdb: None,
///     puppet_ca: None,
///     rbac: Arc::new(RbacService::new()),
///     rbac_db: Arc::new(DbRbacService::new(openvox_webui::db::init_pool(
///         &openvox_webui::config::DatabaseConfig {
///             url: "sqlite::memory:".into(),
///             max_connections: 1, min_connections: 1,
///             connect_timeout_secs: 30, idle_timeout_secs: 600,
///         }).await.unwrap())),
///     code_deploy_config: None,
/// };
///
/// let app = Router::<AppState>::new()
///     .route("/nodes", get(list_nodes))
///     .layer(axum::middleware::from_fn_with_state(
///         state.clone(),
///         |state, req, next| require_permission_middleware(
///             state,
///             req,
///             next,
///             RequirePermission::read(Resource::Nodes),
///         ),
///     ));
/// # })
/// ```
pub async fn require_permission_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
    permission: RequirePermission,
) -> Result<Response, RbacError> {
    // Get the authenticated user from request extensions
    let auth_user = request
        .extensions()
        .get::<AuthUser>()
        .ok_or(RbacError::NotAuthenticated)?;

    // Check permission using the RBAC service
    check_permission(
        &state.rbac,
        auth_user,
        permission.resource,
        permission.action,
        None, // No specific resource ID
        None, // No specific environment
    )?;

    // Permission granted, continue
    Ok(next.run(request).await)
}

/// Create a middleware layer that requires a specific permission
#[macro_export]
macro_rules! require_permission {
    ($state:expr, $resource:expr, $action:expr) => {
        axum::middleware::from_fn_with_state($state.clone(), move |state, req, next| {
            $crate::middleware::rbac::require_permission_middleware(
                state,
                req,
                next,
                $crate::middleware::rbac::RequirePermission::new($resource, $action),
            )
        })
    };
}

/// Extractor for checking permissions in handlers
///
/// This extractor checks if the authenticated user has a specific permission
/// and returns the AuthUser if successful.
pub struct RequiredPermission<const R: u8, const A: u8>;

/// Helper trait for converting resource/action constants to types
pub trait PermissionSpec {
    fn resource() -> Resource;
    fn action() -> Action;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SystemRole;

    #[test]
    fn test_require_permission_creation() {
        let perm = RequirePermission::read(Resource::Nodes);
        assert_eq!(perm.resource, Resource::Nodes);
        assert_eq!(perm.action, Action::Read);
    }

    #[test]
    fn test_rbac_error_response() {
        let error = RbacError::PermissionDenied {
            resource: Resource::Nodes,
            action: Action::Create,
            reason: "No permission".to_string(),
        };

        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_check_permission_admin() {
        let rbac_service = RbacService::new();
        let auth_user = AuthUser {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            username: "admin".to_string(),
            email: "admin@example.com".to_string(),
            roles: vec!["admin".to_string()],
            role_ids: vec![SystemRole::Admin.uuid()],
        };

        let result = check_permission(
            &rbac_service,
            &auth_user,
            Resource::Nodes,
            Action::Admin,
            None,
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_check_permission_viewer_denied_create() {
        let rbac_service = RbacService::new();
        let auth_user = AuthUser {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            username: "viewer".to_string(),
            email: "viewer@example.com".to_string(),
            roles: vec!["viewer".to_string()],
            role_ids: vec![SystemRole::Viewer.uuid()],
        };

        let result = check_permission(
            &rbac_service,
            &auth_user,
            Resource::Groups,
            Action::Create,
            None,
            None,
        );

        assert!(result.is_err());
        assert!(matches!(result, Err(RbacError::PermissionDenied { .. })));
    }

    #[test]
    fn test_check_permission_operator() {
        let rbac_service = RbacService::new();
        let auth_user = AuthUser {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            username: "operator".to_string(),
            email: "operator@example.com".to_string(),
            roles: vec!["operator".to_string()],
            role_ids: vec![SystemRole::Operator.uuid()],
        };

        // Operator can create groups
        let result = check_permission(
            &rbac_service,
            &auth_user,
            Resource::Groups,
            Action::Create,
            None,
            None,
        );
        assert!(result.is_ok());

        // Operator cannot delete groups
        let result = check_permission(
            &rbac_service,
            &auth_user,
            Resource::Groups,
            Action::Delete,
            None,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_check_permission_super_admin_has_all() {
        let rbac_service = RbacService::new();
        let auth_user = AuthUser {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            username: "superadmin".to_string(),
            email: "superadmin@example.com".to_string(),
            roles: vec!["super_admin".to_string()],
            role_ids: vec![SystemRole::SuperAdmin.uuid()],
        };

        // SuperAdmin should have all permissions on all resources
        for resource in Resource::all() {
            for action in Action::all() {
                let result = check_permission(
                    &rbac_service,
                    &auth_user,
                    resource,
                    action,
                    None,
                    None,
                );
                assert!(
                    result.is_ok(),
                    "SuperAdmin should have {:?} permission for {:?}",
                    action,
                    resource
                );
            }
        }
    }

    #[test]
    fn test_check_permission_super_admin_bypasses_rbac() {
        // Even with an empty RBAC service, SuperAdmin should have access
        let rbac_service = RbacService::with_roles(vec![]);
        let auth_user = AuthUser {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            username: "superadmin".to_string(),
            email: "superadmin@example.com".to_string(),
            roles: vec!["super_admin".to_string()],
            role_ids: vec![SystemRole::SuperAdmin.uuid()],
        };

        // SuperAdmin should bypass the permission check entirely
        let result = check_permission(
            &rbac_service,
            &auth_user,
            Resource::Users,
            Action::Delete,
            None,
            None,
        );
        assert!(result.is_ok());
    }
}
