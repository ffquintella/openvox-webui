//! Plugin API routes (H-7): `contributes.api_routes` served at
//! `/api/v1/plugins/<plugin-id><path>` to signed-in users, through handlers
//! of kind `route` that get the caller's delegated identity and never any
//! header, cookie or token. Any failure is a generic `503 plugin_unavailable`.
//! The contract is in PLUGIN_PLATFORM.md section 17.16.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use serde_json::{json, Value};
use uuid::Uuid;

use super::invoke::{
    invoke_with_timeout, DelegatedActor, HandlerCall, HandlerKind, HookIndex, InvokeError,
    DEFAULT_HANDLER_TIMEOUT_MS,
};
use super::manager::PluginManager;
use crate::models::{ApiRouteDecl, PluginState, Requires, RuntimeKind, RuntimeSpec};

pub const ROUTES_CAPABILITY: &str = "api:routes";
pub const DEFAULT_ROUTE_HANDLER: &str = "on_request";
pub const DEFAULT_RATE_LIMIT: &str = "60/min";
pub const ROUTE_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE"];
pub const MAX_API_ROUTES: usize = 32;
pub const MAX_ROUTE_PATH_LEN: usize = 128;
pub const MAX_ROUTE_SEGMENTS: usize = 8;
pub const MAX_RATE_COUNT: u32 = 10_000;
/// Largest request body passed to a route handler.
pub const MAX_ROUTE_BODY_BYTES: usize = 256 * 1024;
pub const MAX_QUERY_PARAMS: usize = 32;
const MAX_QUERY_KEY_BYTES: usize = 64;
const MAX_QUERY_VALUE_BYTES: usize = 1024;
const MAX_PATH_SEGMENT_BYTES: usize = 256;
/// Statuses a plugin may answer. 401 (authentication and step-up), 3xx
/// (redirects) and 5xx stay the host's, so a plugin can neither imitate a
/// step-up demand nor send a client elsewhere.
pub const ALLOWED_STATUSES: &[u16] = &[200, 201, 202, 204, 400, 403, 404, 409, 413, 422, 429];
/// Query keys that can carry a credential (the authentication middleware
/// reads `?token=`); they are never passed to a plugin.
pub const CREDENTIAL_QUERY_KEYS: &[&str] = &["token", "access_token", "api_key", "apikey"];
/// First segments of the core routes under `/api/v1/plugins/<id>/`. A route
/// may not start with one: static core routes win in the router anyway, and
/// the host also refuses them at request time.
pub const RESERVED_ROUTE_SEGMENTS: &[&str] = &[
    "capabilities",
    "circuit",
    "classification",
    "config",
    "disable",
    "enable",
    "facts",
    "jobs",
    "metrics",
    "module-trust",
    "rollback",
    "storage",
    "update",
    "update-policy",
    "verify",
    "versions",
    "webhooks",
];
/// Static segments directly under `/api/v1/plugins/`. A plugin with one of
/// these ids cannot serve routes, so no plugin route sits beside a core one.
pub const RESERVED_PLUGIN_IDS: &[&str] = &[
    "classification",
    "events",
    "facts",
    "governance",
    "interceptors",
    "jobs",
    "metrics",
    "preflight",
    "trust",
    "ui",
    "ui-assets",
    "ui-modules",
    "updates",
];

// ---- declarations --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Segment {
    Static(String),
    Param(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RatePeriod {
    Second,
    Minute,
    Hour,
}

/// `<count>/<period>` of the manifest, applied per route and per user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RateLimit {
    pub count: u32,
    pub period: RatePeriod,
}

impl RateLimit {
    fn quota(&self) -> Quota {
        let n = NonZeroU32::new(self.count).unwrap_or(NonZeroU32::MIN);
        match self.period {
            RatePeriod::Second => Quota::per_second(n),
            RatePeriod::Minute => Quota::per_minute(n),
            RatePeriod::Hour => Quota::per_hour(n),
        }
    }
}

