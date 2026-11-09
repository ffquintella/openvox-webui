//! Plugin API routes (H-7) at `/api/v1/plugins/{id}/{*rest}`. Behind the
//! authentication middleware; core routes are static segments and always
//! win in the router. Order of checks: route and plugin (404), `requires`
//! (403), rate limit (429), step-up (401), query and body (400/413/415).

use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    extract::{OriginalUri, Query, RawPathParams, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::AuthUser,
    models::{Action, Requires, Resource},
    services::plugins::{
        routes::{
            request_segments, route_query, ResolvedRoute, RouteCaller, RouteFailure, RouteReply,
            RouteRequest, MAX_ROUTE_BODY_BYTES,
        },
        AUDIT_PLUGINS,
    },
    utils::error::ErrorResponse,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/{id}/{*rest}",
        get(dispatch)
            .post(dispatch)
            .put(dispatch)
            .patch(dispatch)
            .delete(dispatch),
    )
}

/// Responses are private and never sniffed or cached.
fn harden(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

fn error(status: StatusCode, kind: &str, code: &str, message: &str) -> Response {
    harden(
        (
            status,
            Json(ErrorResponse::new(kind, message).with_code(code)),
        )
            .into_response(),
    )
}

/// One answer for an unknown plugin, an unknown route and a plugin that
/// cannot run, so nothing about installed plugins leaks.
fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "not_found", "not_found", "Not found")
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "service_unavailable",
        "plugin_unavailable",
        "The plugin could not handle the request",
    )
}

fn bad_request(code: &str, message: &str) -> Response {
    error(StatusCode::BAD_REQUEST, "bad_request", code, message)
}

fn rate_limited(wait: Duration) -> Response {
    let secs = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
    let mut r = error(
        StatusCode::TOO_MANY_REQUESTS,
        "too_many_requests",
        "rate_limited",
        "Too many requests to this route",
    );
    r.headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(secs.max(1)));
    r
}

/// Deny by default: no `requires` means any signed-in user; a plugin-owned
/// permission is held by plugin administrators until plugin permissions can
/// be assigned to roles (as for slots); a core permission goes to RBAC; any
/// other name, or a failing check, is a denial.
async fn permitted(state: &AppState, user: &AuthUser, r: &ResolvedRoute) -> bool {
    let Some(Requires { resource, action }) = &r.route.requires else {
        return true;
    };
    let needed = if resource.starts_with(&format!("plugin.{}.", r.plugin_id)) {
        Some((Resource::Plugins, Action::Admin))
    } else {
        Resource::from_name(resource).zip(Action::from_name(action))
    };
    match needed {
        Some((res, act)) => super::plugins::allowed(state, user, res, act)
            .await
            .unwrap_or(false),
        None => false,
    }
}

enum BodyError {
    TooLarge,
    NotJson,
    Invalid,
}

impl BodyError {
    fn response(self) -> Response {
        match self {
            BodyError::TooLarge => error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "request_too_large",
                "The request body is too large",
            ),
            BodyError::NotJson => error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "unsupported_media_type",
                "The request body must be application/json",
            ),
            BodyError::Invalid => bad_request("invalid_json", "The request body is not valid JSON"),
        }
    }
}

/// The JSON body, if any: at most `MAX_ROUTE_BODY_BYTES`, `application/json`.
async fn json_body(headers: &HeaderMap, body: Body) -> Result<Option<Value>, BodyError> {
    let bytes = to_bytes(body, MAX_ROUTE_BODY_BYTES)
        .await
        .map_err(|_| BodyError::TooLarge)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return Err(BodyError::NotJson);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| BodyError::Invalid)
}

fn reply(r: RouteReply) -> Response {
    let status = StatusCode::from_u16(r.status).unwrap_or(StatusCode::OK);
    if status == StatusCode::NO_CONTENT {
        return harden(status.into_response());
    }
    harden((status, Json(r.body.unwrap_or(Value::Null))).into_response())
}

async fn dispatch(
    State(state): State<AppState>,
    user: AuthUser,
    method: Method,
    OriginalUri(uri): OriginalUri,
    params: RawPathParams,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let (mut id, mut rest) = (None, None);
    for (k, v) in &params {
        match k {
            "id" => id = Some(v.to_string()),
            "rest" => rest = Some(v.to_string()),
            _ => {}
        }
    }
    let (Some(id), Some(rest)) = (id, rest) else {
        return not_found();
    };
    let Some(segments) = request_segments(uri.path(), &rest) else {
        return not_found();
    };
    // HEAD is served by the GET route.
    let method_name = if method == Method::HEAD {
        "GET"
    } else {
        method.as_str()
    };
    let manager = state.plugin_manager.clone();
    let Some(resolved) = manager.resolve_route(&id, method_name, &segments) else {
        return not_found();
    };
    if !manager.route_admitted(&resolved).await {
        return not_found();
    }
    if !permitted(&state, &user, &resolved).await {
        return error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "forbidden",
            "You do not have the permission this route requires",
        );
    }
    if let Err(wait) = manager
        .route_runtime()
        .check(&resolved.plugin_id, &resolved.route, user.id)
    {
        return rate_limited(wait);
    }
    if resolved.route.step_up {
        if let Err(e) = super::auth_pipeline::require_step_up(&state, &user).await {
            return harden(e.into_response());
        }
    }
    let query = match Query::<Vec<(String, String)>>::try_from_uri(&uri) {
        Ok(Query(pairs)) => match route_query(pairs) {
            Ok(q) => q,
            Err(why) => return bad_request("invalid_request", why),
        },
        Err(_) => return bad_request("invalid_request", "The query string is invalid"),
    };
    let body = match json_body(&headers, body).await {
        Ok(b) => b,
        Err(e) => return e.response(),
    };
    let caller = RouteCaller {
        user_id: user.id,
        organization_id: user.organization_id,
        username: user.username.clone(),
        roles: user.roles.clone(),
    };
    let request = RouteRequest {
        query,
        body,
        request_id: Uuid::new_v4(),
    };
    let outcome = manager.invoke_route(&resolved, &caller, &request).await;
    let status = match &outcome {
        Ok(r) => r.status,
        Err(RouteFailure::NotFound) => 404,
        Err(RouteFailure::Unavailable) => 503,
    };
    // Calls that change state are audited by the host with the real actor,
    // whatever the plugin records itself (never the payload).
    if method != Method::GET && method != Method::HEAD {
        manager
            .audit(
                user.organization_id,
                Some(user.id),
                AUDIT_PLUGINS,
                "plugin.route.invoked",
                &resolved.plugin_id,
                json!({
                    "version": resolved.version,
                    "method": resolved.route.method,
                    "route": resolved.route.path,
                    "status": status,
                    "request_id": request.request_id,
                }),
            )
            .await;
    }
    match outcome {
        Ok(r) => reply(r),
        Err(RouteFailure::NotFound) => not_found(),
        Err(RouteFailure::Unavailable) => unavailable(),
    }
}
