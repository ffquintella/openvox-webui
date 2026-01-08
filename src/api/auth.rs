//! Authentication API endpoints
//!
//! Provides login, logout, token refresh, and registration endpoints.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::{
    middleware::auth::{
        create_access_token, create_refresh_token, validate_token, AuthUser, TokenType,
    },
    models::{AuthResponse, LoginRequest, RefreshTokenRequest, TokenResponse, UserPublic},
    services::AuthService,
    utils::error::ErrorResponse,
    AppState,
};

/// Create public routes for authentication endpoints (no auth required)
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh_token))
        .route("/logout", post(logout))
        .route("/register", post(register))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
}

/// Create protected routes for authentication endpoints (auth required)
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/change-password", post(change_password))
        .route("/me", get(get_current_user))
}

/// Login request body for registration
#[derive(Debug, serde::Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Login handler
///
/// POST /api/v1/auth/login
async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auth_service = AuthService::new(state.db.clone());

    // Authenticate user
    let user = auth_service
        .authenticate(&payload.username, &payload.password)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Authentication failed: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "unauthorized".to_string(),
                    message: "Invalid username or password".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Get user roles from RBAC
    let roles = auth_service
        .get_user_roles(&user.id)
        .await
        .unwrap_or_else(|_| vec![user.role.clone()]);

    // Create tokens
    let access_token = create_access_token(
        &user.id,
        &user.organization_id,
        &user.username,
        &user.email,
        roles,
        &state.config.auth.jwt_secret,
        state.config.auth.token_expiry_hours,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to create access token: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    let refresh_token = create_refresh_token(
        &user.id,
        &user.username,
        &user.email,
        &state.config.auth.jwt_secret,
        state.config.auth.refresh_token_expiry_days,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to create refresh token: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: state.config.auth.token_expiry_hours * 3600,
        user: user.into(),
    }))
}

/// Refresh token handler
///
/// POST /api/v1/auth/refresh
async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<RefreshTokenRequest>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate the refresh token
    let token_data = validate_token(&payload.refresh_token, &state.config.auth.jwt_secret)
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "unauthorized".to_string(),
                    message: "Invalid or expired refresh token".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Ensure it's a refresh token
    if token_data.claims.token_type != TokenType::Refresh {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Invalid token type".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    // Get user from database to ensure they still exist and get current roles
    let auth_service = AuthService::new(state.db.clone());
    let user_id = uuid::Uuid::parse_str(&token_data.claims.sub).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Invalid user ID in token".to_string(),
                details: None,
                code: None,
            }),
        )
    })?;

    let user = auth_service
        .get_user_by_id(&user_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: "Failed to fetch user".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "unauthorized".to_string(),
                    message: "User not found".to_string(),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // Get current roles
    let roles = auth_service
        .get_user_roles(&user.id)
        .await
        .unwrap_or_else(|_| vec![user.role.clone()]);

    // Create new access token
    let access_token = create_access_token(
        &user.id,
        &user.organization_id,
        &user.username,
        &user.email,
        roles,
        &state.config.auth.jwt_secret,
        state.config.auth.token_expiry_hours,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "internal_error".to_string(),
                message: format!("Failed to create access token: {}", e),
                details: None,
                code: None,
            }),
        )
    })?;

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: state.config.auth.token_expiry_hours * 3600,
    }))
}

/// Logout response
#[derive(Debug, Serialize)]
struct LogoutResponse {
    message: String,
}

/// Logout handler
///
/// POST /api/v1/auth/logout
///
/// Note: Since we use stateless JWT tokens, logout is handled client-side
/// by discarding the tokens. This endpoint is provided for consistency
/// and could be extended to support token blacklisting in the future.
async fn logout() -> Json<LogoutResponse> {
    Json(LogoutResponse {
        message: "Successfully logged out".to_string(),
    })
}

/// Register a new user
///
/// POST /api/v1/auth/register
async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserPublic>), (StatusCode, Json<ErrorResponse>)> {
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

    if payload.password.len() < 8 {
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

    let auth_service = AuthService::new(state.db.clone());

    // Create user with default role
    let user = auth_service
        .create_user(
            &payload.username,
            &payload.email,
            &payload.password,
            "viewer",
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

    Ok((StatusCode::CREATED, Json(user.into())))
}

/// Forgot password request
#[derive(Debug, serde::Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

/// Forgot password response
#[derive(Debug, Serialize)]
pub struct ForgotPasswordResponse {
    pub message: String,
    /// Only included in development mode for testing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_token: Option<String>,
}

/// Request a password reset
///
/// POST /api/v1/auth/forgot-password
///
/// Note: In production, this would send an email with the reset link.
/// For development/testing, the token is returned directly in the response.
async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<Json<ForgotPasswordResponse>, (StatusCode, Json<ErrorResponse>)> {
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

    let auth_service = AuthService::new(state.db.clone());

    // Create reset token (returns None if email doesn't exist, but we don't reveal that)
    let _token = auth_service
        .create_password_reset_token(&payload.email)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to process request: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?;

    // In a real application, you would send an email here
    // For now, we return the token in development mode
    #[cfg(debug_assertions)]
    let reset_token = _token;

    #[cfg(not(debug_assertions))]
    let reset_token: Option<String> = None;

    // Always return success to prevent email enumeration
    Ok(Json(ForgotPasswordResponse {
        message: "If an account with that email exists, a password reset link has been sent."
            .to_string(),
        reset_token,
    }))
}

/// Reset password request
#[derive(Debug, serde::Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

/// Reset password response
#[derive(Debug, Serialize)]
pub struct ResetPasswordResponse {
    pub message: String,
}

/// Reset password using a valid reset token
///
/// POST /api/v1/auth/reset-password
async fn reset_password(
    State(state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<Json<ResetPasswordResponse>, (StatusCode, Json<ErrorResponse>)> {
    if payload.new_password.len() < 8 {
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

    let auth_service = AuthService::new(state.db.clone());

    let success = auth_service
        .reset_password(&payload.token, &payload.new_password)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to reset password: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?;

    if success {
        Ok(Json(ResetPasswordResponse {
            message: "Password has been reset successfully.".to_string(),
        }))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_token".to_string(),
                message: "Invalid or expired reset token".to_string(),
                details: None,
                code: None,
            }),
        ))
    }
}

/// Change password request
#[derive(Debug, serde::Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Change password response
#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub message: String,
}

/// Change password for the authenticated user
///
/// POST /api/v1/auth/change-password
async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, (StatusCode, Json<ErrorResponse>)> {
    if payload.new_password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "New password must be at least 8 characters".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    if payload.current_password == payload.new_password {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "validation_error".to_string(),
                message: "New password must be different from current password".to_string(),
                details: None,
                code: None,
            }),
        ));
    }

    let auth_service = AuthService::new(state.db.clone());

    let success = auth_service
        .change_password(
            &auth_user.id,
            &payload.current_password,
            &payload.new_password,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to change password: {}", e),
                    details: None,
                    code: None,
                }),
            )
        })?;

    if success {
        Ok(Json(ChangePasswordResponse {
            message: "Password changed successfully".to_string(),
        }))
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Current password is incorrect".to_string(),
                details: None,
                code: None,
            }),
        ))
    }
}

/// Get current authenticated user profile
///
/// GET /api/v1/auth/me
async fn get_current_user(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<UserPublic>, (StatusCode, Json<ErrorResponse>)> {
    let auth_service = AuthService::new(state.db.clone());

    let user = auth_service
        .get_user_by_id(&auth_user.id)
        .await
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

    Ok(Json(user.into()))
}