/// `<n>/s`, `<n>/min` or `<n>/h`, `n` from 1 to 10000.
pub fn parse_rate_limit(s: &str) -> Result<RateLimit, String> {
    let bad = || {
        format!(
            "rate_limit '{}' is invalid: use <n>/s, <n>/min or <n>/h with n from 1 to {}",
            s, MAX_RATE_COUNT
        )
    };
    let (n, unit) = s.split_once('/').ok_or_else(bad)?;
    if n.is_empty() || !n.bytes().all(|c| c.is_ascii_digit()) {
        return Err(bad());
    }
    let count: u32 = n
        .parse()
        .ok()
        .filter(|c| (1..=MAX_RATE_COUNT).contains(c))
        .ok_or_else(bad)?;
    let period = match unit {
        "s" => RatePeriod::Second,
        "min" => RatePeriod::Minute,
        "h" => RatePeriod::Hour,
        _ => return Err(bad()),
    };
    Ok(RateLimit { count, period })
}

fn valid_static_segment(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=64).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_'))
}

fn valid_ident(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    (1..=max).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// A route path: `/` then 1 to 8 segments, each a fixed segment
/// (`[a-z0-9][a-z0-9_-]`) or a whole-segment parameter `{name}`. The first
/// segment is fixed and is not one of the core segments.
pub fn parse_route_path(path: &str) -> Result<Vec<Segment>, String> {
    if path.len() > MAX_ROUTE_PATH_LEN {
        return Err(format!(
            "path '{}' is longer than {} characters",
            path, MAX_ROUTE_PATH_LEN
        ));
    }
    let Some(rest) = path.strip_prefix('/') else {
        return Err(format!("path '{}' must start with /", path));
    };
    let mut segments = Vec::new();
    let mut names = HashSet::new();
    for raw in rest.split('/') {
        let seg = match raw.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
            Some(name) if valid_ident(name, 32) => {
                if !names.insert(name) {
                    return Err(format!("path '{}' repeats the parameter '{}'", path, name));
                }
                Segment::Param(name.to_string())
            }
            Some(_) => {
                return Err(format!(
                    "path '{}': parameter '{}' must be {{[a-z][a-z0-9_]}}, at most 32 characters",
                    path, raw
                ))
            }
            None if valid_static_segment(raw) => Segment::Static(raw.to_string()),
            None => return Err(format!("path '{}': segment '{}' is invalid", path, raw)),
        };
        segments.push(seg);
    }
    if segments.len() > MAX_ROUTE_SEGMENTS {
        return Err(format!(
            "path '{}' has more than {} segments",
            path, MAX_ROUTE_SEGMENTS
        ));
    }
    match &segments[0] {
        Segment::Param(_) => Err(format!("path '{}' must start with a fixed segment", path)),
        Segment::Static(s) if RESERVED_ROUTE_SEGMENTS.contains(&s.as_str()) => Err(format!(
            "path '{}' would shadow the core routes under /api/v1/plugins/<id>/{}",
            path, s
        )),
        Segment::Static(_) => Ok(segments),
    }
}

/// One route of an enabled plugin, parsed from its declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginRoute {
    pub method: String,
    /// As declared, e.g. `/me/credentials/{credential_ref}`.
    pub path: String,
    pub segments: Vec<Segment>,
    pub handler: String,
    pub requires: Option<Requires>,
    pub rate: RateLimit,
    pub step_up: bool,
}

impl PluginRoute {
    pub fn from_decl(d: &ApiRouteDecl) -> Result<Self, String> {
        if !ROUTE_METHODS.contains(&d.method.as_str()) {
            return Err(format!(
                "route '{}': method '{}' is not supported (use {})",
                d.path,
                d.method,
                ROUTE_METHODS.join(", ")
            ));
        }
        let segments = parse_route_path(&d.path)?;
        let handler = d
            .handler
            .clone()
            .unwrap_or_else(|| DEFAULT_ROUTE_HANDLER.to_string());
        if !valid_ident(&handler, 64) {
            return Err(format!(
                "route '{}': handler '{}' is invalid",
                d.path, handler
            ));
        }
        let rate = parse_rate_limit(d.rate_limit.as_deref().unwrap_or(DEFAULT_RATE_LIMIT))?;
        Ok(Self {
            method: d.method.clone(),
            path: d.path.clone(),
            segments,
            handler,
            requires: d.requires.clone(),
            rate,
            step_up: d.step_up,
        })
    }

