//! `auth.mfa` configuration: factor sealing, WebAuthn relying party and SSO providers.

use serde::{Deserialize, Deserializer, Serialize};

/// Shortest enrollment grace window that is not "disabled" (one minute).
pub const MIN_ENROLLMENT_GRACE_SECS: u64 = 60;
/// Longest enrollment grace window (seven days).
pub const MAX_ENROLLMENT_GRACE_SECS: u64 = 7 * 24 * 3600;

fn default_max_attempts() -> u32 {
    5
}

/// `0` disables the grace window; other values must lie between the bounds,
/// so a typo fails at startup instead of opening a long window.
fn grace_secs<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let secs = u64::deserialize(d)?;
    if secs != 0 && !(MIN_ENROLLMENT_GRACE_SECS..=MAX_ENROLLMENT_GRACE_SECS).contains(&secs) {
        return Err(serde::de::Error::custom(format!(
            "auth.mfa.enrollment_grace_secs must be 0 (disabled) or between \
             {MIN_ENROLLMENT_GRACE_SECS} and {MAX_ENROLLMENT_GRACE_SECS}, got {secs}"
        )));
    }
    Ok(secs)
}

/// Shortest step-up freshness that is not "disabled" (one minute).
pub const MIN_STEP_UP_MAX_AGE_SECS: u64 = 60;
/// Longest step-up freshness (30 days).
pub const MAX_STEP_UP_MAX_AGE_SECS: u64 = 30 * 24 * 3600;

fn step_up_max_age<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let secs = u64::deserialize(d)?;
    if secs != 0 && !(MIN_STEP_UP_MAX_AGE_SECS..=MAX_STEP_UP_MAX_AGE_SECS).contains(&secs) {
        return Err(serde::de::Error::custom(format!(
            "auth.step_up.max_age_secs must be 0 (disabled) or between \
             {MIN_STEP_UP_MAX_AGE_SECS} and {MAX_STEP_UP_MAX_AGE_SECS}, got {secs}"
        )));
    }
    Ok(secs)
}

/// `auth.step_up` (OQ-13).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StepUpConfig {
    /// A session at the step-up level whose last second-factor
    /// verification is older than this must step up again before a
    /// sensitive action. `0` (default) disables the check: the level alone
    /// is enough, as before.
    #[serde(default, deserialize_with = "step_up_max_age")]
    pub max_age_secs: u64,
}

impl StepUpConfig {
    /// The freshness window, `None` when disabled.
    pub fn max_age(&self) -> Option<chrono::Duration> {
        (self.max_age_secs > 0).then(|| chrono::Duration::seconds(self.max_age_secs as i64))
    }
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
    /// Enrollment grace window in seconds (OQ-12). When an administrator
    /// resets a user's factors, the user may sign in with the primary factor
    /// to an enrollment-only session for this long. `0` (default) disables
    /// it: a user with nothing enrolled is denied, as before.
    #[serde(default, deserialize_with = "grace_secs")]
    pub enrollment_grace_secs: u64,
    /// Also open one window, once per account, the first time a user with
    /// nothing enrolled is stopped by a policy. Needs `enrollment_grace_secs`.
    #[serde(default)]
    pub enrollment_grace_on_policy: bool,
}

impl MfaConfig {
    /// Length of a grace window, `None` when the feature is disabled.
    pub fn enrollment_grace(&self) -> Option<chrono::Duration> {
        (self.enrollment_grace_secs > 0)
            .then(|| chrono::Duration::seconds(self.enrollment_grace_secs as i64))
    }
}

impl Default for MfaConfig {
    fn default() -> Self {
        Self {
            factor_key: None,
            max_attempts: default_max_attempts(),
            totp_issuer: default_totp_issuer(),
            webauthn: None,
            oidc: Vec::new(),
            enrollment_grace_secs: 0,
            enrollment_grace_on_policy: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<MfaConfig, serde_norway::Error> {
        serde_norway::from_str(yaml)
    }

    /// OQ-13: step-up freshness is off by default and bounded when set.
    #[test]
    fn step_up_max_age_is_off_by_default_and_bounded() {
        let parse = |y: &str| serde_norway::from_str::<StepUpConfig>(y);
        assert_eq!(StepUpConfig::default().max_age(), None);
        assert_eq!(parse("{}").unwrap().max_age(), None);
        assert_eq!(parse("max_age_secs: 0").unwrap().max_age(), None);
        assert_eq!(
            parse("max_age_secs: 900").unwrap().max_age(),
            Some(chrono::Duration::seconds(900))
        );
        assert!(parse("max_age_secs: 60").is_ok());
        assert!(parse("max_age_secs: 2592000").is_ok());
        for bad in ["1", "59", "2592001", "-1"] {
            assert!(parse(&format!("max_age_secs: {bad}")).is_err(), "{bad}");
        }
    }

    #[test]
    fn enrollment_grace_is_off_by_default() {
        let c = parse("max_attempts: 5").unwrap();
        assert_eq!(c.enrollment_grace_secs, 0);
        assert!(!c.enrollment_grace_on_policy);
        assert!(c.enrollment_grace().is_none());
        assert!(MfaConfig::default().enrollment_grace().is_none());
    }

    #[test]
    fn enrollment_grace_accepts_values_within_the_bounds() {
        let c = parse("enrollment_grace_secs: 86400\nenrollment_grace_on_policy: true").unwrap();
        assert_eq!(c.enrollment_grace(), Some(chrono::Duration::hours(24)));
        assert!(c.enrollment_grace_on_policy);
        assert!(parse("enrollment_grace_secs: 60").is_ok());
        assert!(parse("enrollment_grace_secs: 604800").is_ok());
    }

    #[test]
    fn enrollment_grace_refuses_values_outside_the_bounds() {
        for bad in ["1", "59", "604801", "31536000"] {
            let err = parse(&format!("enrollment_grace_secs: {bad}"))
                .unwrap_err()
                .to_string();
            assert!(err.contains("enrollment_grace_secs"), "{bad}: {err}");
        }
        assert!(parse("enrollment_grace_secs: -5").is_err());
    }
}
