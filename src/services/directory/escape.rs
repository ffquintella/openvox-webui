//! Escaping (RFC 4515 filters, RFC 4514 DNs), login name parsing and Active
//! Directory value decoding. Pure functions, unit tested.

use chrono::{DateTime, Utc};

/// Escapes a value for use inside an LDAP search filter (RFC 4515 §3):
/// `*`, `(`, `)`, `\` and NUL, plus every other control character.
pub fn escape_filter_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        let code = c as u32;
        if matches!(c, '*' | '(' | ')' | '\\') || code < 0x20 || code == 0x7f {
            out.push_str(&format!("\\{code:02x}"));
        } else {
            out.push(c);
        }
    }
    out
}

/// Escapes an attribute value for use inside a distinguished name
/// (RFC 4514 §2.4). Search-then-bind never builds a DN from user input (it
/// binds with the DN the server returned); this is used for host-built DNs.
pub fn escape_dn_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    let n = value.chars().count();
    for (i, c) in value.chars().enumerate() {
        match c {
            ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=' => {
                out.push('\\');
                out.push(c);
            }
            '\0' => out.push_str("\\00"),
            '#' if i == 0 => out.push_str("\\#"),
            ' ' if i == 0 || i + 1 == n => out.push_str("\\ "),
            c if c.is_control() => out.push_str(&format!("\\{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Replaces every `{u}` in a filter template with the escaped login name.
pub fn render_user_filter(template: &str, login: &str) -> String {
    template.replace("{u}", &escape_filter_value(login))
}

/// The forms a user may type: `user`, `user@realm` and `REALM\user`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginName {
    pub user: String,
    pub realm: Option<String>,
}

pub const MAX_LOGIN_LEN: usize = 256;

/// Parses a login. Returns `None` for an empty or oversized name, control
/// characters or a malformed qualified form.
pub fn parse_login(input: &str) -> Option<LoginName> {
    let s = input.trim();
    if s.is_empty() || s.len() > MAX_LOGIN_LEN || s.chars().any(|c| c.is_control()) {
        return None;
    }
    if let Some((realm, user)) = s.split_once('\\') {
        if realm.is_empty() || user.is_empty() || user.contains('\\') {
            return None;
        }
        return Some(LoginName {
            user: user.to_string(),
            realm: Some(realm.to_string()),
        });
    }
    if let Some((user, realm)) = s.rsplit_once('@') {
        if user.is_empty() || realm.is_empty() {
            return None;
        }
        return Some(LoginName {
            user: user.to_string(),
            realm: Some(realm.to_string()),
        });
    }
    Some(LoginName {
        user: s.to_string(),
        realm: None,
    })
}

/// The name a directory searches for, or `None` when the directory does not
/// handle this login. Without configured `domains` no realm is stripped: the
/// whole input is the login name.
pub fn login_for_directory(input: &str, domains: &[String]) -> Option<String> {
    let parsed = parse_login(input)?;
    if domains.is_empty() {
        return Some(input.trim().to_string());
    }
    match parsed.realm {
        None => Some(parsed.user),
        Some(r) if domains.iter().any(|d| d.eq_ignore_ascii_case(&r)) => Some(parsed.user),
        Some(_) => None,
    }
}

// --------------------------------------------------------- Active Directory

pub const UAC_ACCOUNTDISABLE: u32 = 0x0002;
pub const UAC_LOCKOUT: u32 = 0x0010;
/// `accountExpires` value meaning "never" (besides 0).
pub const AD_NEVER: i64 = i64::MAX;
/// Seconds between 1601-01-01 and 1970-01-01.
const FILETIME_EPOCH_OFFSET: i64 = 11_644_473_600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountState {
    Active,
    Disabled,
    Expired,
    Locked,
}

impl AccountState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountState::Active => "active",
            AccountState::Disabled => "account_disabled",
            AccountState::Expired => "account_expired",
            AccountState::Locked => "account_locked",
        }
    }
}

/// Converts a time to Windows FILETIME (100 ns intervals since 1601).
pub fn to_filetime(t: DateTime<Utc>) -> i64 {
    (t.timestamp() + FILETIME_EPOCH_OFFSET) * 10_000_000 + (t.timestamp_subsec_nanos() as i64 / 100)
}

