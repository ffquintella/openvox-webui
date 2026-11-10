//! User management API endpoints
//!
//! Provides user CRUD operations and role/permission management.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::plugins::allowed;
use crate::{
    db::AuditRepository,
    middleware::AuthUser,
    models::{
        Action, AssignRolesRequest, EffectivePermissions, EventActor, Resource, Role, UserPublic,
        UserRoleInfo,
    },
    services::{
        plugins::{
            events::EventBus,
            interceptors::{InterceptActor, InterceptRefusal, BEFORE_USER_UPDATE},
        },
        AuthService,
    },
    utils::error::ErrorResponse,
    AppState,
};

/// Create routes for user management
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/{id}", get(get_user).put(update_user).delete(delete_user))
        .route("/{id}/roles", get(get_user_roles).put(assign_user_roles))
        .route("/{id}/permissions", get(get_user_permissions))
}

/// Create user request
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    /// Password - required for local auth, optional for SAML-only users
    pub password: Option<String>,
    pub organization_id: Option<Uuid>,
    pub role: Option<String>,
    pub role_ids: Option<Vec<Uuid>>,
    /// Authentication provider: "local", "saml", or "both" (default: "local")
    #[serde(default = "default_auth_provider")]
    pub auth_provider: String,
    /// External ID for SAML users (e.g., email from IdP)
    pub external_id: Option<String>,
}

fn default_auth_provider() -> String {
    "local".to_string()
}

/// Update user request
#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    /// Authentication provider: "local", "saml", or "both"
    pub auth_provider: Option<String>,
    /// External ID for SAML users
    pub external_id: Option<String>,
    /// Require the user to choose a new password at next login. Setting a
    /// temporary `password` implies `true` unless this says otherwise.
    pub force_password_change: Option<bool>,
}

/// Shortest password an administrator may set (same rule as user creation).
const MIN_PASSWORD_LEN: usize = 8;

fn bad_request(code: Option<&str>, message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "validation_error".to_string(),
            message: message.to_string(),
            details: None,
            code: code.map(str::to_string),
        }),
    )
}

/// User response with roles
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct UserWithRolesResponse {
    #[serde(flatten)]
    pub user: UserPublic,
    pub roles: Vec<Role>,
}

#[derive(Debug, Deserialize, Default)]
struct OrgQuery {
    organization_id: Option<Uuid>,
}

fn forbidden(message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            error: "forbidden".to_string(),
            message: message.to_string(),
            details: None,
            code: None,
        }),
    )
}

fn resolve_org(
    auth_user: &AuthUser,
    requested: Option<Uuid>,
) -> Result<Option<Uuid>, (StatusCode, Json<ErrorResponse>)> {
    match requested {
        Some(org_id) if !auth_user.is_super_admin() && org_id != auth_user.organization_id => Err(
            forbidden("organization_id can only be specified by super_admin"),
        ),
        Some(org_id) => Ok(Some(org_id)),
        None => Ok(None),
    }
}

