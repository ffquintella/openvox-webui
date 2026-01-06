//! SAML 2.0 Authentication API endpoints
//!
//! Provides SAML SSO endpoints for authentication via external Identity Providers.

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    middleware::auth::{create_access_token, create_refresh_token},
    services::{AuthService, SamlService},
    utils::error::ErrorResponse,
    AppState,
};

/// Create public routes for SAML endpoints (no auth required)
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/metadata", get(saml_metadata))
        .route("/login", get(saml_login))
        .route("/acs", post(saml_acs))
}

/// Query parameters for SAML login initiation
#[derive(Debug, Deserialize)]
pub struct SamlLoginQuery {
    /// URL to redirect to after successful authentication
    #[serde(default = "default_redirect")]
    pub redirect: String,
}

fn default_redirect() -> String {
    "/".to_string()
}

/// SAML ACS (Assertion Consumer Service) form data
#[derive(Debug, Deserialize)]
pub struct SamlAcsForm {
    #[serde(rename = "SAMLResponse")]
    pub saml_response: String,
    #[serde(rename = "RelayState")]
    pub relay_state: Option<String>,
}

/// Error response page for SAML errors
#[derive(Debug, Serialize)]
struct SamlErrorPage {
    error: String,
    message: String,
}

impl SamlErrorPage {
    fn to_html(&self) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>SAML Authentication Error</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
               display: flex; justify-content: center; align-items: center;
               height: 100vh; margin: 0; background: #f5f5f5; }}
        .error-box {{ background: white; padding: 40px; border-radius: 8px;
                     box-shadow: 0 2px 10px rgba(0,0,0,0.1); text-align: center; max-width: 500px; }}
        h1 {{ color: #dc3545; margin-bottom: 16px; font-size: 24px; }}
        p {{ color: #666; margin-bottom: 24px; }}
        a {{ color: #0066cc; text-decoration: none; }}
        a:hover {{ text-decoration: underline; }}
    </style>
</head>
<body>
    <div class="error-box">
        <h1>Authentication Error</h1>
        <p>{}: {}</p>
        <a href="/login">Return to Login</a>
    </div>
</body>
</html>"#,
            html_escape(&self.error),
            html_escape(&self.message)
        )
    }
}

/// Simple HTML escaping
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Get SP metadata XML
///
/// GET /api/v1/auth/saml/metadata
///
/// Returns the Service Provider metadata XML that can be imported into the IdP.
async fn saml_metadata(State(state): State<AppState>) -> Response {
    tracing::debug!("SAML metadata request received");

    let saml_config = match &state.config.saml {
        Some(config) if config.enabled => {
            tracing::debug!("SAML is enabled, generating SP metadata");
            config
        }
        Some(_) => {
            tracing::warn!("SAML metadata requested but SAML is disabled");
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "SAML is not enabled".to_string(),
                    details: None,
                    code: None,
                }),
            )
                .into_response();
        }
        None => {
            tracing::warn!("SAML metadata requested but no SAML configuration exists");
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "SAML is not enabled".to_string(),
                    details: None,
                    code: None,
                }),
            )
                .into_response();
        }
    };

    let saml_service = SamlService::new(saml_config.clone(), state.db.clone());
    let metadata = saml_service.generate_metadata();

    tracing::info!(
        "SAML SP metadata generated for entity_id='{}'",
        saml_config.sp.entity_id
    );

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        metadata,
    )
        .into_response()
}

