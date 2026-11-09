//! Second factors. The `AuthFactor` contract is what a plugin factor will
//! implement; TOTP, recovery codes and WebAuthn are the built-ins that prove it.
//!
//! A factor verifies and the host decides: a factor never mints a session. The
//! host stores enrollment data sealed, hands it to the factor only while the
//! owner's challenge is verified, and writes any state the factor returns
//! atomically.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use webauthn_rs::prelude::*;

use super::config::{MfaConfig, WebAuthnConfig};
use super::totp;
use crate::services::AuthService;

/// What the host shows and checks for a factor. Plugin factors (H-1) carry
/// their qualified id `<plugin-id>:<factor-id>`, so the fields are owned.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FactorDescriptor {
    pub id: String,
    pub kind: String,
    pub label: String,
    /// Values added to the session's `amr` once the factor verifies (OQ-5:
    /// a method and optional qualifiers). One value is serialized as a
    /// string, as before.
    #[serde(serialize_with = "one_or_many")]
    pub amr: Vec<String>,
    /// `host-managed` (the host stores the data) or `external`.
    pub enrollment: String,
    /// What the client must supply for a challenge, so the host can render it.
    pub fields: Vec<String>,
    /// Satisfies a policy that requires phishing resistance (OQ-6).
    pub phishing_resistant: bool,
}

/// Methods that, proved through a WebAuthn ceremony, resist phishing (OQ-6).
pub const PHISHING_RESISTANT_METHODS: &[&str] = &["hwk", "swk"];

/// A factor resists phishing when the host runs a WebAuthn ceremony for it
/// (the browser binds the answer to the origin) and it proves a key method.
/// A one-time code, even from a hardware token, does not.
pub fn is_phishing_resistant(kind: &str, amr: &[String]) -> bool {
    kind == "webauthn"
        && amr
            .iter()
            .any(|m| PHISHING_RESISTANT_METHODS.contains(&m.as_str()))
}

fn one_or_many<S: serde::Serializer>(v: &[String], s: S) -> Result<S::Ok, S::Error> {
    match v {
        [one] => s.serialize_str(one),
        many => many.serialize(s),
    }
}

impl FactorDescriptor {
    fn builtin(id: &str, kind: &str, label: &str, amr: &str, fields: &[&str]) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            label: label.into(),
            amr: vec![amr.into()],
            enrollment: "host-managed".into(),
            fields: fields.iter().map(|f| f.to_string()).collect(),
            phishing_resistant: is_phishing_resistant(kind, &[amr.to_string()]),
        }
    }
}

/// What kind of failure a factor error is; it decides the HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactorErrorKind {
    Unavailable,
    Invalid,
    Internal,
}

/// Default codes of the uncoded variants (H-8/OQ-10).
pub const CODE_UNAVAILABLE: &str = "factor_unavailable";
pub const CODE_INVALID: &str = "factor_invalid";
pub const CODE_INTERNAL: &str = "factor_internal";
/// Replaces a code that is not a safe identifier.
pub const CODE_FALLBACK: &str = "plugin_error";
const MAX_CODE_LEN: usize = 64;

/// A stable code is `[a-z0-9_.-]{1,64}`: safe to log, return and use as a
/// message key.
pub fn is_safe_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= MAX_CODE_LEN
        && code.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'.' | b'-')
        })
}

#[derive(Debug, thiserror::Error)]
pub enum FactorError {
    #[error("factor unavailable: {0}")]
    Unavailable(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("{0}")]
    Internal(String),
    /// An error with a stable code (H-8/OQ-10) that clients may translate;
    /// `message` is plain text for people and logs. Plugin errors are coded.
    #[error("{message}")]
    Coded {
        kind: FactorErrorKind,
        code: String,
        message: String,
    },
}

impl FactorError {
    /// A coded error; an unsafe code becomes `plugin_error`.
    pub fn coded(kind: FactorErrorKind, code: &str, message: impl Into<String>) -> Self {
        FactorError::Coded {
            kind,
            code: if is_safe_code(code) {
                code.to_string()
            } else {
                CODE_FALLBACK.to_string()
            },
            message: message.into(),
        }
    }

