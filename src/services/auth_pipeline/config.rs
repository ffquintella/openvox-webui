//! `auth.mfa` configuration: factor sealing, WebAuthn relying party and SSO providers.

use serde::{Deserialize, Serialize};

fn default_max_attempts() -> u32 {
    5
}

fn default_totp_issuer() -> String {
    "OpenVox WebUI".to_string()
}

fn default_scopes() -> Vec<String> {
    vec!["openid".into(), "profile".into(), "email".into()]
}

/// Settings of the authentication pipeline (`auth.mfa` in `config.yaml`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MfaConfig {
    /// Key that seals factor seeds and ceremony state. Falls back to
    /// `auth.jwt_secret`; set it (via `secret://`) to rotate them apart.
    #[serde(default)]
    pub factor_key: Option<String>,
    /// Wrong factor responses allowed on one challenge before it is burned.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// Issuer shown by authenticator apps.
    #[serde(default = "default_totp_issuer")]
    pub totp_issuer: String,
    /// WebAuthn relying party. Without it the WebAuthn factor is unavailable.
    #[serde(default)]
    pub webauthn: Option<WebAuthnConfig>,
    /// Built-in OIDC providers.
    #[serde(default)]
    pub oidc: Vec<OidcProviderConfig>,
}

impl Default for MfaConfig {
    fn default() -> Self {
        Self {
            factor_key: None,
            max_attempts: default_max_attempts(),
            totp_issuer: default_totp_issuer(),
            webauthn: None,
            oidc: Vec::new(),
        }
    }
}

/// WebAuthn relying party. Behind a proxy the RP ID and the allowed origin are
/// explicit: they cannot be derived from the request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebAuthnConfig {
    /// Registrable domain, for example `puppet.example.com`.
    pub rp_id: String,
    #[serde(default = "default_totp_issuer")]
    pub rp_name: String,
    /// Exact origin the browser reports, for example `https://puppet.example.com`.
    pub origin: String,
}

/// One OIDC provider (authorization code flow with PKCE).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OidcProviderConfig {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Issuer URL; `/.well-known/openid-configuration` is read from it.
    pub issuer: String,
    pub client_id: String,
    /// May be a `secret://` reference, resolved at startup.
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Absolute callback URL registered at the IdP, i.e.
    /// `https://host/api/v1/auth/providers/<id>/callback`.
    pub redirect_uri: String,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    /// The IdP enforces MFA: a matching `amr`/`acr` satisfies the policy.
    #[serde(default)]
    pub asserts_mfa: bool,
    /// The IdP is authoritative for `email_verified`.
    #[serde(default)]
    pub trust_email: bool,
    /// Create the user on first login. Off by default: an unlinked identity
    /// is denied unless an administrator provisioned it.
    #[serde(default)]
    pub jit: bool,
    /// Role of a JIT user. Never granted from IdP claims.
    #[serde(default)]
    pub default_role: Option<String>,
    /// Claim used as the username of a JIT user.
    #[serde(default)]
    pub username_claim: Option<String>,
}
