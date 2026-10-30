//! Built-in OIDC provider: authorization code flow with PKCE.
//!
//! The host controls the edge: a fixed `redirect_uri`, `state`/`nonce`/PKCE
//! verifier kept server-side and used once, binding to the browser by cookie
//! (against login CSRF) and a `return_to` allowlist (against open redirects).
//! Accounts are linked by `(provider, subject)`, never by e-mail alone.

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::config::OidcProviderConfig;
use super::crypto;
use crate::models::{default_organization_uuid, User};
use crate::services::AuthService;

pub const FLOW_TTL_MINUTES: i64 = 10;
pub const BROWSER_COOKIE: &str = "ovx_auth_flow";

#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FlowPayload {
    nonce: String,
    verifier: String,
    return_to: String,
}

/// Claims of a validated ID token that the host cares about.
#[derive(Debug, Clone)]
pub struct IdentityClaims {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
    pub amr: Vec<String>,
    pub acr: Option<String>,
    pub raw: Value,
}

/// Only same-site absolute paths: stops open redirects.
pub fn safe_return_to(candidate: Option<&str>) -> String {
    match candidate {
        Some(p)
            if p.starts_with('/')
                && !p.starts_with("//")
                && !p.contains('\\')
                && !p.chars().any(|c| c.is_control()) =>
        {
            p.to_string()
        }
        _ => "/".to_string(),
    }
}

fn pkce_challenge(verifier: &str) -> String {
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_token() -> String {
    let b: [u8; 32] = rand::random();
    B64URL.encode(b)
}

pub struct OidcClient<'a> {
    pub cfg: &'a OidcProviderConfig,
    http: reqwest::Client,
}

pub struct BeginFlow {
    pub authorize_url: String,
    pub state: String,
    pub browser_token: String,
}

impl<'a> OidcClient<'a> {
    pub fn new(cfg: &'a OidcProviderConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self { cfg, http }
    }

    pub async fn discover(&self) -> Result<Discovery> {
        let url = format!(
            "{}/.well-known/openid-configuration",
            self.cfg.issuer.trim_end_matches('/')
        );
        let d: Discovery = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if d.issuer.trim_end_matches('/') != self.cfg.issuer.trim_end_matches('/') {
            bail!("discovery issuer does not match the configured issuer");
        }
        Ok(d)
    }

    /// Builds the authorization URL and stores the flow state.
    pub async fn begin(
        &self,
        pool: &SqlitePool,
        key: &str,
        return_to: Option<&str>,
    ) -> Result<BeginFlow> {
        let d = self.discover().await?;
        let state = random_token();
        let browser_token = random_token();
        let payload = FlowPayload {
            nonce: random_token(),
            verifier: random_token(),
            return_to: safe_return_to(return_to),
        };
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO auth_flow_state (id, kind, provider, browser_hash, payload_encrypted, expires_at, created_at) \
             VALUES (?, 'oidc', ?, ?, ?, ?, ?)",
        )
        .bind(&state)
        .bind(&self.cfg.id)
        .bind(crypto::bind_hash(&[&browser_token]))
        .bind(crypto::seal_json(key, &payload)?)
        .bind((now + Duration::minutes(FLOW_TTL_MINUTES)).to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(pool)
        .await?;

        let q = |k: &str, v: &str| format!("{}={}", k, urlencoding::encode(v));
        let sep = if d.authorization_endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        let authorize_url = format!(
            "{}{}{}",
            d.authorization_endpoint,
            sep,
            [
                q("response_type", "code"),
                q("client_id", &self.cfg.client_id),
                q("redirect_uri", &self.cfg.redirect_uri),
                q("scope", &self.cfg.scopes.join(" ")),
                q("state", &state),
                q("nonce", &payload.nonce),
                q("code_challenge", &pkce_challenge(&payload.verifier)),
                q("code_challenge_method", "S256"),
            ]
            .join("&")
        );
        Ok(BeginFlow {
            authorize_url,
            state,
            browser_token,
        })
    }

