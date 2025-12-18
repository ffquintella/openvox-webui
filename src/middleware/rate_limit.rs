//! Rate limiting middleware
//!
//! Provides IP-based rate limiting to protect against brute force attacks
//! and API abuse. Uses the governor crate with a keyed rate limiter.

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub requests_per_second: u32,
    /// Burst capacity (maximum requests allowed at once)
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10,
            burst_size: 30,
        }
    }
}

/// Stricter rate limit for authentication endpoints
pub fn auth_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        requests_per_second: 1,
        burst_size: 5,
    }
}

/// Standard API rate limit
pub fn api_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        requests_per_second: 50,
        burst_size: 100,
    }
}

/// Per-IP rate limiter using governor
pub type IpRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

/// Thread-safe map of IP addresses to their rate limiters
#[derive(Clone)]
pub struct RateLimitState {
    /// Map of IP to rate limiter
    limiters: Arc<RwLock<HashMap<IpAddr, Arc<IpRateLimiter>>>>,
    /// Configuration for creating new limiters
    config: RateLimitConfig,
}

impl RateLimitState {
    /// Create a new rate limit state with the given configuration
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get or create a rate limiter for the given IP address
    async fn get_limiter(&self, ip: IpAddr) -> Arc<IpRateLimiter> {
        // Try to get existing limiter with read lock first
        {
            let limiters = self.limiters.read().await;
            if let Some(limiter) = limiters.get(&ip) {
                return limiter.clone();
            }
        }

        // Create new limiter with write lock
        let mut limiters = self.limiters.write().await;

        // Double-check after acquiring write lock
        if let Some(limiter) = limiters.get(&ip) {
            return limiter.clone();
        }

        // Create new limiter
        let quota = Quota::per_second(
            NonZeroU32::new(self.config.requests_per_second).unwrap_or(NonZeroU32::MIN),
        )
        .allow_burst(
            NonZeroU32::new(self.config.burst_size).unwrap_or(NonZeroU32::MIN),
        );

        let limiter = Arc::new(RateLimiter::direct(quota));
        limiters.insert(ip, limiter.clone());
        limiter
    }

    /// Clean up old limiters to prevent memory leaks
    /// Should be called periodically (e.g., every hour)
    pub async fn cleanup(&self) {
        let mut limiters = self.limiters.write().await;
        let initial_count = limiters.len();

        // Remove limiters that haven't been used recently
        // In practice, governor limiters don't have an "idle" concept,
        // so we just limit the total number of tracked IPs
        const MAX_TRACKED_IPS: usize = 10000;

        if limiters.len() > MAX_TRACKED_IPS {
            // Remove oldest entries (in practice, just clear half)
            let to_remove: Vec<_> = limiters
                .keys()
                .take(limiters.len() / 2)
                .cloned()
                .collect();

            for ip in to_remove {
                limiters.remove(&ip);
            }

            debug!(
                "Rate limiter cleanup: {} -> {} entries",
                initial_count,
                limiters.len()
            );
        }
    }
}

/// Rate limiting middleware for Axum
pub async fn rate_limit_middleware(
    State(rate_limit): State<RateLimitState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let ip = addr.ip();
    let limiter = rate_limit.get_limiter(ip).await;

    match limiter.check() {
        Ok(_) => {
            debug!(ip = %ip, "Rate limit check passed");
            next.run(request).await
        }
        Err(_) => {
            warn!(ip = %ip, "Rate limit exceeded");
            RateLimitExceeded.into_response()
        }
    }
}

/// Rate limit exceeded response
pub struct RateLimitExceeded;

impl IntoResponse for RateLimitExceeded {
    fn into_response(self) -> Response {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("Retry-After", "1"),
                ("X-RateLimit-Limit", ""),
                ("X-RateLimit-Remaining", "0"),
            ],
            "Too many requests. Please try again later.",
        )
            .into_response()
    }
}

/// Create a rate limit layer for use with Axum routes
///
/// # Example
/// ```ignore
/// use openvox_webui::middleware::rate_limit::{RateLimitState, api_rate_limit_config};
///
/// let rate_limit_state = RateLimitState::new(api_rate_limit_config());
///
/// let app = Router::new()
///     .route("/api/v1/resource", get(handler))
///     .layer(axum::middleware::from_fn_with_state(
///         rate_limit_state,
///         rate_limit_middleware,
///     ));
/// ```
pub fn create_rate_limit_state(config: RateLimitConfig) -> RateLimitState {
    RateLimitState::new(config)
}

/// Spawn a background task to periodically clean up rate limiters
pub fn spawn_rate_limit_cleanup(state: RateLimitState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Every hour
        loop {
            interval.tick().await;
            state.cleanup().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_state_creation() {
        let config = RateLimitConfig {
            requests_per_second: 10,
            burst_size: 20,
        };
        let state = RateLimitState::new(config);

        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let limiter = state.get_limiter(ip).await;

        // Should allow the first request
        assert!(limiter.check().is_ok());
    }

    #[tokio::test]
    async fn test_rate_limit_burst() {
        let config = RateLimitConfig {
            requests_per_second: 1,
            burst_size: 3,
        };
        let state = RateLimitState::new(config);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        let limiter = state.get_limiter(ip).await;

        // Should allow burst_size requests
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());

        // Should reject after burst is exhausted
        assert!(limiter.check().is_err());
    }

    #[tokio::test]
    async fn test_different_ips_have_separate_limits() {
        let config = RateLimitConfig {
            requests_per_second: 1,
            burst_size: 1,
        };
        let state = RateLimitState::new(config);

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        let limiter1 = state.get_limiter(ip1).await;
        let limiter2 = state.get_limiter(ip2).await;

        // Exhaust ip1's limit
        assert!(limiter1.check().is_ok());
        assert!(limiter1.check().is_err());

        // ip2 should still have its own limit
        assert!(limiter2.check().is_ok());
    }
}
