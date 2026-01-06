//! SAML 2.0 Service Provider implementation
//!
//! Provides SAML authentication support. This implementation handles the SAML protocol
//! including AuthnRequest generation and Response parsing.
//!
//! Note: This is a simplified implementation that works without the xmlsec native library.
//! For full signature verification, enable the xmlsec feature and install the required
//! system libraries (libxml2, xmlsec1).

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::SamlConfig;

/// SAML assertion data extracted after successful authentication
#[derive(Debug, Clone)]
pub struct SamlAssertion {
    pub name_id: String,
    pub session_index: Option<String>,
    pub attributes: std::collections::HashMap<String, Vec<String>>,
    pub in_response_to: Option<String>,
    pub issuer: Option<String>,
}

/// IdP metadata information
#[derive(Debug, Clone)]
pub struct IdpMetadata {
    pub entity_id: Option<String>,
    pub sso_url: Option<String>,
}

/// SAML Service Provider service
pub struct SamlService {
    config: SamlConfig,
    pool: SqlitePool,
    idp_metadata: Arc<RwLock<Option<IdpMetadata>>>,
}

impl SamlService {
    /// Create a new SAML service
    pub fn new(config: SamlConfig, pool: SqlitePool) -> Self {
        Self {
            config,
            pool,
            idp_metadata: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if SAML is enabled and configured
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Initialize the SAML service provider
    pub async fn initialize(&self) -> Result<()> {
        if !self.config.enabled {
            tracing::info!("SAML is disabled, skipping initialization");
            return Ok(());
        }

        tracing::info!("Initializing SAML Service Provider...");

        // Load IdP metadata
        let idp_metadata = self.fetch_idp_metadata().await?;
        *self.idp_metadata.write().await = Some(idp_metadata);

        tracing::info!("SAML Service Provider initialized successfully");
        Ok(())
    }

    /// Fetch IdP metadata from URL or file and extract relevant information
    async fn fetch_idp_metadata(&self) -> Result<IdpMetadata> {
        // Try to parse metadata using samael if available, otherwise use manual config
        let xml = if let Some(ref url) = self.config.idp.metadata_url {
            tracing::info!("Fetching IdP metadata from URL: {}", url);
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .context("Failed to create HTTP client")?;

            let response = client
                .get(url)
                .send()
                .await
                .context("Failed to fetch IdP metadata")?;

            if !response.status().is_success() {
                anyhow::bail!(
                    "IdP metadata fetch failed with status: {}",
                    response.status()
                );
            }

            Some(response.text().await?)
        } else if let Some(ref path) = self.config.idp.metadata_file {
            tracing::info!("Loading IdP metadata from file: {:?}", path);
            Some(
                std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read IdP metadata file: {:?}", path))?,
            )
        } else {
            None
        };

        // Try to parse metadata to extract SSO URL and entity ID
        let (entity_id, sso_url) = if let Some(xml) = xml {
            // Try to parse with samael
            if let Ok(metadata) = xml.parse::<samael::metadata::EntityDescriptor>() {
                let entity_id = metadata.entity_id.clone();
                let sso_url = metadata
                    .idp_sso_descriptors
                    .as_ref()
                    .and_then(|descriptors| descriptors.first())
                    .and_then(|desc| {
                        desc.single_sign_on_services
                            .iter()
                            .find(|svc| svc.binding.contains("HTTP-Redirect"))
                            .or_else(|| desc.single_sign_on_services.first())
                            .map(|svc| svc.location.clone())
                    });
                (entity_id, sso_url)
            } else {
                // Fallback to manual config
                (self.config.idp.entity_id.clone(), self.config.idp.sso_url.clone())
            }
        } else {
            // Use manual config
            (self.config.idp.entity_id.clone(), self.config.idp.sso_url.clone())
        };

        if sso_url.is_none() {
            anyhow::bail!("No SSO URL found in IdP metadata or configuration");
        }

        Ok(IdpMetadata { entity_id, sso_url })
    }

    /// Generate SP metadata XML for IdP configuration
    pub fn generate_metadata(&self) -> String {
        let sp_entity_id = &self.config.sp.entity_id;
        let acs_url = &self.config.sp.acs_url;

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{}">
  <md:SPSSODescriptor AuthnRequestsSigned="{}" WantAssertionsSigned="{}" protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
    <md:NameIDFormat>urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress</md:NameIDFormat>
    <md:NameIDFormat>urn:oasis:names:tc:SAML:2.0:nameid-format:unspecified</md:NameIDFormat>
    <md:AssertionConsumerService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST" Location="{}" index="0" isDefault="true"/>
  </md:SPSSODescriptor>
</md:EntityDescriptor>"#,
            sp_entity_id,
            self.config.sp.sign_requests,
            self.config.sp.require_signed_assertions,
            acs_url
        )
    }

    /// Get the IdP SSO URL
    pub async fn get_idp_sso_url(&self) -> Result<String> {
        let idp = self.idp_metadata.read().await;
        idp.as_ref()
            .and_then(|m| m.sso_url.clone())
            .ok_or_else(|| anyhow::anyhow!("IdP SSO URL not configured"))
    }

    /// Get IdP entity ID
    pub async fn get_idp_entity_id(&self) -> Option<String> {
        let idp = self.idp_metadata.read().await;
        idp.as_ref().and_then(|m| m.entity_id.clone())
    }

    /// Create a SAML authentication request URL
    pub async fn create_authn_request_url(
        &self,
        relay_state: Option<&str>,
    ) -> Result<(String, String)> {
        let sso_url = self.get_idp_sso_url().await?;
        let request_id = format!("_{}", Uuid::new_v4());
        let issue_instant = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        // Create AuthnRequest XML
        let authn_request_xml = format!(
            r#"<samlp:AuthnRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="{}" Version="2.0" IssueInstant="{}" Destination="{}" AssertionConsumerServiceURL="{}" ProtocolBinding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST">
  <saml:Issuer>{}</saml:Issuer>
</samlp:AuthnRequest>"#,
            request_id,
            issue_instant,
            sso_url,
            self.config.sp.acs_url,
            self.config.sp.entity_id
        );

        // Compress and base64 encode (for Redirect binding)
        let compressed = deflate_compress(authn_request_xml.as_bytes());
        let encoded = BASE64.encode(&compressed);
        let url_encoded = urlencoding::encode(&encoded);

        // Build redirect URL
        let mut redirect_url = if sso_url.contains('?') {
            format!("{}&SAMLRequest={}", sso_url, url_encoded)
        } else {
            format!("{}?SAMLRequest={}", sso_url, url_encoded)
        };

        // Add RelayState if provided
        if let Some(state) = relay_state {
            let encoded_state = urlencoding::encode(state);
            redirect_url.push_str(&format!("&RelayState={}", encoded_state));
        }

        // Store the auth request for verification
        self.store_auth_request(&request_id, relay_state).await?;

        Ok((redirect_url, request_id))
    }

    /// Parse a SAML response
    pub async fn parse_response(
        &self,
        saml_response_b64: &str,
    ) -> Result<(SamlAssertion, Option<String>)> {
        // Decode base64
        let xml_bytes = BASE64
            .decode(saml_response_b64)
            .context("Failed to decode base64 SAML response")?;

        let xml = String::from_utf8(xml_bytes).context("SAML response is not valid UTF-8")?;

        // Try to parse with samael
        let response: samael::schema::Response = xml
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse SAML response: {}", e))?;

        // Get InResponseTo for verification
        let in_response_to = response.in_response_to.clone();

        // Verify this is a response to our request (if not IdP-initiated)
        let relay_state = if let Some(ref irt) = in_response_to {
            let stored_relay = self.verify_request_id(irt).await?;
            if stored_relay.is_none() && !self.config.user_mapping.allow_idp_initiated {
                anyhow::bail!("Invalid or expired SAML request ID");
            }
            stored_relay
        } else {
            if !self.config.user_mapping.allow_idp_initiated {
                anyhow::bail!("IdP-initiated SSO is not allowed");
            }
            None
        };

        // Check response status
        if let Some(ref status) = response.status {
            if let Some(ref value) = status.status_code.value {
                if !value.ends_with("Success") {
                    let message = status
                        .status_message
                        .as_ref()
                        .and_then(|m| m.value.clone())
                        .unwrap_or_default();
                    anyhow::bail!("SAML authentication failed: {} - {}", value, message);
                }
            }
        }

        // Get the assertion
        let assertion = response
            .assertion
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No assertion found in SAML response (encrypted assertions not supported without xmlsec)"))?;

        // Extract NameID
        let name_id = assertion
            .subject
            .as_ref()
            .and_then(|s| s.name_id.as_ref())
            .map(|n| n.value.clone())
            .ok_or_else(|| anyhow::anyhow!("No NameID found in SAML assertion"))?;

        // Extract session index
        let session_index = assertion
            .authn_statements
            .as_ref()
            .and_then(|stmts| stmts.first())
            .and_then(|stmt| stmt.session_index.clone());

        // Extract issuer
        let issuer = assertion
            .issuer
            .value
            .clone()
            .or_else(|| response.issuer.as_ref().and_then(|i| i.value.clone()));

        // Extract attributes
        let mut attributes: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        if let Some(ref attr_statements) = assertion.attribute_statements {
            for stmt in attr_statements {
                for attr in &stmt.attributes {
                    if let Some(ref name) = attr.name {
                        let values: Vec<String> = attr
                            .values
                            .iter()
                            .filter_map(|v| v.value.clone())
                            .collect();
                        attributes.insert(name.clone(), values);
                    }
                }
            }
        }

        Ok((
            SamlAssertion {
                name_id,
                session_index,
                attributes,
                in_response_to,
                issuer,
            },
            relay_state,
        ))
    }

    /// Extract username from SAML assertion based on configuration
    pub fn extract_username(&self, assertion: &SamlAssertion) -> String {
        let attr = &self.config.user_mapping.username_attribute;

        if attr == "NameID" || attr.is_empty() {
            return assertion.name_id.clone();
        }

        // Look for the attribute
        if let Some(values) = assertion.attributes.get(attr) {
            if let Some(value) = values.first() {
                return value.clone();
            }
        }

        // Fall back to NameID
        assertion.name_id.clone()
    }

    /// Extract email from SAML assertion based on configuration
    pub fn extract_email(&self, assertion: &SamlAssertion) -> Option<String> {
        if let Some(ref attr) = self.config.user_mapping.email_attribute {
            if let Some(values) = assertion.attributes.get(attr) {
                return values.first().cloned();
            }
        }

        // Try common email attributes
        let common_attrs = [
            "email",
            "mail",
            "emailAddress",
            "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress",
            "http://schemas.xmlsoap.org/claims/EmailAddress",
        ];

        for attr in common_attrs {
            if let Some(values) = assertion.attributes.get(attr) {
                if let Some(value) = values.first() {
                    return Some(value.clone());
                }
            }
        }

        // Check if NameID looks like an email
        if assertion.name_id.contains('@') {
            return Some(assertion.name_id.clone());
        }

        None
    }

    /// Store a SAML auth request for later verification
    async fn store_auth_request(&self, request_id: &str, relay_state: Option<&str>) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let expires_at = (chrono::Utc::now()
            + chrono::Duration::seconds(self.config.session.request_max_age_secs as i64))
        .to_rfc3339();

        sqlx::query(
            "INSERT INTO saml_auth_requests (id, request_id, relay_state, created_at, expires_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(request_id)
        .bind(relay_state)
        .bind(&created_at)
        .bind(&expires_at)
        .execute(&self.pool)
        .await
        .context("Failed to store SAML auth request")?;

        Ok(())
    }

    /// Verify a request ID and get the associated relay state
    pub async fn verify_request_id(&self, request_id: &str) -> Result<Option<String>> {
        let now = chrono::Utc::now().to_rfc3339();

        let row = sqlx::query(
            "SELECT relay_state FROM saml_auth_requests WHERE request_id = ? AND expires_at > ?",
        )
        .bind(request_id)
        .bind(&now)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to verify request ID")?;

        if let Some(row) = row {
            // Delete the request (single use)
            sqlx::query("DELETE FROM saml_auth_requests WHERE request_id = ?")
                .bind(request_id)
                .execute(&self.pool)
                .await
                .context("Failed to delete used request")?;

            let relay_state: Option<String> = row.try_get("relay_state").ok();
            Ok(relay_state)
        } else {
            Ok(None)
        }
    }

    /// Clean up expired auth requests
    pub async fn cleanup_expired_requests(&self) -> Result<u64> {
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query("DELETE FROM saml_auth_requests WHERE expires_at <= ?")
            .bind(&now)
            .execute(&self.pool)
            .await
            .context("Failed to cleanup expired SAML auth requests")?;

        Ok(result.rows_affected())
    }

    /// Get the SP entity ID
    pub fn get_sp_entity_id(&self) -> &str {
        &self.config.sp.entity_id
    }

    /// Get config for checking settings
    pub fn get_config(&self) -> &SamlConfig {
        &self.config
    }
}

/// DEFLATE compression for SAML requests (HTTP-Redirect binding)
fn deflate_compress(data: &[u8]) -> Vec<u8> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}