    pub fn kind(&self) -> FactorErrorKind {
        match self {
            FactorError::Unavailable(_) => FactorErrorKind::Unavailable,
            FactorError::Invalid(_) => FactorErrorKind::Invalid,
            FactorError::Internal(_) => FactorErrorKind::Internal,
            FactorError::Coded { kind, .. } => *kind,
        }
    }

    /// The stable code: the coded one, or the default of the variant.
    pub fn code(&self) -> &str {
        match self {
            FactorError::Unavailable(_) => CODE_UNAVAILABLE,
            FactorError::Invalid(_) => CODE_INVALID,
            FactorError::Internal(_) => CODE_INTERNAL,
            FactorError::Coded { code, .. } => code,
        }
    }
}

fn invalid(code: &str, message: &str) -> FactorError {
    FactorError::coded(FactorErrorKind::Invalid, code, message)
}

/// One stored enrollment record, already opened by the host.
#[derive(Debug, Clone)]
pub struct FactorRecord {
    pub id: String,
    pub data: Value,
}

pub struct EnrollContext {
    pub user_id: Uuid,
    pub username: String,
    /// Verified records of this kind (used to avoid re-registering a key).
    pub existing: Vec<FactorRecord>,
    /// The pending record of an enrollment in progress, on completion.
    pub pending: Option<FactorRecord>,
    /// Host time, read once per call as for a challenge (H-3/OQ-3).
    pub now_unix: u64,
}

pub enum EnrollMode {
    /// The user must prove possession first; `pending` is stored unverified.
    Pending(Value),
    /// Nothing to prove (recovery codes); the record is stored verified.
    Immediate(Value),
}

pub struct EnrollStart {
    /// Sent to the client (QR URI, WebAuthn creation options, codes).
    pub client: Value,
    pub mode: EnrollMode,
}

pub struct EnrollRecord {
    pub data: Value,
    pub label: Option<String>,
}

/// Why a challenge runs (H-4/OQ-4): a factor may, for example, demand user
/// verification for step-up only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengePurpose {
    /// Second factor of a sign-in.
    Login,
    /// Raising an existing session before a sensitive action.
    StepUp,
}

impl ChallengePurpose {
    /// The name stored on the challenge and sent to plugin factors.
    pub fn as_str(self) -> &'static str {
        match self {
            ChallengePurpose::Login => "login",
            ChallengePurpose::StepUp => "step_up",
        }
    }

    /// Strict: any other value is not a purpose.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "login" => Some(ChallengePurpose::Login),
            "step_up" => Some(ChallengePurpose::StepUp),
            _ => None,
        }
    }
}

pub struct ChallengeContext {
    pub user_id: Uuid,
    pub username: String,
    pub records: Vec<FactorRecord>,
    /// State the factor returned from `begin_challenge`, sealed by the host.
    pub flow_state: Option<String>,
    pub now_unix: u64,
    /// Sign-in or step-up (H-4/OQ-4).
    pub purpose: ChallengePurpose,
}

pub struct ChallengeStart {
    pub client: Value,
    pub flow_state: Option<String>,
}

pub enum FactorOutcome {
    Verified {
        /// Methods proved, with optional qualifiers (OQ-5), e.g.
        /// `["hwk", "pin"]`.
        amr: Vec<String>,
        /// `(record id, new data)` written atomically by the host.
        updates: Vec<(String, Value)>,
    },
    Failed {
        retryable: bool,
    },
    /// Push-style factors that were not answered yet.
    Pending,
}

#[async_trait]
pub trait AuthFactor: Send + Sync {
    fn descriptor(&self) -> FactorDescriptor;
    async fn begin_enroll(&self, ctx: &EnrollContext) -> Result<EnrollStart, FactorError>;
    async fn complete_enroll(
        &self,
        ctx: &EnrollContext,
        resp: Value,
    ) -> Result<EnrollRecord, FactorError>;
    async fn begin_challenge(&self, ctx: &ChallengeContext) -> Result<ChallengeStart, FactorError>;
    async fn verify(
        &self,
        ctx: &ChallengeContext,
        resp: Value,
    ) -> Result<FactorOutcome, FactorError>;
}

