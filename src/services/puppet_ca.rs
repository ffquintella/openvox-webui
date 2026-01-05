//! Puppet CA service for certificate management

use crate::config::PuppetCAConfig;
use crate::models::{
    CAStatus, Certificate, CertificateRequest, CertificateStatus, RejectResponse, RenewCARequest,
    RenewCAResponse, RevokeResponse, SignRequest, SignResponse,
};
use crate::utils::error::AppError;
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::{Client, Identity, StatusCode};
use std::time::Duration;

/// Parse Puppet CA date format (e.g., "2030-12-17T10:50:34UTC")
/// Puppet CA returns dates with "UTC" suffix instead of "Z" or offset
fn parse_puppet_date(date_str: &str) -> Option<DateTime<Utc>> {
    // Try RFC3339 first (standard format with Z or offset)
    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return Some(dt.with_timezone(&Utc));
    }

    // Try Puppet CA format: "2030-12-17T10:50:34UTC"
    let normalized = date_str.trim_end_matches("UTC");
    if let Ok(naive) = NaiveDateTime::parse_from_str(normalized, "%Y-%m-%dT%H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
    }

    // Try without time zone suffix
    if let Ok(naive) = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
    }

    tracing::warn!("Failed to parse Puppet CA date: {}", date_str);
    None
}

/// Puppet CA client for managing certificates
#[derive(Clone)]
pub struct PuppetCAService {
    client: Client,
    base_url: String,
}

impl PuppetCAService {
    /// Create a new Puppet CA service from configuration
    pub fn new(config: &PuppetCAConfig) -> Result<Self, AppError> {
        tracing::info!("Initializing Puppet CA client for {}", config.url);

        let mut client_builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .use_rustls_tls();

        // Use effective methods to support both flat and nested SSL config formats
        let effective_ca = config.effective_ssl_ca();
        let effective_cert = config.effective_ssl_cert();
        let effective_key = config.effective_ssl_key();
        let effective_verify = config.effective_ssl_verify();

        // Log which SSL configuration format is being used
        if config.ssl.is_some() {
            tracing::info!("Puppet CA SSL: Using nested ssl configuration block");
        } else if effective_ca.is_some() || effective_cert.is_some() {
            tracing::info!("Puppet CA SSL: Using flat ssl_* configuration");
        } else {
            tracing::info!("Puppet CA SSL: No SSL client configuration provided");
        }

        // Add CA certificate if provided (must be done before identity for rustls)
        if let Some(ca_path) = effective_ca {
            tracing::info!("Puppet CA SSL: Loading CA certificate from {:?}", ca_path);
            let ca_pem = std::fs::read(ca_path)
                .map_err(|e| AppError::Internal(format!("Failed to read CA bundle: {}", e)))?;

            // Parse all certificates from the CA file (may contain a chain)
            let certs = reqwest::Certificate::from_pem_bundle(&ca_pem).map_err(|e| {
                AppError::Internal(format!("Failed to parse CA certificate(s): {}", e))
            })?;

            tracing::info!(
                "Puppet CA SSL: Parsed {} certificate(s) from CA bundle",
                certs.len()
            );

            // Use tls_certs_only() to disable the platform verifier and use only our CA
            // This avoids issues with platform-specific certificate compliance checks
            client_builder = client_builder.tls_certs_only(certs);
        }

        // Configure SSL certificates if provided
        if let (Some(cert_path), Some(key_path)) = (effective_cert, effective_key) {
            tracing::info!(
                "Puppet CA SSL: Loading client certificate from {:?}",
                cert_path
            );
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
            tracing::info!("Puppet CA SSL: Client identity configured successfully");
        }

        // Configure SSL verification (must be after identity for rustls compatibility)
        if !effective_verify {
            tracing::warn!("Puppet CA SSL: Certificate verification is DISABLED - this is insecure!");
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }

        let client = client_builder
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        tracing::info!("Puppet CA client initialized successfully for {}", config.url);

        Ok(Self {
            client,
            base_url: config.url.trim_end_matches('/').to_string(),
        })
    }

    /// Get CA status information
    pub async fn get_status(&self) -> Result<CAStatus, AppError> {
        tracing::debug!("Puppet CA: Fetching CA status");

        // Get counts from the working endpoints
        let requests = self.list_requests().await?;
        let certificates = self.list_certificates().await?;

        // Try to find the CA certificate in the list of all certificates
        let ca_info = self.get_ca_certificate_info().await.ok();

        Ok(CAStatus {
            available: true,
            ca_fingerprint: ca_info.as_ref().and_then(|c| c.get("fingerprint")).and_then(|v| v.as_str()).map(String::from),
            ca_expires_at: ca_info.as_ref()
                .and_then(|c| c.get("not_after"))
                .and_then(|v| v.as_str())
                .and_then(parse_puppet_date),
            pending_requests: requests.len(),
            signed_certificates: certificates.len(),
        })
    }

