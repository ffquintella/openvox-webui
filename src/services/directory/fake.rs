//! In-memory directory for tests. It parses and evaluates the RFC 4515
//! filters the host sends (so escaping is exercised end to end), supports AD
//! transitive membership (`LDAP_MATCHING_RULE_IN_CHAIN`) and, like many real
//! servers, accepts a bind with an empty password as anonymous.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use super::escape::normalize_dn;
use super::{DirectoryError, DirectoryTransport, RawEntry, MATCHING_RULE_IN_CHAIN};

#[derive(Debug, Clone)]
struct FakeEntry {
    dn: String,
    attrs: HashMap<String, Vec<Vec<u8>>>,
    password: Option<String>,
}

/// A fake LDAP server. All methods take `&self`; share it with `Arc`.
#[derive(Default)]
pub struct FakeDirectory {
    entries: Mutex<Vec<FakeEntry>>,
    down: AtomicBool,
    binds: AtomicUsize,
    searches: AtomicUsize,
    filters: Mutex<Vec<String>>,
    bound_dns: Mutex<Vec<String>>,
}

impl FakeDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    fn put(&self, dn: &str, attrs: Vec<(&str, Vec<Vec<u8>>)>, password: Option<&str>) {
        let mut map = HashMap::new();
        for (k, v) in attrs {
            map.insert(k.to_ascii_lowercase(), v);
        }
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|e| normalize_dn(&e.dn) != normalize_dn(dn));
        entries.push(FakeEntry {
            dn: dn.to_string(),
            attrs: map,
            password: password.map(String::from),
        });
    }

    /// An Active Directory user (`userAccountControl` 512, enabled).
    pub fn add_ad_user(
        &self,
        dn: &str,
        sam: &str,
        guid: uuid::Uuid,
        email: Option<&str>,
        password: &str,
    ) {
        let mut attrs = vec![
            ("objectClass", vec![b"user".to_vec(), b"person".to_vec()]),
            ("objectCategory", vec![b"person".to_vec()]),
            ("sAMAccountName", vec![sam.as_bytes().to_vec()]),
            ("displayName", vec![format!("User {sam}").into_bytes()]),
            ("objectGUID", vec![guid.to_bytes_le().to_vec()]),
            ("userAccountControl", vec![b"512".to_vec()]),
            ("msDS-User-Account-Control-Computed", vec![b"0".to_vec()]),
            ("accountExpires", vec![b"0".to_vec()]),
        ];
        if let Some(m) = email {
            attrs.push(("mail", vec![m.as_bytes().to_vec()]));
        }
        self.put(dn, attrs, Some(password));
    }

    /// An OpenLDAP `inetOrgPerson`.
    pub fn add_ldap_user(&self, dn: &str, uid: &str, entry_uuid: uuid::Uuid, password: &str) {
        self.put(
            dn,
            vec![
                ("objectClass", vec![b"inetOrgPerson".to_vec()]),
                ("uid", vec![uid.as_bytes().to_vec()]),
                ("cn", vec![format!("User {uid}").into_bytes()]),
                ("mail", vec![format!("{uid}@example.org").into_bytes()]),
                ("entryUUID", vec![entry_uuid.to_string().into_bytes()]),
            ],
            Some(password),
        );
    }

    /// An AD group (`objectClass=group`) with direct members (DNs).
    pub fn add_ad_group(&self, dn: &str, cn: &str, guid: uuid::Uuid, members: &[&str]) {
        self.put(
            dn,
            vec![
                ("objectClass", vec![b"group".to_vec()]),
                ("cn", vec![cn.as_bytes().to_vec()]),
                ("objectGUID", vec![guid.to_bytes_le().to_vec()]),
                (
                    "member",
                    members.iter().map(|m| m.as_bytes().to_vec()).collect(),
                ),
            ],
            None,
        );
    }

    /// An LDAP `groupOfNames`.
    pub fn add_ldap_group(&self, dn: &str, cn: &str, members: &[&str]) {
        self.put(
            dn,
            vec![
                ("objectClass", vec![b"groupOfNames".to_vec()]),
                ("cn", vec![cn.as_bytes().to_vec()]),
                (
                    "member",
                    members.iter().map(|m| m.as_bytes().to_vec()).collect(),
                ),
            ],
            None,
        );
    }

    /// The service account.
    pub fn add_service_account(&self, dn: &str, password: &str) {
        self.put(
            dn,
            vec![("objectClass", vec![b"account".to_vec()])],
            Some(password),
        );
    }

    pub fn set_attr(&self, dn: &str, attr: &str, values: &[&str]) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(e) = entries
            .iter_mut()
            .find(|e| normalize_dn(&e.dn) == normalize_dn(dn))
        {
            e.attrs.insert(
                attr.to_ascii_lowercase(),
                values.iter().map(|v| v.as_bytes().to_vec()).collect(),
            );
        }
    }

    pub fn set_members(&self, group_dn: &str, members: &[&str]) {
        self.set_attr(group_dn, "member", members);
    }

    pub fn remove(&self, dn: &str) {
        self.entries
            .lock()
            .unwrap()
            .retain(|e| normalize_dn(&e.dn) != normalize_dn(dn));
    }

    /// Simulates an outage: every operation fails.
    pub fn set_down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }

    pub fn bind_count(&self) -> usize {
        self.binds.load(Ordering::SeqCst)
    }

    pub fn search_count(&self) -> usize {
        self.searches.load(Ordering::SeqCst)
    }

    /// Filters received, in order.
    pub fn filters(&self) -> Vec<String> {
        self.filters.lock().unwrap().clone()
    }

    /// DNs used in binds, in order (never passwords).
    pub fn bound_dns(&self) -> Vec<String> {
        self.bound_dns.lock().unwrap().clone()
    }

    fn is_down(&self) -> Result<(), DirectoryError> {
        if self.down.load(Ordering::SeqCst) {
            Err(DirectoryError::Unavailable("fake directory is down".into()))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl DirectoryTransport for FakeDirectory {
    async fn search(
        &self,
        base: &str,
        filter: &str,
        attrs: &[String],
        limit: usize,
    ) -> Result<Vec<RawEntry>, DirectoryError> {
        self.is_down()?;
        self.searches.fetch_add(1, Ordering::SeqCst);
        self.filters.lock().unwrap().push(filter.to_string());
        let parsed = parse_filter(filter)
            .ok_or_else(|| DirectoryError::Protocol(format!("invalid filter {filter}")))?;
        let entries = self.entries.lock().unwrap().clone();
        let base_n = normalize_dn(base);
        let wanted: HashSet<String> = attrs.iter().map(|a| a.to_ascii_lowercase()).collect();
        let mut out = Vec::new();
        for e in &entries {
            let dn_n = normalize_dn(&e.dn);
            if !(dn_n == base_n || dn_n.ends_with(&format!(",{base_n}"))) {
                continue;
            }
            if !eval(&parsed, e, &entries) {
                continue;
            }
            out.push(RawEntry {
                dn: e.dn.clone(),
                attrs: e
                    .attrs
                    .iter()
                    .filter(|(k, _)| wanted.contains(*k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            });
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    async fn bind(&self, dn: &str, password: &str) -> Result<bool, DirectoryError> {
        self.is_down()?;
        self.binds.fetch_add(1, Ordering::SeqCst);
        self.bound_dns.lock().unwrap().push(dn.to_string());
        if password.is_empty() {
            // Unauthenticated bind (RFC 4513 §5.1.2): accepted, as many
            // servers do. The host must never get here.
            return Ok(true);
        }
        let entries = self.entries.lock().unwrap();
        Ok(entries.iter().any(|e| {
            normalize_dn(&e.dn) == normalize_dn(dn) && e.password.as_deref() == Some(password)
        }))
    }

    async fn check(&self) -> Result<String, DirectoryError> {
        self.is_down()?;
        Ok("fake://directory".into())
    }
}

// ------------------------------------------------------------ RFC 4515

#[derive(Debug, Clone)]
enum Filter {
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
    Present(String),
    Equal(String, Vec<u8>),
    Substring(String, Vec<Option<Vec<u8>>>),
    InChain(String, Vec<u8>),
}

fn unescape(v: &str) -> Option<Vec<u8>> {
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'(' | b')' => return None,
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(out)
}

fn parse_filter(s: &str) -> Option<Filter> {
    let (f, rest) = parse_one(s.trim())?;
    rest.is_empty().then_some(f)
}

fn parse_one(s: &str) -> Option<(Filter, &str)> {
    let s = s.strip_prefix('(')?;
    match s.chars().next()? {
        '&' | '|' => {
            let mut rest = &s[1..];
            let mut list = Vec::new();
            while rest.starts_with('(') {
                let (f, r) = parse_one(rest)?;
                list.push(f);
                rest = r;
            }
            let rest = rest.strip_prefix(')')?;
            Some((
                if s.starts_with('&') {
                    Filter::And(list)
                } else {
                    Filter::Or(list)
                },
                rest,
            ))
        }
        '!' => {
            let (f, r) = parse_one(&s[1..])?;
            Some((Filter::Not(Box::new(f)), r.strip_prefix(')')?))
        }
        _ => {
            // An item runs to the first ')' (values are escaped).
            let end = s.find(')')?;
            let item = &s[..end];
            let (left, value) = item.split_once('=')?;
            let f = if let Some(ext) = left.strip_suffix(':') {
                let (attr, rule) = ext.split_once(':')?;
                if rule != MATCHING_RULE_IN_CHAIN {
                    return None;
                }
                Filter::InChain(attr.to_ascii_lowercase(), unescape(value)?)
            } else if value == "*" {
                Filter::Present(left.to_ascii_lowercase())
            } else if value.contains('*') {
                let parts = value
                    .split('*')
                    .map(|p| {
                        if p.is_empty() {
                            Some(None)
                        } else {
                            unescape(p).map(Some)
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                Filter::Substring(left.to_ascii_lowercase(), parts)
            } else {
                Filter::Equal(left.to_ascii_lowercase(), unescape(value)?)
            };
            Some((f, &s[end + 1..]))
        }
    }
}

fn lower(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_lowercase()
}

fn is_dn_attr(a: &str) -> bool {
    matches!(a, "member" | "uniquemember" | "memberof")
}

fn values_equal(attr: &str, a: &[u8], b: &[u8]) -> bool {
    if is_dn_attr(attr) {
        normalize_dn(&String::from_utf8_lossy(a)) == normalize_dn(&String::from_utf8_lossy(b))
    } else if a.len() == 16 && b.len() == 16 && attr.eq_ignore_ascii_case("objectguid") {
        a == b
    } else {
        lower(a) == lower(b)
    }
}

fn substring_match(value: &str, parts: &[Option<Vec<u8>>]) -> bool {
    let mut pos = 0;
    let last = parts.len().saturating_sub(1);
    for (i, p) in parts.iter().enumerate() {
        let Some(p) = p else { continue };
        let p = lower(p);
        if i == 0 {
            if !value.starts_with(&p) {
                return false;
            }
            pos = p.len();
        } else if i == last {
            return value.len() >= pos + p.len() && value[pos..].ends_with(&p);
        } else {
            match value[pos..].find(&p) {
                Some(at) => pos += at + p.len(),
                None => return false,
            }
        }
    }
    true
}

/// Whether `target` is a member of `group`, directly or through nested groups.
fn in_chain(
    group: &FakeEntry,
    target: &str,
    all: &[FakeEntry],
    seen: &mut HashSet<String>,
) -> bool {
    if !seen.insert(normalize_dn(&group.dn)) {
        return false;
    }
    for m in group
        .attrs
        .get("member")
        .map(|v| v.as_slice())
        .unwrap_or(&[])
    {
        let m = normalize_dn(&String::from_utf8_lossy(m));
        if m == target {
            return true;
        }
        if let Some(sub) = all.iter().find(|e| normalize_dn(&e.dn) == m) {
            if in_chain(sub, target, all, seen) {
                return true;
            }
        }
    }
    false
}

fn eval(f: &Filter, e: &FakeEntry, all: &[FakeEntry]) -> bool {
    let vals = |a: &str| e.attrs.get(a).map(|v| v.as_slice()).unwrap_or(&[]);
    match f {
        Filter::And(l) => l.iter().all(|x| eval(x, e, all)),
        Filter::Or(l) => l.iter().any(|x| eval(x, e, all)),
        Filter::Not(x) => !eval(x, e, all),
        Filter::Present(a) => !vals(a).is_empty(),
        Filter::Equal(a, v) => vals(a).iter().any(|x| values_equal(a, x, v)),
        Filter::Substring(a, parts) => vals(a).iter().any(|x| substring_match(&lower(x), parts)),
        Filter::InChain(a, v) => {
            a == "member"
                && in_chain(
                    e,
                    &normalize_dn(&String::from_utf8_lossy(v)),
                    all,
                    &mut HashSet::new(),
                )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::directory::escape::render_user_filter;

    fn dir() -> FakeDirectory {
        let d = FakeDirectory::new();
        d.add_ad_user(
            "CN=Alice,OU=Users,DC=x",
            "alice",
            uuid::Uuid::new_v4(),
            None,
            "pw",
        );
        d.add_ad_user(
            "CN=Bob (IT),OU=Users,DC=x",
            "bob",
            uuid::Uuid::new_v4(),
            None,
            "pw",
        );
        d
    }

    async fn count(d: &FakeDirectory, filter: &str) -> usize {
        d.search("DC=x", filter, &[], 100).await.unwrap().len()
    }

    #[tokio::test]
    async fn filters_are_evaluated() {
        let d = dir();
        assert_eq!(count(&d, "(sAMAccountName=alice)").await, 1);
        assert_eq!(count(&d, "(sAMAccountName=*)").await, 2);
        assert_eq!(count(&d, "(sAMAccountName=a*)").await, 1);
        assert_eq!(
            count(&d, "(&(objectClass=user)(!(sAMAccountName=bob)))").await,
            1
        );
        assert_eq!(
            count(&d, "(|(sAMAccountName=alice)(sAMAccountName=bob))").await,
            2
        );
    }

    #[tokio::test]
    async fn escaped_injection_matches_nothing() {
        let d = dir();
        let tpl = "(&(objectClass=user)(sAMAccountName={u}))";
        // Unescaped, this would match every user.
        assert_eq!(
            count(&d, "(&(objectClass=user)(sAMAccountName=*))").await,
            2
        );
        for evil in ["*", "*)(sAMAccountName=*", "alice)(|(objectClass=*", "a*"] {
            assert_eq!(count(&d, &render_user_filter(tpl, evil)).await, 0, "{evil}");
        }
        assert_eq!(count(&d, &render_user_filter(tpl, "alice")).await, 1);
    }

    #[tokio::test]
    async fn nested_membership_follows_the_chain() {
        let d = dir();
        d.add_ad_group(
            "CN=Inner,DC=x",
            "Inner",
            uuid::Uuid::new_v4(),
            &["CN=Alice,OU=Users,DC=x"],
        );
        d.add_ad_group(
            "CN=Outer,DC=x",
            "Outer",
            uuid::Uuid::new_v4(),
            &["CN=Inner,DC=x"],
        );
        let f = format!(
            "(&(objectClass=group)(member:{MATCHING_RULE_IN_CHAIN}:=CN=Alice,OU=Users,DC=x))"
        );
        assert_eq!(count(&d, &f).await, 2);
        assert_eq!(
            count(&d, "(&(objectClass=group)(member=CN=Alice,OU=Users,DC=x))").await,
            1
        );
    }
}
