//! SCIM 2.0 service (M31, T106), mounted at `/api/v1/scim/v2`.
//!
//! No session and no cookie: every request carries `Authorization: Bearer`
//! with a SCIM token. The gate refuses requests while SCIM is disabled,
//! limits failed authentications per client address and requests per token,
//! caps the body size and answers errors in the SCIM format.

use std::{
    collections::HashMap,
    net::SocketAddr,
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::{Body, Bytes},
    extract::{ConnectInfo, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Router,
};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use serde_json::{json, Value};

use crate::{
    db::AuditRepository,
    models::default_organization_uuid,
    services::scim::{
        filter::parse_filter,
        patch::{apply_group_patch, apply_user_patch, parse_patch},
        resources::{etag, group_from_body, project, user_from_body, GroupAttrs, UserAttrs},
        schemas,
        store::{self, Page},
        tokens::{self, AuthResult},
        ScimActor, ScimContext, ScimError, AUDIT_SCIM, LIST_SCHEMA, SCIM_CONTENT_TYPE,
    },
    AppState,
};

type ScimResult = Result<Response, ScimError>;

/// Window of the failed-authentication counter.
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
/// Most client addresses tracked by the failure counter.
const MAX_TRACKED_CLIENTS: usize = 10_000;
/// `last_used_at` is written at most this often per token.
const TOUCH_EVERY: Duration = Duration::from_secs(60);

/// Rate limits of the SCIM service (one per router).
pub struct ScimGuard {
    tokens: DefaultKeyedRateLimiter<String>,
    retry_after: u64,
    max_failures: u32,
    failures: Mutex<HashMap<String, (u32, Instant)>>,
    touched: Mutex<HashMap<String, Instant>>,
}

impl ScimGuard {
    pub fn new(per_minute: u32, max_failures: u32) -> Self {
        let n = NonZeroU32::new(per_minute.max(1)).unwrap_or(NonZeroU32::MIN);
        Self {
            tokens: RateLimiter::keyed(Quota::per_minute(n)),
            retry_after: (60 / u64::from(n.get())).max(1),
            max_failures: max_failures.max(1),
            failures: Mutex::new(HashMap::new()),
            touched: Mutex::new(HashMap::new()),
        }
    }

    /// Whether this client failed to authenticate too often lately.
    pub fn blocked(&self, client: &str) -> bool {
        let map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.get(client)
            .is_some_and(|(n, since)| since.elapsed() < FAILURE_WINDOW && *n >= self.max_failures)
    }

    pub fn record_failure(&self, client: &str) {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_TRACKED_CLIENTS {
            map.retain(|_, (_, since)| since.elapsed() < FAILURE_WINDOW);
            if map.len() >= MAX_TRACKED_CLIENTS {
                map.clear();
            }
        }
        let e = map.entry(client.to_string()).or_insert((0, Instant::now()));
        if e.1.elapsed() >= FAILURE_WINDOW {
            *e = (0, Instant::now());
        }
        e.0 += 1;
    }

    /// Takes one request from the token's budget.
    pub fn allow(&self, token_id: &str) -> bool {
        self.tokens.check_key(&token_id.to_string()).is_ok()
    }

    fn should_touch(&self, token_id: &str) -> bool {
        let mut map = self.touched.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(token_id) {
            Some(t) if t.elapsed() < TOUCH_EVERY => false,
            _ => {
                map.insert(token_id.to_string(), Instant::now());
                true
            }
        }
    }
}

#[derive(Clone)]
struct Gate {
    state: AppState,
    guard: Arc<ScimGuard>,
}

/// The SCIM service. Built with the state because its gate keeps the rate
/// limiters.
pub fn routes(state: &AppState) -> Router<AppState> {
    let cfg = &state.config.auth.scim;
    let gate = Gate {
        state: state.clone(),
        guard: Arc::new(ScimGuard::new(
            cfg.rate_limit_per_minute,
            cfg.max_auth_failures_per_minute,
        )),
    };
    Router::new()
        .route("/ServiceProviderConfig", get(service_provider_config))
        .route("/ResourceTypes", get(resource_types))
        .route("/ResourceTypes/{id}", get(resource_type))
        .route("/Schemas", get(schema_list))
        .route("/Schemas/{id}", get(schema_one))
        .route("/Users", get(list_users).post(create_user))
        .route(
            "/Users/{id}",
            get(get_user)
                .put(replace_user)
                .patch(patch_user)
                .delete(delete_user),
        )
        .route("/Groups", get(list_groups).post(create_group))
        .route(
            "/Groups/{id}",
            get(get_group)
                .put(replace_group)
                .patch(patch_group)
                .delete(delete_group),
        )
        .fallback(unknown_endpoint)
        .layer(middleware::from_fn_with_state(gate, scim_gate))
}

fn client_ip(request: &Request) -> String {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let h = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = h.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

fn json_content_type(headers: &HeaderMap) -> bool {
    match headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        None => true,
        Some(ct) => {
            let mime = ct
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            mime == SCIM_CONTENT_TYPE || mime == "application/json"
        }
    }
}

async fn audit_auth_failure(state: &AppState, reason: &str, presented: Option<&str>, ip: &str) {
    let token_id = presented
        .and_then(tokens::parse)
        .map(|(id, _)| id.to_string());
    if let Err(e) = AuditRepository::new(&state.db)
        .insert(
            default_organization_uuid(),
            None,
            "scim.auth_failed",
            AUDIT_SCIM,
            token_id.as_deref(),
            Some(&json!({"reason": reason, "token": token_id})),
            Some(ip),
        )
        .await
    {
        tracing::warn!("Failed to audit a SCIM authentication failure: {}", e);
    }
}

/// Authentication, rate limits and body limit of every SCIM request.
async fn scim_gate(State(gate): State<Gate>, request: Request, next: Next) -> Response {
    let cfg = &gate.state.config.auth.scim;
    if !cfg.enabled {
        return ScimError::not_found("SCIM provisioning is not enabled").into_response();
    }
    let ip = client_ip(&request);
    let presented = bearer(request.headers()).map(str::to_string);
    let outcome = match presented.as_deref() {
        None => AuthResult::Invalid("missing_token"),
        Some(t) => match tokens::authenticate(&gate.state.db, t).await {
            Ok(o) => o,
            Err(e) => return ScimError::internal(e).into_response(),
        },
    };
    let info = match outcome {
        AuthResult::Valid(info) => info,
        // A valid token is never locked out by failures from the same
        // address (a shared proxy, say); failures beyond the limit get 429
        // and are no longer audited.
        AuthResult::Invalid(_) if gate.guard.blocked(&ip) => {
            return ScimError::too_many_requests(FAILURE_WINDOW.as_secs()).into_response();
        }
        AuthResult::Invalid(reason) => {
            gate.guard.record_failure(&ip);
            tracing::info!("SCIM authentication failed from {}: {}", ip, reason);
            audit_auth_failure(&gate.state, reason, presented.as_deref(), &ip).await;
            return ScimError::unauthorized().into_response();
        }
    };
    if !gate.guard.allow(&info.id) {
        return ScimError::too_many_requests(gate.guard.retry_after).into_response();
    }
    let too_large = || {
        ScimError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "The request body is larger than {} bytes",
                cfg.max_body_bytes
            ),
        )
        .into_response()
    };
    let declared = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|n| n > cfg.max_body_bytes) {
        return too_large();
    }
    let (parts, body) = request.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, cfg.max_body_bytes).await else {
        return too_large();
    };
    if !bytes.is_empty() && !json_content_type(&parts.headers) {
        return ScimError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Send application/scim+json or application/json",
        )
        .into_response();
    }
    let mut request = Request::from_parts(parts, Body::from(bytes));
    request
        .extensions_mut()
        .insert(ScimActor::token(&info.id, &info.name, Some(ip)));
    if gate.guard.should_touch(&info.id) {
        tokens::touch(&gate.state.db, &info.id).await;
    }
    next.run(request).await
}

