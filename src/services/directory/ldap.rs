//! `ldap3` transport: LDAPS or StartTLS with certificate validation, server
//! failover, per-operation timeouts, RFC 2696 paging and no referrals.

use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ldap3::adapters::{Adapter, EntriesOnly, PagedResults};
use ldap3::{Ldap, LdapConnAsync, LdapConnSettings, Scope, SearchEntry};
use rustls::{ClientConfig, RootCertStore};

use super::config::DirectoryConfig;
use super::{DirectoryError, DirectoryTransport, RawEntry};

/// `invalidCredentials` (RFC 4511); AD adds a sub-code in the message.
const RC_INVALID_CREDENTIALS: u32 = 49;

pub struct Ldap3Transport {
    cfg: DirectoryConfig,
    tls: Arc<ClientConfig>,
}

fn load_roots(cfg: &DirectoryConfig) -> anyhow::Result<RootCertStore> {
    let mut roots = RootCertStore::empty();
    match &cfg.ca_file {
        Some(path) => {
            let file = std::fs::File::open(path)
                .map_err(|e| anyhow::anyhow!("cannot read ca_file {}: {e}", path.display()))?;
            for cert in rustls_pemfile::certs(&mut BufReader::new(file)) {
                let cert = cert.map_err(|e| anyhow::anyhow!("invalid ca_file: {e}"))?;
                roots
                    .add(cert)
                    .map_err(|e| anyhow::anyhow!("invalid CA certificate: {e}"))?;
            }
        }
        None => {
            for cert in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
        }
    }
    if roots.is_empty() {
        anyhow::bail!("no trusted CA certificates (set ca_file)");
    }
    Ok(roots)
}

impl Ldap3Transport {
    pub fn new(cfg: DirectoryConfig) -> anyhow::Result<Self> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let tls = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(load_roots(&cfg)?)
            .with_no_client_auth();
        Ok(Self {
            cfg,
            tls: Arc::new(tls),
        })
    }

    fn op_timeout(&self) -> Duration {
        Duration::from_secs(self.cfg.timeout_secs)
    }

    /// Opens a connection to the first server that answers.
    async fn connect(&self) -> Result<(Ldap, String), DirectoryError> {
        let mut last = String::from("no servers");
        for server in &self.cfg.servers {
            let starttls = self.cfg.start_tls && server.to_ascii_lowercase().starts_with("ldap://");
            let settings = LdapConnSettings::new()
                .set_conn_timeout(Duration::from_secs(self.cfg.connect_timeout_secs))
                .set_starttls(starttls)
                .set_config(self.tls.clone());
            let budget = Duration::from_secs(self.cfg.connect_timeout_secs + 2);
            match tokio::time::timeout(budget, LdapConnAsync::with_settings(settings, server)).await
            {
                Ok(Ok((conn, ldap))) => {
                    ldap3::drive!(conn);
                    return Ok((ldap, server.clone()));
                }
                Ok(Err(e)) => {
                    tracing::warn!("Directory '{}': {} failed: {}", self.cfg.id, server, e);
                    last = format!("{server}: {e}");
                }
                Err(_) => {
                    tracing::warn!("Directory '{}': {} timed out", self.cfg.id, server);
                    last = format!("{server}: connect timeout");
                }
            }
        }
        Err(DirectoryError::Unavailable(last))
    }

    /// Connection bound as the service account.
    async fn service(&self) -> Result<(Ldap, String), DirectoryError> {
        let (mut ldap, server) = self.connect().await?;
        let res = ldap
            .with_timeout(self.op_timeout())
            .simple_bind(&self.cfg.bind_dn, &self.cfg.bind_password)
            .await
            .map_err(|e| DirectoryError::Unavailable(e.to_string()))?;
        match res.rc {
            0 => Ok((ldap, server)),
            RC_INVALID_CREDENTIALS => {
                let _ = ldap.unbind().await;
                Err(DirectoryError::ServiceBind)
            }
            rc => {
                let _ = ldap.unbind().await;
                Err(DirectoryError::Protocol(format!(
                    "service bind result {rc}"
                )))
            }
        }
    }

    async fn search_inner(
        &self,
        base: &str,
        filter: &str,
        attrs: &[String],
        limit: usize,
    ) -> Result<Vec<RawEntry>, DirectoryError> {
        let (mut ldap, _) = self.service().await?;
        let adapters: Vec<Box<dyn Adapter<_, _>>> = vec![
            Box::new(EntriesOnly::new()),
            Box::new(PagedResults::new(self.cfg.page_size as i32)),
        ];
        let mut out = Vec::new();
        let mut stream = ldap
            .with_timeout(self.op_timeout())
            .streaming_search_with(adapters, base, Scope::Subtree, filter, attrs.to_vec())
            .await
            .map_err(|e| DirectoryError::Protocol(e.to_string()))?;
        while out.len() < limit {
            match stream.next().await {
                Ok(Some(entry)) => {
                    let e = SearchEntry::construct(entry);
                    let mut raw = RawEntry {
                        dn: e.dn,
                        ..Default::default()
                    };
                    for (k, vals) in e.attrs {
                        raw.attrs
                            .entry(k.to_ascii_lowercase())
                            .or_default()
                            .extend(vals.into_iter().map(String::into_bytes));
                    }
                    for (k, vals) in e.bin_attrs {
                        raw.attrs
                            .entry(k.to_ascii_lowercase())
                            .or_default()
                            .extend(vals);
                    }
                    out.push(raw);
                }
                Ok(None) => break,
                Err(e) => return Err(DirectoryError::Protocol(e.to_string())),
            }
        }
        let truncated = out.len() >= limit;
        let res = stream.finish().await;
        // A server-side limit (sizeLimitExceeded, adminLimitExceeded) means a
        // partial answer: an error, or a sync would miss users.
        if !truncated && res.rc != 0 {
            let _ = ldap.unbind().await;
            return Err(DirectoryError::Protocol(format!(
                "search result {}",
                res.rc
            )));
        }
        let _ = ldap.unbind().await;
        Ok(out)
    }
}

