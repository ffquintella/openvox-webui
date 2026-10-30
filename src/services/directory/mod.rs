//! Directory providers (M30): LDAP and Active Directory.
//!
//! [`DirectoryTransport`] is the protocol (ldap3 or the in-memory fake);
//! [`Directory`] holds the security logic over any transport (escaping, empty
//! passwords, account state, dummy bind, groups, stable id); [`service`]
//! links, provisions, maps roles, deactivates and synchronizes.

pub mod config;
pub mod escape;
pub mod fake;
#[cfg(feature = "ldap")]
mod ldap;
pub mod mapping;
pub mod service;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};

pub use config::{DirectoryConfig, DirectoryKind};
use escape::{
    ad_account_state, escape_dn_value, escape_filter_value, guid_to_string, ldap_account_state,
    parse_ad_int, render_user_filter, sid_to_string, AccountState,
};

/// Resource type of directory entries in the audit log.
pub const AUDIT_DIRECTORY: &str = "directory";

/// OID of `LDAP_MATCHING_RULE_IN_CHAIN` (transitive membership on AD).
pub const MATCHING_RULE_IN_CHAIN: &str = "1.2.840.113556.1.4.1941";

/// Maximum groups read for one user.
const MAX_GROUPS_PER_USER: usize = 2000;
/// Maximum users read by a synchronization run.
pub const MAX_SYNC_USERS: usize = 100_000;

#[derive(Debug, thiserror::Error)]
pub enum DirectoryError {
    /// No server reachable, TLS failure or timeout.
    #[error("directory unavailable: {0}")]
    Unavailable(String),
    /// The service account was refused.
    #[error("the service account bind was refused")]
    ServiceBind,
    /// The server answered with an error.
    #[error("directory error: {0}")]
    Protocol(String),
}

impl DirectoryError {
    pub fn code(&self) -> &'static str {
        match self {
            DirectoryError::Unavailable(_) => "directory_unavailable",
            DirectoryError::ServiceBind => "service_bind_failed",
            DirectoryError::Protocol(_) => "directory_error",
        }
    }
}

/// An entry as returned by the server. Attribute names are lowercased; values
/// are raw bytes (binary attributes such as `objectGUID` stay intact).
#[derive(Debug, Clone, Default)]
pub struct RawEntry {
    pub dn: String,
    pub attrs: HashMap<String, Vec<Vec<u8>>>,
}

