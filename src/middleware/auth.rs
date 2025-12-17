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
use uuid::Uuid;

use crate::{utils::error::ErrorResponse, AppState};

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
        Ok(Self {
            id,
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
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Authentication token has expired"),
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

/// Authentication middleware
///
/// This middleware extracts and validates JWT tokens from the Authorization header.
/// On success, it injects the AuthUser into request extensions.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Extract the Authorization header
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    // Extract the bearer token
    let token = extract_bearer_token(auth_header).ok_or(AuthError::InvalidToken)?;

    // Validate the token
    let token_data = validate_token(token, &state.config.auth.jwt_secret)?;

    // Ensure it's an access token
    if token_data.claims.token_type != TokenType::Access {
        return Err(AuthError::InvalidTokenType);
    }

    // Convert claims to AuthUser
    let mut auth_user: AuthUser = token_data
        .claims
        .try_into()
        .map_err(|_| AuthError::InvalidToken)?;

    // Resolve role names to UUIDs using the RBAC service
    let role_ids: Vec<Uuid> = auth_user
        .roles
        .iter()
        .filter_map(|name| state.rbac.get_role_by_name(name).map(|r| r.id))
        .collect();
    auth_user.role_ids = role_ids;

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
    // Try to extract and validate token
    if let Some(auth_header) = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
    {
        if let Some(token) = extract_bearer_token(auth_header) {
            if let Ok(token_data) = validate_token(token, &state.config.auth.jwt_secret) {
                if token_data.claims.token_type == TokenType::Access {
                    if let Ok(mut auth_user) = AuthUser::try_from(token_data.claims) {
                        // Resolve role names to UUIDs
                        let role_ids: Vec<Uuid> = auth_user
                            .roles
                            .iter()
                            .filter_map(|name| state.rbac.get_role_by_name(name).map(|r| r.id))
                            .collect();
                        auth_user.role_ids = role_ids;
                        request.extensions_mut().insert(auth_user);
                    }
                }
            }
        }
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
        let token = create_access_token(
            &user_id,
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
        let token = create_access_token(
            &user_id,
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
        assert_eq!(
            extract_bearer_token("Bearer abc123"),
            Some("abc123")
        );
        assert_eq!(
            extract_bearer_token("bearer abc123"),
            Some("abc123")
        );
        assert_eq!(extract_bearer_token("Basic abc123"), None);
    }

    #[test]
    fn test_auth_user_from_claims() {
        let user_id = Uuid::new_v4();
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
        };

        let auth_user = AuthUser::try_from(claims).unwrap();
        assert_eq!(auth_user.id, user_id);
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
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            roles: vec!["admin".to_string()],
            role_ids: vec![],
        };

        let auth_user = auth_user.with_role_ids(vec![role_id]);
        assert_eq!(auth_user.role_ids, vec![role_id]);
    }
}
