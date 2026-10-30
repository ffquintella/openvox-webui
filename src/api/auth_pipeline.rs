//! Authentication pipeline API (M29): providers, challenges, factor
//! enrollment, step-up and MFA policies.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::plugins::require;
use crate::{
    middleware::AuthUser,
    models::*,
    services::auth_pipeline::{
        oidc::{self, LinkResult, OidcClient, BROWSER_COOKIE},
        policy::{self, PolicyInput},
        AuthPipeline, ClientInfo, LoginOutcome, PipelineError, PrimaryIdentity, VerifyOutcome,
    },
    services::AuthService,
    utils::error::{AppError, ErrorResponse},
    AppState,
};

/// An error with a stable machine-readable code.
pub struct ApiErr {
    status: StatusCode,
    error: &'static str,
    message: String,
    code: Option<&'static str>,
    details: Option<Value>,
}

impl ApiErr {
    fn new(status: StatusCode, error: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            error,
            message: message.into(),
            code: None,
            details: None,
        }
    }

    fn code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    fn details(mut self, d: Value) -> Self {
        self.details = Some(d);
        self
    }
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        let mut body = ErrorResponse::new(self.error, self.message);
        if let Some(c) = self.code {
            body = body.with_code(c);
        }
        if let Some(d) = self.details {
            body = body.with_details(d);
        }
        (self.status, Json(body)).into_response()
    }
}

impl From<AppError> for ApiErr {
    fn from(e: AppError) -> Self {
        let (status, error) = match &e {
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        ApiErr::new(status, error, e.to_string())
    }
}

impl From<PipelineError> for ApiErr {
    fn from(e: PipelineError) -> Self {
        match e {
            PipelineError::InvalidChallenge => {
                ApiErr::new(StatusCode::UNAUTHORIZED, "unauthorized", e.to_string())
                    .code("invalid_challenge")
            }
            PipelineError::Locked => ApiErr::new(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                e.to_string(),
            )
            .code("locked"),
            PipelineError::Unavailable(m) => {
                ApiErr::new(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", m)
                    .code("factor_unavailable")
            }
            PipelineError::Invalid(m) => ApiErr::new(StatusCode::BAD_REQUEST, "bad_request", m),
            PipelineError::Internal(m) => {
                tracing::error!("auth pipeline error: {m}");
                ApiErr::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Authentication failed",
                )
            }
        }
    }
}

type ApiResult<T> = Result<T, ApiErr>;

pub fn client_info(headers: &HeaderMap) -> ClientInfo {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
        .unwrap_or("")
        .trim()
        .to_string();
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    ClientInfo { ip, user_agent }
}

/// Renders a login outcome: 200 session, 202 challenge, 403 denied.
pub fn outcome_response(outcome: LoginOutcome) -> Response {
    match outcome {
        LoginOutcome::Session(s) => Json(s).into_response(),
        LoginOutcome::Challenge(offer) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "status": "mfa_required",
                "challenge_id": offer.challenge_id,
                "expires_in": offer.expires_in,
                "required_acr": offer.required_acr,
                "factors": offer.factors,
            })),
        )
            .into_response(),
        LoginOutcome::Denied { code, message } => {
            ApiErr::new(StatusCode::FORBIDDEN, "forbidden", message)
                .code(code)
                .into_response()
        }
    }
}

/// Step-up: sensitive operations call this. A user with no usable second
/// factor cannot step up and is let through, so MFA never locks out an account
/// that has not enrolled; a user who can step up must hold level 2.
pub async fn require_step_up(state: &AppState, user: &AuthUser) -> ApiResult<()> {
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    if pipeline.store().usable_kinds(user.id).await?.is_empty() {
        return Ok(());
    }
    match pipeline.session_levels(&user.session_id).await {
        Some((_, acr)) if acr >= policy::ACR_MFA => Ok(()),
        _ => Err(ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "This action needs a stronger authentication level",
        )
        .code("step_up_required")
        .details(json!({"required_acr": policy::ACR_MFA, "step_up": "/api/v1/auth/step-up"}))),
    }
}

impl From<anyhow::Error> for ApiErr {
    fn from(e: anyhow::Error) -> Self {
        PipelineError::from(e).into()
    }
}