    /// Consumes the flow, exchanges the code and validates the ID token.
    /// Returns the claims and the `return_to` chosen at the start.
    pub async fn complete(
        &self,
        pool: &SqlitePool,
        key: &str,
        state: &str,
        code: &str,
        browser_token: &str,
    ) -> Result<(IdentityClaims, String)> {
        #[allow(clippy::type_complexity)]
        let row: Option<(String, Option<String>, String, String, Option<String>)> = sqlx::query_as(
            "SELECT provider, browser_hash, payload_encrypted, expires_at, consumed_at \
             FROM auth_flow_state WHERE id = ? AND kind = 'oidc'",
        )
        .bind(state)
        .fetch_optional(pool)
        .await?;
        let (provider, browser_hash, sealed, expires_at, consumed) =
            row.ok_or_else(|| anyhow!("unknown flow"))?;
        if provider != self.cfg.id || consumed.is_some() {
            bail!("flow not valid for this provider");
        }
        if chrono::DateTime::parse_from_rfc3339(&expires_at)?.with_timezone(&Utc) <= Utc::now() {
            bail!("flow expired");
        }
        let bound = browser_hash
            .map(|h| crypto::ct_eq(h.as_bytes(), crypto::bind_hash(&[browser_token]).as_bytes()))
            .unwrap_or(false);
        if !bound {
            bail!("flow is not bound to this browser");
        }
        // Single use: the first caller wins.
        let used = sqlx::query(
            "UPDATE auth_flow_state SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(state)
        .execute(pool)
        .await?;
        if used.rows_affected() != 1 {
            bail!("flow already used");
        }
        let payload: FlowPayload = crypto::open_json(key, &sealed)?;

        let d = self.discover().await?;
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", self.cfg.redirect_uri.clone()),
            ("client_id", self.cfg.client_id.clone()),
            ("code_verifier", payload.verifier.clone()),
        ];
        if let Some(secret) = &self.cfg.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let token: Value = self
            .http
            .post(&d.token_endpoint)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(
                form.iter()
                    .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
                    .collect::<Vec<_>>()
                    .join("&"),
            )
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let id_token = token
            .get("id_token")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("token response has no id_token"))?;
        let jwks: JwkSet = self
            .http
            .get(&d.jwks_uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let claims = validate_id_token(
            id_token,
            &jwks,
            &d.issuer,
            &self.cfg.client_id,
            &payload.nonce,
        )?;
        Ok((claims, payload.return_to))
    }
}