/// Built-in factors for a configuration.
pub fn builtin_factors(cfg: &MfaConfig) -> Vec<Box<dyn AuthFactor>> {
    let mut v: Vec<Box<dyn AuthFactor>> = vec![
        Box::new(TotpFactor {
            issuer: cfg.totp_issuer.clone(),
        }),
        Box::new(RecoveryFactor),
    ];
    if let Some(w) = &cfg.webauthn {
        v.push(Box::new(WebAuthnFactor { cfg: w.clone() }));
    }
    v
}

// ---------------------------------------------------------------- TOTP

pub struct TotpFactor {
    pub issuer: String,
}

#[derive(Serialize, Deserialize)]
struct TotpData {
    secret: String,
    #[serde(default)]
    last_step: Option<u64>,
}

#[async_trait]
impl AuthFactor for TotpFactor {
    fn descriptor(&self) -> FactorDescriptor {
        FactorDescriptor::builtin("totp", "otp", "Authenticator app", "otp", &["code"])
    }

    async fn begin_enroll(&self, ctx: &EnrollContext) -> Result<EnrollStart, FactorError> {
        if !ctx.existing.is_empty() {
            return Err(invalid(
                "totp_already_enrolled",
                "An authenticator app is already enrolled; remove it first",
            ));
        }
        let secret = totp::generate_secret();
        let b32 = totp::secret_to_base32(&secret);
        Ok(EnrollStart {
            client: json!({
                "otpauth_uri": totp::provisioning_uri(&self.issuer, &ctx.username, &secret),
                "secret": b32,
            }),
            mode: EnrollMode::Pending(json!({ "secret": b32 })),
        })
    }

    async fn complete_enroll(
        &self,
        ctx: &EnrollContext,
        resp: Value,
    ) -> Result<EnrollRecord, FactorError> {
        let pending = ctx
            .pending
            .as_ref()
            .ok_or_else(|| invalid("no_enrollment_in_progress", "No enrollment in progress"))?;
        let data: TotpData = serde_json::from_value(pending.data.clone())
            .map_err(|e| FactorError::Internal(e.to_string()))?;
        let secret = totp::secret_from_base32(&data.secret)
            .ok_or_else(|| FactorError::Internal("corrupt seed".into()))?;
        let code = resp.get("code").and_then(Value::as_str).unwrap_or("");
        let step = totp::verify(&secret, code, ctx.now_unix, None)
            .ok_or_else(|| invalid("invalid_code", "The code is not valid"))?;
        Ok(EnrollRecord {
            data: json!({ "secret": data.secret, "last_step": step }),
            label: Some("Authenticator app".into()),
        })
    }

    async fn begin_challenge(
        &self,
        _ctx: &ChallengeContext,
    ) -> Result<ChallengeStart, FactorError> {
        Ok(ChallengeStart {
            client: json!({ "fields": ["code"] }),
            flow_state: None,
        })
    }

    async fn verify(
        &self,
        ctx: &ChallengeContext,
        resp: Value,
    ) -> Result<FactorOutcome, FactorError> {
        let code = resp.get("code").and_then(Value::as_str).unwrap_or("");
        for rec in &ctx.records {
            let Ok(data) = serde_json::from_value::<TotpData>(rec.data.clone()) else {
                continue;
            };
            let Some(secret) = totp::secret_from_base32(&data.secret) else {
                continue;
            };
            if let Some(step) = totp::verify(&secret, code, ctx.now_unix, data.last_step) {
                return Ok(FactorOutcome::Verified {
                    amr: vec!["otp".into()],
                    updates: vec![(
                        rec.id.clone(),
                        json!({ "secret": data.secret, "last_step": step }),
                    )],
                });
            }
        }
        Ok(FactorOutcome::Failed { retryable: true })
    }
}

// ------------------------------------------------------ Recovery codes

