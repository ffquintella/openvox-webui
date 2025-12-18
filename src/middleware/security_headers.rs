//! Security headers middleware
//!
//! Adds security headers to all responses to protect against common web vulnerabilities.
//! Headers follow OWASP security best practices.

use axum::{body::Body, http::Request, middleware::Next, response::Response};

/// Middleware that adds security headers to all responses
pub async fn security_headers_middleware(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();

    // Strict-Transport-Security (HSTS)
    // Forces browsers to use HTTPS for all future requests to this domain
    // max-age=31536000 = 1 year, includeSubDomains applies to all subdomains
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains".parse().unwrap(),
    );

    // X-Content-Type-Options
    // Prevents browsers from MIME-sniffing a response away from the declared content-type
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());

    // X-Frame-Options
    // Protects against clickjacking attacks by preventing the page from being embedded in iframes
    // SAMEORIGIN allows embedding only from the same origin
    headers.insert("X-Frame-Options", "SAMEORIGIN".parse().unwrap());

    // X-XSS-Protection
    // Enables the browser's built-in XSS filter (legacy, but still useful for older browsers)
    headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());

    // Referrer-Policy
    // Controls how much referrer information is included with requests
    // strict-origin-when-cross-origin sends full URL for same-origin, origin only for cross-origin HTTPS
    headers.insert(
        "Referrer-Policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );

    // Permissions-Policy (formerly Feature-Policy)
    // Restricts which browser features can be used
    headers.insert(
        "Permissions-Policy",
        "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()"
            .parse()
            .unwrap(),
    );

    // Content-Security-Policy
    // Restricts the sources from which content can be loaded
    // This is a relatively permissive policy suitable for an admin UI
    // - default-src 'self': Only allow resources from the same origin by default
    // - script-src 'self' 'unsafe-inline': Allow scripts from same origin and inline scripts (needed for React)
    // - style-src 'self' 'unsafe-inline': Allow styles from same origin and inline styles (needed for Tailwind)
    // - img-src 'self' data: blob:: Allow images from same origin, data URIs, and blob URIs
    // - font-src 'self': Allow fonts from same origin
    // - connect-src 'self': Allow XHR/fetch to same origin
    // - frame-ancestors 'self': Only allow framing from same origin (reinforces X-Frame-Options)
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; frame-ancestors 'self'; base-uri 'self'; form-action 'self'"
            .parse()
            .unwrap(),
    );

    // Cache-Control for API responses
    // Prevents caching of sensitive data
    // Only apply to API routes (not static assets)
    // This is handled separately for static assets which should be cached

    response
}

/// Middleware for API routes that adds cache control headers
pub async fn api_cache_control_middleware(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();

    // Prevent caching of API responses
    headers.insert(
        "Cache-Control",
        "no-store, no-cache, must-revalidate, private".parse().unwrap(),
    );
    headers.insert("Pragma", "no-cache".parse().unwrap());
    headers.insert("Expires", "0".parse().unwrap());

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt;

    async fn test_handler() -> &'static str {
        "OK"
    }

    #[tokio::test]
    async fn test_security_headers_are_added() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(axum::middleware::from_fn(security_headers_middleware));

        let request = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        // Check all security headers are present
        assert!(response.headers().contains_key("strict-transport-security"));
        assert!(response.headers().contains_key("x-content-type-options"));
        assert!(response.headers().contains_key("x-frame-options"));
        assert!(response.headers().contains_key("x-xss-protection"));
        assert!(response.headers().contains_key("referrer-policy"));
        assert!(response.headers().contains_key("permissions-policy"));
        assert!(response.headers().contains_key("content-security-policy"));

        // Verify specific values
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        assert_eq!(
            response.headers().get("x-frame-options").unwrap(),
            "SAMEORIGIN"
        );
    }

    #[tokio::test]
    async fn test_api_cache_control_headers() {
        let app = Router::new()
            .route("/api/test", get(test_handler))
            .layer(axum::middleware::from_fn(api_cache_control_middleware));

        let request = Request::builder()
            .uri("/api/test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert!(response.headers().contains_key("cache-control"));
        assert!(response.headers().contains_key("pragma"));
        assert_eq!(response.headers().get("pragma").unwrap(), "no-cache");
    }
}
