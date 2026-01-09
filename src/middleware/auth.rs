//! JWT Authentication Middleware
//!
//! This module provides JWT-based authentication for the API.

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData, Validation};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    models::default_organization_uuid, services::AuthService, utils::error::ErrorResponse, AppState,
};

/// JWT Claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Username
    pub username: String,
    /// User email
    pub email: String,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
    /// Not before timestamp
    pub nbf: i64,
    /// JWT ID (unique identifier for this token)
    pub jti: String,
    /// Token type (access or refresh)
    #[serde(default = "default_token_type")]
    pub token_type: TokenType,
    /// User roles
    #[serde(default)]
    pub roles: Vec<String>,
    /// Organization/tenant ID
    #[serde(default)]
    pub organization_id: Option<String>,
}

fn default_token_type() -> TokenType {
    TokenType::Access
}

/// Token type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    #[default]
    Access,
    Refresh,
}

/// Authenticated user information extracted from JWT
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub username: String,
    pub email: String,
    /// Role names (from JWT)
    pub roles: Vec<String>,
    /// Role UUIDs (resolved from database)
    pub role_ids: Vec<Uuid>,
}

impl TryFrom<Claims> for AuthUser {
    type Error = &'static str;

    fn try_from(claims: Claims) -> Result<Self, Self::Error> {
        let id = Uuid::parse_str(&claims.sub).map_err(|_| "Invalid user ID in token")?;
        let organization_id = match claims.organization_id {
            Some(org) => Uuid::parse_str(&org).map_err(|_| "Invalid organization ID in token")?,
            None => default_organization_uuid(),
        };
        Ok(Self {
            id,
            organization_id,
            username: claims.username,
            email: claims.email,
            roles: claims.roles,
            role_ids: vec![], // Will be populated by middleware
        })
    }
}

impl AuthUser {
    /// Create AuthUser with role IDs resolved from role names
    pub fn with_role_ids(mut self, role_ids: Vec<Uuid>) -> Self {
        self.role_ids = role_ids;
        self
    }

    /// Get the user ID
    pub fn user_id(&self) -> Uuid {
        self.id
    }

    pub fn is_super_admin(&self) -> bool {
        // Role IDs may be empty in some contexts; check both sources.
        self.roles.iter().any(|r| r == "super_admin")
            || self.roles.iter().any(|r| r == "superadmin")
    }
}

/// Extractor for AuthUser from request extensions
///
/// This allows using AuthUser as a handler parameter after auth middleware has run.
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<AuthUser>().cloned().ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "unauthorized".to_string(),
                    message: "Authentication required".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })
    }
}

/// Create a new JWT access token
pub fn create_access_token(
    user_id: &Uuid,
    organization_id: &Uuid,
    username: &str,
    email: &str,
    roles: Vec<String>,
    secret: &str,
    expiry_hours: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::hours(expiry_hours as i64);

    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        email: email.to_string(),
        iat: now.timestamp(),
        exp: exp.timestamp(),
        nbf: now.timestamp(),
        jti: Uuid::new_v4().to_string(),
        token_type: TokenType::Access,
        roles,
        organization_id: Some(organization_id.to_string()),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Create a new JWT refresh token
pub fn create_refresh_token(
    user_id: &Uuid,
    username: &str,
    email: &str,
    secret: &str,
    expiry_days: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::days(expiry_days as i64);

    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        email: email.to_string(),
        iat: now.timestamp(),
        exp: exp.timestamp(),
        nbf: now.timestamp(),
        jti: Uuid::new_v4().to_string(),
        token_type: TokenType::Refresh,
        roles: vec![],
        organization_id: None,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Validate and decode a JWT token
pub fn validate_token(token: &str, secret: &str) -> Result<TokenData<Claims>, AuthError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    validation.validate_nbf = true;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
        jsonwebtoken::errors::ErrorKind::InvalidToken => AuthError::InvalidToken,
        _ => AuthError::InvalidToken,
    })
}

/// Authentication error types
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    TokenExpired,
    InvalidTokenType,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authentication token"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid authentication token"),
            AuthError::TokenExpired => {
                (StatusCode::UNAUTHORIZED, "Authentication token has expired")
            }
            AuthError::InvalidTokenType => (StatusCode::UNAUTHORIZED, "Invalid token type"),
        };

        let body = ErrorResponse {
            error: "unauthorized".to_string(),
            message: message.to_string(),
            details: None,
            code: None,
        };

        (status, Json(body)).into_response()
    }
}

/// Extract bearer token from Authorization header
fn extract_bearer_token(auth_header: &str) -> Option<&str> {
    auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
}

fn extract_api_key_token(auth_header: &str) -> Option<&str> {
    auth_header
        .strip_prefix("ApiKey ")
        .or_else(|| auth_header.strip_prefix("apikey "))
        .or_else(|| auth_header.strip_prefix("APIKEY "))
}