pub struct RecoveryFactor;

pub const RECOVERY_CODES: usize = 10;
const RECOVERY_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub fn normalize_recovery_code(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// `(shown to the user, stored)`: ten `XXXXX-XXXXX` codes and their hashes.
pub fn generate_recovery_codes() -> Result<(Vec<String>, Value), FactorError> {
    let mut shown = Vec::new();
    let mut stored = Vec::new();
    for _ in 0..RECOVERY_CODES {
        let raw: Vec<u8> = (0..10)
            .map(|_| RECOVERY_ALPHABET[rand::random::<u8>() as usize % RECOVERY_ALPHABET.len()])
            .collect();
        let raw = String::from_utf8(raw).expect("ascii alphabet");
        let hash =
            AuthService::hash_password(&raw).map_err(|e| FactorError::Internal(e.to_string()))?;
        shown.push(format!("{}-{}", &raw[..5], &raw[5..]));
        stored.push(json!({ "hash": hash, "used": false }));
    }
    Ok((shown, json!({ "codes": stored })))
}

#[async_trait]
impl AuthFactor for RecoveryFactor {
    fn descriptor(&self) -> FactorDescriptor {
        FactorDescriptor::builtin("recovery", "recovery", "Recovery code", "rcv", &["code"])
    }

    async fn begin_enroll(&self, _ctx: &EnrollContext) -> Result<EnrollStart, FactorError> {
        let (shown, stored) = tokio::task::spawn_blocking(generate_recovery_codes)
            .await
            .map_err(|e| FactorError::Internal(e.to_string()))??;
        Ok(EnrollStart {
            client: json!({ "codes": shown, "shown_once": true }),
            mode: EnrollMode::Immediate(stored),
        })
    }

    async fn complete_enroll(
        &self,
        _ctx: &EnrollContext,
        _resp: Value,
    ) -> Result<EnrollRecord, FactorError> {
        Err(invalid(
            "recovery_needs_no_confirmation",
            "Recovery codes need no confirmation",
        ))
    }

    async fn begin_challenge(
        &self,
        _ctx: &ChallengeContext,
    ) -> Result<ChallengeStart, FactorError> {
        Ok(ChallengeStart {
            client: json!({ "fields": ["code"] }),
            flow_state: None,
        })
    }

    async fn verify(
        &self,
        ctx: &ChallengeContext,
        resp: Value,
    ) -> Result<FactorOutcome, FactorError> {
        let code = normalize_recovery_code(resp.get("code").and_then(Value::as_str).unwrap_or(""));
        if code.len() != 10 {
            return Ok(FactorOutcome::Failed { retryable: true });
        }
        let records = ctx.records.clone();
        tokio::task::spawn_blocking(move || {
            for rec in records {
                let Some(codes) = rec.data.get("codes").and_then(Value::as_array) else {
                    continue;
                };
                let mut codes = codes.clone();
                for entry in codes.iter_mut() {
                    if entry.get("used").and_then(Value::as_bool).unwrap_or(true) {
                        continue;
                    }
                    let hash = entry.get("hash").and_then(Value::as_str).unwrap_or("");
                    if AuthService::verify_password(&code, hash).unwrap_or(false) {
                        entry["used"] = Value::Bool(true);
                        return FactorOutcome::Verified {
                            amr: vec!["rcv".into()],
                            updates: vec![(rec.id.clone(), json!({ "codes": codes }))],
                        };
                    }
                }
            }
            FactorOutcome::Failed { retryable: true }
        })
        .await
        .map_err(|e| FactorError::Internal(e.to_string()))
    }
}

// ------------------------------------------------------------ WebAuthn

pub struct WebAuthnFactor {
    pub cfg: WebAuthnConfig,
}

impl WebAuthnFactor {
    fn build(&self) -> Result<Webauthn, FactorError> {
        let origin = Url::parse(&self.cfg.origin)
            .map_err(|e| FactorError::Unavailable(format!("bad webauthn origin: {e}")))?;
        WebauthnBuilder::new(&self.cfg.rp_id, &origin)
            .map_err(|e| FactorError::Unavailable(format!("bad webauthn relying party: {e}")))?
            .rp_name(&self.cfg.rp_name)
            .build()
            .map_err(|e| FactorError::Unavailable(format!("webauthn: {e}")))
    }
}

fn passkeys(records: &[FactorRecord]) -> Vec<(String, Passkey)> {
    records
        .iter()
        .filter_map(|r| {
            let pk = r.data.get("passkey")?.clone();
            Some((r.id.clone(), serde_json::from_value(pk).ok()?))
        })
        .collect()
}

#[async_trait]
impl AuthFactor for WebAuthnFactor {
    fn descriptor(&self) -> FactorDescriptor {
        FactorDescriptor::builtin(
            "webauthn",
            "webauthn",
            "Security key or passkey",
            "hwk",
            &["credential"],
        )
    }

    async fn begin_enroll(&self, ctx: &EnrollContext) -> Result<EnrollStart, FactorError> {
        let w = self.build()?;
        let exclude: Vec<CredentialID> = passkeys(&ctx.existing)
            .iter()
            .map(|(_, p)| p.cred_id().clone())
            .collect();
        let (options, state) = w
            .start_passkey_registration(
                ctx.user_id,
                &ctx.username,
                &ctx.username,
                if exclude.is_empty() {
                    None
                } else {
                    Some(exclude)
                },
            )
            .map_err(|e| FactorError::Internal(e.to_string()))?;
        Ok(EnrollStart {
            client: serde_json::to_value(options)
                .map_err(|e| FactorError::Internal(e.to_string()))?,
            mode: EnrollMode::Pending(json!({
                "registration": serde_json::to_value(state)
                    .map_err(|e| FactorError::Internal(e.to_string()))?
            })),
        })
    }

    async fn complete_enroll(
        &self,
        ctx: &EnrollContext,
        resp: Value,
    ) -> Result<EnrollRecord, FactorError> {
        let w = self.build()?;
        let pending = ctx
            .pending
            .as_ref()
            .ok_or_else(|| invalid("no_enrollment_in_progress", "No enrollment in progress"))?;
        let state: PasskeyRegistration = serde_json::from_value(
            pending
                .data
                .get("registration")
                .cloned()
                .unwrap_or(Value::Null),
        )
        .map_err(|e| FactorError::Internal(e.to_string()))?;
        let cred: RegisterPublicKeyCredential =
            serde_json::from_value(resp.get("credential").cloned().unwrap_or(resp.clone()))
                .map_err(|e| {
                    invalid(
                        "malformed_credential",
                        &format!("malformed credential: {e}"),
                    )
                })?;
        let passkey = w.finish_passkey_registration(&cred, &state).map_err(|_| {
            invalid(
                "credential_not_verified",
                "The credential could not be verified",
            )
        })?;
        let label = resp
            .get("label")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("Security key")
            .to_string();
        Ok(EnrollRecord {
            data: json!({ "passkey": serde_json::to_value(passkey)
                .map_err(|e| FactorError::Internal(e.to_string()))? }),
            label: Some(label),
        })
    }

    async fn begin_challenge(&self, ctx: &ChallengeContext) -> Result<ChallengeStart, FactorError> {
        let w = self.build()?;
        let keys: Vec<Passkey> = passkeys(&ctx.records).into_iter().map(|(_, p)| p).collect();
        if keys.is_empty() {
            return Err(FactorError::coded(
                FactorErrorKind::Unavailable,
                "no_usable_credential",
                "no security key enrolled",
            ));
        }
        let (options, state) = w
            .start_passkey_authentication(&keys)
            .map_err(|e| FactorError::Internal(e.to_string()))?;
        Ok(ChallengeStart {
            client: serde_json::to_value(options)
                .map_err(|e| FactorError::Internal(e.to_string()))?,
            flow_state: Some(
                serde_json::to_string(&state).map_err(|e| FactorError::Internal(e.to_string()))?,
            ),
        })
    }

    async fn verify(
        &self,
        ctx: &ChallengeContext,
        resp: Value,
    ) -> Result<FactorOutcome, FactorError> {
        let w = self.build()?;
        let state: PasskeyAuthentication = match ctx
            .flow_state
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
        {
            Some(s) => s,
            None => return Ok(FactorOutcome::Failed { retryable: false }),
        };
        let cred: PublicKeyCredential =
            serde_json::from_value(resp.get("credential").cloned().unwrap_or(resp.clone()))
                .map_err(|e| {
                    invalid(
                        "malformed_credential",
                        &format!("malformed credential: {e}"),
                    )
                })?;
        let result = match w.finish_passkey_authentication(&cred, &state) {
            Ok(r) => r,
            Err(_) => return Ok(FactorOutcome::Failed { retryable: true }),
        };
        let mut updates = Vec::new();
        for (id, mut pk) in passkeys(&ctx.records) {
            if pk.update_credential(&result) == Some(true) {
                updates.push((
                    id,
                    json!({ "passkey": serde_json::to_value(&pk)
                        .map_err(|e| FactorError::Internal(e.to_string()))? }),
                ));
            }
        }
        Ok(FactorOutcome::Verified {
            amr: vec!["hwk".into()],
            updates,
        })
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> EnrollContext {
        EnrollContext {
            user_id: Uuid::new_v4(),
            username: "alice".into(),
            existing: vec![],
            pending: None,
            now_unix: now_unix(),
        }
    }

    /// H-3/OQ-3: enrollment reads the time from the context, like a challenge.
    #[tokio::test]
    async fn totp_enrollment_checks_the_code_at_the_context_time() {
        let f = TotpFactor {
            issuer: "OpenVox".into(),
        };
        let start = f.begin_enroll(&ctx()).await.unwrap();
        let EnrollMode::Pending(pending) = start.mode else {
            panic!("pending expected")
        };
        let secret = totp::secret_from_base32(pending["secret"].as_str().unwrap()).unwrap();
        let then = 1_000_000_000;
        let code = totp::hotp(&secret, totp::step_at(then));
        let mut c = ctx();
        c.pending = Some(FactorRecord {
            id: "p1".into(),
            data: pending,
        });
        c.now_unix = then;
        let rec = f.complete_enroll(&c, json!({"code": code})).await.unwrap();
        assert_eq!(rec.data["last_step"], totp::step_at(then));
        c.now_unix = now_unix();
        assert_eq!(
            f.complete_enroll(&c, json!({"code": code}))
                .await
                .err()
                .map(|e| e.code().to_string()),
            Some("invalid_code".to_string())
        );
    }

    #[tokio::test]
    async fn webauthn_enrollment_offers_creation_options_for_the_relying_party() {
        let f = WebAuthnFactor {
            cfg: WebAuthnConfig {
                rp_id: "puppet.example.com".into(),
                rp_name: "OpenVox".into(),
                origin: "https://puppet.example.com".into(),
            },
        };
        let start = f.begin_enroll(&ctx()).await.unwrap();
        assert_eq!(start.client["publicKey"]["rp"]["id"], "puppet.example.com");
        assert!(start.client["publicKey"]["challenge"].is_string());
        assert!(matches!(start.mode, EnrollMode::Pending(_)));
    }

    #[tokio::test]
    async fn webauthn_with_a_mismatched_origin_is_unavailable() {
        let f = WebAuthnFactor {
            cfg: WebAuthnConfig {
                rp_id: "puppet.example.com".into(),
                rp_name: "OpenVox".into(),
                origin: "https://evil.example.net".into(),
            },
        };
        assert!(matches!(
            f.begin_enroll(&ctx()).await,
            Err(FactorError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn webauthn_without_a_key_cannot_start_a_challenge() {
        let f = WebAuthnFactor {
            cfg: WebAuthnConfig {
                rp_id: "puppet.example.com".into(),
                rp_name: "OpenVox".into(),
                origin: "https://puppet.example.com".into(),
            },
        };
        let c = ChallengeContext {
            user_id: Uuid::new_v4(),
            username: "alice".into(),
            records: vec![],
            flow_state: None,
            now_unix: 0,
            purpose: ChallengePurpose::Login,
        };
        assert!(f.begin_challenge(&c).await.is_err());
    }

    /// OQ-6: only a WebAuthn ceremony with a key method is phishing-resistant.
    #[test]
    fn only_webauthn_key_factors_are_phishing_resistant() {
        let cfg = MfaConfig {
            webauthn: Some(WebAuthnConfig {
                rp_id: "puppet.example.com".into(),
                rp_name: "OpenVox".into(),
                origin: "https://puppet.example.com".into(),
            }),
            ..MfaConfig::default()
        };
        let pr: Vec<(String, bool)> = builtin_factors(&cfg)
            .iter()
            .map(|f| (f.descriptor().id, f.descriptor().phishing_resistant))
            .collect();
        assert_eq!(
            pr,
            [
                ("totp".to_string(), false),
                ("recovery".to_string(), false),
                ("webauthn".to_string(), true)
            ]
        );
        let amr = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_phishing_resistant("webauthn", &amr(&["swk", "pin"])));
        assert!(!is_phishing_resistant("webauthn", &amr(&["otp"])));
        assert!(!is_phishing_resistant("otp", &amr(&["hwk"])));
        assert!(!is_phishing_resistant("push", &amr(&["hwk"])));
    }

    /// H-8/OQ-10: every factor error has a stable code.
    #[tokio::test]
    async fn factor_errors_carry_stable_codes() {
        assert_eq!(
            FactorError::Unavailable("x".into()).code(),
            "factor_unavailable"
        );
        assert_eq!(FactorError::Invalid("x".into()).code(), "factor_invalid");
        assert_eq!(FactorError::Internal("x".into()).code(), "factor_internal");
        let e = FactorError::coded(
            FactorErrorKind::Invalid,
            "invalid_code",
            "The code is not valid",
        );
        assert_eq!(
            (e.kind(), e.code()),
            (FactorErrorKind::Invalid, "invalid_code")
        );
        assert_eq!(e.to_string(), "The code is not valid");
        // An unsafe code is replaced, never passed on.
        assert_eq!(
            FactorError::coded(FactorErrorKind::Invalid, "Bad Code", "m").code(),
            "plugin_error"
        );
        // Built-in factors use codes for what a user can act on.
        let totp = TotpFactor {
            issuer: "OpenVox".into(),
        };
        let mut c = ctx();
        c.existing = vec![FactorRecord {
            id: "t".into(),
            data: json!({}),
        }];
        assert_eq!(
            totp.begin_enroll(&c).await.err().unwrap().code(),
            "totp_already_enrolled"
        );
        assert_eq!(
            totp.complete_enroll(&ctx(), json!({}))
                .await
                .err()
                .unwrap()
                .code(),
            "no_enrollment_in_progress"
        );
    }

    #[test]
    fn challenge_purposes_have_stable_names() {
        assert_eq!(ChallengePurpose::Login.as_str(), "login");
        assert_eq!(ChallengePurpose::StepUp.as_str(), "step_up");
        assert_eq!(
            ChallengePurpose::parse("step_up"),
            Some(ChallengePurpose::StepUp)
        );
        assert_eq!(
            ChallengePurpose::parse("login"),
            Some(ChallengePurpose::Login)
        );
        assert_eq!(ChallengePurpose::parse("Login"), None);
        assert_eq!(ChallengePurpose::parse(""), None);
    }

    #[test]
    fn recovery_codes_normalize_and_hash() {
        assert_eq!(normalize_recovery_code(" ab-cde fg12-3 "), "ABCDEFG123");
        let (shown, stored) = generate_recovery_codes().unwrap();
        assert_eq!(shown.len(), RECOVERY_CODES);
        let first = normalize_recovery_code(&shown[0]);
        let hash = stored["codes"][0]["hash"].as_str().unwrap();
        assert!(AuthService::verify_password(&first, hash).unwrap());
        assert!(!stored.to_string().contains(&first));
    }
}