/// Initiate SAML login (SP-initiated SSO)
///
/// GET /api/v1/auth/saml/login
///
/// Redirects the user to the IdP for authentication.
async fn saml_login(
    State(state): State<AppState>,
    Query(query): Query<SamlLoginQuery>,
) -> Response {
    tracing::info!("=== SAML Login Initiation ===");
    tracing::debug!("Redirect after auth: {}", query.redirect);

    let saml_config = match &state.config.saml {
        Some(config) if config.enabled => {
            tracing::debug!("SAML configuration found and enabled");
            config
        }
        Some(_) => {
            tracing::error!("SAML login attempted but SAML is disabled in configuration");
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "SAML is not enabled".to_string(),
                    details: None,
                    code: None,
                }),
            )
                .into_response();
        }
        None => {
            tracing::error!("SAML login attempted but no SAML configuration exists");
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: "SAML is not enabled".to_string(),
                    details: None,
                    code: None,
                }),
            )
                .into_response();
        }
    };

    tracing::debug!(
        "Using SP config: entity_id='{}', acs_url='{}'",
        saml_config.sp.entity_id,
        saml_config.sp.acs_url
    );

    let saml_service = SamlService::new(saml_config.clone(), state.db.clone());

    // Initialize the service to fetch IdP metadata
    tracing::debug!("Initializing SAML service for login...");
    if let Err(e) = saml_service.initialize().await {
        tracing::error!(
            "SAML login failed: service initialization error: {}. \
            Check IdP metadata configuration (metadata_url, metadata_file, or sso_url)",
            e
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "saml_error".to_string(),
                message: "Failed to initialize SAML service".to_string(),
                details: None,
                code: None,
            }),
        )
            .into_response();
    }

    // Create the AuthnRequest URL
    tracing::debug!("Creating SAML AuthnRequest...");
    match saml_service
        .create_authn_request_url(Some(&query.redirect))
        .await
    {
        Ok((redirect_url, request_id)) => {
            tracing::info!(
                "SAML AuthnRequest created: request_id={}, redirecting to IdP",
                request_id
            );
            tracing::debug!("IdP redirect URL: {}", redirect_url);
            Redirect::temporary(&redirect_url).into_response()
        }
        Err(e) => {
            tracing::error!(
                "Failed to create SAML AuthnRequest: {}. \
                This may indicate an issue with IdP SSO URL configuration",
                e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "saml_error".to_string(),
                    message: "Failed to initiate SAML login".to_string(),
                    details: None,
                    code: None,
                }),
            )
                .into_response()
        }
    }
}