// ---------------------------------------------------------------- helpers

fn ctx<'a>(state: &'a AppState, actor: &'a ScimActor) -> ScimContext<'a> {
    ScimContext {
        pool: &state.db,
        rbac: Some(&state.rbac_db),
        config: &state.config.auth.scim,
        actor,
    }
}

fn scim_json(status: StatusCode, body: &Value) -> Response {
    let mut resp = (status, body.to_string()).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(SCIM_CONTENT_TYPE),
    );
    resp
}

/// A single resource with its `ETag` (and `Location` on 201).
fn resource_response(status: StatusCode, body: &Value, version: i64) -> Response {
    let mut resp = scim_json(status, body);
    if let Ok(v) = HeaderValue::from_str(&etag(version)) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    if status == StatusCode::CREATED {
        if let Some(loc) = body["meta"]["location"]
            .as_str()
            .and_then(|l| HeaderValue::from_str(l).ok())
        {
            resp.headers_mut().insert(header::LOCATION, loc);
        }
    }
    resp
}

fn parse_body(bytes: &Bytes) -> Result<Value, ScimError> {
    serde_json::from_slice(bytes)
        .map_err(|_| ScimError::invalid_syntax("The request body is not valid JSON"))
}

fn weak(tag: &str) -> &str {
    let t = tag.trim();
    t.strip_prefix("W/").unwrap_or(t)
}