#[async_trait]
impl DirectoryTransport for Ldap3Transport {
    async fn search(
        &self,
        base: &str,
        filter: &str,
        attrs: &[String],
        limit: usize,
    ) -> Result<Vec<RawEntry>, DirectoryError> {
        let total = self.op_timeout() * 6 + Duration::from_secs(self.cfg.connect_timeout_secs);
        tokio::time::timeout(total, self.search_inner(base, filter, attrs, limit))
            .await
            .map_err(|_| DirectoryError::Unavailable("search timed out".into()))?
    }

    async fn bind(&self, dn: &str, password: &str) -> Result<bool, DirectoryError> {
        let total = self.op_timeout() + Duration::from_secs(self.cfg.connect_timeout_secs + 2);
        let fut = async {
            let (mut ldap, _) = self.connect().await?;
            let res = ldap
                .with_timeout(self.op_timeout())
                .simple_bind(dn, password)
                .await
                .map_err(|e| DirectoryError::Unavailable(e.to_string()));
            let _ = ldap.unbind().await;
            match res?.rc {
                0 => Ok(true),
                RC_INVALID_CREDENTIALS => Ok(false),
                rc => Err(DirectoryError::Protocol(format!("bind result {rc}"))),
            }
        };
        tokio::time::timeout(total, fut)
            .await
            .map_err(|_| DirectoryError::Unavailable("bind timed out".into()))?
    }

    async fn check(&self) -> Result<String, DirectoryError> {
        let (mut ldap, server) = self.service().await?;
        let _ = ldap.unbind().await;
        Ok(server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(servers: Vec<String>, ca: &str) -> DirectoryConfig {
        serde_json::from_value(json!({
            "id": "t", "kind": "ldap", "servers": servers,
            "ca_file": ca, "bind_dn": "cn=svc,dc=x", "bind_password": "pw",
            "base_dn": "dc=x", "connect_timeout_secs": 1, "timeout_secs": 1,
        }))
        .unwrap()
    }

    fn ca() -> String {
        format!(
            "{}/tests/fixtures/ldap_test_ca.txt",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    #[test]
    fn the_ca_file_must_be_readable() {
        assert!(Ldap3Transport::new(cfg(vec!["ldaps://localhost".into()], &ca())).is_ok());
        assert!(
            Ldap3Transport::new(cfg(vec!["ldaps://localhost".into()], "/nonexistent/ca.pem"))
                .is_err()
        );
    }

    #[tokio::test]
    async fn unreachable_servers_fail_closed_after_trying_each() {
        // A port that was just closed refuses connections.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let t = Ldap3Transport::new(cfg(
            vec![
                format!("ldaps://127.0.0.1:{port}"),
                format!("ldap://127.0.0.1:{port}"),
            ],
            &ca(),
        ))
        .unwrap();
        match t.bind("cn=a,dc=x", "pw").await {
            Err(DirectoryError::Unavailable(m)) => {
                assert!(m.contains(&format!("ldap://127.0.0.1:{port}")))
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
        assert!(matches!(
            t.search("dc=x", "(uid=a)", &[], 1).await,
            Err(DirectoryError::Unavailable(_))
        ));
    }

    /// A TLS server whose certificate is not signed by `ca_file` is refused.
    #[tokio::test]
    async fn an_untrusted_server_certificate_is_refused() {
        let dir = env!("CARGO_MANIFEST_DIR");
        let read = |f: &str| std::fs::read(format!("{dir}/tests/fixtures/{f}")).unwrap();
        let certs = rustls_pemfile::certs(&mut &read("ldap_untrusted_server_cert.txt")[..])
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let key = rustls_pemfile::private_key(&mut &read("ldap_untrusted_server_key.txt")[..])
            .unwrap()
            .unwrap();
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server));
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let handshakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ok = handshakes.clone();
        tokio::spawn(async move {
            while let Ok((s, _)) = l.accept().await {
                if acceptor.accept(s).await.is_ok() {
                    ok.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });
        let t = Ldap3Transport::new(cfg(vec![format!("ldaps://127.0.0.1:{port}")], &ca())).unwrap();
        match t.check().await {
            Err(DirectoryError::Unavailable(m)) => {
                assert!(m.to_lowercase().contains("certificate"), "{m}")
            }
            other => panic!("expected a certificate failure, got {other:?}"),
        }
        assert_eq!(handshakes.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_silent_server_times_out() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((s, _)) = l.accept().await {
                held.push(s); // never answers the TLS handshake
            }
        });
        let t = Ldap3Transport::new(cfg(vec![format!("ldaps://127.0.0.1:{port}")], &ca())).unwrap();
        let started = std::time::Instant::now();
        assert!(matches!(
            t.check().await,
            Err(DirectoryError::Unavailable(_))
        ));
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