    /// The path with parameter names erased. Two routes of one method with
    /// the same shape could not be told apart.
    pub fn shape(&self) -> String {
        self.segments
            .iter()
            .map(|s| match s {
                Segment::Static(v) => v.as_str(),
                Segment::Param(_) => "{}",
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    fn params_for(&self, request: &[String]) -> Option<BTreeMap<String, String>> {
        if request.len() != self.segments.len() {
            return None;
        }
        let mut params = BTreeMap::new();
        for (seg, value) in self.segments.iter().zip(request) {
            match seg {
                Segment::Static(s) if s == value => {}
                Segment::Static(_) => return None,
                Segment::Param(name) => {
                    params.insert(name.clone(), value.clone());
                }
            }
        }
        Some(params)
    }

    /// Fixed segments rank before parameters, segment by segment.
    fn rank(&self) -> Vec<u8> {
        self.segments
            .iter()
            .map(|s| u8::from(matches!(s, Segment::Param(_))))
            .collect()
    }
}

/// The route of `method` matching the decoded request segments; when
/// several match, the one with fixed segments earliest wins.
pub fn find_route<'a>(
    routes: &'a [PluginRoute],
    method: &str,
    request: &[String],
) -> Option<(&'a PluginRoute, BTreeMap<String, String>)> {
    routes
        .iter()
        .filter(|r| r.method == method)
        .filter_map(|r| r.params_for(request).map(|p| (r, p)))
        .min_by(|a, b| a.0.rank().cmp(&b.0.rank()))
}

/// Splits the decoded remainder of the request path into segments. The raw
/// path must not encode a separator (`%2F`, `%5C`) or NUL, so the decoded
/// segments are exactly the segments the client sent; empty and dot segments
/// and control characters never match.
pub fn request_segments(raw_path: &str, decoded_rest: &str) -> Option<Vec<String>> {
    let lower = raw_path.to_ascii_lowercase();
    if ["%2f", "%5c", "%00"].iter().any(|e| lower.contains(e)) {
        return None;
    }
    if decoded_rest.is_empty() || decoded_rest.len() > MAX_ROUTE_SEGMENTS * MAX_PATH_SEGMENT_BYTES {
        return None;
    }
    let segments: Vec<String> = decoded_rest.split('/').map(str::to_string).collect();
    let ok = segments.len() <= MAX_ROUTE_SEGMENTS
        && segments.iter().all(|s| {
            !s.is_empty()
                && s != "."
                && s != ".."
                && s.len() <= MAX_PATH_SEGMENT_BYTES
                && !s.chars().any(|c| c.is_control() || c == '\\')
        });
    ok.then_some(segments)
}

/// The query parameters passed to the plugin: credential keys dropped, at
/// most 32, no key repeated, keys up to 64 and values up to 1024 bytes.
pub fn route_query(pairs: Vec<(String, String)>) -> Result<BTreeMap<String, String>, &'static str> {
    let mut out = BTreeMap::new();
    for (k, v) in pairs {
        if CREDENTIAL_QUERY_KEYS.contains(&k.to_ascii_lowercase().as_str()) {
            continue;
        }
        if k.is_empty()
            || k.len() > MAX_QUERY_KEY_BYTES
            || v.len() > MAX_QUERY_VALUE_BYTES
            || k.chars().chain(v.chars()).any(char::is_control)
        {
            return Err("a query parameter is empty, too long or has control characters");
        }
        if out.insert(k, v).is_some() {
            return Err("a query parameter is repeated");
        }
        if out.len() > MAX_QUERY_PARAMS {
            return Err("too many query parameters");
        }
    }
    Ok(out)
}

// ---- rate limits -----------------------------------------------------------------

type LimiterKey = (String, String, String, RateLimit);

/// Per-user rate limits of the plugin routes, kept across reloads while the
/// route exists with the same limit.
#[derive(Default)]
pub struct RouteRuntime {
    limiters: Mutex<HashMap<LimiterKey, Arc<DefaultKeyedRateLimiter<Uuid>>>>,
}

impl std::fmt::Debug for RouteRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteRuntime").finish_non_exhaustive()
    }
}

