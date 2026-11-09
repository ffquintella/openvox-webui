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
//! * Enrollment grace (OQ-12): during a window an administrator opened, a
//!   user with nothing enrolled gets an enrollment-only session (level 1, no
//!   refresh, minutes long) instead of a denial; the first enrolled factor
//!   closes the window and ends that session.

pub mod config;
pub mod crypto;
pub mod factors;
pub mod grace;
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
    builtin_factors, now_unix, ChallengeContext, ChallengePurpose, EnrollContext, EnrollMode,
    FactorError, FactorErrorKind, FactorOutcome, FactorRecord,
};
use self::grace::{GraceStore, GraceWindow, LoginSpend};
use self::policy::{ACR_MFA, ACR_PHISHING_RESISTANT, ACR_PRIMARY};
use self::store::{FactorStore, StoredRecord};
use crate::{
    config::AppConfig,
    db::AuditRepository,
    middleware::auth::{
        create_access_token_expiring_at, create_access_token_with_levels,
        create_auth_session_scoped, create_auth_session_with_levels, create_refresh_token,
        revoke_auth_session, SessionScope, SCOPE_ENROLLMENT,
    },
    models::{AuthResponse, EventActor, User},
    services::{plugins::events::EventBus, AuthService},
};

pub const AUDIT_AUTH: &str = "auth";
/// Core events of the catalog (PLUGIN_PLATFORM.md section 9.1).
pub const EVENT_FACTOR_ENROLLED: &str = "auth.factor.enrolled";
pub const EVENT_FACTOR_REMOVED: &str = "auth.factor.removed";
/// Code of a refusal because a policy requires a phishing-resistant factor
/// and the user has none that can run (OQ-6).
pub const PHISHING_RESISTANT_REQUIRED: &str = "phishing_resistant_factor_required";
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

/// `Session` also carries enrollment-only sessions, marked by
/// `AuthResponse::session_scope` (OQ-12).
pub enum LoginOutcome {
    Session(Box<AuthResponse>),
    Challenge(ChallengeOffer),
    Denied { code: &'static str, message: String },
}

/// What a session row records.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub amr: Vec<String>,
    pub acr: u8,
    pub scope: SessionScope,
    /// When the session was issued. Every verification issues a new session
    /// and a refresh keeps the row, so this is the time of the last
    /// verification (OQ-13); `None` when unreadable.
    pub created_at: Option<DateTime<Utc>>,
}

/// Result of looking for an enrollment grace window at login.
enum GraceDecision {
    Session(Box<AuthResponse>),
    /// No window, or the feature is disabled.
    None,
    Expired,
    Exhausted,
    /// A window is open but this login path cannot carry an enrollment-only
    /// session (single sign-on redirects).
    NotOffered,
}

/// What step-up decides for a session (`api::auth_pipeline::require_step_up`).
#[derive(Debug, PartialEq, Eq)]
pub enum StepUpCheck {
    Passed,
    /// An enrollment-only session never confirms anything (OQ-12).
    EnrollmentOnly,
    /// The session must be raised to `required_acr` first; `stale` when it
    /// holds the level but its verification is older than
    /// `auth.step_up.max_age_secs` (OQ-13).
    Required {
        required_acr: u8,
        stale: bool,
    },
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
    /// A refusal with a stable code (403).
    #[error("{message}")]
    Forbidden { code: &'static str, message: String },
    /// A coded factor error (H-8/OQ-10), passed on with its code.
    #[error("{message}")]
    Factor {
        kind: FactorErrorKind,
        code: String,
        message: String,
    },
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
            FactorError::Coded {
                kind,
                code,
                message,
            } => PipelineError::Factor {
                kind,
                code,
                message,
            },
        }
    }
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Methods that reach level 2 (OQ-5 adds `swk`). Qualifiers such as `pin`
/// never do on their own.
pub const MULTI_FACTOR_METHODS: &[&str] = &["otp", "hwk", "swk", "rcv", "mfa"];