impl From<sqlx::Error> for ApiErr {
    fn from(e: sqlx::Error) -> Self {
        PipelineError::from(e).into()
    }
}

// ----------------------------------------------------------------- routes

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/providers", get(list_providers))
        .route("/providers/{id}/begin", get(sso_begin))
        .route("/providers/{id}/callback", get(sso_callback))
        .route("/challenge/{id}/begin", post(challenge_begin))
        .route("/challenge/{id}/verify", post(challenge_verify))
}

pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/step-up", post(step_up))
        .route("/policies", get(get_policies).put(put_policies))
}

pub fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/factors", get(my_factors))
        .route(
            "/users/me/factors/{factor_id}/enroll/begin",
            post(enroll_begin),
        )
        .route(
            "/users/me/factors/{factor_id}/enroll/complete",
            post(enroll_complete),
        )
        .route("/users/me/factors/{id}", delete(remove_factor))
        .route("/users/{id}/factors/reset", post(reset_factors))
}

// -------------------------------------------------------------- providers

async fn list_providers(State(state): State<AppState>) -> Json<Value> {
    let mut providers = vec![json!({
        "id": "local",
        "kind": "credentials",
        "label": "Local account",
        "fields": [
            {"name": "username", "type": "text"},
            {"name": "password", "type": "password"},
        ],
    })];
    if state
        .config
        .saml
        .as_ref()
        .map(|s| s.enabled)
        .unwrap_or(false)
    {
        providers.push(json!({
            "id": "saml",
            "kind": "redirect",
            "label": "Single sign-on (SAML)",
            "begin_url": "/api/v1/auth/saml/login",
        }));
    }
    for p in &state.config.auth.mfa.oidc {
        providers.push(json!({
            "id": p.id,
            "kind": "redirect",
            "label": p.label.clone().unwrap_or_else(|| format!("Sign in with {}", p.id)),
            "begin_url": format!("/api/v1/auth/providers/{}/begin", p.id),
            "asserts_mfa": p.asserts_mfa,
        }));
    }
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    Json(json!({ "providers": providers, "factors": pipeline.descriptors() }))
}

#[derive(Deserialize)]
struct BeginQuery {
    return_to: Option<String>,
}

fn sso_failure(reason: &str) -> Response {
    let url = format!("/saml-callback?error={}", urlencoding::encode(reason));
    (StatusCode::SEE_OTHER, [(header::LOCATION, url)]).into_response()
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

fn flow_cookie(value: &str, max_age: i64, secure: bool) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{BROWSER_COOKIE}={value}; Path=/api/v1/auth/providers; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    ))
    .expect("cookie is ascii")
}

