//! Inbound and outbound webhooks (T79).
//!
//! Both directions use the same signature: HMAC-SHA256 over
//! `"<unix timestamp>.<raw body>"`, sent as `X-OpenVox-Signature: sha256=<hex>`
//! next to `X-OpenVox-Timestamp`. Inbound requests have no session, so they are
//! limited by signature, timestamp window, replay cache, rate and body size.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

use super::filter::{valid_event_pattern, Filter};
use super::invoke::{invoke_with_timeout, HandlerKind, InvokeError};
use super::manager::PluginManager;
use super::manifest::validate_egress_host;
use crate::config::PluginEventsConfig;
use crate::db::plugin_events_repository::WebhookEndpointRecord;
use crate::db::PluginEventsRepository;
use crate::models::{OutboundWebhookView, PluginWebhookView};

pub const HEADER_TIMESTAMP: &str = "x-openvox-timestamp";
pub const HEADER_SIGNATURE: &str = "x-openvox-signature";
pub const HEADER_EVENT: &str = "x-openvox-event";
pub const HEADER_DELIVERY: &str = "x-openvox-delivery";

pub fn sign(secret: &str, timestamp: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac takes any key");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time check of a `sha256=<hex>` signature header.
pub fn verify(secret: &str, timestamp: &str, body: &[u8], header: &str) -> bool {
    let Some(sig) = header
        .strip_prefix("sha256=")
        .and_then(|h| hex::decode(h).ok())
    else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

pub fn generate_secret() -> String {
    let bytes: [u8; 32] = rand::random();
    format!("whsec_{}", hex::encode(bytes))
}

/// Outbound endpoint URL: https only, no credentials, and a host that is not
/// loopback, link-local or a metadata address.
pub fn validate_endpoint_url(url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url).map_err(|_| "url is not valid".to_string())?;
    if u.scheme() != "https" {
        return Err("url must use https".into());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("url must not contain credentials".into());
    }
    let host = u.host_str().ok_or("url has no host")?;
    validate_egress_host(host)?;
    if url.len() > 512 {
        return Err("url is longer than 512 characters".into());
    }
    Ok(())
}

pub fn validate_endpoint_fields(
    name: &str,
    event_types: &[String],
    filter: Option<&str>,
) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(|c| c.is_control()) {
        return Err("name must be 1 to 80 characters".into());
    }
    if event_types.is_empty() || event_types.len() > 32 {
        return Err("event_types needs 1 to 32 patterns".into());
    }
    if let Some(bad) = event_types.iter().find(|p| !valid_event_pattern(p)) {
        return Err(format!("event pattern '{}' is invalid", bad));
    }
    if let Some(f) = filter {
        Filter::parse(f).map_err(|e| format!("filter: {}", e))?;
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("not found")]
    NotFound,
    #[error("invalid signature")]
    Unauthorized,
    #[error("too many requests")]
    RateLimited,
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Handler(String),
    #[error("handler timed out")]
    Timeout,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct InboundWebhooks {
    manager: Arc<PluginManager>,
    cfg: PluginEventsConfig,
    windows: Mutex<HashMap<(String, String), (Instant, u32)>>,
    seen: Mutex<HashMap<String, i64>>,
}

impl InboundWebhooks {
    pub fn new(manager: Arc<PluginManager>, cfg: PluginEventsConfig) -> Self {
        Self {
            manager,
            cfg,
            windows: Mutex::new(HashMap::new()),
            seen: Mutex::new(HashMap::new()),
        }
    }

    pub fn max_body_bytes(&self) -> usize {
        (self.cfg.webhook_max_body_kb.max(1) * 1024) as usize
    }

    fn repo(&self) -> PluginEventsRepository<'_> {
        PluginEventsRepository::new(self.manager.pool())
    }

    fn within_rate(&self, plugin_id: &str, hook_id: &str) -> bool {
        let mut w = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let e = w
            .entry((plugin_id.to_string(), hook_id.to_string()))
            .or_insert((Instant::now(), 0));
        if e.0.elapsed() >= Duration::from_secs(60) {
            *e = (Instant::now(), 0);
        }
        e.1 += 1;
        e.1 <= self.cfg.webhook_rate_per_minute
    }

    /// Remembers a signature for the replay window; false when already seen.
    fn first_use(&self, signature: &str, now: i64) -> bool {
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        let window = self.cfg.webhook_replay_window_secs.max(1) * 2;
        seen.retain(|_, t| now - *t <= window);
        seen.insert(signature.to_string(), now).is_none()
    }

    /// Verifies and dispatches one inbound request to the plugin's handler.
    pub async fn handle(
        &self,
        plugin_id: &str,
        hook_id: &str,
        timestamp: Option<&str>,
        signature: Option<&str>,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<Value, WebhookError> {
        let hooks = self.manager.hooks();
        let plugin = hooks.get(plugin_id).ok_or(WebhookError::NotFound)?;
        let hook = plugin
            .webhooks
            .iter()
            .find(|h| h.id == hook_id)
            .ok_or(WebhookError::NotFound)?;
        let secret = self
            .repo()
            .webhook_secret(plugin_id, hook_id)
            .await?
            .ok_or(WebhookError::NotFound)?;
        if !self.within_rate(plugin_id, hook_id) {
            return Err(WebhookError::RateLimited);
        }

        let now = Utc::now().timestamp();
        let (Some(ts), Some(sig)) = (timestamp, signature) else {
            return Err(WebhookError::Unauthorized);
        };
        let fresh = ts
            .parse::<i64>()
            .is_ok_and(|t| (now - t).abs() <= self.cfg.webhook_replay_window_secs);
        if !fresh || !verify(&secret, ts, body, sig) || !self.first_use(sig, now) {
            return Err(WebhookError::Unauthorized);
        }
        self.repo().touch_webhook(plugin_id, hook_id).await?;

        let (text, encoding) = match std::str::from_utf8(body) {
            Ok(t) => (t.to_string(), "utf8"),
            Err(_) => (
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, body),
                "base64",
            ),
        };
        let input = json!({
            "hook_id": hook_id,
            "content_type": content_type,
            "body": text,
            "body_encoding": encoding,
            "received_at": Utc::now(),
        });
        let call = plugin.call(HandlerKind::Webhook, &hook.handler, input);
        match invoke_with_timeout(self.manager.as_ref(), call, plugin.timeout_ms()).await {
            Ok(v) => Ok(v),
            Err(InvokeError::Unavailable(m)) => Err(WebhookError::Unavailable(m)),
            Err(InvokeError::Failed(m)) => Err(WebhookError::Handler(m)),
            Err(InvokeError::Timeout(_)) => Err(WebhookError::Timeout),
        }
    }

    /// Declared webhooks of a plugin with the state of their secrets.
    pub async fn list(&self, plugin_id: &str) -> Result<Vec<PluginWebhookView>, WebhookError> {
        let hooks = self.manager.hooks();
        let plugin = hooks.get(plugin_id).ok_or(WebhookError::NotFound)?;
        let meta = self.repo().webhook_secret_meta(plugin_id).await?;
        Ok(plugin
            .webhooks
            .iter()
            .map(|h| {
                let m = meta.iter().find(|m| m.hook_id == h.id);
                PluginWebhookView {
                    plugin_id: plugin_id.to_string(),
                    hook_id: h.id.clone(),
                    auth: h.auth.clone(),
                    handler: h.handler.clone(),
                    path: format!("/api/v1/plugins/{}/webhooks/{}", plugin_id, h.id),
                    secret_configured: m.is_some(),
                    rotated_at: m.map(|m| m.rotated_at),
                    last_used_at: m.and_then(|m| m.last_used_at),
                }
            })
            .collect())
    }

    /// Generates (or rotates) the secret of a declared webhook. The previous
    /// secret stops working immediately.
    pub async fn rotate_secret(
        &self,
        plugin_id: &str,
        hook_id: &str,
    ) -> Result<String, WebhookError> {
        let hooks = self.manager.hooks();
        let plugin = hooks.get(plugin_id).ok_or(WebhookError::NotFound)?;
        if !plugin.webhooks.iter().any(|h| h.id == hook_id) {
            return Err(WebhookError::NotFound);
        }
        let secret = generate_secret();
        self.repo()
            .set_webhook_secret(plugin_id, hook_id, &secret)
            .await?;
        Ok(secret)
    }
}