/// `If-Match` (weak comparison; `*` matches any version).
fn check_if_match(headers: &HeaderMap, version: i64) -> Result<(), ScimError> {
    let Some(v) = headers.get(header::IF_MATCH) else {
        return Ok(());
    };
    let current = etag(version);
    let ok = v.to_str().ok().is_some_and(|s| {
        s.split(',')
            .any(|t| t.trim() == "*" || weak(t) == weak(&current))
    });
    if ok {
        Ok(())
    } else {
        Err(ScimError::precondition_failed())
    }
}

fn not_modified(headers: &HeaderMap, version: i64) -> Option<Response> {
    let v = headers.get(header::IF_NONE_MATCH)?.to_str().ok()?;
    let current = etag(version);
    v.split(',')
        .any(|t| t.trim() == "*" || weak(t) == weak(&current))
        .then(|| {
            let mut resp = StatusCode::NOT_MODIFIED.into_response();
            if let Ok(h) = HeaderValue::from_str(&current) {
                resp.headers_mut().insert(header::ETAG, h);
            }
            resp
        })
}

fn param<'a>(q: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    q.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

struct ListParams {
    start_index: usize,
    count: usize,
}

fn list_params(q: &HashMap<String, String>, max: usize) -> Result<ListParams, ScimError> {
    let num = |name: &str| -> Result<Option<i64>, ScimError> {
        param(q, name)
            .map(|v| {
                v.trim()
                    .parse::<i64>()
                    .map_err(|_| ScimError::invalid_value(format!("{name} must be an integer")))
            })
            .transpose()
    };
    let start_index = num("startIndex")?.unwrap_or(1).max(1) as usize;
    let count = num("count")?
        .map(|c| c.clamp(0, max as i64) as usize)
        .unwrap_or(max);
    Ok(ListParams { start_index, count })
}

fn list_response(page: Page, start_index: usize, q: &HashMap<String, String>) -> Response {
    let resources: Vec<Value> = page
        .resources
        .into_iter()
        .map(|r| project(r, param(q, "attributes"), param(q, "excludedAttributes")))
        .collect();
    scim_json(
        StatusCode::OK,
        &json!({
            "schemas": [LIST_SCHEMA],
            "totalResults": page.total,
            "startIndex": start_index,
            "itemsPerPage": resources.len(),
            "Resources": resources,
        }),
    )
}

fn static_list(items: Vec<Value>) -> Response {
    scim_json(
        StatusCode::OK,
        &json!({
            "schemas": [LIST_SCHEMA],
            "totalResults": items.len(),
            "startIndex": 1,
            "itemsPerPage": items.len(),
            "Resources": items,
        }),
    )
}

fn base(state: &AppState) -> String {
    ctx(state, &ScimActor::default()).base()
}

// ---------------------------------------------------------------- discovery

async fn service_provider_config(State(state): State<AppState>) -> Response {
    scim_json(
        StatusCode::OK,
        &schemas::service_provider_config(&state.config.auth.scim, &base(&state)),
    )
}

async fn resource_types(State(state): State<AppState>) -> Response {
    static_list(schemas::resource_types(&base(&state)))
}

async fn resource_type(State(state): State<AppState>, Path(id): Path<String>) -> ScimResult {
    schemas::resource_types(&base(&state))
        .into_iter()
        .find(|r| r["id"] == id.as_str())
        .map(|r| scim_json(StatusCode::OK, &r))
        .ok_or_else(|| ScimError::not_found("Unknown resource type"))
}

