//! Authentication pipeline (M29).
//!
//! Login is a stateful pipeline: a provider validates the primary identity, a
//! policy decides the level the session must reach, and a challenge collects
//! the missing factors before any session exists. The host decides and issues
//! the session; providers and factors only prove identity.
//!
//! * Challenges are opaque, single use, short lived and bound to the client.
//! * Every step produces a new session (no fixation); the session records the
//!   methods (`amr`) and the level (`acr`) reached.
//! * Without a policy nothing changes: login keeps returning a session.
//! * Fail closed: an unavailable factor, a malformed response or an expired
//!   challenge is a denial. The only way around a policy is the break-glass
//!   path for a local super administrator who has no usable factor, which is
//!   audited so they can enroll one.

pub mod config;
pub mod crypto;
pub mod factors;
pub mod oidc;
pub mod policy;
pub mod store;
pub mod totp;

use std::collections::HashMap;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

pub use config::{MfaConfig, OidcProviderConfig, WebAuthnConfig};
pub use factors::{AuthFactor, FactorDescriptor};

use self::factors::{
    builtin_factors, now_unix, ChallengeContext, EnrollContext, EnrollMode, FactorError,
    FactorOutcome, FactorRecord,
};
use self::policy::{ACR_MFA, ACR_PRIMARY};
use self::store::{FactorStore, StoredRecord};
use crate::{
    config::AppConfig,
    db::AuditRepository,
    middleware::auth::{
        create_access_token_with_levels, create_auth_session_with_levels, create_refresh_token,
        revoke_auth_session,
    },
    models::{AuthResponse, User},
    services::AuthService,
};

pub const AUDIT_AUTH: &str = "auth";
const CHALLENGE_TTL_MINUTES: i64 = 5;
const LOCKOUT_WINDOW_MINUTES: i64 = 15;

/// What identifies the client a challenge is bound to.
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: String,
    pub user_agent: String,
}

impl ClientInfo {
    pub fn hash(&self) -> String {
        crypto::bind_hash(&[&self.ip, &self.user_agent])
    }
}

/// A validated primary identity, from any provider.
#[derive(Debug, Clone)]
pub struct PrimaryIdentity {
    pub user: User,
    /// `local`, `saml`, or the id of an OIDC provider.
    pub provider: String,
    /// Methods the provider proved (`pwd`, `fed`, and `mfa` when asserted).
    pub amr: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ChallengeOffer {
    pub challenge_id: String,
    pub expires_in: i64,
    pub factors: Vec<FactorDescriptor>,
    pub required_acr: u8,
}

pub enum LoginOutcome {
    Session(Box<AuthResponse>),
    Challenge(ChallengeOffer),
    Denied { code: &'static str, message: String },
}

pub enum VerifyOutcome {
    Session(Box<AuthResponse>),
    Failed { attempts_left: u32 },
    Pending,
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// Unknown, expired, consumed or foreign challenge: deliberately uniform.
    #[error("The challenge is invalid or has expired")]
    InvalidChallenge,
    #[error("Too many failed attempts; try again later")]
    Locked,
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Internal(String),
}

impl From<anyhow::Error> for PipelineError {
    fn from(e: anyhow::Error) -> Self {
        PipelineError::Internal(e.to_string())
    }
}

impl From<sqlx::Error> for PipelineError {
    fn from(e: sqlx::Error) -> Self {
        PipelineError::Internal(e.to_string())
    }
}

impl From<FactorError> for PipelineError {
    fn from(e: FactorError) -> Self {
        match e {
            FactorError::Unavailable(m) => PipelineError::Unavailable(m),
            FactorError::Invalid(m) => PipelineError::Invalid(m),
            FactorError::Internal(m) => PipelineError::Internal(m),
        }
    }
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Level reached by a set of methods.
pub fn acr_of(amr: &[String]) -> u8 {
    if amr
        .iter()
        .any(|m| matches!(m.as_str(), "otp" | "hwk" | "rcv" | "mfa"))
    {
        ACR_MFA
    } else {
        ACR_PRIMARY
    }
}

#[derive(sqlx::FromRow)]
struct ChallengeRow {
    id: String,
    user_id: String,
    purpose: String,
    provider: String,
    amr: String,
    required_acr: i64,
    session_id: Option<String>,
    client_hash: String,
    active_factor: Option<String>,
    flow_state: Option<String>,
    attempts: i64,
    expires_at: String,
    consumed_at: Option<String>,
}

pub struct AuthPipeline<'a> {
    pub pool: &'a SqlitePool,
    pub config: &'a AppConfig,
    factors: Vec<Box<dyn AuthFactor>>,
}

impl<'a> AuthPipeline<'a> {
    pub fn new(pool: &'a SqlitePool, config: &'a AppConfig) -> Self {
        Self {
            pool,
            config,
            factors: builtin_factors(&config.auth.mfa),
        }
    }