async fn sso_begin(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<BeginQuery>,
) -> Response {
    let Some(cfg) = state.config.auth.mfa.oidc.iter().find(|p| p.id == id) else {
        return AppError::not_found("Unknown provider").into_response();
    };
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    let flow = match OidcClient::new(cfg)
        .begin(&state.db, pipeline.key_for_flows(), q.return_to.as_deref())
        .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("OIDC begin for '{}' failed: {:#}", id, e);
            return AppError::service_unavailable("The identity provider is unavailable")
                .into_response();
        }
    };
    let secure = cfg.redirect_uri.starts_with("https://");
    let mut resp = (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, flow.authorize_url)],
    )
        .into_response();
    resp.headers_mut().append(
        header::SET_COOKIE,
        flow_cookie(&flow.browser_token, oidc::FLOW_TTL_MINUTES * 60, secure),
    );
    resp
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn sso_callback(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(cfg) = state.config.auth.mfa.oidc.iter().find(|p| p.id == id) else {
        return sso_failure("Unknown provider");
    };
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    let org = default_organization_uuid();
    let fail = |reason: &str, detail: String| {
        let mut r = sso_failure("Sign-in failed");
        tracing::warn!("OIDC login via '{}' denied: {} ({})", id, reason, detail);
        r.headers_mut()
            .append(header::SET_COOKIE, flow_cookie("", 0, false));
        r
    };

    if let Some(err) = q.error {
        pipeline
            .audit(
                org,
                None,
                "auth.login_denied",
                &id,
                json!({"provider": id, "reason": "idp_error", "idp_error": err}),
            )
            .await;
        return fail("idp_error", err);
    }
    let (Some(code), Some(st), Some(browser)) =
        (q.code, q.state, cookie_value(&headers, BROWSER_COOKIE))
    else {
        return fail("missing_parameters", String::new());
    };

    let (claims, return_to) = match OidcClient::new(cfg)
        .complete(&state.db, pipeline.key_for_flows(), &st, &code, &browser)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            pipeline
                .audit(
                    org,
                    None,
                    "auth.login_denied",
                    &id,
                    json!({"provider": id, "reason": "token_validation_failed"}),
                )
                .await;
            return fail("token_validation_failed", format!("{e:#}"));
        }
    };

    let user = match oidc::resolve_user(&state.db, cfg, &claims).await {
        Ok(LinkResult::User(u)) => *u,
        Ok(LinkResult::Denied(reason)) => {
            pipeline
                .audit(
                    org,
                    None,
                    "auth.login_denied",
                    &id,
                    json!({"provider": id, "reason": reason, "subject": claims.subject}),
                )
                .await;
            return fail(reason, claims.subject);
        }
        Err(e) => return fail("link_failed", format!("{e:#}")),
    };

    let mut amr = vec!["fed".to_string()];
    if cfg.asserts_mfa && oidc::idp_asserts_mfa(&claims) {
        amr.push("mfa".to_string());
    }
    let ident = PrimaryIdentity {
        user,
        provider: id.clone(),
        amr,
    };
    let target = match pipeline.start_login(ident, &client_info(&headers)).await {
        Ok(LoginOutcome::Session(s)) => format!(
            "/saml-callback?access_token={}&refresh_token={}&redirect={}",
            urlencoding::encode(&s.access_token),
            urlencoding::encode(&s.refresh_token),
            urlencoding::encode(&return_to)
        ),
        Ok(LoginOutcome::Challenge(offer)) => format!(
            "/login?challenge_id={}&factors={}&redirect={}",
            offer.challenge_id,
            offer
                .factors
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>()
                .join(","),
            urlencoding::encode(&return_to)
        ),
        Ok(LoginOutcome::Denied { code, .. }) => {
            return fail(code, String::new());
        }
        Err(e) => return fail("pipeline_error", e.to_string()),
    };
    let mut resp = (StatusCode::SEE_OTHER, [(header::LOCATION, target)]).into_response();
    resp.headers_mut()
        .append(header::SET_COOKIE, flow_cookie("", 0, false));
    resp
}

// -------------------------------------------------------------- challenges

#[derive(Deserialize)]
struct BeginChallengeBody {
    factor_id: String,
}

async fn challenge_begin(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<BeginChallengeBody>,
) -> ApiResult<Json<Value>> {
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    let client = pipeline
        .begin_challenge(&id, &body.factor_id, &client_info(&headers))
        .await?;
    Ok(Json(
        json!({ "factor_id": body.factor_id, "challenge": client }),
    ))
}

#[derive(Deserialize)]
struct VerifyBody {
    #[serde(default)]
    factor_id: Option<String>,
    response: Value,
}

async fn challenge_verify(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<VerifyBody>,
) -> ApiResult<Response> {
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    match pipeline
        .verify_challenge(
            &id,
            body.factor_id.as_deref(),
            body.response,
            &client_info(&headers),
        )
        .await?
    {
        VerifyOutcome::Session(s) => Ok(Json(*s).into_response()),
        VerifyOutcome::Pending => {
            Ok((StatusCode::ACCEPTED, Json(json!({"status": "pending"}))).into_response())
        }
        VerifyOutcome::Failed { attempts_left } => Err(ApiErr::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "The response was not accepted",
        )
        .code("factor_failed")
        .details(json!({"attempts_left": attempts_left}))),
    }
}

async fn current_user(state: &AppState, user: &AuthUser) -> ApiResult<User> {
    AuthService::new(state.db.clone())
        .get_user_by_id(&user.id)
        .await?
        .ok_or_else(|| AppError::not_found("User not found").into())
}

