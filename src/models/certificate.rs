//! Certificate and CSR models for Puppet CA management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Certificate signing request status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CertificateStatus {
    /// Certificate request is pending signature
    Requested,
    /// Certificate is signed and valid
    Signed,
    /// Certificate request was rejected
    Rejected,
    /// Certificate has been revoked
    Revoked,
}

/// A certificate signing request from a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRequest {
    /// Node certname (CN)
    pub certname: String,
    /// Request timestamp
    pub requested_at: DateTime<Utc>,
    /// DNS alternative names
    #[serde(default)]
    pub dns_alt_names: Vec<String>,
    /// Fingerprint (SHA256)
    pub fingerprint: String,
    /// Request state
    pub state: CertificateStatus,
}

/// A signed certificate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    /// Node certname (CN)
    pub certname: String,
    /// Serial number
    pub serial: String,
    /// Not valid before timestamp
    pub not_before: DateTime<Utc>,
    /// Not valid after timestamp
    pub not_after: DateTime<Utc>,
    /// DNS alternative names
    #[serde(default)]
    pub dns_alt_names: Vec<String>,
    /// Fingerprint (SHA256)
    pub fingerprint: String,
    /// Certificate state
    pub state: CertificateStatus,
}

/// CA status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CAStatus {
    /// CA service is available
    pub available: bool,
    /// CA certificate fingerprint
    pub ca_fingerprint: Option<String>,
    /// CA certificate expiration
    pub ca_expires_at: Option<DateTime<Utc>>,
    /// Number of pending requests
    pub pending_requests: usize,
    /// Number of signed certificates
    pub signed_certificates: usize,
}

/// Request body for signing a certificate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRequest {
    /// Optional DNS alternative names to add
    #[serde(default)]
    pub dns_alt_names: Vec<String>,
}

/// Response from signing a certificate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignResponse {
    /// The signed certificate
    pub certificate: Certificate,
    /// Status message
    pub message: String,
}

/// Response from rejecting a certificate request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectResponse {
    /// The certname that was rejected
    pub certname: String,
    /// Status message
    pub message: String,
}

/// Response from revoking a certificate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeResponse {
    /// The certname that was revoked
    pub certname: String,
    /// Status message
    pub message: String,
}

/// Request body for CA renewal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenewCARequest {
    /// Number of days until expiration
    pub days: u32,
}

/// Response from CA renewal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenewCAResponse {
    /// New CA fingerprint
    pub fingerprint: String,
    /// New expiration date
    pub expires_at: DateTime<Utc>,
    /// Status message
    pub message: String,
}
