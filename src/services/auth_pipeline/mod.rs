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
    FactorError, FactorOutcome, FactorRecord,
};
use self::grace::{GraceStore, GraceWindow, LoginSpend};
use self::policy::{ACR_MFA, ACR_PRIMARY};
use self::store::{FactorStore, StoredRecord};
use crate::{
    config::AppConfig,
    db::AuditRepository,
    middleware::auth::{
        create_access_token_expiring_at, create_access_token_with_levels,
        create_auth_session_scoped, create_auth_session_with_levels, create_refresh_token,
        revoke_auth_session, SessionScope, SCOPE_ENROLLMENT,
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
        let row: Option<(String, i64, Option<String>)> = sqlx::query_as(
            "SELECT amr, acr, scope FROM auth_sessions WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(session_id)
        .fetch_optional(self.pool)
        .await
        .ok()?;
        row.map(|(amr, acr, scope)| SessionInfo {
            amr: serde_json::from_str(&amr).unwrap_or_default(),
            acr: acr as u8,
            scope: if scope.is_none() {
                SessionScope::Full
            } else {
                SessionScope::Enrollment
            },
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
            let grace = self
                .enrollment_grace_login(&ident, allow_enrollment)
                .await?;
            let grace_label = match grace {
                GraceDecision::Session(session) => return Ok(LoginOutcome::Session(session)),
                GraceDecision::None => None,
                GraceDecision::Expired => Some("expired"),
                GraceDecision::Exhausted => Some("exhausted"),
                GraceDecision::NotOffered => Some("not_offered_for_provider"),
            };
            let mut details =
                json!({"reason": "mfa_enrollment_required", "provider": ident.provider});
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
            return Ok(LoginOutcome::Denied {
                code: "mfa_enrollment_required",
                message: match grace_label {
                    Some("expired") | Some("exhausted") => {
                        "Policy requires a second factor and the time to enroll one has ended; \
                         ask an administrator"
                    }
                    _ => {
                        "Policy requires a second factor and none is enrolled; ask an administrator"
                    }
                }
                .into(),
            });
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

        let offer = self
            .create_challenge(
                user,
                ChallengePurpose::Login.as_str(),
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
        self.create_challenge(
            user,
            ChallengePurpose::StepUp.as_str(),
            "step_up",
            &info.amr,
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
                for m in &amr {
                    if !methods.contains(m) {
                        methods.push(m.clone());
                    }
                }
                let acr = acr_of(&methods).max(ACR_PRIMARY);
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
        self.audit(
            user.organization_id,
            Some(user.id),
            "auth.factor_enrolled",
            &user.username,
            json!({"factor": kind}),
        )
        .await;
        self.after_enrolled(user, kind).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amr(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
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