async fn step_up(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let u = current_user(&state, &user).await?;
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    let offer = pipeline
        .start_step_up(&u, &user.session_id, &client_info(&headers))
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "mfa_required",
            "challenge_id": offer.challenge_id,
            "expires_in": offer.expires_in,
            "required_acr": offer.required_acr,
            "factors": offer.factors,
        })),
    ))
}

// ---------------------------------------------------------------- factors

async fn my_factors(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    let enrolled = pipeline.store().list(user.id).await?;
    let (amr, acr) = pipeline
        .session_levels(&user.session_id)
        .await
        .unwrap_or((vec![], policy::ACR_PRIMARY));
    Ok(Json(json!({
        "factors": enrolled.into_iter().filter(|f| f.verified).collect::<Vec<_>>(),
        "available": pipeline.descriptors(),
        "session": {"amr": amr, "acr": acr},
    })))
}

async fn enroll_begin(
    State(state): State<AppState>,
    user: AuthUser,
    Path(factor_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    // Replacing recovery codes invalidates the old ones: that needs step-up.
    if factor_id == "recovery"
        && pipeline
            .store()
            .list(user.id)
            .await?
            .iter()
            .any(|f| f.kind == "recovery" && f.verified)
    {
        require_step_up(&state, &user).await?;
    }
    let u = current_user(&state, &user).await?;
    let client = pipeline.begin_enroll(&u, &factor_id).await?;
    Ok(Json(
        json!({ "factor_id": factor_id, "enrollment": client }),
    ))
}

async fn enroll_complete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(factor_id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    let u = current_user(&state, &user).await?;
    // The first factor also gets recovery codes, shown once.
    let had_any = !pipeline.store().usable_kinds(u.id).await?.is_empty();
    pipeline.complete_enroll(&u, &factor_id, body).await?;
    let mut out = json!({ "enrolled": factor_id });
    if !had_any && factor_id != "recovery" {
        let codes = pipeline.begin_enroll(&u, "recovery").await?;
        out["recovery"] = codes;
    }
    Ok(Json(out))
}

async fn remove_factor(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    require_step_up(&state, &user).await?;
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    if !pipeline.store().delete(user.id, &id).await? {
        return Err(AppError::not_found("Factor not found").into());
    }
    pipeline
        .audit(
            user.organization_id,
            Some(user.id),
            "auth.factor_removed",
            &user.username,
            json!({"factor_record": id}),
        )
        .await;
    Ok(Json(json!({"removed": id})))
}

async fn reset_factors(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    require(&state, &user, Resource::Users, Action::Admin).await?;
    require_step_up(&state, &user).await?;
    let target = AuthService::new(state.db.clone())
        .get_user_by_id_in_org(user.organization_id, &id)
        .await?
        .ok_or_else(|| AppError::not_found("User not found"))?;
    let pipeline = AuthPipeline::new(&state.db, &state.config);
    let removed = pipeline.store().delete_all(target.id).await?;
    pipeline
        .audit(
            user.organization_id,
            Some(user.id),
            "auth.factors_reset",
            &target.username,
            json!({"target": target.id, "removed": removed}),
        )
        .await;
    Ok(Json(json!({"removed": removed})))
}

// --------------------------------------------------------------- policies

async fn get_policies(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    require(&state, &user, Resource::Settings, Action::Admin).await?;
    let policies = policy::list(&state.db, user.organization_id).await?;
    Ok(Json(json!({ "policies": policies })))
}

#[derive(Deserialize)]
struct PoliciesBody {
    policies: Vec<PolicyInput>,
}

async fn put_policies(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<PoliciesBody>,
) -> ApiResult<Json<Value>> {
    require(&state, &user, Resource::Settings, Action::Admin).await?;
    require_step_up(&state, &user).await?;
    policy::validate(&body.policies).map_err(|e| AppError::bad_request(e.to_string()))?;
    let saved = policy::replace(&state.db, user.organization_id, &body.policies).await?;
    AuthPipeline::new(&state.db, &state.config)
        .audit(
            user.organization_id,
            Some(user.id),
            "auth.policies_updated",
            "policies",
            json!({"count": saved.len()}),
        )
        .await;
    Ok(Json(json!({ "policies": saved })))
}