/// List all users
///
/// GET /api/v1/users
async fn list_users(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
) -> Result<Json<Vec<UserPublic>>, (StatusCode, Json<ErrorResponse>)> {
    let auth_service = AuthService::new(state.db.clone());

    let requested_org = resolve_org(&auth_user, query.organization_id)?;

    let users = match (auth_user.is_super_admin(), requested_org) {
        (true, Some(org_id)) => auth_service.list_users_in_org(org_id).await,
        (true, None) => auth_service.list_users().await,
        (false, _) => {
            auth_service
                .list_users_in_org(auth_user.organization_id)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch users: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    // Fetch roles for each user
    let mut users_with_roles = Vec::with_capacity(users.len());
    for user in users {
        let roles = state
            .rbac_db
            .get_user_roles(&user.id)
            .await
            .unwrap_or_default();
        let role_infos: Vec<UserRoleInfo> = roles
            .into_iter()
            .map(|r| UserRoleInfo {
                id: r.id,
                name: r.name,
                display_name: r.display_name,
            })
            .collect();
        users_with_roles.push(user.with_roles(role_infos));
    }

    Ok(Json(users_with_roles))
}

/// Create a new user
///
/// POST /api/v1/users
async fn create_user(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserPublic>), (StatusCode, Json<ErrorResponse>)> {
    use crate::models::AuthProvider;

    // Parse auth_provider
    let auth_provider: AuthProvider = payload.auth_provider.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Invalid auth_provider. Must be 'local', 'saml', or 'both'".to_string(),
                details: None,
                code: None,
            }),
        )
    })?;

    // Validate input
    if payload.username.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Username must be at least 3 characters".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Email is always required and must be valid
    if !payload.email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "Invalid email address".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Password validation depends on auth_provider
    let password = match (&auth_provider, &payload.password) {
        // Local or Both auth requires a password
        (AuthProvider::Local | AuthProvider::Both, None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: "Password is required for local authentication".to_string(),
                    details: None,
                    code: None,
                }),
            ));
        }
        (AuthProvider::Local | AuthProvider::Both, Some(p)) if p.len() < 8 => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: "Password must be at least 8 characters".to_string(),
                    details: None,
                    code: None,
                }),
            ));
        }
        (AuthProvider::Local | AuthProvider::Both, Some(p)) => Some(p.as_str()),
        // SAML-only users don't need a password
        (AuthProvider::Saml, _) => None,
    };

    // SAML users should have an external_id (use email as default)
    let external_id = if auth_provider.allows_saml() {
        payload
            .external_id
            .as_deref()
            .or(Some(payload.email.as_str()))
    } else {
        None
    };

    let auth_service = AuthService::new(state.db.clone());
    let role = payload.role.as_deref().unwrap_or("viewer");

    let org_id = match payload.organization_id {
        Some(org_id) if !auth_user.is_super_admin() && org_id != auth_user.organization_id => {
            return Err(forbidden(
                "organization_id can only be specified by super_admin",
            ));
        }
        Some(org_id) => org_id,
        None => auth_user.organization_id,
    };

    let user = auth_service
        .create_user_with_auth_provider(
            &payload.username,
            &payload.email,
            password,
            role,
            org_id,
            auth_provider,
            external_id,
        )
        .await
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("already exists") {
                (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: "conflict".to_string(),
                        message,
                        details: None,
                        code: None,
                    }),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        message: format!("Failed to create user: {}", e),
                        details: None,
                        code: None,
                    }),
                )
            }
        })?;

    // Assign roles if provided
    if let Some(role_ids) = payload.role_ids {
        if !role_ids.is_empty() {
            state
                .rbac_db
                .assign_roles(&user.id, &role_ids)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "internal_error".to_string(),
                            message: format!("Failed to assign roles: {}", e),
                            details: None,
                            code: None,
                        }),
                    )
                })?;
        }
    }

    Ok((StatusCode::CREATED, Json(user.into())))
}

/// Get a specific user
///
/// GET /api/v1/users/:id
async fn get_user(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserPublic>, (StatusCode, Json<ErrorResponse>)> {
    let auth_service = AuthService::new(state.db.clone());

    let requested_org = resolve_org(&auth_user, query.organization_id)?;

    let user = match (auth_user.is_super_admin(), requested_org) {
        (true, Some(org_id)) => auth_service.get_user_by_id_in_org(org_id, &id).await,
        (true, None) => auth_service.get_user_by_id(&id).await,
        (false, _) => {
            auth_service
                .get_user_by_id_in_org(auth_user.organization_id, &id)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user: {}", e),
                details: None,
                code: None,
            }),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "User not found".to_string(),
                details: None,
                code: None,
            }),
        )
    })?;

    // Fetch user's roles
    let roles = state
        .rbac_db
        .get_user_roles(&user.id)
        .await
        .unwrap_or_default();
    let role_infos: Vec<UserRoleInfo> = roles
        .into_iter()
        .map(|r| UserRoleInfo {
            id: r.id,
            name: r.name,
            display_name: r.display_name,
        })
        .collect();

    let user_public: UserPublic = user.into();
    Ok(Json(user_public.with_roles(role_infos)))
}