/// Level reached by a set of methods.
pub fn acr_of(amr: &[String]) -> u8 {
    if amr
        .iter()
        .any(|m| MULTI_FACTOR_METHODS.contains(&m.as_str()))
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
    /// A pipeline with the built-in factors only.
    pub fn new(pool: &'a SqlitePool, config: &'a AppConfig) -> Self {
        Self {
            pool,
            config,
            factors: builtin_factors(&config.auth.mfa),
        }
    }

    /// The pipeline of a request: built-in factors plus the factors of the
    /// enabled plugins (H-1).
    pub fn for_state(state: &'a crate::AppState) -> Self {
        Self::new(&state.db, &state.config).with_plugin_factors(state.plugin_manager.auth_factors())
    }

    /// Adds plugin factors. Only qualified ids (`<plugin-id>:<factor-id>`)
    /// that are not taken yet are kept, so a plugin factor can never stand
    /// in for a built-in factor or for another plugin's factor.
    pub fn with_plugin_factors(mut self, extra: Vec<Box<dyn AuthFactor>>) -> Self {
        for f in extra {
            let id = f.descriptor().id;
            let qualified = id
                .split_once(':')
                .is_some_and(|(p, f)| !p.is_empty() && !f.is_empty() && !f.contains(':'));
            if !qualified || self.factor(&id).is_some() {
                tracing::warn!("ignoring plugin factor '{}': id not qualified or taken", id);
                continue;
            }
            self.factors.push(f);
        }
        self
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

    /// The enrolled kinds whose factor can run now. Records of a factor that
    /// is not registered (a disabled plugin, an unconfigured built-in) are
    /// kept but neither offered nor counted as usable.
    pub fn available(&self, enrolled: &[String]) -> Vec<String> {
        enrolled
            .iter()
            .filter(|k| self.factor(k).is_some())
            .cloned()
            .collect()
    }

    /// The kinds that can satisfy `required`: all of them, or only the
    /// phishing-resistant ones at level 3 (OQ-6).
    fn qualifying(&self, kinds: &[String], required: u8) -> Vec<String> {
        kinds
            .iter()
            .filter(|k| {
                required < ACR_PHISHING_RESISTANT
                    || self
                        .factor(k)
                        .is_some_and(|f| f.descriptor().phishing_resistant)
            })
            .cloned()
            .collect()
    }

    /// Level a step-up must reach: 2, or 3 when a policy that matches the
    /// user's roles requires phishing resistance (any provider condition, as
    /// the session does not record its provider).
    pub async fn step_up_level(&self, user: Uuid, org: Uuid) -> Result<u8, PipelineError> {
        let roles = AuthService::new(self.pool.clone())
            .get_user_roles(&user)
            .await
            .unwrap_or_default();
        let policies = policy::list(self.pool, org).await?;
        Ok(policy::required_acr_for_roles(&policies, &roles).max(ACR_MFA))
    }

    /// Whether the user's policies require a phishing-resistant factor.
    pub async fn requires_phishing_resistant(&self, user: &User) -> Result<bool, PipelineError> {
        Ok(self.step_up_level(user.id, user.organization_id).await? >= ACR_PHISHING_RESISTANT)
    }

    /// In an enrollment-only session, a user whose policies require
    /// phishing resistance may only enroll a factor that provides it, so the
    /// window is not spent on a factor that cannot sign them in (OQ-6).
    pub async fn check_enrollment_only_factor(
        &self,
        user: &User,
        factor_id: &str,
    ) -> Result<(), PipelineError> {
        let qualifies = self
            .factor(factor_id)
            .is_some_and(|f| f.descriptor().phishing_resistant);
        if !qualifies && self.requires_phishing_resistant(user).await? {
            return Err(PipelineError::Forbidden {
                code: PHISHING_RESISTANT_REQUIRED,
                message: "Your account requires a phishing-resistant factor; add a security key \
                          or passkey"
                    .into(),
            });
        }
        Ok(())
    }

    /// The step-up gate. A user with no usable second factor passes (MFA
    /// never locks out an account that has not enrolled); otherwise the
    /// session must hold the step-up level.
    pub async fn step_up_check(
        &self,
        user: Uuid,
        org: Uuid,
        session_id: &str,
    ) -> Result<StepUpCheck, PipelineError> {
        let session = self.session_info(session_id).await;
        if session
            .as_ref()
            .is_some_and(|s| s.scope != SessionScope::Full)
        {
            return Ok(StepUpCheck::EnrollmentOnly);
        }
        if self.store().usable_kinds(user).await?.is_empty() {
            return Ok(StepUpCheck::Passed);
        }
        let required = self.step_up_level(user, org).await?;
        Ok(match session {
            Some(s) if s.acr >= required => {
                // OQ-13: an unreadable time counts as stale (fail closed).
                let stale = self
                    .config
                    .auth
                    .step_up
                    .max_age()
                    .is_some_and(|max| s.created_at.is_none_or(|t| Utc::now() - t > max));
                if stale {
                    StepUpCheck::Required {
                        required_acr: required,
                        stale: true,
                    }
                } else {
                    StepUpCheck::Passed
                }
            }
            _ => StepUpCheck::Required {
                required_acr: required,
                stale: false,
            },
        })
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
            session_scope: None,
        })
    }

    /// An enrollment-only session (OQ-12): level 1 whatever the methods, no
    /// roles in the token, no refresh token, and at most `SESSION_MINUTES`
    /// long and never past the window. It replaces the user's previous
    /// enrollment-only session.
    async fn issue_enrollment_session(
        &self,
        user: &User,
        amr: &[String],
        window_expiry: DateTime<Utc>,
    ) -> Result<AuthResponse, PipelineError> {
        self.revoke_enrollment_sessions(user.id).await?;
        let now = Utc::now();
        let expires = grace::session_expiry(window_expiry, now);
        let session_id = Uuid::new_v4();
        create_auth_session_scoped(
            self.pool,
            &session_id,
            &user.id,
            expires,
            amr,
            ACR_PRIMARY,
            SessionScope::Enrollment,
        )
        .await
        .map_err(|_| PipelineError::Internal("Failed to create session".into()))?;
        let access_token = create_access_token_expiring_at(
            &user.id,
            &user.organization_id,
            &session_id,
            &user.username,
            &user.email,
            Vec::new(),
            &self.config.auth.jwt_secret,
            expires,
            amr.to_vec(),
            ACR_PRIMARY,
        )
        .map_err(|e| PipelineError::Internal(format!("Failed to create access token: {e}")))?;
        Ok(AuthResponse {
            access_token,
            refresh_token: String::new(),
            token_type: "Bearer".to_string(),
            expires_in: (expires - now).num_seconds().max(0) as u64,
            user: user.clone().into(),
            session_scope: Some(SCOPE_ENROLLMENT.to_string()),
        })
    }

    /// Revokes every live enrollment-only session of a user.
    async fn revoke_enrollment_sessions(&self, user: Uuid) -> Result<u64, PipelineError> {
        let now = Utc::now().to_rfc3339();
        let r = sqlx::query(
            "UPDATE auth_sessions SET revoked_at = ?1, updated_at = ?1 \
             WHERE user_id = ?2 AND scope = ?3 AND revoked_at IS NULL",
        )
        .bind(&now)
        .bind(user.to_string())
        .bind(SCOPE_ENROLLMENT)
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    /// Methods, level and scope recorded on a live session, `None` when it
    /// has no row (API keys). Any scope but none reads as enrollment-only.
    pub async fn session_info(&self, session_id: &str) -> Option<SessionInfo> {
        let row: Option<(String, i64, Option<String>, String)> = sqlx::query_as(
            "SELECT amr, acr, scope, created_at FROM auth_sessions WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(session_id)
        .fetch_optional(self.pool)
        .await
        .ok()?;
        row.map(|(amr, acr, scope, created_at)| SessionInfo {
            amr: serde_json::from_str(&amr).unwrap_or_default(),
            acr: acr as u8,
            scope: if scope.is_none() {
                SessionScope::Full
            } else {
                SessionScope::Enrollment
            },
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .ok()
                .map(|t| t.with_timezone(&Utc)),
        })
    }

    /// Methods and level recorded on a session, `None` when it has no row
    /// (API keys).
    pub async fn session_levels(&self, session_id: &str) -> Option<(Vec<String>, u8)> {
        self.session_info(session_id)
            .await
            .map(|info| (info.amr, info.acr))
    }

    // ------------------------------------------------------------- login

    /// Applies policy to a validated primary identity. Never opens an
    /// enrollment-only session: SSO redirects and other callers that cannot
    /// carry one keep the denial.
    pub async fn start_login(
        &self,
        ident: PrimaryIdentity,
        client: &ClientInfo,
    ) -> Result<LoginOutcome, PipelineError> {
        self.login(ident, client, false).await
    }

    /// `start_login` for the password login, whose client can carry an
    /// enrollment-only session during an enrollment grace window (OQ-12).
    pub async fn start_login_allowing_enrollment(
        &self,
        ident: PrimaryIdentity,
        client: &ClientInfo,
    ) -> Result<LoginOutcome, PipelineError> {
        self.login(ident, client, true).await
    }

    async fn login(
        &self,
        ident: PrimaryIdentity,
        client: &ClientInfo,
        allow_enrollment: bool,
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

        let enrolled = self.store().usable_kinds(user.id).await?;
        if enrolled.is_empty() {
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
            return self
                .enrollment_path(
                    &ident,
                    allow_enrollment,
                    "mfa_enrollment_required",
                    "Policy requires a second factor and none is enrolled; ask an administrator",
                )
                .await;
        }
        // Enrolled but none of the factors can run (a disabled or failed
        // plugin): a denial, never the break-glass path, which is only for
        // accounts with nothing enrolled.
        let kinds = self.available(&enrolled);
        if kinds.is_empty() {
            self.audit(
                user.organization_id,
                Some(user.id),
                "auth.login_denied",
                &user.username,
                json!({"reason": "factor_unavailable", "provider": ident.provider, "enrolled": enrolled}),
            )
            .await;
            return Ok(LoginOutcome::Denied {
                code: "factor_unavailable",
                message: "None of the second factors enrolled for this account is available; \
                          use a recovery code or ask an administrator"
                    .into(),
            });
        }

        // OQ-6: only phishing-resistant factors are offered at level 3. A
        // user who has none follows the enrollment path; there is never a
        // fallback to a weaker factor.
        let offered = self.qualifying(&kinds, required);
        if offered.is_empty() {
            if kinds.len() < enrolled.len() {
                // An unavailable factor may be the qualifying one: fail
                // closed rather than open an enrollment window.
                self.audit(
                    user.organization_id,
                    Some(user.id),
                    "auth.login_denied",
                    &user.username,
                    json!({"reason": "factor_unavailable", "provider": ident.provider, "enrolled": enrolled, "required_acr": required}),
                )
                .await;
                return Ok(LoginOutcome::Denied {
                    code: "factor_unavailable",
                    message: "None of the second factors this policy accepts is available; \
                              ask an administrator"
                        .into(),
                });
            }
            return self
                .enrollment_path(
                    &ident,
                    allow_enrollment,
                    PHISHING_RESISTANT_REQUIRED,
                    "Policy requires a phishing-resistant factor such as a security key or a \
                     passkey, and none is enrolled; ask an administrator",
                )
                .await;
        }

        let offer = self
            .create_challenge(
                user,
                ChallengePurpose::Login.as_str(),
                &ident.provider,
                &ident.amr,
                required,
                None,
                client,
                &offered,
            )
            .await?;
        Ok(LoginOutcome::Challenge(offer))
    }

    /// A user stopped by a policy without a factor that satisfies it: an
    /// enrollment-only session during a grace window (OQ-12), else a denial
    /// with `code`.
    async fn enrollment_path(
        &self,
        ident: &PrimaryIdentity,
        allow_enrollment: bool,
        code: &'static str,
        message: &str,
    ) -> Result<LoginOutcome, PipelineError> {
        let user = &ident.user;
        let grace = self.enrollment_grace_login(ident, allow_enrollment).await?;
        let grace_label = match grace {
            GraceDecision::Session(session) => return Ok(LoginOutcome::Session(session)),
            GraceDecision::None => None,
            GraceDecision::Expired => Some("expired"),
            GraceDecision::Exhausted => Some("exhausted"),
            GraceDecision::NotOffered => Some("not_offered_for_provider"),
        };
        let mut details = json!({"reason": code, "provider": ident.provider});
        if let Some(label) = grace_label {
            details["enrollment_grace"] = json!(label);
        }
        self.audit(
            user.organization_id,
            Some(user.id),
            "auth.login_denied",
            &user.username,
            details,
        )
        .await;
        Ok(LoginOutcome::Denied {
            code,
            message: match grace_label {
                Some("expired") | Some("exhausted") => {
                    "Policy requires a second factor and the time to enroll one has ended; \
                     ask an administrator"
                        .to_string()
                }
                _ => message.to_string(),
            },
        })
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

    // ------------------------------------------------- enrollment grace

    /// A user with nothing enrolled was stopped by a policy: use their open
    /// grace window, or grant the one policy window of the account when
    /// configured. Each login spends one use of the window.
    async fn enrollment_grace_login(
        &self,
        ident: &PrimaryIdentity,
        allow_enrollment: bool,
    ) -> Result<GraceDecision, PipelineError> {
        let user = &ident.user;
        let mfa = &self.config.auth.mfa;
        let Some(ttl) = mfa.enrollment_grace() else {
            return Ok(GraceDecision::None);
        };
        let store = GraceStore::new(self.pool);
        let open = store.open_window(user.id).await?;
        if !allow_enrollment {
            return Ok(match open {
                Some(_) => GraceDecision::NotOffered,
                None => GraceDecision::None,
            });
        }
        let window = match open {
            Some(w) => w,
            None if mfa.enrollment_grace_on_policy => {
                match store.grant_policy_once(user.id, ttl).await? {
                    Some(w) => {
                        self.audit_grant(user, None, &w, None).await;
                        w
                    }
                    None => return Ok(GraceDecision::None),
                }
            }
            None => return Ok(GraceDecision::None),
        };
        match store.spend_login(&window).await? {
            LoginSpend::Granted(window_expiry) => {
                let session = self
                    .issue_enrollment_session(user, &ident.amr, window_expiry)
                    .await?;
                self.audit(
                    user.organization_id,
                    Some(user.id),
                    "auth.enrollment_grace_login",
                    &user.username,
                    json!({
                        "window": window.id,
                        "reason": window.reason,
                        "provider": ident.provider,
                        "logins": window.logins + 1,
                        "session_expires_in": session.expires_in,
                        "window_expires_at": window.expires_at,
                    }),
                )
                .await;
                Ok(GraceDecision::Session(Box::new(session)))
            }
            LoginSpend::Expired => {
                if store.close(&window.id, grace::CLOSED_EXPIRED).await? {
                    self.audit(
                        user.organization_id,
                        Some(user.id),
                        "auth.enrollment_grace_expired",
                        &user.username,
                        json!({"window": window.id, "reason": window.reason, "expires_at": window.expires_at}),
                    )
                    .await;
                }
                Ok(GraceDecision::Expired)
            }
            LoginSpend::Exhausted => {
                if store.close(&window.id, grace::CLOSED_EXHAUSTED).await? {
                    self.audit(
                        user.organization_id,
                        Some(user.id),
                        "auth.enrollment_grace_exhausted",
                        &user.username,
                        json!({"window": window.id, "reason": window.reason, "logins": grace::MAX_LOGINS, "alert": true}),
                    )
                    .await;
                    tracing::warn!(
                        "Enrollment grace window of '{}' closed after {} logins without an enrollment",
                        user.username,
                        grace::MAX_LOGINS
                    );
                }
                Ok(GraceDecision::Exhausted)
            }
            LoginSpend::Closed => Ok(GraceDecision::None),
        }
    }

    async fn audit_grant(
        &self,
        target: &User,
        actor: Option<Uuid>,
        window: &GraceWindow,
        superseded: Option<String>,
    ) {
        self.audit(
            target.organization_id,
            actor,
            "auth.enrollment_grace_granted",
            &target.username,
            json!({
                "window": window.id,
                "target": target.id,
                "reason": window.reason,
                "actor": actor.map(|a| a.to_string()).unwrap_or_else(|| "system".into()),
                "expires_at": window.expires_at,
                "ttl_secs": self.config.auth.mfa.enrollment_grace_secs,
                "superseded": superseded,
            }),
        )
        .await;
    }

    /// After an administrator reset a user's factors: opens a grace window
    /// (replacing an open one) and ends the user's enrollment-only sessions.
    /// `None` when the feature is disabled.
    pub async fn grant_enrollment_grace(
        &self,
        target: &User,
        actor: Uuid,
    ) -> Result<Option<GraceWindow>, PipelineError> {
        let Some(ttl) = self.config.auth.mfa.enrollment_grace() else {
            return Ok(None);
        };
        self.revoke_enrollment_sessions(target.id).await?;
        let (window, superseded) = GraceStore::new(self.pool)
            .grant_after_reset(target.id, actor, ttl)
            .await?;
        self.audit_grant(target, Some(actor), &window, superseded)
            .await;
        Ok(Some(window))
    }

    /// The first enrolled factor closes the user's grace window and ends
    /// their enrollment-only sessions: the client signs in again through the
    /// normal challenge to get a full session.
    async fn after_enrolled(&self, user: &User, kind: &str) -> Result<(), PipelineError> {
        let window = GraceStore::new(self.pool)
            .close_open(user.id, grace::CLOSED_ENROLLED)
            .await?;
        let revoked = self.revoke_enrollment_sessions(user.id).await?;
        if window.is_some() || revoked > 0 {
            self.audit(
                user.organization_id,
                Some(user.id),
                "auth.enrollment_grace_consumed",
                &user.username,
                json!({
                    "window": window.as_ref().map(|w| w.id.clone()),
                    "reason": window.as_ref().map(|w| w.reason.clone()),
                    "factor": kind,
                    "sessions_revoked": revoked,
                }),
            )
            .await;
        }
        Ok(())
    }

    /// Starts a step-up for an already authenticated session. An
    /// enrollment-only session can never be raised.
    pub async fn start_step_up(
        &self,
        user: &User,
        session_id: &str,
        client: &ClientInfo,
    ) -> Result<ChallengeOffer, PipelineError> {
        let enrolled = self.store().usable_kinds(user.id).await?;
        if enrolled.is_empty() {
            return Err(PipelineError::Unavailable(
                "No second factor is enrolled for this account".into(),
            ));
        }
        let kinds = self.available(&enrolled);
        if kinds.is_empty() {
            return Err(PipelineError::Unavailable(
                "None of the second factors enrolled for this account is available".into(),
            ));
        }
        let info = self
            .session_info(session_id)
            .await
            .ok_or(PipelineError::InvalidChallenge)?;
        if info.scope != SessionScope::Full {
            return Err(PipelineError::InvalidChallenge);
        }
        let required = self.step_up_level(user.id, user.organization_id).await?;
        let offered = self.qualifying(&kinds, required);
        if offered.is_empty() {
            if kinds.len() < enrolled.len() {
                return Err(PipelineError::Unavailable(
                    "None of the second factors this policy accepts is available".into(),
                ));
            }
            return Err(PipelineError::Forbidden {
                code: PHISHING_RESISTANT_REQUIRED,
                message: "Your account requires a phishing-resistant factor such as a security \
                          key or a passkey; add one first"
                    .into(),
            });
        }
        self.create_challenge(
            user,
            ChallengePurpose::StepUp.as_str(),
            "step_up",
            &info.amr,
            required,
            Some(session_id),
            client,
            &offered,
        )
        .await
    }

    /// A factor that cannot reach the challenge's level is refused before it
    /// runs (OQ-6), without spending an attempt.
    fn check_factor_level(
        &self,
        row: &ChallengeRow,
        factor: &dyn AuthFactor,
    ) -> Result<(), PipelineError> {
        if row.required_acr >= ACR_PHISHING_RESISTANT as i64
            && !factor.descriptor().phishing_resistant
        {
            return Err(PipelineError::Invalid(
                "That factor does not meet the policy; use a security key or a passkey".into(),
            ));
        }
        Ok(())
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
                .filter(|d| kinds.contains(&d.id))
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

    /// The purpose stored on a challenge row. The host writes only the two
    /// known names, so anything else is refused (fail closed).
    fn purpose_of(row: &ChallengeRow) -> Result<ChallengePurpose, PipelineError> {
        ChallengePurpose::parse(&row.purpose).ok_or(PipelineError::InvalidChallenge)
    }

    fn challenge_ctx(
        &self,
        user: &User,
        records: &[StoredRecord],
        flow_state: Option<String>,
        purpose: ChallengePurpose,
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
            purpose,
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
        self.check_factor_level(&row, factor)?;
        let kind = factor.descriptor().id;
        let purpose = Self::purpose_of(&row)?;
        let records = self.store().records(user.id, &kind, true).await?;
        if records.is_empty() {
            return Err(PipelineError::Invalid(
                "That factor is not enrolled for this account".into(),
            ));
        }
        let start = factor
            .begin_challenge(&self.challenge_ctx(&user, &records, None, purpose))
            .await?;
        let sealed = match &start.flow_state {
            Some(s) => Some(crypto::seal(self.key(), s.as_bytes())?),
            None => None,
        };
        sqlx::query("UPDATE auth_challenges SET active_factor = ?, flow_state = ? WHERE id = ? AND consumed_at IS NULL")
            .bind(&kind)
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
        self.check_factor_level(&row, factor)?;
        let purpose = Self::purpose_of(&row)?;
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
            .verify(
                &self.challenge_ctx(&user, &records, flow_state, purpose),
                response,
            )
            .await;

        let outcome = match outcome {
            Ok(o) => o,
            Err(e) if e.kind() == FactorErrorKind::Invalid => {
                FactorOutcome::Failed { retryable: true }
            }
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
                for m in &amr {
                    if !methods.contains(m) {
                        methods.push(m.clone());
                    }
                }
                let mut acr = acr_of(&methods).max(ACR_PRIMARY);
                // OQ-6: level 3 is recorded only when the challenge asked for
                // it (sessions without such a policy keep level 2, as before)
                // and this verification was phishing-resistant.
                if acr >= ACR_MFA
                    && row.required_acr >= ACR_PHISHING_RESISTANT as i64
                    && factor.descriptor().phishing_resistant
                    && amr
                        .iter()
                        .any(|m| factors::PHISHING_RESISTANT_METHODS.contains(&m.as_str()))
                {
                    acr = ACR_PHISHING_RESISTANT;
                }
                if (acr as i64) < row.required_acr {
                    return Err(PipelineError::Internal(
                        "Factor did not reach the required level".into(),
                    ));
                }
                if purpose == ChallengePurpose::StepUp {
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
                if amr.iter().any(|m| m == "rcv") {
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

    /// OQ-14: adding a credential of a factor the user already holds (a
    /// second key, new recovery codes) needs a step-up, so a session that
    /// only proved the password cannot add an attacker's key next to the
    /// user's. The first credential of a factor needs none.
    pub async fn adding_needs_step_up(
        &self,
        user: Uuid,
        factor_id: &str,
    ) -> Result<bool, PipelineError> {
        Ok(self
            .store()
            .list(user)
            .await?
            .iter()
            .any(|f| f.verified && f.kind == factor_id))
    }

    /// Deletes records of `owner` (one record or all of them) and, in the
    /// same transaction, publishes `auth.factor.removed`
    /// `{user_id, factor, record_id}` for each verified one (H-10/OQ-20), so
    /// the event exists only if the removal commits. Returns how many
    /// records were deleted.
    async fn delete_records(
        &self,
        owner: Uuid,
        record_id: Option<&str>,
        actor: &str,
    ) -> Result<u64, PipelineError> {
        let mut tx = self.pool.begin().await?;
        let rows: Vec<(String, String, i64)> = match record_id {
            Some(id) => {
                sqlx::query_as(
                    "DELETE FROM user_factors WHERE id = ? AND user_id = ? RETURNING id, kind, verified",
                )
                .bind(id)
                .bind(owner.to_string())
                .fetch_all(&mut *tx)
                .await?
            }
            None => {
                sqlx::query_as(
                    "DELETE FROM user_factors WHERE user_id = ? RETURNING id, kind, verified",
                )
                .bind(owner.to_string())
                .fetch_all(&mut *tx)
                .await?
            }
        };
        for (id, kind, verified) in &rows {
            if *verified != 0 {
                EventBus::publish_in_tx(
                    &mut tx,
                    EVENT_FACTOR_REMOVED,
                    json!({"user_id": owner, "factor": kind, "record_id": id}),
                    EventActor::user(actor),
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(rows.len() as u64)
    }

    /// Removes one record of `owner`; `false` when there is none.
    pub async fn remove_factor(
        &self,
        owner: Uuid,
        record_id: &str,
        actor: &str,
    ) -> Result<bool, PipelineError> {
        Ok(self.delete_records(owner, Some(record_id), actor).await? > 0)
    }

    /// Removes every record of `owner` (administrator reset).
    pub async fn remove_all_factors(&self, owner: Uuid, actor: &str) -> Result<u64, PipelineError> {
        self.delete_records(owner, None, actor).await
    }

    /// Renames one of the user's verified records (OQ-15). The label is
    /// trimmed and must be 1 to 64 characters without control characters
    /// (`invalid_label`, as for a plugin's enrolment label). `None` when the
    /// user has no such verified record.
    pub async fn rename_factor(
        &self,
        user: &User,
        record_id: &str,
        label: &str,
    ) -> Result<Option<String>, PipelineError> {
        let label = label.trim();
        let n = label.chars().count();
        if !(1..=64).contains(&n) || label.chars().any(char::is_control) {
            return Err(FactorError::coded(
                FactorErrorKind::Invalid,
                "invalid_label",
                "A name is 1 to 64 characters without control characters",
            )
            .into());
        }
        let Some(kind) = self.store().rename(user.id, record_id, label).await? else {
            return Ok(None);
        };
        self.audit(
            user.organization_id,
            Some(user.id),
            "auth.factor_renamed",
            &user.username,
            json!({"factor": kind, "record": record_id, "label": label}),
        )
        .await;
        Ok(Some(label.to_string()))
    }

    /// Records a new credential: the audit entry and the core event
    /// `auth.factor.enrolled` `{user_id, factor, record_id}`. The event is
    /// the hook for telling the user about it; the host sends no
    /// notification itself while `notify.send` does not exist (OQ-14).
    async fn factor_enrolled(&self, user: &User, kind: &str, record_id: &str, additional: bool) {
        self.audit(
            user.organization_id,
            Some(user.id),
            "auth.factor_enrolled",
            &user.username,
            json!({"factor": kind, "record": record_id, "additional": additional}),
        )
        .await;
        EventBus::new(self.pool.clone())
            .emit(
                EVENT_FACTOR_ENROLLED,
                json!({"user_id": user.id, "factor": kind, "record_id": record_id}),
                EventActor::user(user.username.clone()),
            )
            .await;
    }

    pub async fn begin_enroll(&self, user: &User, factor_id: &str) -> Result<Value, PipelineError> {
        let factor = self
            .factor(factor_id)
            .ok_or_else(|| PipelineError::Invalid(format!("Unknown factor '{factor_id}'")))?;
        let kind = factor.descriptor().id;
        let kind = kind.as_str();
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
            now_unix: now_unix(),
        };
        let start = factor.begin_enroll(&ctx).await?;
        match start.mode {
            EnrollMode::Pending(data) => {
                self.store().put_pending(user.id, kind, &data).await?;
            }
            EnrollMode::Immediate(data) => {
                let additional = !existing.is_empty();
                let record = if kind == "recovery" {
                    self.store().replace_recovery(user.id, &data).await?
                } else {
                    self.store().insert(user.id, kind, "", &data, true).await?
                };
                self.factor_enrolled(user, kind, &record.id, additional)
                    .await;
                self.after_enrolled(user, kind).await?;
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
        let kind = kind.as_str();
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
            now_unix: now_unix(),
        };
        let rec = factor.complete_enroll(&ctx, resp).await?;
        self.store()
            .finalize(&pending, rec.label.as_deref().unwrap_or(""), &rec.data)
            .await?;
        self.factor_enrolled(user, kind, &pending.id, !existing.is_empty())
            .await;
        self.after_enrolled(user, kind).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn amr(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// An in-memory database with the user `alice`.
    async fn db_user() -> (SqlitePool, User) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role) \
             VALUES (?, 'alice', 'alice@example.com', 'x', 'viewer')",
        )
        .bind(id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        let user = AuthService::new(pool.clone())
            .get_user_by_id(&id)
            .await
            .unwrap()
            .unwrap();
        (pool, user)
    }

    async fn latest_session(pool: &SqlitePool, user: &User) -> String {
        sqlx::query_scalar(
            "SELECT id FROM auth_sessions WHERE user_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user.id.to_string())
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn removed_events(pool: &SqlitePool) -> Vec<Value> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT payload FROM event_outbox WHERE type = 'auth.factor.removed' ORDER BY created_at",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        rows.iter()
            .map(|p| serde_json::from_str(p).unwrap())
            .collect()
    }

    /// H-10/OQ-20: removal and reset publish `auth.factor.removed` with the
    /// user, the factor and the record, for verified records only.
    #[tokio::test]
    async fn removing_factors_publishes_auth_factor_removed() {
        let (pool, user) = db_user().await;
        let cfg = AppConfig::default();
        let p = AuthPipeline::new(&pool, &cfg);
        let insert = |kind: &'static str, verified: bool| {
            let p = &p;
            let uid = user.id;
            async move {
                p.store()
                    .insert(uid, kind, "", &json!({}), verified)
                    .await
                    .unwrap()
                    .id
            }
        };
        let key = insert("mfa-fido2:security-key", true).await;
        let totp = insert("totp", true).await;
        let pending = insert("webauthn", false).await;
        assert!(p.remove_factor(user.id, &key, "alice").await.unwrap());
        assert!(!p.remove_factor(user.id, &key, "alice").await.unwrap());
        assert!(p.remove_factor(user.id, &pending, "alice").await.unwrap());
        assert_eq!(
            removed_events(&pool).await,
            vec![json!({"user_id": user.id, "factor": "mfa-fido2:security-key", "record_id": key})]
        );
        insert("recovery", true).await;
        assert_eq!(p.remove_all_factors(user.id, "admin").await.unwrap(), 2);
        let events = removed_events(&pool).await;
        assert_eq!(events.len(), 3);
        assert!(events.iter().any(|e| e["record_id"] == totp.as_str()));
        let actors: Vec<String> = sqlx::query_scalar(
            "SELECT actor_id FROM event_outbox WHERE type = 'auth.factor.removed' ORDER BY created_at",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(actors, ["alice", "admin", "admin"]);
    }

    /// OQ-15: owners rename their verified records; labels are 1 to 64
    /// characters without control characters, and the rename is audited.
    #[tokio::test]
    async fn owners_rename_their_factor_records() {
        let (pool, user) = db_user().await;
        let cfg = AppConfig::default();
        let p = AuthPipeline::new(&pool, &cfg);
        let mine = p
            .store()
            .insert(user.id, "webauthn", "Key", &json!({}), true)
            .await
            .unwrap();
        let pending = p
            .store()
            .insert(user.id, "webauthn", "", &json!({}), false)
            .await
            .unwrap();
        assert_eq!(
            p.rename_factor(&user, &mine.id, "  Office key ")
                .await
                .unwrap(),
            Some("Office key".to_string())
        );
        let listed = p.store().list(user.id).await.unwrap();
        assert_eq!(listed[0].label, "Office key");
        for bad in ["", "   ", "a\nb", "tab\there", &"x".repeat(65)] {
            match p.rename_factor(&user, &mine.id, bad).await {
                Err(PipelineError::Factor { code, kind, .. }) => {
                    assert_eq!(
                        (code.as_str(), kind),
                        ("invalid_label", FactorErrorKind::Invalid)
                    )
                }
                other => panic!("{bad:?}: {:?}", other.map(|_| ())),
            }
        }
        assert!(p
            .rename_factor(&user, &"y".repeat(64), "ok")
            .await
            .unwrap()
            .is_none());
        assert!(p
            .rename_factor(&user, &pending.id, "x")
            .await
            .unwrap()
            .is_none());
        // Somebody else's record is not found.
        let other = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role) \
             VALUES (?, 'bob', 'bob@example.com', 'x', 'viewer')",
        )
        .bind(other.to_string())
        .execute(&pool)
        .await
        .unwrap();
        let bob = AuthService::new(pool.clone())
            .get_user_by_id(&other)
            .await
            .unwrap()
            .unwrap();
        assert!(p
            .rename_factor(&bob, &mine.id, "mine now")
            .await
            .unwrap()
            .is_none());
        let audited: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'auth.factor_renamed'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audited, 1);
    }

    /// OQ-13: with `auth.step_up.max_age_secs`, a level 2 session older
    /// than that since its last verification must step up again.
    #[tokio::test]
    async fn step_up_freshness_expires_old_verifications() {
        let (pool, user) = db_user().await;
        let base = AppConfig::default();
        let mut cfg = base.clone();
        let p = AuthPipeline::new(&pool, &base);
        p.store()
            .insert(user.id, "totp", "", &json!({"secret": "x"}), true)
            .await
            .unwrap();
        p.issue_session(&user, &amr(&["pwd", "otp"]), ACR_MFA)
            .await
            .unwrap();
        let sid = latest_session(&pool, &user).await;
        let check = |cfg: &AppConfig| {
            let pool = &pool;
            let sid = sid.clone();
            let (uid, org) = (user.id, user.organization_id);
            let cfg = cfg.clone();
            async move {
                AuthPipeline::new(pool, &cfg)
                    .step_up_check(uid, org, &sid)
                    .await
                    .unwrap()
            }
        };
        let old = (Utc::now() - Duration::seconds(120)).to_rfc3339();
        sqlx::query("UPDATE auth_sessions SET created_at = ? WHERE id = ?")
            .bind(&old)
            .bind(&sid)
            .execute(&pool)
            .await
            .unwrap();
        // Disabled (default): the level is enough, as before.
        assert_eq!(check(&cfg).await, StepUpCheck::Passed);
        cfg.auth.step_up.max_age_secs = 600;
        assert_eq!(check(&cfg).await, StepUpCheck::Passed, "still fresh");
        cfg.auth.step_up.max_age_secs = 60;
        assert_eq!(
            check(&cfg).await,
            StepUpCheck::Required {
                required_acr: ACR_MFA,
                stale: true
            }
        );
        // A refresh keeps the session row and its verification time; only
        // a new verification (a new session) is fresh again.
        p.issue_session(&user, &amr(&["pwd", "otp", "hwk"]), ACR_MFA)
            .await
            .unwrap();
        let fresh = latest_session(&pool, &user).await;
        assert_eq!(
            AuthPipeline::new(&pool, &cfg)
                .step_up_check(user.id, user.organization_id, &fresh)
                .await
                .unwrap(),
            StepUpCheck::Passed
        );
    }

    /// OQ-5: level 2 only when a recognised multi-factor method is present.
    #[test]
    fn acr_counts_only_recognised_multi_factor_methods() {
        for m in ["otp", "hwk", "swk", "rcv", "mfa"] {
            assert_eq!(acr_of(&amr(&["pwd", m])), ACR_MFA, "{m}");
        }
        assert_eq!(acr_of(&amr(&["pwd", "hwk", "pin"])), ACR_MFA);
        for weak in [
            &["pwd"][..],
            &["fed"],
            &["pwd", "pin"],
            &["pwd", "user"],
            &[],
        ] {
            assert_eq!(acr_of(&amr(weak)), ACR_PRIMARY, "{weak:?}");
        }
    }
}
