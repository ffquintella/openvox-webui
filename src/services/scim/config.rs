//! `auth.scim` configuration.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Which SCIM attribute of a user is the subject of the SSO link.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScimLinkAttribute {
    /// `userName` (for example the UPN, which SAML IdPs often send as NameID).
    #[default]
    UserName,
    /// `externalId` (for example the IdP's user id, often the OIDC `sub`).
    ExternalId,
}

impl ScimLinkAttribute {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScimLinkAttribute::UserName => "user_name",
            ScimLinkAttribute::ExternalId => "external_id",
        }
    }
}

/// Inbound SCIM 2.0 provisioning (M31). Off by default; tokens and group
/// mappings are managed from the UI, everything else is here.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimConfig {
    /// Serve `/api/v1/scim/v2`. While `false` every SCIM request answers 404.
    #[serde(default)]
    pub enabled: bool,
    /// Requests per minute and per token (the burst is the same number).
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,
    /// Failed authentications per minute and per client address before the
    /// address is answered 429.
    #[serde(default = "default_auth_failures")]
    pub max_auth_failures_per_minute: u32,
    /// Largest request body accepted, in bytes.
    #[serde(default = "default_max_body")]
    pub max_body_bytes: usize,
    /// Largest page of a list response (`count` is capped to it).
    #[serde(default = "default_page_size")]
    pub max_page_size: usize,
    /// When set, every token must expire within this many days.
    #[serde(default)]
    pub max_token_lifetime_days: Option<u32>,
    /// Public base URL (`https://webui.example.com`) used for `meta.location`;
    /// without it the location is a path.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Sign-in provider of SCIM users: `saml` or the id of an OIDC provider
    /// under `auth.mfa.oidc`. SCIM users never have a local password, so
    /// without this they cannot sign in.
    #[serde(default)]
    pub sso_provider: Option<String>,
    /// SCIM attribute linked to the SSO identity (SAML NameID or OIDC `sub`).
    #[serde(default)]
    pub sso_link_attribute: ScimLinkAttribute,
}

fn default_rate_limit() -> u32 {
    600
}

fn default_auth_failures() -> u32 {
    10
}

fn default_max_body() -> usize {
    1024 * 1024
}

fn default_page_size() -> usize {
    200
}

impl Default for ScimConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate_limit_per_minute: default_rate_limit(),
            max_auth_failures_per_minute: default_auth_failures(),
            max_body_bytes: default_max_body(),
            max_page_size: default_page_size(),
            max_token_lifetime_days: None,
            base_url: None,
            sso_provider: None,
            sso_link_attribute: ScimLinkAttribute::default(),
        }
    }
}

impl ScimConfig {
    /// Validates limits, the base URL and the SSO provider against the
    /// configured OIDC providers.
    pub fn validate(&self, oidc_ids: &[String]) -> Result<()> {
        if self.rate_limit_per_minute == 0 || self.rate_limit_per_minute > 100_000 {
            bail!("auth.scim.rate_limit_per_minute must be between 1 and 100000");
        }
        if self.max_auth_failures_per_minute == 0 || self.max_auth_failures_per_minute > 1000 {
            bail!("auth.scim.max_auth_failures_per_minute must be between 1 and 1000");
        }
        if !(4096..=16 * 1024 * 1024).contains(&self.max_body_bytes) {
            bail!("auth.scim.max_body_bytes must be between 4096 and 16777216");
        }
        if !(1..=1000).contains(&self.max_page_size) {
            bail!("auth.scim.max_page_size must be between 1 and 1000");
        }
        if self
            .max_token_lifetime_days
            .is_some_and(|d| d == 0 || d > 3650)
        {
            bail!("auth.scim.max_token_lifetime_days must be between 1 and 3650");
        }
        if let Some(url) = &self.base_url {
            let ok = (url.starts_with("https://") || url.starts_with("http://"))
                && !url.ends_with('/')
                && !url.chars().any(|c| c.is_whitespace() || c.is_control());
            if !ok {
                bail!("auth.scim.base_url must be an http(s) URL without a trailing slash");
            }
        }
        if let Some(p) = &self.sso_provider {
            if p != "saml" && !oidc_ids.iter().any(|o| o == p) {
                bail!(
                    "auth.scim.sso_provider '{p}' is neither 'saml' nor an OIDC provider id under auth.mfa.oidc"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_off_and_valid() {
        let c = ScimConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.rate_limit_per_minute, 600);
        assert_eq!(c.sso_link_attribute, ScimLinkAttribute::UserName);
        c.validate(&[]).unwrap();
        let parsed: ScimConfig = serde_norway::from_str("enabled: true").unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.max_body_bytes, 1024 * 1024);
    }

    #[test]
    fn validation_rejects_bad_limits_urls_and_providers() {
        let mut c = ScimConfig {
            rate_limit_per_minute: 0,
            ..Default::default()
        };
        assert!(c.validate(&[]).is_err());
        c = ScimConfig {
            max_body_bytes: 10,
            ..Default::default()
        };
        assert!(c.validate(&[]).is_err());
        c = ScimConfig {
            base_url: Some("https://webui.example.com/".into()),
            ..Default::default()
        };
        assert!(c.validate(&[]).is_err());
        c = ScimConfig {
            base_url: Some("ftp://x".into()),
            ..Default::default()
        };
        assert!(c.validate(&[]).is_err());
        c = ScimConfig {
            sso_provider: Some("corp-oidc".into()),
            ..Default::default()
        };
        assert!(c.validate(&[]).is_err());
        assert!(c.validate(&["corp-oidc".into()]).is_ok());
        c.sso_provider = Some("saml".into());
        assert!(c.validate(&[]).is_ok());
        c.max_token_lifetime_days = Some(0);
        assert!(c.validate(&[]).is_err());
    }
}
