//! Bolt inventory rendering (T123).
//!
//! The inventory of a run is built from the nodes selected for it, so it is
//! always current and never lists a node outside the run. Names are checked
//! before they reach the file or the command line.

use serde_json::{json, Map, Value};

use super::config::BoltConfig;

/// A node a run can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoltTarget {
    pub certname: String,
    /// Address to connect to; the certname when absent.
    pub address: Option<String>,
    /// `kernel` fact. Windows nodes use WinRM, every other kernel SSH.
    pub kernel: Option<String>,
}

impl BoltTarget {
    pub fn is_windows(&self) -> bool {
        self.kernel
            .as_deref()
            .is_some_and(|k| k.eq_ignore_ascii_case("windows"))
    }
    pub fn transport(&self) -> &'static str {
        if self.is_windows() {
            "winrm"
        } else {
            "ssh"
        }
    }
}

/// A certname or address is a host name: letters, digits, dots, hyphens and
/// underscores, at most 253 bytes. Anything else could change what Bolt does.
pub fn valid_host(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && !name.starts_with(['-', '.'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Builds the inventory (version 2) for `targets`. It holds credentials, so
/// the caller writes it with mode 0600 and deletes it after the run.
pub fn render(targets: &[BoltTarget], cfg: &BoltConfig) -> Result<Value, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut list = Vec::with_capacity(targets.len());
    for t in targets {
        if !valid_host(&t.certname) {
            return Err(format!("'{}' is not a valid node name", t.certname));
        }
        let address = t.address.as_deref().unwrap_or(&t.certname);
        if !valid_host(address) {
            return Err(format!(
                "'{address}' is not a valid address for {}",
                t.certname
            ));
        }
        if !seen.insert(t.certname.clone()) {
            continue;
        }
        list.push(json!({
            "name": t.certname,
            "uri": address,
            "config": {"transport": t.transport()},
        }));
    }
    if list.is_empty() {
        return Err("no targets".into());
    }

    let mut ssh = Map::new();
    ssh.insert("user".into(), json!(cfg.ssh.user));
    ssh.insert("host-key-check".into(), json!(cfg.ssh.host_key_check));
    if let Some(k) = &cfg.ssh.private_key_path {
        ssh.insert("private-key".into(), json!(k));
    }
    if let Some(p) = &cfg.ssh.password {
        ssh.insert("password".into(), json!(p));
    }
    if let Some(r) = &cfg.ssh.run_as {
        ssh.insert("run-as".into(), json!(r));
    }
    let mut winrm = Map::new();
    winrm.insert("user".into(), json!(cfg.winrm.user));
    winrm.insert("ssl".into(), json!(cfg.winrm.ssl));
    winrm.insert("ssl-verify".into(), json!(cfg.winrm.ssl_verify));
    if let Some(p) = &cfg.winrm.password {
        winrm.insert("password".into(), json!(p));
    }

    Ok(json!({
        "version": 2,
        "targets": list,
        "config": {"ssh": ssh, "winrm": winrm},
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, kernel: Option<&str>) -> BoltTarget {
        BoltTarget {
            certname: name.into(),
            address: None,
            kernel: kernel.map(str::to_string),
        }
    }

    #[test]
    fn picks_the_transport_by_kernel() {
        let inv = render(
            &[
                t("a.example", Some("Linux")),
                t("w.example", Some("windows")),
            ],
            &BoltConfig::default(),
        )
        .unwrap();
        assert_eq!(inv["targets"][0]["config"]["transport"], "ssh");
        assert_eq!(inv["targets"][1]["config"]["transport"], "winrm");
        assert_eq!(inv["version"], 2);
    }

    #[test]
    fn carries_credentials_and_tls_settings() {
        let mut cfg = BoltConfig::default();
        cfg.ssh.password = Some("pw".into());
        cfg.ssh.run_as = Some("root".into());
        cfg.winrm.ssl_verify = false;
        let inv = render(&[t("a", None)], &cfg).unwrap();
        assert_eq!(inv["config"]["ssh"]["password"], "pw");
        assert_eq!(inv["config"]["ssh"]["run-as"], "root");
        assert_eq!(inv["config"]["ssh"]["host-key-check"], true);
        assert_eq!(inv["config"]["winrm"]["ssl-verify"], false);
    }

    #[test]
    fn address_overrides_the_certname_and_duplicates_collapse() {
        let a = BoltTarget {
            address: Some("10.0.0.5".into()),
            ..t("a", None)
        };
        let inv = render(&[a, t("a", None)], &BoltConfig::default()).unwrap();
        assert_eq!(inv["targets"].as_array().unwrap().len(), 1);
        assert_eq!(inv["targets"][0]["uri"], "10.0.0.5");
    }

    #[test]
    fn rejects_names_that_could_change_the_command() {
        for bad in [
            "", "a b", "a,b", "-rf", ".hidden", "a;b", "a\nb", "a/b", "$(x)",
        ] {
            assert!(
                render(&[t(bad, None)], &BoltConfig::default()).is_err(),
                "{bad:?}"
            );
        }
        let bad_addr = BoltTarget {
            address: Some("x y".into()),
            ..t("ok", None)
        };
        assert!(render(&[bad_addr], &BoltConfig::default()).is_err());
        assert!(render(&[], &BoltConfig::default()).is_err());
    }
}
