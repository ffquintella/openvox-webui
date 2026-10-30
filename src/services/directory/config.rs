//! `auth.directories` configuration: one entry per LDAP or Active Directory
//! directory.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Kind of directory; it decides the defaults for filters and attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryKind {
    /// OpenLDAP, 389-ds and other RFC 4511 servers.
    Ldap,
    /// Microsoft Active Directory.
    ActiveDirectory,
}

impl DirectoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DirectoryKind::Ldap => "ldap",
            DirectoryKind::ActiveDirectory => "active_directory",
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_connect_timeout() -> u64 {
    5
}

fn default_timeout() -> u64 {
    10
}

fn default_page_size() -> u32 {
    500
}

fn default_sync_interval() -> u64 {
    3600
}

/// Attribute names read from user entries. Empty values take the defaults of
/// the directory kind.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttributeMapping {
    /// Login name (`sAMAccountName` on AD, `uid` on LDAP).
    #[serde(default)]
    pub username: Option<String>,
    /// E-mail address (`mail`).
    #[serde(default)]
    pub email: Option<String>,
    /// Display name (`displayName`, or `cn` on LDAP).
    #[serde(default)]
    pub display_name: Option<String>,
    /// Stable identifier used to link the account: `objectGUID` on AD,
    /// `entryUUID` on OpenLDAP, `nsUniqueId` on 389-ds. Never a DN, an e-mail
    /// or a login name, which change on renames and moves.
    #[serde(default)]
    pub id: Option<String>,
}

/// Group search.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroupSearchConfig {
    /// Search base for groups; defaults to the directory `base_dn`.
    #[serde(default)]
    pub base_dn: Option<String>,
    /// Filter selecting group entries (without the membership part).
    #[serde(default)]
    pub filter: Option<String>,
    /// Resolve nested groups. On Active Directory this uses the
    /// `LDAP_MATCHING_RULE_IN_CHAIN` rule (1.2.840.113556.1.4.1941); other
    /// servers only report direct membership.
    #[serde(default = "default_true")]
    pub nested: bool,
}

impl Default for GroupSearchConfig {
    fn default() -> Self {
        Self {
            base_dn: None,
            filter: None,
            nested: true,
        }
    }
}

/// Scheduled synchronization.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirectorySyncConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Seconds between runs (minimum 300).
    #[serde(default = "default_sync_interval")]
    pub interval_secs: u64,
}

impl Default for DirectorySyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: default_sync_interval(),
        }
    }
}

/// One LDAP or Active Directory directory.
#[derive(Clone, Deserialize, Serialize)]
pub struct DirectoryConfig {
    /// Identifier, also the provider name used by MFA policies and audit.
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub kind: DirectoryKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// `ldaps://host:636` or `ldap://host:389` with `start_tls`, tried in
    /// order (failover). Plain `ldap://` without StartTLS is only accepted for
    /// localhost.
    pub servers: Vec<String>,
    #[serde(default)]
    pub start_tls: bool,
    /// PEM file with the CA certificates that sign the directory servers.
    /// Without it the system trust store is used. Certificates are always
    /// verified.
    #[serde(default)]
    pub ca_file: Option<PathBuf>,
    /// Service account used to search for users and groups.
    pub bind_dn: String,
    /// Service account password. May be a `secret://` reference, resolved at
    /// startup; never returned by the API or written to logs.
    pub bind_password: String,
    pub base_dn: String,
    /// User filter; `{u}` is replaced by the escaped login name. Defaults:
    /// `(&(objectCategory=person)(objectClass=user)(sAMAccountName={u}))` on
    /// AD and `(&(objectClass=inetOrgPerson)(uid={u}))` on LDAP.
    #[serde(default)]
    pub user_filter: Option<String>,
    /// Realms accepted in `user@realm` and `REALM\user` (for example the
    /// NetBIOS name and the DNS domain). A qualified login with any other
    /// realm is not handled by this directory.
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub attributes: AttributeMapping,
    #[serde(default)]
    pub groups: GroupSearchConfig,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// Timeout of each operation (search, bind).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Page size of the RFC 2696 paged results control.
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// Search users and groups from the administration pages.
    #[serde(default = "default_true")]
    pub lookup: bool,
    /// Create the local account on the first successful login.
    #[serde(default)]
    pub jit: bool,
    /// Refuse the login of a directory user who ends up with no role at all
    /// (no mapping and no role assigned by an administrator), and never create
    /// such a user.
    #[serde(default = "default_true")]
    pub deny_without_role: bool,
    #[serde(default)]
    pub sync: DirectorySyncConfig,
}

