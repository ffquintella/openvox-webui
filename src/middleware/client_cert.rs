//! Client Certificate Authentication Middleware
//!
//! Provides authentication using client SSL certificates (mTLS).
//! This is used by Puppet agents to authenticate and ensure they can only
//! access their own classification data.
//!
//! ## How it works
//!
//! When OpenVox WebUI is deployed behind a reverse proxy (nginx, Apache, HAProxy),
//! the proxy terminates SSL and can pass the client certificate information via headers:
//!
//! - `X-SSL-Client-Cert`: The full PEM-encoded certificate
//! - `X-SSL-Client-DN`: The certificate's Distinguished Name
//! - `X-SSL-Client-CN`: The certificate's Common Name (certname)
//! - `X-SSL-Client-Verify`: Verification status ("SUCCESS", "NONE", "FAILED")
//!
//! When running with direct TLS termination, the certificate is extracted from
//! the TLS connection directly (requires axum-server with rustls).
//!
//! ## Configuration Example (nginx)
//!
//! ```nginx
//! server {
//!     listen 443 ssl;
//!     ssl_client_certificate /etc/puppetlabs/puppet/ssl/certs/ca.pem;
//!     ssl_verify_client optional;
//!
//!     location /api/v1/nodes/ {
//!         proxy_pass http://127.0.0.1:5051;
//!         proxy_set_header X-SSL-Client-Verify $ssl_client_verify;
//!         proxy_set_header X-SSL-Client-DN $ssl_client_s_dn;
//!         proxy_set_header X-SSL-Client-CN $ssl_client_s_dn_cn;
//!     }
//! }
//! ```

use axum::{
    extract::FromRequestParts,
    http::{header::HeaderMap, request::Parts, StatusCode},
};
use serde::{Deserialize, Serialize};

/// Client certificate information extracted from the request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCert {
    /// The Common Name (CN) from the certificate - typically the Puppet certname
    pub cn: String,
    /// The full Distinguished Name (DN)
    pub dn: Option<String>,
    /// Whether the certificate was verified by the proxy
    pub verified: bool,
}

impl ClientCert {
    /// Check if the certificate CN matches the expected certname
    pub fn matches_certname(&self, certname: &str) -> bool {
        self.cn.eq_ignore_ascii_case(certname)
    }
}

/// Error returned when client certificate authentication fails
#[derive(Debug)]
pub enum ClientCertError {
    /// No client certificate was provided
    NoCertificate,
    /// Certificate verification failed
    VerificationFailed,
    /// Could not parse certificate information
    ParseError(String),
}

impl std::fmt::Display for ClientCertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientCertError::NoCertificate => write!(f, "No client certificate provided"),
            ClientCertError::VerificationFailed => write!(f, "Client certificate verification failed"),
            ClientCertError::ParseError(msg) => write!(f, "Failed to parse client certificate: {}", msg),
        }
    }
}

impl std::error::Error for ClientCertError {}

impl axum::response::IntoResponse for ClientCertError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ClientCertError::NoCertificate => (
                StatusCode::UNAUTHORIZED,
                "Client certificate required for this endpoint",
            ),
            ClientCertError::VerificationFailed => (
                StatusCode::FORBIDDEN,
                "Client certificate verification failed",
            ),
            ClientCertError::ParseError(_) => (
                StatusCode::BAD_REQUEST,
                "Invalid client certificate format",
            ),
        };

        let body = serde_json::json!({
            "error": message,
            "code": status.as_u16()
        });

        (status, axum::Json(body)).into_response()
    }
}

/// Extract client certificate from request headers
///
/// This extractor attempts to get client certificate information from headers
/// set by a reverse proxy. If no certificate is found, it returns an error.
impl<S> FromRequestParts<S> for ClientCert
where
    S: Send + Sync,
{
    type Rejection = ClientCertError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        extract_client_cert(&parts.headers)
    }
}

/// Optional client certificate extractor
///
/// Same as ClientCert but returns None instead of an error if no certificate is present.
#[derive(Debug, Clone)]
pub struct OptionalClientCert(pub Option<ClientCert>);

impl<S> FromRequestParts<S> for OptionalClientCert
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(OptionalClientCert(extract_client_cert(&parts.headers).ok()))
    }
}