fn limiter_key(plugin_id: &str, r: &PluginRoute) -> LimiterKey {
    (
        plugin_id.to_string(),
        r.method.clone(),
        r.path.clone(),
        r.rate,
    )
}

impl RouteRuntime {
    /// Counts one call of `user`; `Err(wait)` once the route's limit is used.
    pub fn check(&self, plugin_id: &str, route: &PluginRoute, user: Uuid) -> Result<(), Duration> {
        let limiter = self
            .limiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(limiter_key(plugin_id, route))
            .or_insert_with(|| Arc::new(RateLimiter::keyed(route.rate.quota())))
            .clone();
        limiter
            .check_key(&user)
            .map_err(|not_until| not_until.wait_time_from(DefaultClock::default().now()))
    }

    /// Drops the limits of routes that are gone and the users whose state
    /// has fully replenished. Called on every reload.
    pub fn prune(&self, hooks: &HookIndex) {
        let live: HashSet<LimiterKey> = hooks
            .plugins()
            .flat_map(|p| p.routes.iter().map(|r| limiter_key(&p.plugin_id, r)))
            .collect();
        let mut map = self.limiters.lock().unwrap_or_else(PoisonError::into_inner);
        map.retain(|k, _| live.contains(k));
        for l in map.values() {
            l.retain_recent();
            l.shrink_to_fit();
        }
    }
}

// ---- dispatch --------------------------------------------------------------------

/// A request matched to a route of an enabled plugin version.
#[derive(Debug, Clone)]
pub struct ResolvedRoute {
    pub plugin_id: String,
    pub version: String,
    pub runtime: RuntimeSpec,
    pub route: PluginRoute,
    pub params: BTreeMap<String, String>,
}

impl ResolvedRoute {
    pub fn timeout_ms(&self) -> u64 {
        self.runtime
            .limits
            .as_ref()
            .and_then(|l| l.timeout_ms)
            .unwrap_or(DEFAULT_HANDLER_TIMEOUT_MS)
    }
}

/// The signed-in caller, from the authenticated request.
#[derive(Debug, Clone)]
pub struct RouteCaller {
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub username: String,
    pub roles: Vec<String>,
}

/// What the host passes besides the route and the caller.
#[derive(Debug, Clone, Default)]
pub struct RouteRequest {
    pub query: BTreeMap<String, String>,
    pub body: Option<Value>,
    pub request_id: Uuid,
}

/// The plugin's answer: an allowed status and an optional JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteReply {
    pub status: u16,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteFailure {
    /// The plugin stopped serving the route (answer 404).
    NotFound,
    /// Refused, failed, timed out or answered outside the contract (503).
    Unavailable,
}

/// `{method, route, params, query, body, user: {id, username,
/// organization_id, roles}, request_id, now_unix}`: nothing else of the
/// HTTP request reaches the plugin.
pub fn route_input(r: &ResolvedRoute, caller: &RouteCaller, req: &RouteRequest) -> Value {
    json!({
        "method": r.route.method,
        "route": r.route.path,
        "params": r.params,
        "query": req.query,
        "body": req.body,
        "user": {
            "id": caller.user_id,
            "username": caller.username,
            "organization_id": caller.organization_id,
            "roles": caller.roles,
        },
        "request_id": req.request_id,
        "now_unix": chrono::Utc::now().timestamp(),
    })
}

/// `{status?, body?}`: `status` defaults to 200 and must be allowed; a 204
/// has no body; unknown fields (headers, cookies) are refused.
pub fn parse_route_reply(v: Value) -> Result<RouteReply, &'static str> {
    let Value::Object(mut m) = v else {
        return Err("the answer is not an object");
    };
    if m.keys().any(|k| k != "status" && k != "body") {
        return Err("the answer has unknown fields");
    }
    let status = match m.remove("status") {
        None | Some(Value::Null) => 200,
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|s| u16::try_from(s).ok())
            .filter(|s| ALLOWED_STATUSES.contains(s))
            .ok_or("the status is not one a plugin may answer")?,
        Some(_) => return Err("status must be a number"),
    };
    let body = match m.remove("body") {
        None | Some(Value::Null) => None,
        Some(b) => Some(b),
    };
    if status == 204 && body.is_some() {
        return Err("a 204 answer has no body");
    }
    Ok(RouteReply { status, body })
}