fn parse_ovk(token: &str) -> Option<(Uuid, &str)> {
    // Format: ovk_<uuid>_<secret>
    let token = token.strip_prefix("ovk_")?;
    let (id_str, secret) = token.split_once('_')?;
    let id = Uuid::parse_str(id_str).ok()?;
    if secret.is_empty() {
        return None;
    }
    Some((id, secret))
}

fn parse_db_timestamp(ts: &str) -> Option<chrono::DateTime<Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    None
}

async fn authenticate_api_key(state: &AppState, token: &str) -> Result<AuthUser, AuthError> {
    let (api_key_id, secret) = parse_ovk(token).ok_or(AuthError::InvalidToken)?;

    let row = sqlx::query(
        r#"
        SELECT ak.user_id, ak.organization_id, ak.key_hash, ak.expires_at,
               u.username, u.email
        FROM api_keys ak
        INNER JOIN users u ON u.id = ak.user_id
        WHERE ak.id = ?
        "#,
    )
    .bind(api_key_id.to_string())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AuthError::InvalidToken)?
    .ok_or(AuthError::InvalidToken)?;

    let user_id_str: String = row.get("user_id");
    let org_id_str: String = row.get("organization_id");
    let key_hash: String = row.get("key_hash");
    let expires_at: Option<String> = row.try_get("expires_at").ok();

    if let Some(expires_at) = expires_at.as_deref() {
        if let Some(exp) = parse_db_timestamp(expires_at) {
            if Utc::now() >= exp {
                return Err(AuthError::TokenExpired);
            }
        }
    }

    let ok =
        AuthService::verify_password(secret, &key_hash).map_err(|_| AuthError::InvalidToken)?;
    if !ok {
        return Err(AuthError::InvalidToken);
    }

    // Fetch key-scoped roles
    let role_rows = sqlx::query(
        r#"
        SELECT r.id AS role_id, r.name AS role_name
        FROM api_key_roles akr
        INNER JOIN roles r ON r.id = akr.role_id
        WHERE akr.api_key_id = ?
        ORDER BY r.name
        "#,
    )
    .bind(api_key_id.to_string())
    .fetch_all(&state.db)
    .await
    .map_err(|_| AuthError::InvalidToken)?;

    let mut roles: Vec<String> = Vec::with_capacity(role_rows.len());
    let mut role_ids: Vec<Uuid> = Vec::with_capacity(role_rows.len());
    for r in role_rows {
        if let Ok(role_id_str) = r.try_get::<String, _>("role_id") {
            if let Ok(role_id) = Uuid::parse_str(&role_id_str) {
                role_ids.push(role_id);
            }
        }
        if let Ok(role_name) = r.try_get::<String, _>("role_name") {
            roles.push(role_name);
        }
    }

    // Update last_used_at (best-effort)
    let _ = sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(api_key_id.to_string())
        .execute(&state.db)
        .await;

    Ok(AuthUser {
        id: Uuid::parse_str(&user_id_str).map_err(|_| AuthError::InvalidToken)?,
        organization_id: Uuid::parse_str(&org_id_str).map_err(|_| AuthError::InvalidToken)?,
        username: row.get("username"),
        email: row.get("email"),
        roles,
        role_ids,
    })
}

/// Extract token from query string (for SSE/EventSource which can't send headers)
fn extract_query_token(uri: &axum::http::Uri) -> Option<String> {
    uri.query().and_then(|query| {
        query.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next()?;
            if key == "token" {
                Some(value.to_string())
            } else {
                None
            }
        })
    })
}

/// Authentication middleware
///
/// This middleware extracts and validates JWT tokens from the Authorization header.
/// On success, it injects the AuthUser into request extensions.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Try Authorization header first; fall back to X-API-Key, then query param (for SSE)
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let auth_user = if let Some(auth_header) = auth_header {
        if let Some(token) = extract_bearer_token(auth_header) {
            let token_data = validate_token(token, &state.config.auth.jwt_secret)?;
            if token_data.claims.token_type != TokenType::Access {
                return Err(AuthError::InvalidTokenType);
            }
            let mut user: AuthUser = token_data
                .claims
                .try_into()
                .map_err(|_| AuthError::InvalidToken)?;
            let role_ids: Vec<Uuid> = user
                .roles
                .iter()
                .filter_map(|name| state.rbac.get_role_by_name(name).map(|r| r.id))
                .collect();
            user.role_ids = role_ids;
            user
        } else if let Some(token) = extract_api_key_token(auth_header) {
            authenticate_api_key(&state, token).await?
        } else {
            return Err(AuthError::InvalidToken);
        }
    } else if let Some(token) = request
        .headers()
        .get("X-API-Key")
        .or_else(|| request.headers().get("X-Api-Key"))
        .or_else(|| request.headers().get("x-api-key"))
        .and_then(|h| h.to_str().ok())
    {
        authenticate_api_key(&state, token).await?
    } else if let Some(token) = extract_query_token(request.uri()) {
        // Support token in query param for SSE/EventSource (which can't send headers)
        let token_data = validate_token(&token, &state.config.auth.jwt_secret)?;
        if token_data.claims.token_type != TokenType::Access {
            return Err(AuthError::InvalidTokenType);
        }
        let mut user: AuthUser = token_data
            .claims
            .try_into()
            .map_err(|_| AuthError::InvalidToken)?;
        let role_ids: Vec<Uuid> = user
            .roles
            .iter()
            .filter_map(|name| state.rbac.get_role_by_name(name).map(|r| r.id))
            .collect();
        user.role_ids = role_ids;
        user
    } else {
        return Err(AuthError::MissingToken);
    };

    // Insert the authenticated user into request extensions
    request.extensions_mut().insert(auth_user);

    // Continue with the request
    Ok(next.run(request).await)
}