    fn key(&self) -> &str {
        self.config
            .auth
            .mfa
            .factor_key
            .as_deref()
            .unwrap_or(&self.config.auth.jwt_secret)
    }

    /// Key used for SSO flow state.
    pub fn key_for_flows(&self) -> &str {
        self.key()
    }

    pub fn store(&self) -> FactorStore<'_> {
        FactorStore::new(self.pool, self.key())
    }

    pub fn factor(&self, id: &str) -> Option<&dyn AuthFactor> {
        self.factors
            .iter()
            .find(|f| f.descriptor().id == id)
            .map(|f| f.as_ref())
    }

    pub fn descriptors(&self) -> Vec<FactorDescriptor> {
        self.factors.iter().map(|f| f.descriptor()).collect()
    }

    pub async fn audit(
        &self,
        org: Uuid,
        user: Option<Uuid>,
        action: &str,
        target: &str,
        details: Value,
    ) {
        if let Err(e) = AuditRepository::new(self.pool)
            .insert(
                org,
                user,
                action,
                AUDIT_AUTH,
                Some(target),
                Some(&details),
                None,
            )
            .await
        {
            tracing::warn!("Failed to write auth audit entry '{}': {}", action, e);
        }
    }

    // ------------------------------------------------------------ sessions

    /// Creates a new session and its tokens. Always a fresh session id.
    pub async fn issue_session(
        &self,
        user: &User,
        amr: &[String],
        acr: u8,
    ) -> Result<AuthResponse, PipelineError> {
        let auth = &self.config.auth;
        let roles = AuthService::new(self.pool.clone())
            .get_user_roles(&user.id)
            .await
            .unwrap_or_else(|_| vec![user.role.clone()]);
        let session_id = Uuid::new_v4();
        let expires = Utc::now() + Duration::days(auth.refresh_token_expiry_days as i64);
        create_auth_session_with_levels(self.pool, &session_id, &user.id, expires, amr, acr)
            .await
            .map_err(|_| PipelineError::Internal("Failed to create session".into()))?;
        let access_token = create_access_token_with_levels(
            &user.id,
            &user.organization_id,
            &session_id,
            &user.username,
            &user.email,
            roles,
            &auth.jwt_secret,
            auth.token_expiry_hours,
            amr.to_vec(),
            acr,
        )
        .map_err(|e| PipelineError::Internal(format!("Failed to create access token: {e}")))?;
        let refresh_token = create_refresh_token(
            &user.id,
            &session_id,
            &user.username,
            &user.email,
            &auth.jwt_secret,
            auth.refresh_token_expiry_days,
        )
        .map_err(|e| PipelineError::Internal(format!("Failed to create refresh token: {e}")))?;
        Ok(AuthResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: auth.token_expiry_hours * 3600,
            user: user.clone().into(),
        })
    }