/// Parses an AD integer attribute (stored as text; `userAccountControl` is a
/// signed 32-bit value).
pub fn parse_ad_int(v: Option<&str>) -> Option<i64> {
    v.and_then(|s| s.trim().parse::<i64>().ok())
}

/// Decides whether an AD account may authenticate.
///
/// * `userAccountControl` bit `ACCOUNTDISABLE`.
/// * `accountExpires` in the past (0 and `i64::MAX` mean never).
/// * Lockout: the constructed `msDS-User-Account-Control-Computed` bit
///   `UF_LOCKOUT` when the server returns it, which already accounts for an
///   expired lockout duration; otherwise any non-zero `lockoutTime`.
pub fn ad_account_state(
    uac: Option<i64>,
    computed: Option<i64>,
    account_expires: Option<i64>,
    lockout_time: Option<i64>,
    now: DateTime<Utc>,
) -> AccountState {
    let uac = uac.map(|v| v as u32).unwrap_or(0);
    if uac & UAC_ACCOUNTDISABLE != 0 {
        return AccountState::Disabled;
    }
    if let Some(exp) = account_expires {
        if exp != 0 && exp != AD_NEVER && exp <= to_filetime(now) {
            return AccountState::Expired;
        }
    }
    let locked = match computed {
        Some(c) => (c as u32) & UAC_LOCKOUT != 0,
        None => lockout_time.unwrap_or(0) > 0,
    };
    if locked {
        return AccountState::Locked;
    }
    AccountState::Active
}

/// OpenLDAP (ppolicy `pwdAccountLockedTime`) and 389-ds (`nsAccountLock`).
pub fn ldap_account_state(
    ns_account_lock: Option<&str>,
    pwd_locked_time: Option<&str>,
) -> AccountState {
    if ns_account_lock
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return AccountState::Disabled;
    }
    if pwd_locked_time
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return AccountState::Locked;
    }
    AccountState::Active
}

/// AD `objectGUID` (16 bytes, first three fields little-endian) as the
/// canonical lowercase string.
pub fn guid_to_string(bytes: &[u8]) -> Option<String> {
    let arr: [u8; 16] = bytes.try_into().ok()?;
    Some(uuid::Uuid::from_bytes_le(arr).to_string())
}

/// Binary SID to `S-1-5-21-...`.
pub fn sid_to_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 8 {
        return None;
    }
    let revision = bytes[0];
    let count = bytes[1] as usize;
    if bytes.len() != 8 + 4 * count {
        return None;
    }
    let mut authority: u64 = 0;
    for b in &bytes[2..8] {
        authority = (authority << 8) | *b as u64;
    }
    let mut s = format!("S-{revision}-{authority}");
    for i in 0..count {
        let o = 8 + 4 * i;
        let sub = u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        s.push_str(&format!("-{sub}"));
    }
    Some(s)
}

/// Splits on separators that are not escaped with a backslash.
fn split_unescaped(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if c == '\\' {
            cur.push(c);
            escaped = true;
        } else if c == sep {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    parts.push(cur);
    parts
}

/// DN in a comparable form: case-insensitive, without spaces around `,` and
/// `=`. Good enough for matching mappings against what the server returns.
pub fn normalize_dn(dn: &str) -> String {
    split_unescaped(dn.trim(), ',')
        .into_iter()
        .map(|rdn| {
            let parts = split_unescaped(&rdn, '=');
            let (t, v) = match parts.split_first() {
                Some((t, rest)) if !rest.is_empty() => (t.clone(), rest.join("=")),
                _ => (rdn.clone(), String::new()),
            };
            format!("{}={}", t.trim().to_lowercase(), v.trim().to_lowercase())
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// What a mapping's group reference is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRefKind {
    Dn,
    Sid,
    Guid,
}

impl GroupRefKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupRefKind::Dn => "dn",
            GroupRefKind::Sid => "sid",
            GroupRefKind::Guid => "guid",
        }
    }
}