async fn schema_list(State(state): State<AppState>) -> Response {
    static_list(schemas::schemas(&base(&state)))
}

async fn schema_one(State(state): State<AppState>, Path(id): Path<String>) -> ScimResult {
    schemas::schemas(&base(&state))
        .into_iter()
        .find(|s| s["id"] == id.as_str())
        .map(|s| scim_json(StatusCode::OK, &s))
        .ok_or_else(|| ScimError::not_found("Unknown schema"))
}

async fn unknown_endpoint() -> Response {
    ScimError::not_found("Unknown SCIM endpoint").into_response()
}

// ---------------------------------------------------------------- users

async fn list_users(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Query(q): Query<HashMap<String, String>>,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let p = list_params(&q, c.config.max_page_size)?;
    let filter = param(&q, "filter").map(parse_filter).transpose()?;
    let page = store::list_users(c, filter.as_ref(), p.start_index, p.count).await?;
    Ok(list_response(page, p.start_index, &q))
}

async fn create_user(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    body: Bytes,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let attrs = user_from_body(&parse_body(&body)?, true)?;
    let row = store::create_user(c, attrs).await?;
    let v = store::user_resource(c, &row).await?;
    Ok(resource_response(StatusCode::CREATED, &v, row.version))
}

async fn find_user(
    state: &AppState,
    id: &str,
) -> Result<crate::services::scim::resources::UserRow, ScimError> {
    store::get_user(&state.db, id)
        .await?
        .ok_or_else(|| ScimError::not_found(format!("User {} not found", short_id(id))))
}

fn short_id(id: &str) -> String {
    id.chars().take(64).collect()
}