/// SAML Assertion Consumer Service (ACS)
///
/// POST /api/v1/auth/saml/acs
///
/// Receives the SAML Response from the IdP, validates it, and creates a session.
async fn saml_acs(State(state): State<AppState>, Form(form): Form<SamlAcsForm>) -> Response {
    tracing::info!("=== SAML ACS: Received IdP Response ===");
    tracing::debug!(
        "SAML Response length: {} bytes, RelayState: {:?}",
        form.saml_response.len(),
        form.relay_state.as_ref().map(|s| if s.len() > 50 {
            format!("{}...", &s[..50])
        } else {
            s.clone()
        })
    );

    let saml_config = match &state.config.saml {
        Some(config) if config.enabled => {
            tracing::debug!("SAML ACS: configuration validated");
            config
        }
        Some(_) => {
            tracing::error!("SAML ACS received response but SAML is disabled");
            let error = SamlErrorPage {
                error: "Configuration Error".to_string(),
                message: "SAML is not enabled".to_string(),
            };
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html")],
                error.to_html(),
            )
                .into_response();
        }
        None => {
            tracing::error!("SAML ACS received response but no SAML configuration exists");
            let error = SamlErrorPage {
                error: "Configuration Error".to_string(),
                message: "SAML is not enabled".to_string(),
            };
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html")],
                error.to_html(),
            )
                .into_response();
        }
    };

    let saml_service = SamlService::new(saml_config.clone(), state.db.clone());

    // Parse and validate the SAML response
    tracing::debug!("Parsing and validating SAML response...");
    let (assertion, relay_state) = match saml_service.parse_response(&form.saml_response).await {
        Ok(result) => {
            tracing::debug!("SAML response parsed successfully");
            result
        }
        Err(e) => {
            tracing::error!(
                "SAML response validation failed: {}. \
                This could be due to: invalid signature, expired assertion, \
                wrong audience, or malformed XML",
                e
            );
            let error = SamlErrorPage {
                error: "Validation Error".to_string(),
                message: e.to_string(),
            };
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "text/html")],
                error.to_html(),
            )
                .into_response();
        }
    };

    tracing::info!(
        "SAML assertion validated: NameID='{}', issuer={:?}, session_index={:?}",
        assertion.name_id,
        assertion.issuer,
        assertion.session_index
    );
    tracing::debug!(
        "SAML assertion attributes: {:?}",
        assertion.attributes.keys().collect::<Vec<_>>()
    );

    // Extract username from the assertion
    let username = saml_service.extract_username(&assertion);
    let email = saml_service.extract_email(&assertion);

    tracing::info!(
        "SAML user mapping result: username='{}', email={:?}",
        username,
        email
    );
    tracing::debug!(
        "User mapping config: username_attribute='{}', email_attribute={:?}",
        saml_config.user_mapping.username_attribute,
        saml_config.user_mapping.email_attribute
    );

    // Look up the user in the database
    let auth_service = AuthService::new(state.db.clone());

    // Try to find user by external_id first, then by username
    tracing::debug!(
        "Looking up user by external_id (NameID): '{}'",
        assertion.name_id
    );
    let user = match auth_service
        .get_user_by_external_id(&assertion.name_id)
        .await
    {
        Ok(Some(user)) => {
            tracing::debug!(
                "User found by external_id: id={}, username='{}'",
                user.id,
                user.username
            );
            Some(user)
        }
        Ok(None) => {
            tracing::debug!(
                "No user found by external_id, trying username lookup: '{}'",
                username
            );
            // Try by username
            match auth_service.get_user_by_username(&username).await {
                Ok(Some(user)) => {
                    tracing::debug!(
                        "User found by username: id={}, username='{}'",
                        user.id,
                        user.username
                    );
                    Some(user)
                }
                Ok(None) => {
                    tracing::debug!("No user found by username either");
                    None
                }
                Err(e) => {
                    tracing::warn!("Error looking up user by username: {}", e);
                    None
                }
            }
        }
        Err(e) => {
            tracing::warn!("Error looking up user by external_id: {}", e);
            // Try by username as fallback
            auth_service.get_user_by_username(&username).await.ok().flatten()
        }
    };

    let user = match user {
        Some(user) => {
            tracing::debug!(
                "User found: id={}, username='{}', auth_provider={}",
                user.id,
                user.username,
                user.auth_provider
            );

            // Check if user is allowed to use SAML
            if !user.auth_provider.allows_saml() {
                tracing::warn!(
                    "SAML login denied for user '{}': auth_provider={} does not allow SAML. \
                    User must have auth_provider='saml' or 'both' to use SSO.",
                    user.username,
                    user.auth_provider
                );
                let error = SamlErrorPage {
                    error: "Access Denied".to_string(),
                    message: "Your account is not configured for SAML authentication".to_string(),
                };
                return (
                    StatusCode::FORBIDDEN,
                    [(header::CONTENT_TYPE, "text/html")],
                    error.to_html(),
                )
                    .into_response();
            }

            tracing::debug!("User auth_provider check passed");
            user
        }
        None => {
            // User not found and we require pre-provisioned users
            if saml_config.user_mapping.require_existing_user {
                tracing::warn!(
                    "SAML login denied: user not found in database. \
                    username='{}', name_id='{}'. \
                    Either create the user first or set require_existing_user=false (if auto-provisioning is implemented)",
                    username,
                    assertion.name_id
                );
                let error = SamlErrorPage {
                    error: "Access Denied".to_string(),
                    message: "Your account has not been provisioned. Please contact an administrator.".to_string(),
                };
                return (
                    StatusCode::FORBIDDEN,
                    [(header::CONTENT_TYPE, "text/html")],
                    error.to_html(),
                )
                    .into_response();
            }

            // Auto-provisioning is enabled but not implemented in this version
            tracing::error!(
                "User auto-provisioning requested but not implemented. \
                User '{}' (name_id='{}') cannot be created automatically.",
                username,
                assertion.name_id
            );
            let error = SamlErrorPage {
                error: "Configuration Error".to_string(),
                message: "User auto-provisioning is not supported".to_string(),
            };
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/html")],
                error.to_html(),
            )
                .into_response();
        }
    };

    // Update SAML authentication info
    tracing::debug!(
        "Updating SAML auth info: external_id='{}', idp_entity_id={:?}",
        assertion.name_id,
        assertion.issuer
    );
    if let Err(e) = auth_service
        .update_saml_auth_info(
            &user.id,
            Some(&assertion.name_id),
            assertion.issuer.as_deref(),
        )
        .await
    {
        tracing::warn!(
            "Failed to update SAML auth info for user {} (non-critical): {}",
            user.id,
            e
        );
        // Continue anyway, this is not critical
    } else {
        tracing::debug!("SAML auth info updated successfully");
    }

    // Get user roles
    tracing::debug!("Fetching roles for user: {}", user.id);
    let roles = auth_service
        .get_user_roles(&user.id)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to get user roles, falling back to legacy role: {}",
                e
            );
            vec![user.role.clone()]
        });
    tracing::debug!("User roles: {:?}", roles);

    // Create JWT tokens
    tracing::debug!("Creating JWT access token...");
    let access_token = match create_access_token(
        &user.id,
        &user.organization_id,
        &user.username,
        &user.email,
        roles,
        &state.config.auth.jwt_secret,
        state.config.auth.token_expiry_hours,
    ) {
        Ok(token) => {
            tracing::debug!("Access token created successfully");
            token
        }
        Err(e) => {
            tracing::error!("Failed to create access token for SAML user: {}", e);
            let error = SamlErrorPage {
                error: "Internal Error".to_string(),
                message: "Failed to create session".to_string(),
            };
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/html")],
                error.to_html(),
            )
                .into_response();
        }
    };

    tracing::debug!("Creating JWT refresh token...");
    let refresh_token = match create_refresh_token(
        &user.id,
        &user.username,
        &user.email,
        &state.config.auth.jwt_secret,
        state.config.auth.refresh_token_expiry_days,
    ) {
        Ok(token) => {
            tracing::debug!("Refresh token created successfully");
            token
        }
        Err(e) => {
            tracing::error!("Failed to create refresh token for SAML user: {}", e);
            let error = SamlErrorPage {
                error: "Internal Error".to_string(),
                message: "Failed to create session".to_string(),
            };
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/html")],
                error.to_html(),
            )
                .into_response();
        }
    };

    // Determine redirect URL from relay_state or form
    let redirect_url = relay_state
        .or(form.relay_state)
        .unwrap_or_else(|| "/".to_string());

    // Validate redirect URL to prevent open redirect
    let safe_redirect = if redirect_url.starts_with('/') && !redirect_url.starts_with("//") {
        redirect_url.clone()
    } else {
        tracing::warn!(
            "Potentially unsafe redirect URL rejected: '{}', using '/' instead",
            redirect_url
        );
        "/".to_string()
    };

    // Build callback URL with tokens
    // The frontend will extract these and store them
    let callback_url = format!(
        "/saml-callback?access_token={}&refresh_token={}&redirect={}",
        urlencoding::encode(&access_token),
        urlencoding::encode(&refresh_token),
        urlencoding::encode(&safe_redirect)
    );

    tracing::info!(
        "=== SAML Login Successful === user='{}', redirect='{}'",
        user.username,
        safe_redirect
    );

    Redirect::temporary(&callback_url).into_response()
}

use axum::Json;