impl std::fmt::Debug for DirectoryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectoryConfig")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("enabled", &self.enabled)
            .field("servers", &self.servers)
            .field("start_tls", &self.start_tls)
            .field("ca_file", &self.ca_file)
            .field("bind_dn", &self.bind_dn)
            .field("bind_password", &"[REDACTED]")
            .field("base_dn", &self.base_dn)
            .finish_non_exhaustive()
    }
}

impl DirectoryConfig {
    pub fn label(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.id.clone())
    }

    pub fn is_ad(&self) -> bool {
        self.kind == DirectoryKind::ActiveDirectory
    }

    pub fn user_filter(&self) -> String {
        match &self.user_filter {
            Some(f) if !f.trim().is_empty() => f.clone(),
            _ if self.is_ad() => {
                "(&(objectCategory=person)(objectClass=user)(sAMAccountName={u}))".into()
            }
            _ => "(&(objectClass=inetOrgPerson)(uid={u}))".into(),
        }
    }

    pub fn group_filter(&self) -> String {
        match &self.groups.filter {
            Some(f) if !f.trim().is_empty() => f.clone(),
            _ if self.is_ad() => "(objectClass=group)".into(),
            _ => "(|(objectClass=groupOfNames)(objectClass=groupOfUniqueNames)(objectClass=posixGroup))"
                .into(),
        }
    }

    pub fn group_base(&self) -> String {
        self.groups
            .base_dn
            .clone()
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| self.base_dn.clone())
    }

    fn attr(&self, set: &Option<String>, ad: &str, ldap: &str) -> String {
        match set {
            Some(a) if !a.trim().is_empty() => a.trim().to_string(),
            _ if self.is_ad() => ad.to_string(),
            _ => ldap.to_string(),
        }
    }

    pub fn username_attr(&self) -> String {
        self.attr(&self.attributes.username, "sAMAccountName", "uid")
    }

    pub fn email_attr(&self) -> String {
        self.attr(&self.attributes.email, "mail", "mail")
    }

    pub fn display_name_attr(&self) -> String {
        self.attr(&self.attributes.display_name, "displayName", "cn")
    }

    /// Stable identifier attributes, in order of preference.
    pub fn id_attrs(&self) -> Vec<String> {
        match &self.attributes.id {
            Some(a) if !a.trim().is_empty() => vec![a.trim().to_string()],
            _ if self.is_ad() => vec!["objectGUID".into()],
            _ => vec!["entryUUID".into(), "nsUniqueId".into()],
        }
    }

    pub fn sync_interval_secs(&self) -> u64 {
        self.sync.interval_secs.max(300)
    }

    /// Validates one directory. Returns a message naming the problem; never
    /// the password.
    pub fn validate(&self) -> Result<()> {
        let id = &self.id;
        if id.is_empty()
            || id.len() > 64
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("directory id '{id}' must be 1 to 64 letters, digits, '-' or '_'");
        }
        if self.servers.is_empty() {
            bail!("directory '{id}' has no servers");
        }
        for s in &self.servers {
            check_server_url(s, self.start_tls)
                .map_err(|e| anyhow::anyhow!("directory '{id}': {e}"))?;
        }
        if self.bind_dn.trim().is_empty() || self.bind_password.is_empty() {
            bail!("directory '{id}' needs bind_dn and bind_password (anonymous search is not supported)");
        }
        if self.base_dn.trim().is_empty() {
            bail!("directory '{id}' has no base_dn");
        }
        if !self.user_filter().contains("{u}") {
            bail!("directory '{id}': user_filter must contain the {{u}} placeholder");
        }
        for f in [self.user_filter(), self.group_filter()] {
            if !(f.starts_with('(') && f.ends_with(')')) {
                bail!("directory '{id}': filters must be enclosed in parentheses");
            }
        }
        if let Some(ca) = &self.ca_file {
            if !ca.is_absolute() {
                bail!("directory '{id}': ca_file must be an absolute path");
            }
        }
        if self.page_size == 0 || self.page_size > 5000 {
            bail!("directory '{id}': page_size must be between 1 and 5000");
        }
        if self.timeout_secs == 0 || self.connect_timeout_secs == 0 {
            bail!("directory '{id}': timeouts must be greater than zero");
        }
        Ok(())
    }
}

fn is_loopback(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost") || h == "127.0.0.1" || h == "::1"
}

