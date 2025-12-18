//! Puppet CA service for certificate management

use crate::config::PuppetCAConfig;
use crate::models::{
    CAStatus, Certificate, CertificateRequest, CertificateStatus, RejectResponse, RenewCARequest,
    RenewCAResponse, RevokeResponse, SignRequest, SignResponse,
};
use crate::utils::error::AppError;
use chrono::{DateTime, Utc};
use reqwest::{Client, Identity, StatusCode};
use std::time::Duration;

/// Puppet CA client for managing certificates
#[derive(Clone)]
pub struct PuppetCAService {
    client: Client,
    base_url: String,
}

impl PuppetCAService {
    /// Create a new Puppet CA service from configuration
    pub fn new(config: &PuppetCAConfig) -> Result<Self, AppError> {
        let mut client_builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .use_rustls_tls();

        // Add CA certificate if provided (must be done before identity for rustls)
        if let Some(ca_path) = &config.ssl_ca {
            let ca_pem = std::fs::read(ca_path)
                .map_err(|e| AppError::Internal(format!("Failed to read CA bundle: {}", e)))?;
            let ca_cert = reqwest::Certificate::from_pem(&ca_pem).map_err(|e| {
                AppError::Internal(format!("Failed to parse CA certificate: {}", e))
            })?;
            client_builder = client_builder.add_root_certificate(ca_cert);
        }

        // Configure SSL certificates if provided
        if let (Some(cert_path), Some(key_path)) = (&config.ssl_cert, &config.ssl_key) {
            let cert_pem = std::fs::read(cert_path)
                .map_err(|e| AppError::Internal(format!("Failed to read CA certificate: {}", e)))?;
            let key_pem = std::fs::read(key_path)
                .map_err(|e| AppError::Internal(format!("Failed to read CA key: {}", e)))?;

            // Combine cert and key into a single PEM bundle for rustls
            let mut pem_bundle = cert_pem.clone();
            pem_bundle.push(b'\n');
            pem_bundle.extend_from_slice(&key_pem);

            let identity = Identity::from_pem(&pem_bundle)
                .map_err(|e| AppError::Internal(format!("Failed to create identity: {}", e)))?;

            client_builder = client_builder.identity(identity);
        }

        // Configure SSL verification (must be after identity for rustls compatibility)
        if !config.ssl_verify {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }

        let client = client_builder
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            base_url: config.url.trim_end_matches('/').to_string(),
        })
    }

    /// Get CA status information
    pub async fn get_status(&self) -> Result<CAStatus, AppError> {
        let url = format!("{}/puppet-ca/v1/certificate_status/ca", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                response.status()
            )));
        }

        // Parse CA certificate info
        let ca_info: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to parse CA response: {}", e)))?;

        // Get counts
        let requests = self.list_requests().await?;
        let certificates = self.list_certificates().await?;

        Ok(CAStatus {
            available: true,
            ca_fingerprint: ca_info["fingerprint"].as_str().map(String::from),
            ca_expires_at: ca_info["not_after"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            pending_requests: requests.len(),
            signed_certificates: certificates.len(),
        })
    }

    /// List pending certificate requests
    pub async fn list_requests(&self) -> Result<Vec<CertificateRequest>, AppError> {
        let url = format!("{}/puppet-ca/v1/certificate_requests/all", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        match response.status() {
            StatusCode::OK => {
                let requests: Vec<serde_json::Value> = response
                    .json()
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to parse requests: {}", e)))?;

                Ok(requests
                    .into_iter()
                    .filter_map(|r| self.parse_certificate_request(r).ok())
                    .collect())
            }
            StatusCode::NOT_FOUND => Ok(vec![]),
            status => Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                status
            ))),
        }
    }

    /// List signed certificates
    pub async fn list_certificates(&self) -> Result<Vec<Certificate>, AppError> {
        let url = format!("{}/puppet-ca/v1/certificate_status/all", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        match response.status() {
            StatusCode::OK => {
                let certs: Vec<serde_json::Value> = response.json().await.map_err(|e| {
                    AppError::Internal(format!("Failed to parse certificates: {}", e))
                })?;

                Ok(certs
                    .into_iter()
                    .filter_map(|c| self.parse_certificate(c).ok())
                    .filter(|c| c.state == CertificateStatus::Signed)
                    .collect())
            }
            StatusCode::NOT_FOUND => Ok(vec![]),
            status => Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                status
            ))),
        }
    }

    /// Get certificate details by certname
    pub async fn get_certificate(&self, certname: &str) -> Result<Certificate, AppError> {
        let url = format!(
            "{}/puppet-ca/v1/certificate_status/{}",
            self.base_url, certname
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        match response.status() {
            StatusCode::OK => {
                let cert_data: serde_json::Value = response.json().await.map_err(|e| {
                    AppError::Internal(format!("Failed to parse certificate: {}", e))
                })?;
                self.parse_certificate(cert_data)
            }
            StatusCode::NOT_FOUND => Err(AppError::NotFound(format!(
                "Certificate not found: {}",
                certname
            ))),
            status => Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                status
            ))),
        }
    }

    /// Sign a certificate request
    pub async fn sign_certificate(
        &self,
        certname: &str,
        request: &SignRequest,
    ) -> Result<SignResponse, AppError> {
        let url = format!(
            "{}/puppet-ca/v1/certificate_status/{}",
            self.base_url, certname
        );

        let mut body = serde_json::json!({
            "desired_state": "signed"
        });

        if !request.dns_alt_names.is_empty() {
            body["dns_alt_names"] = serde_json::json!(request.dns_alt_names);
        }

        let response = self
            .client
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => {
                // Fetch the newly signed certificate
                let certificate = self.get_certificate(certname).await?;
                Ok(SignResponse {
                    certificate,
                    message: format!("Certificate signed successfully: {}", certname),
                })
            }
            StatusCode::NOT_FOUND => Err(AppError::NotFound(format!(
                "Certificate request not found: {}",
                certname
            ))),
            StatusCode::CONFLICT => Err(AppError::BadRequest(format!(
                "Certificate already signed: {}",
                certname
            ))),
            status => Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                status
            ))),
        }
    }

    /// Reject a certificate request
    pub async fn reject_certificate(&self, certname: &str) -> Result<RejectResponse, AppError> {
        let url = format!(
            "{}/puppet-ca/v1/certificate_status/{}",
            self.base_url, certname
        );

        let body = serde_json::json!({
            "desired_state": "revoked"
        });

        let response = self
            .client
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(RejectResponse {
                certname: certname.to_string(),
                message: format!("Certificate request rejected: {}", certname),
            }),
            StatusCode::NOT_FOUND => Err(AppError::NotFound(format!(
                "Certificate request not found: {}",
                certname
            ))),
            status => Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                status
            ))),
        }
    }

    /// Revoke a signed certificate
    pub async fn revoke_certificate(&self, certname: &str) -> Result<RevokeResponse, AppError> {
        let url = format!(
            "{}/puppet-ca/v1/certificate_status/{}",
            self.base_url, certname
        );

        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(RevokeResponse {
                certname: certname.to_string(),
                message: format!("Certificate revoked successfully: {}", certname),
            }),
            StatusCode::NOT_FOUND => Err(AppError::NotFound(format!(
                "Certificate not found: {}",
                certname
            ))),
            status => Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                status
            ))),
        }
    }

    /// Renew the CA certificate
    pub async fn renew_ca(&self, request: &RenewCARequest) -> Result<RenewCAResponse, AppError> {
        let url = format!("{}/puppet-ca/v1/certificate/ca", self.base_url);

        let body = serde_json::json!({
            "ttl": format!("{}d", request.days)
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        match response.status() {
            StatusCode::OK => {
                let result: serde_json::Value = response.json().await.map_err(|e| {
                    AppError::Internal(format!("Failed to parse CA renewal response: {}", e))
                })?;

                Ok(RenewCAResponse {
                    fingerprint: result["fingerprint"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    expires_at: result["not_after"]
                        .as_str()
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(Utc::now),
                    message: "CA certificate renewed successfully".to_string(),
                })
            }
            StatusCode::FORBIDDEN => Err(AppError::Forbidden("CA renewal not allowed".to_string())),
            status => Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                status
            ))),
        }
    }

    // Helper methods to parse Puppet CA JSON responses

    fn parse_certificate_request(
        &self,
        data: serde_json::Value,
    ) -> Result<CertificateRequest, AppError> {
        Ok(CertificateRequest {
            certname: data["name"]
                .as_str()
                .ok_or_else(|| AppError::Internal("Missing certname".to_string()))?
                .to_string(),
            requested_at: data["requested_at"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            dns_alt_names: data["dns_alt_names"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            fingerprint: data["fingerprint"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            state: CertificateStatus::Requested,
        })
    }

    fn parse_certificate(&self, data: serde_json::Value) -> Result<Certificate, AppError> {
        let state_str = data["state"].as_str().unwrap_or("unknown");
        let state = match state_str {
            "signed" => CertificateStatus::Signed,
            "requested" => CertificateStatus::Requested,
            "revoked" => CertificateStatus::Revoked,
            _ => CertificateStatus::Requested,
        };

        Ok(Certificate {
            certname: data["name"]
                .as_str()
                .ok_or_else(|| AppError::Internal("Missing certname".to_string()))?
                .to_string(),
            serial: data["serial_number"]
                .as_str()
                .or(data["serial"].as_str())
                .unwrap_or("unknown")
                .to_string(),
            not_before: data["not_before"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            not_after: data["not_after"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            dns_alt_names: data["dns_alt_names"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            fingerprint: data["fingerprint"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            state,
        })
    }
}
