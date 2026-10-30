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

#[derive(Debug, Clone, Serialize)]
pub struct FactorDescriptor {
    pub id: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
    /// Value added to the session's `amr` once the factor verifies.
    pub amr: &'static str,
    /// `host-managed` (the host stores the data) or `external`.
    pub enrollment: &'static str,
    /// What the client must supply for a challenge, so the host can render it.
    pub fields: Vec<&'static str>,
}

#[derive(Debug, thiserror::Error)]
pub enum FactorError {
    #[error("factor unavailable: {0}")]
    Unavailable(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("{0}")]
    Internal(String),
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

pub struct ChallengeContext {
    pub user_id: Uuid,
    pub username: String,
    pub records: Vec<FactorRecord>,
    /// State the factor returned from `begin_challenge`, sealed by the host.
    pub flow_state: Option<String>,
    pub now_unix: u64,
}

pub struct ChallengeStart {
    pub client: Value,
    pub flow_state: Option<String>,
}

pub enum FactorOutcome {
    Verified {
        amr: String,
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
        FactorDescriptor {
            id: "totp",
            kind: "otp",
            label: "Authenticator app",
            amr: "otp",
            enrollment: "host-managed",
            fields: vec!["code"],
        }
    }

    async fn begin_enroll(&self, ctx: &EnrollContext) -> Result<EnrollStart, FactorError> {
        if !ctx.existing.is_empty() {
            return Err(FactorError::Invalid(
                "An authenticator app is already enrolled; remove it first".into(),
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
            .ok_or_else(|| FactorError::Invalid("No enrollment in progress".into()))?;
        let data: TotpData = serde_json::from_value(pending.data.clone())
            .map_err(|e| FactorError::Internal(e.to_string()))?;
        let secret = totp::secret_from_base32(&data.secret)
            .ok_or_else(|| FactorError::Internal("corrupt seed".into()))?;
        let code = resp.get("code").and_then(Value::as_str).unwrap_or("");
        let now = now_unix();
        let step = totp::verify(&secret, code, now, None)
            .ok_or_else(|| FactorError::Invalid("The code is not valid".into()))?;
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
                    amr: "otp".into(),
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
        FactorDescriptor {
            id: "recovery",
            kind: "recovery",
            label: "Recovery code",
            amr: "rcv",
            enrollment: "host-managed",
            fields: vec!["code"],
        }
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
        Err(FactorError::Invalid(
            "Recovery codes need no confirmation".into(),
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
                            amr: "rcv".into(),
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
        FactorDescriptor {
            id: "webauthn",
            kind: "webauthn",
            label: "Security key or passkey",
            amr: "hwk",
            enrollment: "host-managed",
            fields: vec!["credential"],
        }
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
            .ok_or_else(|| FactorError::Invalid("No enrollment in progress".into()))?;
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
                .map_err(|e| FactorError::Invalid(format!("malformed credential: {e}")))?;
        let passkey = w
            .finish_passkey_registration(&cred, &state)
            .map_err(|_| FactorError::Invalid("The credential could not be verified".into()))?;
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
            return Err(FactorError::Unavailable("no security key enrolled".into()));
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
                .map_err(|e| FactorError::Invalid(format!("malformed credential: {e}")))?;
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
            amr: "hwk".into(),
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
        }
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
        };
        assert!(f.begin_challenge(&c).await.is_err());
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