/// `ldaps://` always; `ldap://` only with StartTLS, or on loopback.
pub fn check_server_url(url: &str, start_tls: bool) -> Result<()> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("server '{url}' is not an ldap:// or ldaps:// URL"))?;
    let host_port = rest.split(['/', '?']).next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        host_port
            .split(']')
            .next()
            .map(|h| format!("{h}]"))
            .unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    if host.is_empty() {
        bail!("server '{url}' has no host");
    }
    match scheme.to_ascii_lowercase().as_str() {
        "ldaps" => {
            if start_tls {
                bail!("server '{url}': start_tls cannot be combined with ldaps://");
            }
            Ok(())
        }
        "ldap" if start_tls || is_loopback(&host) => Ok(()),
        "ldap" => bail!(
            "server '{url}': plain ldap:// is only allowed on localhost; use ldaps:// or start_tls"
        ),
        _ => bail!("server '{url}' must use ldaps:// or ldap://"),
    }
}

/// Validates all directories and checks that their ids do not collide with
/// each other or with the other providers.
pub fn validate_all(dirs: &[DirectoryConfig], other_providers: &[String]) -> Result<()> {
    let mut seen: HashSet<&str> = HashSet::new();
    for d in dirs {
        d.validate()?;
        if matches!(d.id.as_str(), "local" | "saml" | "step_up" | "scim")
            || other_providers.iter().any(|p| p == &d.id)
        {
            bail!(
                "directory id '{}' is already used by another provider",
                d.id
            );
        }
        if !seen.insert(d.id.as_str()) {
            bail!("directory id '{}' is defined twice", d.id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> DirectoryConfig {
        serde_norway::from_str(
            r#"
id: corp
kind: active_directory
servers: ["ldaps://dc1.example.com"]
bind_dn: "CN=svc,DC=example,DC=com"
bind_password: "pw"
base_dn: "DC=example,DC=com"
"#,
        )
        .unwrap()
    }

    #[test]
    fn defaults_follow_the_kind() {
        let d = sample();
        assert!(d.enabled && d.lookup && !d.jit && d.deny_without_role);
        assert_eq!(d.username_attr(), "sAMAccountName");
        assert_eq!(d.id_attrs(), vec!["objectGUID".to_string()]);
        assert!(d.user_filter().contains("sAMAccountName={u}"));
        let mut l = d.clone();
        l.kind = DirectoryKind::Ldap;
        assert_eq!(l.username_attr(), "uid");
        assert_eq!(
            l.id_attrs(),
            vec!["entryUUID".to_string(), "nsUniqueId".to_string()]
        );
        d.validate().unwrap();
    }

    #[test]
    fn plain_ldap_only_on_localhost_or_with_starttls() {
        assert!(check_server_url("ldaps://dc.example.com:636", false).is_ok());
        assert!(check_server_url("ldap://dc.example.com", false).is_err());
        assert!(check_server_url("ldap://dc.example.com", true).is_ok());
        assert!(check_server_url("ldap://localhost:389", false).is_ok());
        assert!(check_server_url("ldap://127.0.0.1", false).is_ok());
        assert!(check_server_url("ldap://[::1]:389", false).is_ok());
        assert!(check_server_url("ldaps://dc.example.com", true).is_err());
        assert!(check_server_url("http://dc.example.com", false).is_err());
        assert!(check_server_url("dc.example.com", false).is_err());
    }

    #[test]
    fn validation_rejects_unsafe_or_incomplete_directories() {
        let mut d = sample();
        d.bind_password = String::new();
        assert!(d.validate().is_err());
        let mut d = sample();
        d.user_filter = Some("(uid=fixed)".into());
        assert!(d.validate().is_err());
        let mut d = sample();
        d.id = "bad id".into();
        assert!(d.validate().is_err());
        let d = sample();
        assert!(validate_all(&[d.clone(), d.clone()], &[]).is_err());
        assert!(validate_all(std::slice::from_ref(&d), &["corp".into()]).is_err());
        let mut local = d.clone();
        local.id = "local".into();
        assert!(validate_all(&[local], &[]).is_err());
        let mut scim = d.clone();
        scim.id = "scim".into();
        assert!(validate_all(&[scim], &[]).is_err());
        assert!(validate_all(&[d], &[]).is_ok());
    }

    #[test]
    fn debug_output_redacts_the_bind_password() {
        let mut d = sample();
        d.bind_password = "hunter2-very-secret".into();
        let s = format!("{d:?}");
        assert!(!s.contains("hunter2"));
        assert!(s.contains("REDACTED"));
    }
}