impl RawEntry {
    pub fn values(&self, name: &str) -> &[Vec<u8>] {
        self.attrs
            .get(&name.to_ascii_lowercase())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn first_bytes(&self, name: &str) -> Option<&[u8]> {
        self.values(name).first().map(|v| v.as_slice())
    }

    pub fn first_str(&self, name: &str) -> Option<String> {
        self.first_bytes(name)
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

/// The LDAP protocol as the host needs it.
#[async_trait]
pub trait DirectoryTransport: Send + Sync {
    /// Subtree search bound as the service account, paged (RFC 2696),
    /// referrals never followed, at most `limit` entries.
    async fn search(
        &self,
        base: &str,
        filter: &str,
        attrs: &[String],
        limit: usize,
    ) -> Result<Vec<RawEntry>, DirectoryError>;
    /// Simple bind on a fresh connection. `Ok(false)` means the server
    /// refused the credentials; anything else that fails is an error.
    async fn bind(&self, dn: &str, password: &str) -> Result<bool, DirectoryError>;
    /// Connects and binds as the service account; returns the server used.
    async fn check(&self) -> Result<String, DirectoryError>;
}

/// A user as the directory describes them.
#[derive(Debug, Clone, Serialize)]
pub struct DirectoryUser {
    pub dn: String,
    /// Stable identifier (`objectGUID`, `entryUUID`, `nsUniqueId`).
    pub external_id: String,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    #[serde(serialize_with = "ser_state")]
    pub state: AccountState,
}

fn ser_state<S: serde::Serializer>(s: &AccountState, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(s.as_str())
}

/// A group, with every identifier a mapping may use.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DirectoryGroup {
    pub dn: String,
    pub name: String,
    pub guid: Option<String>,
    pub sid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DirectoryIdentity {
    pub user: DirectoryUser,
    pub groups: Vec<DirectoryGroup>,
}

/// Outcome of a credential check. `Rejected` carries the reason for the audit
/// log only: every rejection looks the same to the client.
#[derive(Debug)]
pub enum BindOutcome {
    Authenticated(Box<DirectoryIdentity>),
    Rejected(&'static str),
}

/// One configured directory with its transport.
pub struct Directory {
    pub config: DirectoryConfig,
    transport: Arc<dyn DirectoryTransport>,
    sync_running: std::sync::atomic::AtomicBool,
}

impl Directory {
    pub fn new(config: DirectoryConfig, transport: Arc<dyn DirectoryTransport>) -> Self {
        Self {
            config,
            transport,
            sync_running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub(crate) fn sync_flag(&self) -> &std::sync::atomic::AtomicBool {
        &self.sync_running
    }

    pub fn id(&self) -> &str {
        &self.config.id
    }

    fn user_attrs(&self) -> Vec<String> {
        let c = &self.config;
        let mut a = vec![c.username_attr(), c.email_attr(), c.display_name_attr()];
        a.extend(c.id_attrs());
        if c.is_ad() {
            a.extend(
                [
                    "userAccountControl",
                    "msDS-User-Account-Control-Computed",
                    "accountExpires",
                    "lockoutTime",
                ]
                .map(String::from),
            );
        } else {
            a.extend(["nsAccountLock", "pwdAccountLockedTime"].map(String::from));
        }
        a
    }

    fn group_attrs(&self) -> Vec<String> {
        if self.config.is_ad() {
            vec!["cn".into(), "objectGUID".into(), "objectSid".into()]
        } else {
            vec!["cn".into(), "entryUUID".into(), "nsUniqueId".into()]
        }
    }

    /// Reads the stable identifier; `None` when the entry has none.
    fn external_id(&self, e: &RawEntry) -> Option<String> {
        for attr in self.config.id_attrs() {
            let Some(raw) = e.first_bytes(&attr) else {
                continue;
            };
            if attr.eq_ignore_ascii_case("objectGUID") {
                if let Some(g) = guid_to_string(raw) {
                    return Some(g);
                }
                continue;
            }
            let s = String::from_utf8_lossy(raw).trim().to_lowercase();
            if !s.is_empty() {
                return Some(s);
            }
        }
        None
    }

    fn account_state(&self, e: &RawEntry) -> AccountState {
        if self.config.is_ad() {
            ad_account_state(
                parse_ad_int(e.first_str("userAccountControl").as_deref()),
                parse_ad_int(e.first_str("msDS-User-Account-Control-Computed").as_deref()),
                parse_ad_int(e.first_str("accountExpires").as_deref()),
                parse_ad_int(e.first_str("lockoutTime").as_deref()),
                Utc::now(),
            )
        } else {
            ldap_account_state(
                e.first_str("nsAccountLock").as_deref(),
                e.first_str("pwdAccountLockedTime").as_deref(),
            )
        }
    }

    /// Builds a user from an entry; `None` without a DN, a login name or a
    /// stable identifier.
    pub fn parse_user(&self, e: &RawEntry) -> Option<DirectoryUser> {
        if e.dn.trim().is_empty() {
            return None;
        }
        Some(DirectoryUser {
            dn: e.dn.clone(),
            external_id: self.external_id(e)?,
            username: e.first_str(&self.config.username_attr())?,
            email: e.first_str(&self.config.email_attr()),
            display_name: e.first_str(&self.config.display_name_attr()),
            state: self.account_state(e),
        })
    }

    fn parse_group(&self, e: &RawEntry) -> DirectoryGroup {
        let guid = if self.config.is_ad() {
            e.first_bytes("objectGUID").and_then(guid_to_string)
        } else {
            e.first_str("entryUUID")
                .and_then(|s| uuid::Uuid::parse_str(&s).ok())
                .map(|u| u.to_string())
        };
        DirectoryGroup {
            dn: e.dn.clone(),
            name: e.first_str("cn").unwrap_or_else(|| e.dn.clone()),
            guid,
            sid: e.first_bytes("objectSid").and_then(sid_to_string),
        }
    }

    /// A bind that always fails, so an unknown or unusable account costs the
    /// same round trip as a wrong password. It never sends the user's password.
    async fn dummy_bind(&self) {
        let rand: [u8; 16] = rand::random();
        let token: String = rand.iter().map(|b| format!("{b:02x}")).collect();
        let dn = format!(
            "cn={},{}",
            escape_dn_value(&format!("ovx-no-such-user-{token}")),
            self.config.base_dn
        );
        let _ = self.transport.bind(&dn, &token).await;
    }

    /// Search-then-bind. `login` is the name for this directory (realm
    /// already handled). An error means the directory could not answer, and
    /// callers treat it as a denial.
    pub async fn authenticate(
        &self,
        login: &str,
        password: &str,
    ) -> Result<BindOutcome, DirectoryError> {
        // An empty password is an unauthenticated bind, which many servers
        // accept: refuse it before talking to the server.
        if password.is_empty() || password.contains('\0') {
            return Ok(BindOutcome::Rejected("empty_password"));
        }
        if login.trim().is_empty() {
            return Ok(BindOutcome::Rejected("invalid_username"));
        }
        let filter = render_user_filter(&self.config.user_filter(), login);
        let found = self
            .transport
            .search(&self.config.base_dn, &filter, &self.user_attrs(), 2)
            .await?;
        if found.len() != 1 {
            self.dummy_bind().await;
            return Ok(BindOutcome::Rejected(if found.is_empty() {
                "unknown_user"
            } else {
                "ambiguous_user"
            }));
        }
        let Some(user) = self.parse_user(&found[0]) else {
            self.dummy_bind().await;
            return Ok(BindOutcome::Rejected("missing_stable_id"));
        };
        if user.state != AccountState::Active {
            self.dummy_bind().await;
            return Ok(BindOutcome::Rejected(user.state.as_str()));
        }
        if !self.transport.bind(&user.dn, password).await? {
            return Ok(BindOutcome::Rejected("invalid_credentials"));
        }
        let groups = self.groups_of(&user).await?;
        Ok(BindOutcome::Authenticated(Box::new(DirectoryIdentity {
            user,
            groups,
        })))
    }

    /// Groups of a user, nested on AD when configured.
    pub async fn groups_of(
        &self,
        user: &DirectoryUser,
    ) -> Result<Vec<DirectoryGroup>, DirectoryError> {
        let dn = escape_filter_value(&user.dn);
        let membership = if self.config.is_ad() {
            if self.config.groups.nested {
                format!("(member:{MATCHING_RULE_IN_CHAIN}:={dn})")
            } else {
                format!("(member={dn})")
            }
        } else {
            format!(
                "(|(member={dn})(uniqueMember={dn})(memberUid={}))",
                escape_filter_value(&user.username)
            )
        };
        let filter = format!("(&{}{})", self.config.group_filter(), membership);
        let entries = self
            .transport
            .search(
                &self.config.group_base(),
                &filter,
                &self.group_attrs(),
                MAX_GROUPS_PER_USER,
            )
            .await?;
        Ok(entries.iter().map(|e| self.parse_group(e)).collect())
    }

    /// Exact lookup of one login name (no password).
    pub async fn find_user(&self, login: &str) -> Result<Option<DirectoryUser>, DirectoryError> {
        let filter = render_user_filter(&self.config.user_filter(), login);
        let found = self
            .transport
            .search(&self.config.base_dn, &filter, &self.user_attrs(), 2)
            .await?;
        if found.len() != 1 {
            return Ok(None);
        }
        Ok(self.parse_user(&found[0]))
    }

    /// Every user the user filter selects (synchronization). A listing that
    /// reaches the limit is refused: a partial list would look like deletions.
    pub async fn list_users(&self) -> Result<Vec<DirectoryUser>, DirectoryError> {
        let filter = self.config.user_filter().replace("{u}", "*");
        let entries = self
            .transport
            .search(
                &self.config.base_dn,
                &filter,
                &self.user_attrs(),
                MAX_SYNC_USERS,
            )
            .await?;
        if entries.len() >= MAX_SYNC_USERS {
            return Err(DirectoryError::Protocol(format!(
                "more than {MAX_SYNC_USERS} users match; narrow the user filter"
            )));
        }
        Ok(entries.iter().filter_map(|e| self.parse_user(e)).collect())
    }

    /// Substring search on the login name, display name and e-mail.
    pub async fn search_users(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DirectoryUser>, DirectoryError> {
        let q = escape_filter_value(query.trim());
        let base = self.config.user_filter().replace("{u}", "*");
        let filter = if q.is_empty() {
            base
        } else {
            format!(
                "(&{base}(|({}=*{q}*)({}=*{q}*)({}=*{q}*)))",
                self.config.username_attr(),
                self.config.display_name_attr(),
                self.config.email_attr()
            )
        };
        let entries = self
            .transport
            .search(&self.config.base_dn, &filter, &self.user_attrs(), limit)
            .await?;
        Ok(entries.iter().filter_map(|e| self.parse_user(e)).collect())
    }

    /// Substring search on group names.
    pub async fn search_groups(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DirectoryGroup>, DirectoryError> {
        let q = escape_filter_value(query.trim());
        let base = self.config.group_filter();
        let filter = if q.is_empty() {
            base
        } else {
            format!("(&{base}(cn=*{q}*))")
        };
        let entries = self
            .transport
            .search(
                &self.config.group_base(),
                &filter,
                &self.group_attrs(),
                limit,
            )
            .await?;
        Ok(entries.iter().map(|e| self.parse_group(e)).collect())
    }

    /// Connection test: service bind, then one user and one group search.
    pub async fn test(&self) -> Value {
        let started = std::time::Instant::now();
        let fail = |stage: &str, server: Option<&str>, e: &DirectoryError| {
            json!({"ok": false, "stage": stage, "server": server, "error": e.code(),
                   "message": e.to_string()})
        };
        let server = match self.transport.check().await {
            Ok(s) => s,
            Err(e) => return fail("connect", None, &e),
        };
        let users = match self.search_users("", 1).await {
            Ok(u) => u.len(),
            Err(e) => return fail("user_search", Some(&server), &e),
        };
        let groups = match self.search_groups("", 1).await {
            Ok(g) => g.len(),
            Err(e) => return fail("group_search", Some(&server), &e),
        };
        json!({
            "ok": true,
            "server": server,
            "users_found": users > 0,
            "groups_found": groups > 0,
            "elapsed_ms": started.elapsed().as_millis() as u64,
        })
    }
}

/// Transport used when a directory cannot be set up (no `ldap` feature, an
/// unreadable CA file): every operation fails, so logins fail closed.
struct UnavailableTransport(String);

#[async_trait]
impl DirectoryTransport for UnavailableTransport {
    async fn search(
        &self,
        _: &str,
        _: &str,
        _: &[String],
        _: usize,
    ) -> Result<Vec<RawEntry>, DirectoryError> {
        Err(DirectoryError::Unavailable(self.0.clone()))
    }
    async fn bind(&self, _: &str, _: &str) -> Result<bool, DirectoryError> {
        Err(DirectoryError::Unavailable(self.0.clone()))
    }
    async fn check(&self) -> Result<String, DirectoryError> {
        Err(DirectoryError::Unavailable(self.0.clone()))
    }
}

fn transport_for(cfg: &DirectoryConfig) -> Arc<dyn DirectoryTransport> {
    #[cfg(feature = "ldap")]
    {
        match ldap::Ldap3Transport::new(cfg.clone()) {
            Ok(t) => Arc::new(t),
            Err(e) => {
                tracing::error!("Directory '{}' is unavailable: {}", cfg.id, e);
                Arc::new(UnavailableTransport(e.to_string()))
            }
        }
    }
    #[cfg(not(feature = "ldap"))]
    {
        tracing::warn!(
            "Directory '{}' is configured but this build has no 'ldap' feature",
            cfg.id
        );
        Arc::new(UnavailableTransport(
            "built without the 'ldap' feature".into(),
        ))
    }
}

/// The configured directories, in configuration order (the login order).
#[derive(Default)]
pub struct DirectoryRegistry {
    dirs: Vec<Arc<Directory>>,
}

impl DirectoryRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds the `ldap3` transports for the configured directories.
    pub fn from_config(configs: &[DirectoryConfig]) -> Self {
        Self {
            dirs: configs
                .iter()
                .map(|c| Arc::new(Directory::new(c.clone(), transport_for(c))))
                .collect(),
        }
    }

    /// Directories with explicit transports (tests).
    pub fn with_transports(entries: Vec<(DirectoryConfig, Arc<dyn DirectoryTransport>)>) -> Self {
        Self {
            dirs: entries
                .into_iter()
                .map(|(c, t)| Arc::new(Directory::new(c, t)))
                .collect(),
        }
    }

    pub fn all(&self) -> &[Arc<Directory>] {
        &self.dirs
    }

    pub fn enabled(&self) -> impl Iterator<Item = &Arc<Directory>> {
        self.dirs.iter().filter(|d| d.config.enabled)
    }

    pub fn get(&self, id: &str) -> Option<&Arc<Directory>> {
        self.dirs.iter().find(|d| d.config.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }
}