/// Extract client certificate information from headers
fn extract_client_cert(headers: &HeaderMap) -> Result<ClientCert, ClientCertError> {
    // Check verification status first
    if let Some(verify) = headers.get("X-SSL-Client-Verify") {
        let verify_str = verify.to_str().unwrap_or("");
        match verify_str.to_uppercase().as_str() {
            "SUCCESS" | "OK" | "0" => {
                // Certificate was verified
            }
            "NONE" | "" => {
                return Err(ClientCertError::NoCertificate);
            }
            _ => {
                tracing::warn!("Client certificate verification failed: {}", verify_str);
                return Err(ClientCertError::VerificationFailed);
            }
        }
    }

    // Try to get CN directly (simplest case)
    if let Some(cn_header) = headers.get("X-SSL-Client-CN") {
        let cn = cn_header
            .to_str()
            .map_err(|e| ClientCertError::ParseError(e.to_string()))?
            .to_string();

        if cn.is_empty() {
            return Err(ClientCertError::NoCertificate);
        }

        let dn = headers
            .get("X-SSL-Client-DN")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        return Ok(ClientCert {
            cn,
            dn,
            verified: true,
        });
    }

    // Try to extract CN from DN
    if let Some(dn_header) = headers.get("X-SSL-Client-DN") {
        let dn = dn_header
            .to_str()
            .map_err(|e| ClientCertError::ParseError(e.to_string()))?;

        if let Some(cn) = extract_cn_from_dn(dn) {
            return Ok(ClientCert {
                cn,
                dn: Some(dn.to_string()),
                verified: true,
            });
        } else {
            return Err(ClientCertError::ParseError(
                "Could not extract CN from DN".to_string(),
            ));
        }
    }

    // Try to parse from full certificate (PEM format)
    if let Some(cert_header) = headers.get("X-SSL-Client-Cert") {
        let cert_pem = cert_header
            .to_str()
            .map_err(|e| ClientCertError::ParseError(e.to_string()))?;

        // URL-decode if needed (nginx URL-encodes the cert)
        let cert_pem = urlencoding::decode(cert_pem)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| cert_pem.to_string());

        if let Some(cn) = extract_cn_from_pem(&cert_pem) {
            return Ok(ClientCert {
                cn,
                dn: None,
                verified: true,
            });
        } else {
            return Err(ClientCertError::ParseError(
                "Could not extract CN from certificate".to_string(),
            ));
        }
    }

    Err(ClientCertError::NoCertificate)
}

/// Extract CN from a Distinguished Name string
///
/// Handles formats like:
/// - "CN=hostname.example.com"
/// - "/CN=hostname.example.com/O=Puppet/..."
/// - "CN=hostname.example.com,O=Puppet,..."
fn extract_cn_from_dn(dn: &str) -> Option<String> {
    // Try comma-separated format: "CN=value,O=..."
    for part in dn.split(',') {
        let part = part.trim();
        if let Some(cn) = part.strip_prefix("CN=") {
            return Some(cn.trim().to_string());
        }
        if let Some(cn) = part.strip_prefix("cn=") {
            return Some(cn.trim().to_string());
        }
    }

    // Try slash-separated format: "/CN=value/O=..."
    for part in dn.split('/') {
        let part = part.trim();
        if let Some(cn) = part.strip_prefix("CN=") {
            return Some(cn.trim().to_string());
        }
        if let Some(cn) = part.strip_prefix("cn=") {
            return Some(cn.trim().to_string());
        }
    }

    None
}

/// Extract CN from a PEM-encoded certificate
///
/// This is a simplified parser that looks for the CN in the subject.
/// For production use with complex certificates, consider using x509-parser crate.
fn extract_cn_from_pem(pem: &str) -> Option<String> {
    // This is a very basic implementation that works for simple cases
    // For proper X.509 parsing, use the x509-parser or openssl crate

    // Try to find CN pattern in the certificate (works for text dumps)
    let cn_patterns = ["CN=", "CN = ", "commonName=", "commonName = "];

    for pattern in cn_patterns {
        if let Some(pos) = pem.find(pattern) {
            let start = pos + pattern.len();
            let rest = &pem[start..];

            // Find the end of the CN value
            let end = rest
                .find(|c: char| c == ',' || c == '/' || c == '\n' || c == '\r')
                .unwrap_or(rest.len());

            let cn = rest[..end].trim().to_string();
            if !cn.is_empty() {
                return Some(cn);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_cn_from_dn_comma_format() {
        let dn = "CN=node1.example.com,O=Puppet,OU=Server";
        assert_eq!(
            extract_cn_from_dn(dn),
            Some("node1.example.com".to_string())
        );
    }

    #[test]
    fn test_extract_cn_from_dn_slash_format() {
        let dn = "/CN=node1.example.com/O=Puppet/OU=Server";
        assert_eq!(
            extract_cn_from_dn(dn),
            Some("node1.example.com".to_string())
        );
    }

    #[test]
    fn test_extract_cn_from_dn_lowercase() {
        let dn = "cn=node1.example.com,o=Puppet";
        assert_eq!(
            extract_cn_from_dn(dn),
            Some("node1.example.com".to_string())
        );
    }

    #[test]
    fn test_client_cert_matches_certname() {
        let cert = ClientCert {
            cn: "node1.example.com".to_string(),
            dn: None,
            verified: true,
        };

        assert!(cert.matches_certname("node1.example.com"));
        assert!(cert.matches_certname("NODE1.EXAMPLE.COM"));
        assert!(!cert.matches_certname("node2.example.com"));
    }
}