    /// Methods and level recorded on a session, `None` when it has no row
    /// (API keys).
    pub async fn session_levels(&self, session_id: &str) -> Option<(Vec<String>, u8)> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT amr, acr FROM auth_sessions WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(session_id)
        .fetch_optional(self.pool)
        .await
        .ok()?;
        row.map(|(amr, acr)| (serde_json::from_str(&amr).unwrap_or_default(), acr as u8))
    }

    // ------------------------------------------------------------- login

    /// Applies policy to a validated primary identity.
    pub async fn start_login(
        &self,
        ident: PrimaryIdentity,
        client: &ClientInfo,
    ) -> Result<LoginOutcome, PipelineError> {
        let user = &ident.user;
        if !self.is_active(user.id).await? {
            self.audit(
                user.organization_id,
                Some(user.id),
                "auth.login_denied",
                &user.username,
                json!({"reason": "account_deactivated", "provider": ident.provider}),
            )
            .await;
            return Ok(LoginOutcome::Denied {
                code: "account_deactivated",
                message: "This account is deactivated".into(),
            });
        }
        let roles = AuthService::new(self.pool.clone())
            .get_user_roles(&user.id)
            .await
            .unwrap_or_else(|_| vec![user.role.clone()]);
        let policies = policy::list(self.pool, user.organization_id).await?;
        let required = policy::required_acr(&policies, &roles, &ident.provider);
        let reached = acr_of(&ident.amr);

        if reached >= required {
            return Ok(LoginOutcome::Session(Box::new(
                self.issue_session(user, &ident.amr, reached).await?,
            )));
        }

        let kinds = self.store().usable_kinds(user.id).await?;
        if kinds.is_empty() {
            let break_glass = ident.provider == "local"
                && roles
                    .iter()
                    .any(|r| r == "super_admin" || r == "superadmin");
            if break_glass {
                self.audit(
                    user.organization_id,
                    Some(user.id),
                    "auth.break_glass",
                    &user.username,
                    json!({"reason": "policy requires a second factor, none enrolled", "alert": true}),
                )
                .await;
                tracing::warn!(
                    "Break-glass login for '{}': policy requires MFA but no factor is enrolled",
                    user.username
                );
                return Ok(LoginOutcome::Session(Box::new(
                    self.issue_session(user, &ident.amr, reached).await?,
                )));
            }
            self.audit(
                user.organization_id,
                Some(user.id),
                "auth.login_denied",
                &user.username,
                json!({"reason": "mfa_enrollment_required", "provider": ident.provider}),
            )
            .await;
            return Ok(LoginOutcome::Denied {
                code: "mfa_enrollment_required",
                message:
                    "Policy requires a second factor and none is enrolled; ask an administrator"
                        .into(),
            });
        }

        let offer = self
            .create_challenge(
                user,
                "login",
                &ident.provider,
                &ident.amr,
                required,
                None,
                client,
                &kinds,
            )
            .await?;
        Ok(LoginOutcome::Challenge(offer))
    }

    /// Reads the flag from the database: the `User` in hand may be stale.
    async fn is_active(&self, user: Uuid) -> Result<bool, PipelineError> {
        let deactivated: Option<Option<String>> =
            sqlx::query_scalar("SELECT deactivated_at FROM users WHERE id = ?")
                .bind(user.to_string())
                .fetch_optional(self.pool)
                .await?;
        Ok(matches!(deactivated, Some(None)))
    }

    /// Starts a step-up for an already authenticated session.
    pub async fn start_step_up(
        &self,
        user: &User,
        session_id: &str,
        client: &ClientInfo,
    ) -> Result<ChallengeOffer, PipelineError> {
        let kinds = self.store().usable_kinds(user.id).await?;
        if kinds.is_empty() {
            return Err(PipelineError::Unavailable(
                "No second factor is enrolled for this account".into(),
            ));
        }
        let (amr, _) = self
            .session_levels(session_id)
            .await
            .ok_or(PipelineError::InvalidChallenge)?;
        self.create_challenge(
            user,
            "step_up",
            "step_up",
            &amr,
            ACR_MFA,
            Some(session_id),
            client,
            &kinds,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_challenge(
        &self,
        user: &User,
        purpose: &str,
        provider: &str,
        amr: &[String],
        required: u8,
        session_id: Option<&str>,
        client: &ClientInfo,
        kinds: &[String],
    ) -> Result<ChallengeOffer, PipelineError> {
        if self.is_locked(user.id).await? {
            return Err(PipelineError::Locked);
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO auth_challenges (id, user_id, purpose, provider, amr, required_acr, session_id, client_hash, expires_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(user.id.to_string())
        .bind(purpose)
        .bind(provider)
        .bind(serde_json::to_string(amr).unwrap_or_else(|_| "[]".into()))
        .bind(required as i64)
        .bind(session_id)
        .bind(client.hash())
        .bind(ts(now + Duration::minutes(CHALLENGE_TTL_MINUTES)))
        .bind(ts(now))
        .execute(self.pool)
        .await?;
        self.audit(
            user.organization_id,
            Some(user.id),
            "auth.challenge_issued",
            &user.username,
            json!({"purpose": purpose, "provider": provider, "required_acr": required}),
        )
        .await;
        Ok(ChallengeOffer {
            challenge_id: id,
            expires_in: CHALLENGE_TTL_MINUTES * 60,
            factors: self
                .descriptors()
                .into_iter()
                .filter(|d| kinds.iter().any(|k| k == d.id))
                .collect(),
            required_acr: required,
        })
    }

    /// Failed attempts over all of a user's recent challenges.
    async fn is_locked(&self, user: Uuid) -> Result<bool, PipelineError> {
        let cutoff = ts(Utc::now() - Duration::minutes(LOCKOUT_WINDOW_MINUTES));
        let failed: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(attempts), 0) FROM auth_challenges WHERE user_id = ? AND created_at > ?",
        )
        .bind(user.to_string())
        .bind(cutoff)
        .fetch_one(self.pool)
        .await?;
        Ok(failed >= (self.config.auth.mfa.max_attempts as i64) * 2)
    }

    async fn load_challenge(
        &self,
        id: &str,
        client: &ClientInfo,
    ) -> Result<(ChallengeRow, User), PipelineError> {
        let row: ChallengeRow = sqlx::query_as(
            "SELECT id, user_id, purpose, provider, amr, required_acr, session_id, client_hash, \
             active_factor, flow_state, attempts, expires_at, consumed_at \
             FROM auth_challenges WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PipelineError::InvalidChallenge)?;
        let expired = DateTime::parse_from_rfc3339(&row.expires_at)
            .map(|e| e.with_timezone(&Utc) <= Utc::now())
            .unwrap_or(true);
        let bound = crypto::ct_eq(row.client_hash.as_bytes(), client.hash().as_bytes());
        if row.consumed_at.is_some() || expired || !bound {
            return Err(PipelineError::InvalidChallenge);
        }
        let uid = Uuid::parse_str(&row.user_id).map_err(|_| PipelineError::InvalidChallenge)?;
        let user = AuthService::new(self.pool.clone())
            .get_user_by_id(&uid)
            .await?
            .ok_or(PipelineError::InvalidChallenge)?;
        if user.deactivated_at.is_some() {
            return Err(PipelineError::InvalidChallenge);
        }
        Ok((row, user))
    }

    fn challenge_ctx(
        &self,
        user: &User,
        records: &[StoredRecord],
        flow_state: Option<String>,
    ) -> ChallengeContext {
        ChallengeContext {
            user_id: user.id,
            username: user.username.clone(),
            records: records
                .iter()
                .map(|r| FactorRecord {
                    id: r.id.clone(),
                    data: r.data.clone(),
                })
                .collect(),
            flow_state,
            now_unix: now_unix(),
        }
    }

    /// Triggers the chosen factor and returns what the client needs.
    pub async fn begin_challenge(
        &self,
        id: &str,
        factor_id: &str,
        client: &ClientInfo,
    ) -> Result<Value, PipelineError> {
        let (row, user) = self.load_challenge(id, client).await?;
        if self.is_locked(user.id).await? {
            return Err(PipelineError::Locked);
        }
        let factor = self
            .factor(factor_id)
            .ok_or_else(|| PipelineError::Invalid(format!("Unknown factor '{factor_id}'")))?;
        let kind = factor.descriptor().id;
        let records = self.store().records(user.id, kind, true).await?;
        if records.is_empty() {
            return Err(PipelineError::Invalid(
                "That factor is not enrolled for this account".into(),
            ));
        }
        let start = factor
            .begin_challenge(&self.challenge_ctx(&user, &records, None))
            .await?;
        let sealed = match &start.flow_state {
            Some(s) => Some(crypto::seal(self.key(), s.as_bytes())?),
            None => None,
        };
        sqlx::query("UPDATE auth_challenges SET active_factor = ?, flow_state = ? WHERE id = ? AND consumed_at IS NULL")
            .bind(kind)
            .bind(sealed)
            .bind(&row.id)
            .execute(self.pool)
            .await?;
        Ok(start.client)
    }

    /// Verifies the response. A challenge is single use: success burns it, and
    /// so do too many failures.
    pub async fn verify_challenge(
        &self,
        id: &str,
        factor_id: Option<&str>,
        response: Value,
        client: &ClientInfo,
    ) -> Result<VerifyOutcome, PipelineError> {
        let (row, user) = self.load_challenge(id, client).await?;
        if self.is_locked(user.id).await? {
            return Err(PipelineError::Locked);
        }
        let active = row
            .active_factor
            .clone()
            .or_else(|| factor_id.map(String::from));
        let Some(active) = active else {
            return Err(PipelineError::Invalid("Choose a factor first".into()));
        };
        if let (Some(a), Some(b)) = (&row.active_factor, factor_id) {
            if a != b {
                return Err(PipelineError::Invalid(
                    "A different factor is active".into(),
                ));
            }
        }
        let factor = self
            .factor(&active)
            .ok_or_else(|| PipelineError::Invalid(format!("Unknown factor '{active}'")))?;
        let max = self.config.auth.mfa.max_attempts as i64;

        // Count the attempt before verifying so concurrent guesses cannot
        // exceed the limit; a success gives it back.
        let bumped = sqlx::query(
            "UPDATE auth_challenges SET attempts = attempts + 1 \
             WHERE id = ? AND consumed_at IS NULL AND attempts < ?",
        )
        .bind(&row.id)
        .bind(max)
        .execute(self.pool)
        .await?;
        if bumped.rows_affected() != 1 {
            self.burn(&row.id).await?;
            self.audit(
                user.organization_id,
                Some(user.id),
                "auth.challenge_failed",
                &user.username,
                json!({"factor": active, "reason": "attempts_exhausted"}),
            )
            .await;
            return Err(PipelineError::InvalidChallenge);
        }

        let records = self.store().records(user.id, &active, true).await?;
        let flow_state = match &row.flow_state {
            Some(s) => crypto::open(self.key(), s)
                .ok()
                .and_then(|b| String::from_utf8(b).ok()),
            None => None,
        };
        let outcome = factor
            .verify(&self.challenge_ctx(&user, &records, flow_state), response)
            .await;

        let outcome = match outcome {
            Ok(o) => o,
            Err(FactorError::Invalid(_)) => FactorOutcome::Failed { retryable: true },
            // An unavailable or broken factor is a denial, never a pass.
            Err(e) => return Err(e.into()),
        };

        match outcome {
            FactorOutcome::Pending => {
                self.refund(&row.id).await?;
                Ok(VerifyOutcome::Pending)
            }
            FactorOutcome::Failed { retryable } => {
                if !retryable {
                    self.burn(&row.id).await?;
                }
                self.audit(
                    user.organization_id,
                    Some(user.id),
                    "auth.challenge_failed",
                    &user.username,
                    json!({"factor": active}),
                )
                .await;
                Ok(VerifyOutcome::Failed {
                    attempts_left: (max - row.attempts - 1).max(0) as u32,
                })
            }
            FactorOutcome::Verified { amr, updates } => {
                // Write factor state first: a replayed code or assertion loses
                // the compare-and-set and counts as a failure.
                let by_id: HashMap<&str, &StoredRecord> =
                    records.iter().map(|r| (r.id.as_str(), r)).collect();
                for (rec_id, data) in &updates {
                    let Some(rec) = by_id.get(rec_id.as_str()) else {
                        continue;
                    };
                    if !self.store().update_if_unchanged(rec, data).await? {
                        return Ok(VerifyOutcome::Failed {
                            attempts_left: (max - row.attempts - 1).max(0) as u32,
                        });
                    }
                }
                if updates.is_empty() {
                    if let Some(r) = records.first() {
                        self.store().touch(&r.id).await?;
                    }
                }
                let consumed = sqlx::query(
                    "UPDATE auth_challenges SET consumed_at = ?, attempts = MAX(attempts - 1, 0) \
                     WHERE id = ? AND consumed_at IS NULL",
                )
                .bind(ts(Utc::now()))
                .bind(&row.id)
                .execute(self.pool)
                .await?;
                if consumed.rows_affected() != 1 {
                    return Err(PipelineError::InvalidChallenge);
                }

                let mut methods: Vec<String> = serde_json::from_str(&row.amr).unwrap_or_default();
                if !methods.contains(&amr) {
                    methods.push(amr.clone());
                }
                let acr = acr_of(&methods).max(ACR_PRIMARY);
                if (acr as i64) < row.required_acr {
                    return Err(PipelineError::Internal(
                        "Factor did not reach the required level".into(),
                    ));
                }
                if row.purpose == "step_up" {
                    if let Some(old) = &row.session_id {
                        let _ = revoke_auth_session(self.pool, old).await;
                    }
                }
                let session = self.issue_session(&user, &methods, acr).await?;
                self.audit(
                    user.organization_id,
                    Some(user.id),
                    "auth.challenge_verified",
                    &user.username,
                    json!({"factor": active, "purpose": row.purpose, "provider": row.provider, "amr": methods, "acr": acr}),
                )
                .await;
                if amr == "rcv" {
                    self.audit(
                        user.organization_id,
                        Some(user.id),
                        "auth.recovery_code_used",
                        &user.username,
                        json!({"alert": true}),
                    )
                    .await;
                    tracing::warn!("Recovery code used by '{}'", user.username);
                }
                Ok(VerifyOutcome::Session(Box::new(session)))
            }
        }
    }

    async fn burn(&self, id: &str) -> Result<(), PipelineError> {
        sqlx::query(
            "UPDATE auth_challenges SET consumed_at = COALESCE(consumed_at, ?) WHERE id = ?",
        )
        .bind(ts(Utc::now()))
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    async fn refund(&self, id: &str) -> Result<(), PipelineError> {
        sqlx::query("UPDATE auth_challenges SET attempts = MAX(attempts - 1, 0) WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64, PipelineError> {
        let cutoff = ts(Utc::now() - Duration::hours(1));
        let a = sqlx::query("DELETE FROM auth_challenges WHERE expires_at < ?")
            .bind(&cutoff)
            .execute(self.pool)
            .await?
            .rows_affected();
        let b = sqlx::query("DELETE FROM auth_flow_state WHERE expires_at < ?")
            .bind(&cutoff)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(a + b)
    }

    // ---------------------------------------------------------- enrollment

    pub async fn begin_enroll(&self, user: &User, factor_id: &str) -> Result<Value, PipelineError> {
        let factor = self
            .factor(factor_id)
            .ok_or_else(|| PipelineError::Invalid(format!("Unknown factor '{factor_id}'")))?;
        let kind = factor.descriptor().id;
        let existing = self.store().records(user.id, kind, true).await?;
        let ctx = EnrollContext {
            user_id: user.id,
            username: user.username.clone(),
            existing: existing
                .iter()
                .map(|r| FactorRecord {
                    id: r.id.clone(),
                    data: r.data.clone(),
                })
                .collect(),
            pending: None,
        };
        let start = factor.begin_enroll(&ctx).await?;
        match start.mode {
            EnrollMode::Pending(data) => {
                self.store().put_pending(user.id, kind, &data).await?;
            }
            EnrollMode::Immediate(data) => {
                if kind == "recovery" {
                    self.store().replace_recovery(user.id, &data).await?;
                } else {
                    self.store().insert(user.id, kind, "", &data, true).await?;
                }
                self.audit(
                    user.organization_id,
                    Some(user.id),
                    "auth.factor_enrolled",
                    &user.username,
                    json!({"factor": kind}),
                )
                .await;
            }
        }
        Ok(start.client)
    }

    pub async fn complete_enroll(
        &self,
        user: &User,
        factor_id: &str,
        resp: Value,
    ) -> Result<(), PipelineError> {
        let factor = self
            .factor(factor_id)
            .ok_or_else(|| PipelineError::Invalid(format!("Unknown factor '{factor_id}'")))?;
        let kind = factor.descriptor().id;
        let pending = self
            .store()
            .records(user.id, kind, false)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| PipelineError::Invalid("No enrollment in progress".into()))?;
        let existing = self.store().records(user.id, kind, true).await?;
        let ctx = EnrollContext {
            user_id: user.id,
            username: user.username.clone(),
            existing: existing
                .iter()
                .map(|r| FactorRecord {
                    id: r.id.clone(),
                    data: r.data.clone(),
                })
                .collect(),
            pending: Some(FactorRecord {
                id: pending.id.clone(),
                data: pending.data.clone(),
            }),
        };
        let rec = factor.complete_enroll(&ctx, resp).await?;
        self.store()
            .finalize(&pending, rec.label.as_deref().unwrap_or(""), &rec.data)
            .await?;
        self.audit(
            user.organization_id,
            Some(user.id),
            "auth.factor_enrolled",
            &user.username,
            json!({"factor": kind}),
        )
        .await;
        Ok(())
    }
}
