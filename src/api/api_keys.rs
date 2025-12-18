//! API key management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use base64::Engine;
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    db::{ApiKeyRepository, AuditRepository},
    middleware::AuthUser,
    models::{CreateApiKeyRequest, CreateApiKeyResponse},
    services::AuthService,
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_api_keys).post(create_api_key))
        .route("/{id}", delete(delete_api_key))
}

#[derive(Debug, Deserialize, Default)]
struct ApiKeyListQuery {
    organization_id: Option<Uuid>,
    user_id: Option<Uuid>,
}

fn resolve_org(auth_user: &AuthUser, requested: Option<Uuid>) -> Result<Uuid, AppError> {
    match requested {
        Some(org_id) if !auth_user.is_super_admin() => Err(AppError::forbidden(
            "organization_id can only be specified by super_admin",
        )),
        Some(org_id) => Ok(org_id),
        None => Ok(auth_user.organization_id),
    }
}

fn is_admin(auth_user: &AuthUser) -> bool {
    auth_user.roles.iter().any(|r| r == "admin") || auth_user.is_super_admin()
}

async fn list_api_keys(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ApiKeyListQuery>,
) -> Result<Json<Vec<crate::models::ApiKey>>, AppError> {
    let org_id = resolve_org(&auth_user, query.organization_id)?;
    let user_id = match query.user_id {
        Some(u) if !auth_user.is_super_admin() => {
            return Err(AppError::forbidden(
                "user_id can only be specified by super_admin",
            ));
        }
        Some(u) => u,
        None => auth_user.user_id(),
    };

    let repo = ApiKeyRepository::new(&state.db);
    let keys = repo.list_for_user(org_id, user_id).await.map_err(|e| {
        tracing::error!("Failed to list api keys: {}", e);
        AppError::internal("Failed to list api keys")
    })?;

    Ok(Json(keys))
}

async fn create_api_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), AppError> {
    let auth_service = AuthService::new(state.db.clone());

    let (org_id, user_id) = match (payload.organization_id, payload.user_id) {
        (Some(org_id), Some(user_id)) => {
            if !auth_user.is_super_admin() {
                return Err(AppError::forbidden(
                    "organization_id/user_id override requires super_admin",
                ));
            }
            (org_id, user_id)
        }
        (None, Some(user_id)) => {
            if !auth_user.is_super_admin() {
                return Err(AppError::forbidden("user_id override requires super_admin"));
            }
            // Infer org from target user
            let user = auth_service
                .get_user_by_id(&user_id)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to fetch user for api key: {}", e);
                    AppError::internal("Failed to fetch user")
                })?
                .ok_or_else(|| AppError::not_found("User not found"))?;
            (user.organization_id, user_id)
        }
        (Some(org_id), None) => {
            if !auth_user.is_super_admin() {
                return Err(AppError::forbidden(
                    "organization_id override requires super_admin",
                ));
            }
            (org_id, auth_user.user_id())
        }
        (None, None) => (auth_user.organization_id, auth_user.user_id()),
    };

    // Role scoping: default to caller roles; if specified, must be a subset unless super_admin.
    let requested_roles = payload.role_ids.clone();
    let role_ids = match requested_roles {
        Some(role_ids) if role_ids.is_empty() => {
            return Err(AppError::bad_request("role_ids cannot be empty"));
        }
        Some(role_ids) if auth_user.is_super_admin() => role_ids,
        Some(role_ids) => {
            let allowed: std::collections::HashSet<Uuid> =
                auth_user.role_ids.iter().copied().collect();
            if role_ids.iter().all(|r| allowed.contains(r)) {
                role_ids
            } else {
                return Err(AppError::forbidden(
                    "role_ids must be a subset of your assigned roles",
                ));
            }
        }
        None => {
            if auth_user.role_ids.is_empty() {
                return Err(AppError::bad_request(
                    "No roles found for caller; provide role_ids explicitly",
                ));
            }
            auth_user.role_ids.clone()
        }
    };

    // Create API key: use an id in the plaintext key so auth can look it up efficiently.
    let api_key_id = Uuid::new_v4();
    let mut secret_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut secret_bytes);
    let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret_bytes);
    let key = format!("ovk_{}_{}", api_key_id, secret);

    let key_hash = AuthService::hash_password(&secret).map_err(|e| {
        tracing::error!("Failed to hash api key: {}", e);
        AppError::internal("Failed to create api key")
    })?;

    let repo = ApiKeyRepository::new(&state.db);
    let api_key = repo
        .create_hashed_key(api_key_id, org_id, user_id, &payload, &key_hash, &role_ids)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create api key: {}", e);
            AppError::internal("Failed to create api key")
        })?;

    // Audit
    let audit_repo = AuditRepository::new(&state.db);
    let _ = audit_repo
        .insert(
            org_id,
            Some(auth_user.user_id()),
            "api_key.create",
            "api_keys",
            Some(&api_key.id.to_string()),
            Some(&serde_json::json!({
                "name": api_key.name,
                "user_id": user_id,
                "role_ids": role_ids,
                "expires_at": payload.expires_at,
            })),
            None,
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse { api_key, key }),
    ))
}

async fn delete_api_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let api_key_id =
        Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid API key ID"))?;

    // Look up key ownership/org.
    let row = sqlx::query("SELECT organization_id, user_id FROM api_keys WHERE id = ?")
        .bind(api_key_id.to_string())
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch api key: {}", e);
            AppError::internal("Failed to delete api key")
        })?
        .ok_or_else(|| AppError::not_found("API key not found"))?;

    let org_id_str: String = row.try_get("organization_id").unwrap_or_default();
    let user_id_str: String = row.try_get("user_id").unwrap_or_default();
    let org_id = Uuid::parse_str(&org_id_str).map_err(|_| AppError::internal("Corrupt api key"))?;
    let owner_id =
        Uuid::parse_str(&user_id_str).map_err(|_| AppError::internal("Corrupt api key"))?;

    // Tenant isolation
    if !auth_user.is_super_admin() && org_id != auth_user.organization_id {
        return Err(AppError::not_found("API key not found"));
    }

    // Ownership/admin checks (admins can revoke within their org).
    if owner_id != auth_user.user_id() && !is_admin(&auth_user) {
        return Err(AppError::forbidden("Not allowed to revoke this API key"));
    }

    let repo = ApiKeyRepository::new(&state.db);
    let deleted = repo.delete(org_id, api_key_id).await.map_err(|e| {
        tracing::error!("Failed to delete api key: {}", e);
        AppError::internal("Failed to delete api key")
    })?;

    if !deleted {
        return Err(AppError::not_found("API key not found"));
    }

    let audit_repo = AuditRepository::new(&state.db);
    let _ = audit_repo
        .insert(
            org_id,
            Some(auth_user.user_id()),
            "api_key.delete",
            "api_keys",
            Some(&api_key_id.to_string()),
            Some(&serde_json::json!({ "owner_id": owner_id })),
            None,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}