    /// Get CA certificate info from the certificate list
    async fn get_ca_certificate_info(&self) -> Result<serde_json::Value, AppError> {
        // Fetch all certificates and look for the CA cert (usually named "ca" or the puppet server certname)
        let url = format!(
            "{}/puppet-ca/v1/certificate_statuses/all?environment=production",
            self.base_url
        );
        tracing::debug!("Puppet CA: Fetching all certificates to find CA info from {}", url);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("CA service error: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::ServiceUnavailable(format!(
                "CA service returned status: {}",
                response.status()
            )));
        }

        let certs: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to parse certificates: {}", e)))?;

        // Look for the CA certificate - it's typically the one with name "ca"
        // or we can look for the certificate that has the earliest not_before date
        for cert in &certs {
            if let Some(name) = cert.get("name").and_then(|v| v.as_str()) {
                if name == "ca" || name.ends_with(" CA") || name.contains("Puppet CA") {
                    tracing::debug!("Found CA certificate: {}", name);
                    return Ok(cert.clone());
                }
            }
        }

        // If no explicit CA found, return the first signed certificate as a fallback
        // (this gives us at least some info about the CA infrastructure)
        certs.into_iter()
            .find(|c| c.get("state").and_then(|v| v.as_str()) == Some("signed"))
            .ok_or_else(|| AppError::NotFound("CA certificate not found in certificate list".to_string()))
    }

    /// List pending certificate requests
    pub async fn list_requests(&self) -> Result<Vec<CertificateRequest>, AppError> {
        // Use certificate_statuses with state=requested to get pending CSRs
        let url = format!(
            "{}/puppet-ca/v1/certificate_statuses/all?environment=production&state=requested",
            self.base_url
        );
        tracing::debug!("Puppet CA: Fetching certificate requests from {}", url);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
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
        // Use certificate_statuses (plural) with state=signed to list signed certs
        let url = format!(
            "{}/puppet-ca/v1/certificate_statuses/all?environment=production&state=signed",
            self.base_url
        );
        tracing::debug!("Puppet CA: Fetching signed certificates from {}", url);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
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
            "{}/puppet-ca/v1/certificate_status/{}?environment=production",
            self.base_url, certname
        );
        tracing::debug!("Puppet CA: Fetching certificate {} from {}", certname, url);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
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
            "{}/puppet-ca/v1/certificate_status/{}?environment=production",
            self.base_url, certname
        );
        tracing::info!("Puppet CA: Signing certificate {}", certname);

        let mut body = serde_json::json!({
            "desired_state": "signed"
        });

        if !request.dns_alt_names.is_empty() {
            body["dns_alt_names"] = serde_json::json!(request.dns_alt_names);
        }

        let response = self
            .client
            .put(&url)
            .header("Content-Type", "text/pson")
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
            "{}/puppet-ca/v1/certificate_status/{}?environment=production",
            self.base_url, certname
        );
        tracing::info!("Puppet CA: Rejecting certificate {}", certname);

        let body = serde_json::json!({
            "desired_state": "revoked"
        });

        let response = self
            .client
            .put(&url)
            .header("Content-Type", "text/pson")
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
            "{}/puppet-ca/v1/certificate_status/{}?environment=production",
            self.base_url, certname
        );
        tracing::info!("Puppet CA: Revoking certificate {}", certname);

        let response = self
            .client
            .delete(&url)
            .header("Accept", "application/json")
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
        let url = format!(
            "{}/puppet-ca/v1/certificate/ca?environment=production",
            self.base_url
        );
        tracing::info!("Puppet CA: Renewing CA certificate for {} days", request.days);

        let body = serde_json::json!({
            "ttl": format!("{}d", request.days)
        });

        let response = self
            .client
            .post(&url)
            .header("Accept", "application/json")
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
                        .and_then(parse_puppet_date)
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
                .and_then(parse_puppet_date)
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

        // Parse dates using Puppet CA format helper
        let not_before = data["not_before"]
            .as_str()
            .and_then(parse_puppet_date)
            .unwrap_or_else(Utc::now);

        let not_after = data["not_after"]
            .as_str()
            .and_then(parse_puppet_date)
            .unwrap_or_else(|| {
                // Default to 5 years from now if not_after is missing (shouldn't happen for signed certs)
                tracing::warn!(
                    "Certificate {} missing not_after date, using default",
                    data["name"].as_str().unwrap_or("unknown")
                );
                Utc::now() + chrono::Duration::days(365 * 5)
            });

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
            not_before,
            not_after,
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