pub fn endpoint_record(
    name: String,
    url: String,
    event_types: Vec<String>,
    filter: Option<String>,
    created_by: Option<String>,
) -> WebhookEndpointRecord {
    let now = Utc::now();
    WebhookEndpointRecord {
        view: OutboundWebhookView {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            url,
            event_types,
            filter,
            enabled: true,
            created_by,
            created_at: now,
            updated_at: now,
        },
        secret: generate_secret(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_round_trip_and_tamper_detection() {
        let sig = format!("sha256={}", sign("s3cret", "1700000000", b"{\"a\":1}"));
        assert!(verify("s3cret", "1700000000", b"{\"a\":1}", &sig));
        assert!(!verify("other", "1700000000", b"{\"a\":1}", &sig));
        assert!(!verify("s3cret", "1700000001", b"{\"a\":1}", &sig));
        assert!(!verify("s3cret", "1700000000", b"{\"a\":2}", &sig));
        assert!(!verify("s3cret", "1700000000", b"{\"a\":1}", "sha256=zz"));
        assert!(!verify("s3cret", "1700000000", b"{\"a\":1}", &sig[7..]));
    }

    #[test]
    fn endpoint_urls_are_restricted() {
        assert!(validate_endpoint_url("https://hooks.example.com/x").is_ok());
        assert!(validate_endpoint_url("http://hooks.example.com/x").is_err());
        assert!(validate_endpoint_url("https://user:pw@hooks.example.com/").is_err());
        assert!(validate_endpoint_url("https://169.254.169.254/latest").is_err());
        assert!(validate_endpoint_url("not a url").is_err());
    }
}