/// Validates signature (asymmetric algorithms only), `iss`, `aud`, `exp` and
/// `nonce`.
pub fn validate_id_token(
    id_token: &str,
    jwks: &JwkSet,
    issuer: &str,
    client_id: &str,
    nonce: &str,
) -> Result<IdentityClaims> {
    let header = decode_header(id_token)?;
    let alg = header.alg;
    if !matches!(
        alg,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512
            | Algorithm::ES256
            | Algorithm::ES384
            | Algorithm::EdDSA
    ) {
        bail!("unsupported signing algorithm");
    }
    let kid = header.kid.ok_or_else(|| anyhow!("id_token has no kid"))?;
    let jwk = jwks
        .find(&kid)
        .ok_or_else(|| anyhow!("signing key not found"))?;
    let key = DecodingKey::from_jwk(jwk)?;
    let mut v = Validation::new(alg);
    v.set_issuer(&[issuer.trim_end_matches('/'), issuer]);
    v.set_audience(&[client_id]);
    v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    v.leeway = 60;
    let data = decode::<Value>(id_token, &key, &v)?;
    let c = data.claims;
    if c.get("nonce").and_then(Value::as_str) != Some(nonce) {
        bail!("nonce mismatch");
    }
    let s = |k: &str| c.get(k).and_then(Value::as_str).map(String::from);
    let subject = s("sub").ok_or_else(|| anyhow!("no subject"))?;
    if subject.is_empty() {
        bail!("empty subject");
    }
    Ok(IdentityClaims {
        subject,
        email: s("email"),
        email_verified: c
            .get("email_verified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        name: s("name"),
        preferred_username: s("preferred_username"),
        amr: c
            .get("amr")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        acr: s("acr"),
        raw: c,
    })
}

/// Whether the IdP's own claims prove multi-factor authentication.
pub fn idp_asserts_mfa(claims: &IdentityClaims) -> bool {
    claims
        .amr
        .iter()
        .any(|m| matches!(m.as_str(), "mfa" | "otp" | "hwk" | "swk" | "fpt" | "sms"))
        || claims
            .acr
            .as_deref()
            .map(|a| a.contains("mfa") || a.ends_with("multi_factor") || a == "2")
            .unwrap_or(false)
}

pub enum LinkResult {
    User(Box<User>),
    Denied(&'static str),
}

/// Finds the user linked to the identity, or provisions one (JIT) when the
/// provider allows it. An unknown identity is denied otherwise; e-mail alone
/// never links an account.
pub async fn resolve_user(
    pool: &SqlitePool,
    cfg: &OidcProviderConfig,
    claims: &IdentityClaims,
) -> Result<LinkResult> {
    let auth = AuthService::new(pool.clone());
    let linked: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM user_identities WHERE provider_id = ? AND external_id = ?",
    )
    .bind(&cfg.id)
    .bind(&claims.subject)
    .fetch_optional(pool)
    .await?;

    if let Some(uid) = linked {
        let Some(user) = auth.get_user_by_id(&Uuid::parse_str(&uid)?).await? else {
            return Ok(LinkResult::Denied("linked_user_missing"));
        };
        sqlx::query("UPDATE user_identities SET last_login_at = ? WHERE provider_id = ? AND external_id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(&cfg.id)
            .bind(&claims.subject)
            .execute(pool)
            .await?;
        return Ok(LinkResult::User(Box::new(user)));
    }

    if !cfg.jit {
        return Ok(LinkResult::Denied("identity_not_provisioned"));
    }
    let Some(role) = cfg.default_role.as_deref() else {
        return Ok(LinkResult::Denied("jit_without_default_role"));
    };
    let Some(email) = claims.email.as_deref() else {
        return Ok(LinkResult::Denied("jit_requires_email"));
    };
    if !(claims.email_verified && cfg.trust_email) {
        return Ok(LinkResult::Denied("email_not_verified"));
    }
    if auth.get_user_by_email(email).await?.is_some() {
        // Never attach an SSO identity to an existing account by e-mail.
        return Ok(LinkResult::Denied("email_already_registered"));
    }
    let wanted = cfg
        .username_claim
        .as_deref()
        .and_then(|k| claims.raw.get(k).and_then(Value::as_str))
        .or(claims.preferred_username.as_deref())
        .unwrap_or_else(|| email.split('@').next().unwrap_or(email));
    let mut username = wanted.to_string();
    if auth.get_user_by_username(&username).await?.is_some() {
        username = format!("{}-{}", wanted, &Uuid::new_v4().to_string()[..8]);
    }
    let user = auth
        .create_user_with_auth_provider(
            &username,
            email,
            None,
            role,
            default_organization_uuid(),
            crate::models::AuthProvider::Local,
            None,
        )
        .await?;
    sqlx::query(
        "INSERT INTO user_identities (provider_id, external_id, user_id, created_at, last_login_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&cfg.id)
    .bind(&claims.subject)
    .bind(user.id.to_string())
    .bind(Utc::now().to_rfc3339())
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(LinkResult::User(Box::new(user)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_to_is_restricted_to_local_paths() {
        assert_eq!(safe_return_to(Some("/nodes?x=1")), "/nodes?x=1");
        assert_eq!(safe_return_to(Some("//evil.example")), "/");
        assert_eq!(safe_return_to(Some("https://evil.example")), "/");
        assert_eq!(safe_return_to(Some("/\\evil.example")), "/");
        assert_eq!(safe_return_to(Some("/a\r\nSet-Cookie: x")), "/");
        assert_eq!(safe_return_to(None), "/");
    }

    #[test]
    fn pkce_challenge_matches_rfc_7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    // A throwaway RSA key used only by these tests.
    const KEY_PEM: &str = include_str!("../../../tests/fixtures/oidc_test_key.txt");
    const KEY_N: &str = include_str!("../../../tests/fixtures/oidc_test_n.txt");
    const ISS: &str = "https://idp.example.com";

    fn jwks(kid: &str) -> JwkSet {
        serde_json::from_value(serde_json::json!({"keys": [
            {"kty": "RSA", "alg": "RS256", "use": "sig", "kid": kid, "n": KEY_N.trim(), "e": "AQAB"}
        ]}))
        .unwrap()
    }

    fn token(alg: Algorithm, kid: Option<&str>, claims: Value) -> String {
        let mut h = jsonwebtoken::Header::new(alg);
        h.kid = kid.map(String::from);
        let key = if alg == Algorithm::RS256 {
            jsonwebtoken::EncodingKey::from_rsa_pem(KEY_PEM.as_bytes()).unwrap()
        } else {
            jsonwebtoken::EncodingKey::from_secret(b"shared-secret")
        };
        jsonwebtoken::encode(&h, &claims, &key).unwrap()
    }

    fn claims(over: Value) -> Value {
        let mut c = serde_json::json!({
            "iss": ISS, "aud": "client-1", "sub": "user-42", "nonce": "n-1",
            "exp": Utc::now().timestamp() + 600, "email": "a@example.com",
            "email_verified": true, "amr": ["pwd", "otp"],
        });
        for (k, v) in over.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    }

    fn check(token: &str) -> Result<IdentityClaims> {
        validate_id_token(token, &jwks("k1"), ISS, "client-1", "n-1")
    }

    #[test]
    fn valid_id_token_is_accepted() {
        let c = check(&token(
            Algorithm::RS256,
            Some("k1"),
            claims(serde_json::json!({})),
        ))
        .unwrap();
        assert_eq!(c.subject, "user-42");
        assert!(c.email_verified);
        assert!(idp_asserts_mfa(&c));
    }

    #[test]
    fn id_token_checks_issuer_audience_nonce_expiry_and_key() {
        let t = |over, kid| token(Algorithm::RS256, kid, claims(over));
        assert!(check(&t(
            serde_json::json!({"iss": "https://evil.example"}),
            Some("k1")
        ))
        .is_err());
        assert!(check(&t(serde_json::json!({"aud": "other"}), Some("k1"))).is_err());
        assert!(check(&t(serde_json::json!({"nonce": "replayed"}), Some("k1"))).is_err());
        assert!(check(&t(
            serde_json::json!({"exp": Utc::now().timestamp() - 3600}),
            Some("k1")
        ))
        .is_err());
        assert!(check(&t(serde_json::json!({}), Some("unknown"))).is_err());
        assert!(check(&t(serde_json::json!({}), None)).is_err());
    }

    #[test]
    fn symmetric_and_unsigned_tokens_are_rejected() {
        // HS256 signed with a shared secret must never validate against the
        // JWKS (algorithm confusion).
        assert!(check(&token(
            Algorithm::HS256,
            Some("k1"),
            claims(serde_json::json!({}))
        ))
        .is_err());
        let unsigned = format!(
            "{}.{}.",
            B64URL.encode(br#"{"alg":"none","kid":"k1"}"#),
            B64URL.encode(claims(serde_json::json!({})).to_string())
        );
        assert!(check(&unsigned).is_err());
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let mut t = token(Algorithm::RS256, Some("k1"), claims(serde_json::json!({})));
        let last = t.pop().unwrap();
        t.push(if last == 'A' { 'B' } else { 'A' });
        assert!(check(&t).is_err());
    }

    #[test]
    fn idp_mfa_assertion() {
        let mk = |amr: &[&str], acr: Option<&str>| IdentityClaims {
            subject: "s".into(),
            email: None,
            email_verified: false,
            name: None,
            preferred_username: None,
            amr: amr.iter().map(|s| s.to_string()).collect(),
            acr: acr.map(String::from),
            raw: Value::Null,
        };
        assert!(idp_asserts_mfa(&mk(&["pwd", "otp"], None)));
        assert!(!idp_asserts_mfa(&mk(&["pwd"], None)));
        assert!(idp_asserts_mfa(&mk(&[], Some("urn:okta:loa:mfa"))));
    }
}