/// Update a user
///
/// PUT /api/v1/users/:id
async fn update_user(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<UserPublic>, (StatusCode, Json<ErrorResponse>)> {
    // Editing accounts is an administrator action, like the directory and
    // SCIM screens.
    match allowed(&state, &auth_user, Resource::Users, Action::Admin).await {
        Ok(true) => {}
        Ok(false) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "forbidden".to_string(),
                    message: "Requires users:admin".to_string(),
                    details: None,
                    code: None,
                }),
            ))
        }
        Err(e) => {
            tracing::error!("permission check failed: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Permission check failed".to_string(),
                    details: None,
                    code: None,
                }),
            ));
        }
    }

    let auth_service = AuthService::new(state.db.clone());

    let requested_org = resolve_org(&auth_user, query.organization_id)?;
    let existing = match (auth_user.is_super_admin(), requested_org) {
        (true, Some(org_id)) => auth_service.get_user_by_id_in_org(org_id, &id).await,
        (true, None) => auth_service.get_user_by_id(&id).await,
        (false, _) => {
            auth_service
                .get_user_by_id_in_org(auth_user.organization_id, &id)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    let Some(existing) = existing else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "User not found".to_string(),
                details: None,
                code: None,
            }),
        ));
    };

    // The directory owns the identity of its users: only roles change here.
    if existing.is_directory_managed() {
        let changes_identity = payload
            .username
            .as_deref()
            .is_some_and(|u| u != existing.username)
            || payload
                .email
                .as_deref()
                .is_some_and(|e| e != existing.email)
            || payload.password.as_deref().is_some_and(|p| !p.is_empty())
            || payload
                .auth_provider
                .as_deref()
                .is_some_and(|a| a != existing.auth_provider.to_string())
            || payload
                .external_id
                .as_deref()
                .is_some_and(|x| Some(x) != existing.external_id.as_deref())
            || payload.force_password_change == Some(true);
        if changes_identity {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message:
                        "Username, e-mail and password of this user are managed by its directory"
                            .to_string(),
                    details: None,
                    code: Some("directory_managed".to_string()),
                }),
            ));
        }
    }

    let managed = existing.is_directory_managed();
    fn identity(managed: bool, v: Option<&str>) -> Option<&str> {
        if managed {
            None
        } else {
            v
        }
    }

    let password = payload.password.as_deref().filter(|p| !p.is_empty());
    if password.is_some_and(|p| p.chars().count() < MIN_PASSWORD_LEN) {
        return Err(bad_request(
            None,
            "Password must be at least 8 characters long",
        ));
    }
    let effective_provider = payload
        .auth_provider
        .clone()
        .unwrap_or_else(|| existing.auth_provider.to_string());
    let sso_only = effective_provider == "saml";
    if sso_only && (password.is_some() || payload.force_password_change == Some(true)) {
        return Err(bad_request(
            None,
            "SSO-only users have no local password to change",
        ));
    }
    let force_password_change = payload
        .force_password_change
        .or_else(|| password.map(|_| true));

    // What changes, for the interceptors, the audit trail and the event.
    let new_username = identity(managed, payload.username.as_deref())
        .unwrap_or(&existing.username)
        .to_string();
    let new_email = identity(managed, payload.email.as_deref())
        .unwrap_or(&existing.email)
        .to_string();
    let mut changed_fields: Vec<&str> = Vec::new();
    if new_username != existing.username {
        changed_fields.push("username");
    }
    if new_email != existing.email {
        changed_fields.push("email");
    }
    if password.is_some() && !managed {
        changed_fields.push("password");
    }
    if force_password_change.is_some_and(|f| f != existing.force_password_change) {
        changed_fields.push("force_password_change");
    }
    if payload.role.as_deref().is_some_and(|r| r != existing.role) {
        changed_fields.push("role");
    }
    if payload
        .auth_provider
        .as_deref()
        .is_some_and(|a| a != existing.auth_provider.to_string())
    {
        changed_fields.push("auth_provider");
    }
    if payload
        .external_id
        .as_deref()
        .is_some_and(|x| Some(x) != existing.external_id.as_deref())
    {
        changed_fields.push("external_id");
    }

    // `before.user.update` interceptors see identity and password policy
    // changes (never a password) before anything is stored. The event fails
    // open: only a veto stops the update.
    let guarded = ["username", "email", "password", "force_password_change"];
    if changed_fields.iter().any(|f| guarded.contains(f))
        && state.plugin_manager.has_interceptors(BEFORE_USER_UPDATE)
    {
        let intercept_payload = serde_json::json!({
            "organization_id": existing.organization_id,
            "user_id": id,
            "actor": {"kind": "user", "id": auth_user.id},
            "current": {"username": existing.username, "email": existing.email},
            "proposed": {"username": new_username, "email": new_email},
            "password_changed": changed_fields.contains(&"password"),
            "force_password_change": force_password_change,
        });
        let actor = InterceptActor {
            organization_id: existing.organization_id,
            user_id: Some(auth_user.id),
        };
        if let Err(refusal) = state
            .plugin_manager
            .intercept(
                BEFORE_USER_UPDATE,
                &id.to_string(),
                actor,
                intercept_payload,
            )
            .await
        {
            let status = match refusal {
                InterceptRefusal::Vetoed { .. } => StatusCode::CONFLICT,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            };
            return Err((
                status,
                Json(ErrorResponse {
                    error: "plugin_refused".to_string(),
                    message: refusal.to_string(),
                    details: None,
                    code: Some("interceptor_refused".to_string()),
                }),
            ));
        }
    }

    let user = auth_service
        .update_user_full(
            &id,
            identity(managed, payload.username.as_deref()),
            identity(managed, payload.email.as_deref()),
            identity(managed, password),
            payload.role.as_deref(),
            identity(managed, payload.auth_provider.as_deref()),
            identity(managed, payload.external_id.as_deref()),
            force_password_change.filter(|_| !managed),
        )
        .await
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "not_found".to_string(),
                        message: "User not found".to_string(),
                        details: None,
                        code: None,
                    }),
                )
            } else if message.contains("already exists") {
                (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: "conflict".to_string(),
                        message,
                        details: None,
                        code: None,
                    }),
                )
            } else if message.contains("Invalid auth_provider") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "validation_error".to_string(),
                        message,
                        details: None,
                        code: None,
                    }),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        message: format!("Failed to update user: {}", e),
                        details: None,
                        code: None,
                    }),
                )
            }
        })?;

    if !changed_fields.is_empty() {
        // Field names only: never a password or its hash.
        let details = serde_json::json!({"changed_fields": changed_fields});
        if let Err(e) = AuditRepository::new(&state.db)
            .insert(
                user.organization_id,
                Some(auth_user.id),
                "user.updated",
                "users",
                Some(&id.to_string()),
                Some(&details),
                None,
            )
            .await
        {
            tracing::warn!("Failed to write user audit entry: {}", e);
        }
        EventBus::new(state.db.clone())
            .emit(
                "user.updated",
                serde_json::json!({
                    "user_id": user.id,
                    "organization_id": user.organization_id,
                    "username": user.username,
                    "source": "admin",
                    "changed_fields": changed_fields,
                }),
                EventActor::user(auth_user.username.clone()),
            )
            .await;
    }

    Ok(Json(user.into()))
}