/// Optional authentication middleware
///
/// Similar to auth_middleware but doesn't fail if no token is provided.
/// Use this for routes that have different behavior for authenticated vs anonymous users.
pub async fn optional_auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // Try to extract and validate token (Bearer or API key)
    let maybe_auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let maybe_user: Option<AuthUser> = if let Some(auth_header) = maybe_auth_header {
        if let Some(token) = extract_bearer_token(auth_header) {
            if let Ok(token_data) = validate_token(token, &state.config.auth.jwt_secret) {
                if token_data.claims.token_type == TokenType::Access {
                    if let Ok(mut auth_user) = AuthUser::try_from(token_data.claims) {
                        let role_ids: Vec<Uuid> = auth_user
                            .roles
                            .iter()
                            .filter_map(|name| state.rbac.get_role_by_name(name).map(|r| r.id))
                            .collect();
                        auth_user.role_ids = role_ids;
                        Some(auth_user)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else if let Some(token) = extract_api_key_token(auth_header) {
            authenticate_api_key(&state, token).await.ok()
        } else {
            None
        }
    } else if let Some(token) = request
        .headers()
        .get("X-API-Key")
        .or_else(|| request.headers().get("X-Api-Key"))
        .or_else(|| request.headers().get("x-api-key"))
        .and_then(|h| h.to_str().ok())
    {
        authenticate_api_key(&state, token).await.ok()
    } else {
        None
    };

    if let Some(auth_user) = maybe_user {
        request.extensions_mut().insert(auth_user);
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-that-is-at-least-32-characters-long";

    #[test]
    fn test_create_and_validate_access_token() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = create_access_token(
            &user_id,
            &org_id,
            "testuser",
            "test@example.com",
            vec!["admin".to_string()],
            TEST_SECRET,
            24,
        )
        .unwrap();

        let validated = validate_token(&token, TEST_SECRET).unwrap();
        assert_eq!(validated.claims.sub, user_id.to_string());
        assert_eq!(validated.claims.username, "testuser");
        assert_eq!(validated.claims.token_type, TokenType::Access);
    }

    #[test]
    fn test_create_and_validate_refresh_token() {
        let user_id = Uuid::new_v4();
        let token =
            create_refresh_token(&user_id, "testuser", "test@example.com", TEST_SECRET, 7).unwrap();

        let validated = validate_token(&token, TEST_SECRET).unwrap();
        assert_eq!(validated.claims.token_type, TokenType::Refresh);
    }

    #[test]
    fn test_invalid_token() {
        let result = validate_token("invalid-token", TEST_SECRET);
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn test_wrong_secret() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = create_access_token(
            &user_id,
            &org_id,
            "testuser",
            "test@example.com",
            vec![],
            TEST_SECRET,
            24,
        )
        .unwrap();

        let result = validate_token(&token, "wrong-secret-that-is-also-long-enough");
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(extract_bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer_token("bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer_token("Basic abc123"), None);
    }

    #[test]
    fn test_auth_user_from_claims() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let claims = Claims {
            sub: user_id.to_string(),
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            iat: Utc::now().timestamp(),
            exp: (Utc::now() + Duration::hours(1)).timestamp(),
            nbf: Utc::now().timestamp(),
            jti: Uuid::new_v4().to_string(),
            token_type: TokenType::Access,
            roles: vec!["admin".to_string()],
            organization_id: Some(org_id.to_string()),
        };

        let auth_user = AuthUser::try_from(claims).unwrap();
        assert_eq!(auth_user.id, user_id);
        assert_eq!(auth_user.organization_id, org_id);
        assert_eq!(auth_user.username, "testuser");
        assert_eq!(auth_user.roles, vec!["admin".to_string()]);
        assert!(auth_user.role_ids.is_empty()); // Role IDs are resolved by middleware
    }

    #[test]
    fn test_auth_user_with_role_ids() {
        let user_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let auth_user = AuthUser {
            id: user_id,
            organization_id: Uuid::new_v4(),
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            roles: vec!["admin".to_string()],
            role_ids: vec![],
        };

        let auth_user = auth_user.with_role_ids(vec![role_id]);
        assert_eq!(auth_user.role_ids, vec![role_id]);
    }
}