/// Classifies and canonicalizes a group reference typed by an administrator.
pub fn parse_group_ref(input: &str) -> Option<(GroupRefKind, String)> {
    let s = input.trim();
    if s.is_empty() || s.len() > 1024 || s.chars().any(|c| c.is_control()) {
        return None;
    }
    let upper = s.to_ascii_uppercase();
    if upper.starts_with("S-1-")
        && upper[4..]
            .split('-')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        return Some((GroupRefKind::Sid, upper));
    }
    if let Ok(u) = uuid::Uuid::parse_str(s.trim_matches(|c| c == '{' || c == '}')) {
        return Some((GroupRefKind::Guid, u.to_string()));
    }
    if s.contains('=') {
        return Some((GroupRefKind::Dn, normalize_dn(s)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn filter_values_are_escaped_per_rfc_4515() {
        assert_eq!(escape_filter_value("john"), "john");
        assert_eq!(escape_filter_value("*)(uid=*"), "\\2a\\29\\28uid=\\2a");
        assert_eq!(escape_filter_value("a\\b"), "a\\5cb");
        assert_eq!(escape_filter_value("x\0y"), "x\\00y");
        assert_eq!(escape_filter_value("*"), "\\2a");
        assert_eq!(escape_filter_value("ação(1)"), "ação\\281\\29");
        // The classic injections cannot close the filter.
        for evil in [
            "*)(uid=*))(|(uid=*",
            "admin)(&)",
            "*)(|(objectClass=*",
            "\\2a",
        ] {
            let f = render_user_filter("(&(objectClass=user)(uid={u}))", evil);
            assert!(!escape_filter_value(evil).contains('('), "{f}");
            assert!(!escape_filter_value(evil).contains(')'), "{f}");
            assert!(!escape_filter_value(evil).contains('*'), "{f}");
            assert_eq!(f.matches('(').count(), 3);
        }
    }

    #[test]
    fn dn_values_are_escaped_per_rfc_4514() {
        assert_eq!(escape_dn_value("Smith, John"), "Smith\\, John");
        assert_eq!(escape_dn_value("a+b=c"), "a\\+b\\=c");
        assert_eq!(escape_dn_value("#lead"), "\\#lead");
        assert_eq!(escape_dn_value(" pad "), "\\ pad\\ ");
        assert_eq!(escape_dn_value("x\"<>;\\"), "x\\\"\\<\\>\\;\\\\");
        assert_eq!(escape_dn_value("n\0l"), "n\\00l");
        assert_eq!(escape_dn_value("cn=admin,dc=x"), "cn\\=admin\\,dc\\=x");
    }

    #[test]
    fn logins_are_parsed_in_the_three_forms() {
        assert_eq!(
            parse_login(" alice "),
            Some(LoginName {
                user: "alice".into(),
                realm: None
            })
        );
        assert_eq!(
            parse_login("alice@example.com"),
            Some(LoginName {
                user: "alice".into(),
                realm: Some("example.com".into())
            })
        );
        assert_eq!(
            parse_login("EXAMPLE\\alice"),
            Some(LoginName {
                user: "alice".into(),
                realm: Some("EXAMPLE".into())
            })
        );
        assert_eq!(parse_login(""), None);
        assert_eq!(parse_login("   "), None);
        assert_eq!(parse_login("\\alice"), None);
        assert_eq!(parse_login("EX\\"), None);
        assert_eq!(parse_login("@example.com"), None);
        assert_eq!(parse_login("a\\b\\c"), None);
        assert_eq!(parse_login("ali\nce"), None);
        assert_eq!(parse_login(&"a".repeat(MAX_LOGIN_LEN + 1)), None);
    }

    #[test]
    fn realms_select_the_directory() {
        let domains = vec!["EXAMPLE".to_string(), "example.com".to_string()];
        assert_eq!(
            login_for_directory("alice", &domains).as_deref(),
            Some("alice")
        );
        assert_eq!(
            login_for_directory("example\\alice", &domains).as_deref(),
            Some("alice")
        );
        assert_eq!(
            login_for_directory("alice@EXAMPLE.COM", &domains).as_deref(),
            Some("alice")
        );
        assert_eq!(login_for_directory("OTHER\\alice", &domains), None);
        assert_eq!(login_for_directory("alice@other.org", &domains), None);
        // Without domains the whole input is the login name.
        assert_eq!(
            login_for_directory("alice@x.org", &[]).as_deref(),
            Some("alice@x.org")
        );
        assert_eq!(login_for_directory("", &[]), None);
    }

    #[test]
    fn ad_account_flags_are_decoded() {
        let now = Utc.with_ymd_and_hms(2026, 10, 30, 12, 0, 0).unwrap();
        let past = to_filetime(now - chrono::Duration::days(1));
        let future = to_filetime(now + chrono::Duration::days(1));
        use AccountState::*;
        assert_eq!(
            ad_account_state(Some(512), Some(0), Some(0), Some(0), now),
            Active
        );
        assert_eq!(ad_account_state(Some(514), None, None, None, now), Disabled);
        // userAccountControl is signed 32-bit in AD.
        assert_eq!(
            ad_account_state(Some(-2147483646), None, None, None, now),
            Disabled
        );
        assert_eq!(
            ad_account_state(Some(512), None, Some(past), None, now),
            Expired
        );
        assert_eq!(
            ad_account_state(Some(512), None, Some(future), None, now),
            Active
        );
        assert_eq!(
            ad_account_state(Some(512), None, Some(AD_NEVER), None, now),
            Active
        );
        assert_eq!(
            ad_account_state(Some(512), Some(0x10), None, None, now),
            Locked
        );
        // The computed attribute wins over a stale lockoutTime.
        assert_eq!(
            ad_account_state(Some(512), Some(0), None, Some(1234), now),
            Active
        );
        assert_eq!(
            ad_account_state(Some(512), None, None, Some(1234), now),
            Locked
        );
        assert_eq!(parse_ad_int(Some(" 514 ")), Some(514));
        assert_eq!(parse_ad_int(Some("x")), None);
        // 2026-10-30T12:00:00Z in FILETIME.
        assert_eq!(
            to_filetime(now),
            (now.timestamp() + 11_644_473_600) * 10_000_000
        );
    }

    #[test]
    fn ldap_lock_attributes_are_decoded() {
        assert_eq!(ldap_account_state(None, None), AccountState::Active);
        assert_eq!(
            ldap_account_state(Some("TRUE"), None),
            AccountState::Disabled
        );
        assert_eq!(
            ldap_account_state(Some("false"), None),
            AccountState::Active
        );
        assert_eq!(
            ldap_account_state(None, Some("000001010000Z")),
            AccountState::Locked
        );
    }

    #[test]
    fn guids_and_sids_are_formatted() {
        // objectGUID bytes of {33221100-5544-7766-8899-aabbccddeeff}
        let b = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        assert_eq!(
            guid_to_string(&b).unwrap(),
            "33221100-5544-7766-8899-aabbccddeeff"
        );
        assert_eq!(guid_to_string(&b[..15]), None);
        // S-1-5-21-1-2-3-512
        let mut sid = vec![1u8, 5, 0, 0, 0, 0, 0, 5];
        for sub in [21u32, 1, 2, 3, 512] {
            sid.extend_from_slice(&sub.to_le_bytes());
        }
        assert_eq!(sid_to_string(&sid).unwrap(), "S-1-5-21-1-2-3-512");
        assert_eq!(sid_to_string(&sid[..10]), None);
    }

    #[test]
    fn dns_and_group_refs_are_normalized() {
        assert_eq!(
            normalize_dn("CN=Ops Team , OU=Groups,DC=Example,DC=com"),
            "cn=ops team,ou=groups,dc=example,dc=com"
        );
        assert_eq!(
            normalize_dn("CN=Smith\\, John,DC=x"),
            "cn=smith\\, john,dc=x"
        );
        assert_eq!(
            parse_group_ref("CN=Admins,DC=x").unwrap(),
            (GroupRefKind::Dn, "cn=admins,dc=x".into())
        );
        assert_eq!(
            parse_group_ref("s-1-5-21-1-2-3-512").unwrap(),
            (GroupRefKind::Sid, "S-1-5-21-1-2-3-512".into())
        );
        assert_eq!(
            parse_group_ref("{33221100-5544-7766-8899-AABBCCDDEEFF}").unwrap(),
            (
                GroupRefKind::Guid,
                "33221100-5544-7766-8899-aabbccddeeff".into()
            )
        );
        assert_eq!(parse_group_ref("just a name"), None);
        assert_eq!(parse_group_ref(""), None);
    }
}
