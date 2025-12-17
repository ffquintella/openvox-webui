//! Puppet CA management API endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};

use crate::models::{RenewCARequest, SignRequest};
use crate::utils::error::AppError;
use crate::AppState;

/// Create CA routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ca/status", get(get_ca_status))
        .route("/ca/requests", get(list_certificate_requests))
        .route("/ca/certificates", get(list_certificates))
        .route("/ca/certificates/{certname}", get(get_certificate))
        .route("/ca/sign/{certname}", post(sign_certificate))
        .route("/ca/reject/{certname}", post(reject_certificate))
        .route("/ca/certificates/{certname}", delete(revoke_certificate))
        .route("/ca/renew", post(renew_ca_certificate))
}

/// GET /api/v1/ca/status - Get CA service status
///
/// Returns information about the CA service including pending requests and signed certificates.
async fn get_ca_status(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "Puppet CA not configured".to_string(),
        ));
    };

    let status = ca.get_status().await?;
    Ok(Json(status))
}

/// GET /api/v1/ca/requests - List pending certificate requests
///
/// Returns all pending certificate signing requests.
async fn list_certificate_requests(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Ok(Json(Vec::<crate::models::CertificateRequest>::new()));
    };

    let requests = ca.list_requests().await?;
    Ok(Json(requests))
}

/// GET /api/v1/ca/certificates - List signed certificates
///
/// Returns all signed certificates.
async fn list_certificates(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Ok(Json(Vec::<crate::models::Certificate>::new()));
    };

    let certificates = ca.list_certificates().await?;
    Ok(Json(certificates))
}

/// GET /api/v1/ca/certificates/:certname - Get certificate details
///
/// Returns details for a specific certificate.
async fn get_certificate(
    State(state): State<AppState>,
    Path(certname): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "Puppet CA not configured".to_string(),
        ));
    };

    let certificate = ca.get_certificate(&certname).await?;
    Ok(Json(certificate))
}

/// POST /api/v1/ca/sign/:certname - Sign a certificate request
///
/// Signs a pending certificate request.
///
/// Request body:
/// ```json
/// {
///   "dns_alt_names": ["node.example.com", "node2.example.com"]
/// }
/// ```
async fn sign_certificate(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    Json(request): Json<SignRequest>,
) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "Puppet CA not configured".to_string(),
        ));
    };

    let response = ca.sign_certificate(&certname, &request).await?;
    Ok((StatusCode::OK, Json(response)))
}

/// POST /api/v1/ca/reject/:certname - Reject a certificate request
///
/// Rejects a pending certificate request.
async fn reject_certificate(
    State(state): State<AppState>,
    Path(certname): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "Puppet CA not configured".to_string(),
        ));
    };

    let response = ca.reject_certificate(&certname).await?;
    Ok((StatusCode::OK, Json(response)))
}

/// DELETE /api/v1/ca/certificates/:certname - Revoke a certificate
///
/// Revokes a signed certificate.
async fn revoke_certificate(
    State(state): State<AppState>,
    Path(certname): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "Puppet CA not configured".to_string(),
        ));
    };

    let response = ca.revoke_certificate(&certname).await?;
    Ok((StatusCode::OK, Json(response)))
}

/// POST /api/v1/ca/renew - Renew CA certificate
///
/// Renews the CA certificate with a new expiration date.
///
/// Request body:
/// ```json
/// {
///   "days": 3650
/// }
/// ```
async fn renew_ca_certificate(
    State(state): State<AppState>,
    Json(request): Json<RenewCARequest>,
) -> Result<impl IntoResponse, AppError> {
    let Some(ca) = state.puppet_ca.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "Puppet CA not configured".to_string(),
        ));
    };

    if request.days == 0 || request.days > 36500 {
        return Err(AppError::BadRequest(
            "Days must be between 1 and 36500".to_string(),
        ));
    }

    let response = ca.renew_ca(&request).await?;
    Ok((StatusCode::OK, Json(response)))
}