/// Delete a user
///
/// DELETE /api/v1/users/:id
async fn delete_user(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let auth_service = AuthService::new(state.db.clone());

    let requested_org = resolve_org(&auth_user, query.organization_id)?;
    let existing = match (auth_user.is_super_admin(), requested_org) {
        (true, Some(org_id)) => auth_service.get_user_by_id_in_org(org_id, &id).await,
        (true, None) => auth_service.get_user_by_id(&id).await,
        (false, _) => {
            auth_service
                .get_user_by_id_in_org(auth_user.organization_id, &id)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    if existing.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "User not found".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    let deleted = auth_service.delete_user(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to delete user: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "User not found".to_string(),
                details: None,
                code: None,
            }),
        ))
    }
}

/// Get roles assigned to a user
///
/// GET /api/v1/users/:id/roles
async fn get_user_roles(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Role>>, (StatusCode, Json<ErrorResponse>)> {
    // Verify user exists
    let auth_service = AuthService::new(state.db.clone());
    let requested_org = resolve_org(&auth_user, query.organization_id)?;
    let existing = match (auth_user.is_super_admin(), requested_org) {
        (true, Some(org_id)) => auth_service.get_user_by_id_in_org(org_id, &id).await,
        (true, None) => auth_service.get_user_by_id(&id).await,
        (false, _) => {
            auth_service
                .get_user_by_id_in_org(auth_user.organization_id, &id)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    if existing.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "User not found".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    let roles = state.rbac_db.get_user_roles(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user roles: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    Ok(Json(roles))
}

/// Assign roles to a user
///
/// PUT /api/v1/users/:id/roles
async fn assign_user_roles(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<Uuid>,
    Json(payload): Json<AssignRolesRequest>,
) -> Result<Json<Vec<Role>>, (StatusCode, Json<ErrorResponse>)> {
    // Verify user exists
    let auth_service = AuthService::new(state.db.clone());
    let requested_org = resolve_org(&auth_user, query.organization_id)?;
    let existing = match (auth_user.is_super_admin(), requested_org) {
        (true, Some(org_id)) => auth_service.get_user_by_id_in_org(org_id, &id).await,
        (true, None) => auth_service.get_user_by_id(&id).await,
        (false, _) => {
            auth_service
                .get_user_by_id_in_org(auth_user.organization_id, &id)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    if existing.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "User not found".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    state
        .rbac_db
        .assign_roles(&id, &payload.role_ids)
        .await
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("not found") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "bad_request".to_string(),
                        message: format!("Role not found: {}", e),
                        details: None,
                        code: None,
                    }),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        message: format!("Failed to assign roles: {}", e),
                        details: None,
                        code: None,
                    }),
                )
            }
        })?;

    // Fetch and return updated roles
    let roles = state.rbac_db.get_user_roles(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user roles: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    Ok(Json(roles))
}

/// Get effective permissions for a user
///
/// GET /api/v1/users/:id/permissions
async fn get_user_permissions(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<Uuid>,
) -> Result<Json<EffectivePermissions>, (StatusCode, Json<ErrorResponse>)> {
    // Verify user exists
    let auth_service = AuthService::new(state.db.clone());
    let requested_org = resolve_org(&auth_user, query.organization_id)?;
    let existing = match (auth_user.is_super_admin(), requested_org) {
        (true, Some(org_id)) => auth_service.get_user_by_id_in_org(org_id, &id).await,
        (true, None) => auth_service.get_user_by_id(&id).await,
        (false, _) => {
            auth_service
                .get_user_by_id_in_org(auth_user.organization_id, &id)
                .await
        }
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to fetch user: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    if existing.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: "User not found".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    let permissions = state
        .rbac_db
        .get_effective_permissions(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch permissions: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?;

    Ok(Json(permissions))
}