async fn get_user(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ScimResult {
    let row = find_user(&state, &id).await?;
    if let Some(r) = not_modified(&headers, row.version) {
        return Ok(r);
    }
    let v = store::user_resource(ctx(&state, &actor), &row).await?;
    let v = project(v, param(&q, "attributes"), param(&q, "excludedAttributes"));
    Ok(resource_response(StatusCode::OK, &v, row.version))
}

async fn replace_user(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let row = find_user(&state, &id).await?;
    check_if_match(&headers, row.version)?;
    let current = UserAttrs::from_row(&row);
    let attrs = user_from_body(&parse_body(&body)?, current.active)?;
    let row = store::update_user(c, &row, attrs).await?;
    let v = store::user_resource(c, &row).await?;
    Ok(resource_response(StatusCode::OK, &v, row.version))
}

async fn patch_user(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let row = find_user(&state, &id).await?;
    check_if_match(&headers, row.version)?;
    let ops = parse_patch(&parse_body(&body)?)?;
    let mut attrs = UserAttrs::from_row(&row);
    apply_user_patch(&mut attrs, &ops)?;
    let row = store::update_user(c, &row, attrs).await?;
    let v = store::user_resource(c, &row).await?;
    let v = project(v, param(&q, "attributes"), param(&q, "excludedAttributes"));
    Ok(resource_response(StatusCode::OK, &v, row.version))
}

async fn delete_user(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ScimResult {
    let row = find_user(&state, &id).await?;
    check_if_match(&headers, row.version)?;
    store::delete_user(ctx(&state, &actor), &row).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------- groups

async fn find_group(
    state: &AppState,
    id: &str,
) -> Result<crate::services::scim::resources::GroupRow, ScimError> {
    store::get_group(&state.db, id)
        .await?
        .ok_or_else(|| ScimError::not_found(format!("Group {} not found", short_id(id))))
}

async fn list_groups(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Query(q): Query<HashMap<String, String>>,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let p = list_params(&q, c.config.max_page_size)?;
    let filter = param(&q, "filter").map(parse_filter).transpose()?;
    let page = store::list_groups(c, filter.as_ref(), p.start_index, p.count).await?;
    Ok(list_response(page, p.start_index, &q))
}

async fn create_group(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    body: Bytes,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let (attrs, members) = group_from_body(&parse_body(&body)?)?;
    let row = store::create_group(c, attrs, members).await?;
    let v = store::group_resource(c, &row).await?;
    Ok(resource_response(StatusCode::CREATED, &v, row.version))
}

async fn get_group(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ScimResult {
    let row = find_group(&state, &id).await?;
    if let Some(r) = not_modified(&headers, row.version) {
        return Ok(r);
    }
    let v = store::group_resource(ctx(&state, &actor), &row).await?;
    let v = project(v, param(&q, "attributes"), param(&q, "excludedAttributes"));
    Ok(resource_response(StatusCode::OK, &v, row.version))
}

async fn replace_group(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let row = find_group(&state, &id).await?;
    check_if_match(&headers, row.version)?;
    let (attrs, members) = group_from_body(&parse_body(&body)?)?;
    let row = store::update_group(c, &row, attrs, members.into_iter().collect()).await?;
    let v = store::group_resource(c, &row).await?;
    Ok(resource_response(StatusCode::OK, &v, row.version))
}

/// Answers 204 with the new `ETag`, as the member list may be long.
async fn patch_group(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ScimResult {
    let c = ctx(&state, &actor);
    let row = find_group(&state, &id).await?;
    check_if_match(&headers, row.version)?;
    let ops = parse_patch(&parse_body(&body)?)?;
    let mut attrs = GroupAttrs::from_row(&row);
    let mut members = store::group_members(&state.db, &row.id).await?;
    apply_group_patch(&mut attrs, &mut members, &ops)?;
    let row = store::update_group(c, &row, attrs, members).await?;
    let mut resp = StatusCode::NO_CONTENT.into_response();
    if let Ok(v) = HeaderValue::from_str(&etag(row.version)) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    Ok(resp)
}

async fn delete_group(
    State(state): State<AppState>,
    Extension(actor): Extension<ScimActor>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ScimResult {
    let row = find_group(&state, &id).await?;
    check_if_match(&headers, row.version)?;
    store::delete_group(ctx(&state, &actor), &row).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_block_a_client_after_the_limit() {
        let g = ScimGuard::new(600, 3);
        for _ in 0..3 {
            assert!(!g.blocked("10.0.0.1"));
            g.record_failure("10.0.0.1");
        }
        assert!(g.blocked("10.0.0.1"));
        assert!(!g.blocked("10.0.0.2"));
    }

    #[test]
    fn tokens_have_their_own_budget() {
        let g = ScimGuard::new(2, 10);
        assert!(g.allow("a"));
        assert!(g.allow("a"));
        assert!(!g.allow("a"));
        assert!(g.allow("b"));
        assert_eq!(g.retry_after, 30);
    }

    #[test]
    fn bearer_and_content_type_parsing() {
        let mut h = HeaderMap::new();
        assert!(bearer(&h).is_none());
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer  abc "),
        );
        assert_eq!(bearer(&h), Some("abc"));
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert!(bearer(&h).is_none());
        assert!(json_content_type(&h));
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/scim+json; charset=utf-8"),
        );
        assert!(json_content_type(&h));
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        assert!(!json_content_type(&h));
    }

    #[test]
    fn etags_compare_weakly() {
        let mut h = HeaderMap::new();
        assert!(check_if_match(&h, 3).is_ok());
        h.insert(header::IF_MATCH, HeaderValue::from_static("W/\"3\""));
        assert!(check_if_match(&h, 3).is_ok());
        h.insert(header::IF_MATCH, HeaderValue::from_static("\"3\""));
        assert!(check_if_match(&h, 3).is_ok());
        assert!(check_if_match(&h, 4).is_err());
        h.insert(header::IF_MATCH, HeaderValue::from_static("*"));
        assert!(check_if_match(&h, 9).is_ok());
        let mut h = HeaderMap::new();
        h.insert(header::IF_NONE_MATCH, HeaderValue::from_static("W/\"2\""));
        assert!(not_modified(&h, 2).is_some());
        assert!(not_modified(&h, 3).is_none());
    }

    #[test]
    fn list_parameters_are_clamped() {
        let q: HashMap<String, String> = [("startIndex", "0"), ("count", "5000")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let p = list_params(&q, 200).unwrap();
        assert_eq!((p.start_index, p.count), (1, 200));
        let q: HashMap<String, String> = [("COUNT".to_string(), "-3".to_string())].into();
        assert_eq!(list_params(&q, 200).unwrap().count, 0);
        let q: HashMap<String, String> = [("count".to_string(), "x".to_string())].into();
        assert!(list_params(&q, 200).is_err());
    }
}
