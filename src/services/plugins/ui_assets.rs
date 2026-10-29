//! Sandboxed iframe pages (T83): short-lived asset sessions and the response
//! headers that keep a plugin's UI inside its sandbox.
//!
//! The host never serves plugin HTML to a page that could trust it. A frame
//! is loaded from `/api/v1/plugins/ui-assets/<token>/<path>`; the token is
//! an unguessable, expiring capability bound to one plugin and one user
//! session, carried in the path so relative asset URLs inherit it. The
//! response carries a CSP with a `sandbox` directive, so even opening the URL
//! in a tab gives the page no same-origin access.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rand::Rng;

pub const SESSION_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_SESSIONS: usize = 10_000;

#[derive(Debug, Clone)]
pub struct UiSession {
    pub plugin_id: String,
    pub version: String,
    pub contribution_id: String,
    pub username: String,
    expires: Instant,
}

#[derive(Default)]
pub struct UiSessions {
    inner: Mutex<HashMap<String, UiSession>>,
}

impl UiSessions {
    pub fn issue(
        &self,
        plugin_id: &str,
        version: &str,
        contribution_id: &str,
        username: &str,
    ) -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        let now = Instant::now();
        if let Ok(mut map) = self.inner.lock() {
            map.retain(|_, s| s.expires > now);
            if map.len() >= MAX_SESSIONS {
                // Drop the oldest-expiring half instead of refusing the user.
                let mut by_age: Vec<_> = map.iter().map(|(k, s)| (s.expires, k.clone())).collect();
                by_age.sort();
                for (_, k) in by_age.into_iter().take(MAX_SESSIONS / 2) {
                    map.remove(&k);
                }
            }
            map.insert(
                token.clone(),
                UiSession {
                    plugin_id: plugin_id.to_string(),
                    version: version.to_string(),
                    contribution_id: contribution_id.to_string(),
                    username: username.to_string(),
                    expires: now + SESSION_TTL,
                },
            );
        }
        token
    }

    pub fn lookup(&self, token: &str) -> Option<UiSession> {
        let now = Instant::now();
        let mut map = self.inner.lock().ok()?;
        match map.get(token) {
            Some(s) if s.expires > now => Some(s.clone()),
            Some(_) => {
                map.remove(token);
                None
            }
            None => None,
        }
    }

    /// Forgets every session of a plugin (disable, uninstall, update).
    pub fn revoke_plugin(&self, plugin_id: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.retain(|_, s| s.plugin_id != plugin_id);
        }
    }
}

/// Extensions a plugin UI bundle may contain; anything else is never served.
pub fn content_type(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => return None,
    })
}

/// Package files that are part of the web bundle (never the manifest,
/// signature material, migrations or WASM modules).
pub fn is_bundle_file(path: &str) -> bool {
    !path.starts_with("migrations/")
        && !matches!(path, "plugin.yaml" | "plugin.sig" | "MANIFEST.sha256")
        && content_type(path).is_some()
}

/// Turns an approved egress host (`api.acme.com`, `*.acme.io`) into a CSP
/// source.
fn egress_source(host: &str) -> Option<String> {
    let ok = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '*' | ':'));
    ok.then(|| format!("https://{host}"))
}

/// CSP for a plugin page: no network but the approved egress hosts, no
/// plugin-supplied framing, and a `sandbox` without `allow-same-origin`.
pub fn content_security_policy(egress_hosts: &[String]) -> String {
    let connect = egress_hosts
        .iter()
        .filter_map(|h| egress_source(h))
        .collect::<Vec<_>>()
        .join(" ");
    let net = if connect.is_empty() {
        "'none'".to_string()
    } else {
        connect
    };
    format!(
        "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; font-src 'self' data:; connect-src {net}; \
         frame-src 'none'; object-src 'none'; base-uri 'none'; form-action {net}; \
         frame-ancestors 'self'; sandbox allow-scripts allow-forms"
    )
}

/// Normalizes the request path inside a bundle; `None` for traversal.
pub fn clean_path(raw: &str) -> Option<String> {
    let p = raw.trim_start_matches('/');
    if p.is_empty()
        || p.len() > 512
        || p.contains('\\')
        || p.contains('\0')
        || p.split('/')
            .any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return None;
    }
    Some(p.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_are_unguessable_and_revocable() {
        let s = UiSessions::default();
        let a = s.issue("p", "1.0.0", "c", "alice");
        let b = s.issue("p", "1.0.0", "c", "alice");
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(s.lookup(&a).unwrap().username, "alice");
        assert!(s.lookup("nope").is_none());
        s.revoke_plugin("p");
        assert!(s.lookup(&a).is_none());
    }

    #[test]
    fn csp_is_sandboxed_without_same_origin() {
        let csp = content_security_policy(&[]);
        assert!(csp.contains("sandbox allow-scripts allow-forms"));
        assert!(!csp.contains("allow-same-origin"));
        assert!(csp.contains("connect-src 'none'"));
        assert!(csp.contains("default-src 'none'"));
        assert!(csp.contains("frame-ancestors 'self'"));
    }

    #[test]
    fn csp_connect_src_follows_the_approved_egress() {
        let csp = content_security_policy(&["api.acme.com".into(), "*.acme.io".into()]);
        assert!(csp.contains("connect-src https://api.acme.com https://*.acme.io;"));
        // Hostile values never reach the header.
        let csp = content_security_policy(&["x.com; script-src *".into()]);
        assert!(!csp.contains("script-src *"));
        assert!(csp.contains("connect-src 'none'"));
    }

    #[test]
    fn only_web_files_are_bundle_files() {
        assert!(is_bundle_file("ui/app.js"));
        assert!(!is_bundle_file("plugin.yaml"));
        assert!(!is_bundle_file("migrations/001.sql"));
        assert!(!is_bundle_file("handler.wasm"));
        assert!(!is_bundle_file("ui/run.exe"));
    }

    #[test]
    fn traversal_is_refused() {
        assert_eq!(clean_path("/ui/a.js").as_deref(), Some("ui/a.js"));
        for bad in ["", "ui/../x", "../x", "ui//a.js", "ui\\a.js", "./a.js"] {
            assert!(clean_path(bad).is_none(), "{bad}");
        }
    }
}