fn failure_class(e: &InvokeError) -> &'static str {
    match e {
        InvokeError::Unavailable(_) => "refused (quota, circuit breaker, busy or no runtime)",
        InvokeError::Timeout(_) => "timed out",
        InvokeError::Failed(_) => "the handler failed (trap, limit or error result)",
    }
}

impl PluginManager {
    /// The route of an enabled plugin matching the request, if any. Core
    /// segments and reserved plugin ids never resolve.
    pub fn resolve_route(
        &self,
        plugin_id: &str,
        method: &str,
        segments: &[String],
    ) -> Option<ResolvedRoute> {
        if RESERVED_PLUGIN_IDS.contains(&plugin_id)
            || segments
                .first()
                .is_some_and(|s| RESERVED_ROUTE_SEGMENTS.contains(&s.as_str()))
        {
            return None;
        }
        let hooks = self.hooks();
        let p = hooks.get(plugin_id)?;
        let (route, params) = find_route(&p.routes, method, segments)?;
        Some(ResolvedRoute {
            plugin_id: p.plugin_id.clone(),
            version: p.version.clone(),
            runtime: p.runtime.clone(),
            route: route.clone(),
            params,
        })
    }

    /// The route is still declared by the active version of an enabled
    /// plugin, `plugins.policy` does not deny `api:routes` and, except for
    /// builtin plugins, the capability is approved.
    pub async fn route_admitted(&self, r: &ResolvedRoute) -> bool {
        let declared = self
            .hooks()
            .get(&r.plugin_id)
            .is_some_and(|h| h.version == r.version && h.routes.iter().any(|x| x == &r.route));
        if !declared || self.policy().denies(ROUTES_CAPABILITY) {
            return false;
        }
        let Ok(Some(rec)) = self.repo().get_plugin(&r.plugin_id).await else {
            return false;
        };
        if rec.state != PluginState::Enabled || rec.active_version.as_deref() != Some(&r.version) {
            return false;
        }
        if r.runtime.kind == RuntimeKind::Builtin {
            return true;
        }
        self.repo()
            .list_grants(&r.plugin_id)
            .await
            .map(|g| {
                g.iter()
                    .any(|g| g.capability == ROUTES_CAPABILITY && g.scope.is_none())
            })
            .unwrap_or(false)
    }

    /// Runs the route's handler for `caller` (kind `route`, governor, handler
    /// timeout, delegated actor). A late answer is discarded; every failure
    /// is `Unavailable`, logged without payloads.
    pub async fn invoke_route(
        &self,
        r: &ResolvedRoute,
        caller: &RouteCaller,
        req: &RouteRequest,
    ) -> Result<RouteReply, RouteFailure> {
        if !self.route_admitted(r).await {
            return Err(RouteFailure::NotFound);
        }
        let call = HandlerCall {
            plugin_id: r.plugin_id.clone(),
            version: r.version.clone(),
            runtime: r.runtime.clone(),
            handler: r.route.handler.clone(),
            kind: HandlerKind::Route,
            input: route_input(r, caller, req),
            actor: Some(DelegatedActor {
                user_id: caller.user_id,
                organization_id: caller.organization_id,
                username: caller.username.clone(),
            }),
        };
        let timeout = r.timeout_ms();
        let started = Instant::now();
        let result = invoke_with_timeout(self, call, timeout).await;
        let failed = |why: &str| {
            tracing::warn!(
                plugin = %r.plugin_id,
                version = %r.version,
                method = %r.route.method,
                route = %r.route.path,
                request_id = %req.request_id,
                "plugin route failed: {}",
                why
            );
            RouteFailure::Unavailable
        };
        match result {
            Ok(_) if started.elapsed() > Duration::from_millis(timeout) => {
                Err(failed("answered after the handler timeout"))
            }
            Ok(v) => parse_route_reply(v).map_err(failed),
            Err(e) => Err(failed(failure_class(&e))),
        }
    }
}

#[cfg(test)]
mod tests;
