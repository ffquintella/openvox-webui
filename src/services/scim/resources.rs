//! SCIM resources: the attributes the host keeps, their JSON (in and out),
//! validation and the `attributes`/`excludedAttributes` projection.
//!
//! Only the attributes below are stored. Other core attributes and extension
//! schemas (enterprise user) are accepted and ignored, and `password` is
//! never stored: a SCIM account has no local password.

use serde_json::{json, Map, Value};

use super::filter::{normalize_attr, parse_path, PatchPath};
use super::{ScimError, GROUP_SCHEMA, PLACEHOLDER_EMAIL_DOMAIN, USER_SCHEMA};

pub const MAX_NAME_LEN: usize = 256;
pub const MAX_EXTERNAL_ID_LEN: usize = 512;
pub const MAX_EMAIL_LEN: usize = 254;

/// A provisioned user as stored (`users` joined with `scim_users`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub email: String,
    pub deactivated_at: Option<String>,
    pub external_id: Option<String>,
    pub display_name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub formatted_name: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GroupRow {
    pub id: String,
    pub display_name: String,
    pub external_id: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A group as seen from one of its members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupRef {
    pub id: String,
    pub display_name: String,
    pub external_id: Option<String>,
}

/// The user attributes the host keeps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserAttrs {
    pub user_name: String,
    pub external_id: Option<String>,
    pub display_name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub formatted_name: Option<String>,
    /// `None` when the client sent no e-mail (a placeholder is stored).
    pub email: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupAttrs {
    pub display_name: String,
    pub external_id: Option<String>,
}

pub fn is_placeholder_email(email: &str) -> bool {
    email
        .to_ascii_lowercase()
        .ends_with(&format!("@{PLACEHOLDER_EMAIL_DOMAIN}"))
}

pub fn placeholder_email(user_id: &str) -> String {
    format!("{user_id}@{PLACEHOLDER_EMAIL_DOMAIN}")
}

pub fn etag(version: i64) -> String {
    format!("W/\"{version}\"")
}

impl UserAttrs {
    pub fn from_row(row: &UserRow) -> Self {
        Self {
            user_name: row.username.clone(),
            external_id: row.external_id.clone(),
            display_name: row.display_name.clone(),
            given_name: row.given_name.clone(),
            family_name: row.family_name.clone(),
            formatted_name: row.formatted_name.clone(),
            email: Some(row.email.clone()).filter(|e| !is_placeholder_email(e)),
            active: row.deactivated_at.is_none(),
        }
    }

    /// Names of the attributes that differ (for audit and events).
    pub fn changed_fields(&self, other: &UserAttrs) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.user_name != other.user_name {
            out.push("userName");
        }
        if self.external_id != other.external_id {
            out.push("externalId");
        }
        if self.display_name != other.display_name {
            out.push("displayName");
        }
        if (&self.given_name, &self.family_name, &self.formatted_name)
            != (&other.given_name, &other.family_name, &other.formatted_name)
        {
            out.push("name");
        }
        if self.email != other.email {
            out.push("emails");
        }
        if self.active != other.active {
            out.push("active");
        }
        out
    }

    pub fn validate(&self) -> Result<(), ScimError> {
        check_text("userName", &self.user_name, MAX_NAME_LEN)?;
        if self.user_name.trim() != self.user_name {
            return Err(ScimError::invalid_value(
                "userName must not start or end with spaces",
            ));
        }
        if let Some(x) = &self.external_id {
            check_text("externalId", x, MAX_EXTERNAL_ID_LEN)?;
        }
        for (n, v) in [
            ("displayName", &self.display_name),
            ("name.givenName", &self.given_name),
            ("name.familyName", &self.family_name),
            ("name.formatted", &self.formatted_name),
        ] {
            if let Some(v) = v {
                check_text(n, v, MAX_NAME_LEN)?;
            }
        }
        if let Some(e) = &self.email {
            let ok = e.len() <= MAX_EMAIL_LEN
                && e.matches('@').count() == 1
                && !e.starts_with('@')
                && !e.ends_with('@')
                && !e.chars().any(|c| c.is_whitespace() || c.is_control())
                && !is_placeholder_email(e);
            if !ok {
                return Err(ScimError::invalid_value(format!(
                    "'{}' is not a valid e-mail address",
                    e.chars().take(64).collect::<String>()
                )));
            }
        }
        Ok(())
    }
}

impl GroupAttrs {
    pub fn from_row(row: &GroupRow) -> Self {
        Self {
            display_name: row.display_name.clone(),
            external_id: row.external_id.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), ScimError> {
        check_text("displayName", &self.display_name, MAX_NAME_LEN)?;
        if let Some(x) = &self.external_id {
            check_text("externalId", x, MAX_EXTERNAL_ID_LEN)?;
        }
        Ok(())
    }
}

fn check_text(name: &str, v: &str, max: usize) -> Result<(), ScimError> {
    if v.is_empty() || v.chars().count() > max || v.chars().any(|c| c.is_control()) {
        return Err(ScimError::invalid_value(format!(
            "{name} must have 1 to {max} printable characters"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------- input

pub fn get_ci<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}

fn string_value(name: &str, v: &Value) -> Result<String, ScimError> {
    match v {
        Value::String(s) => Ok(s.trim().to_string()),
        _ => Err(ScimError::invalid_value(format!("{name} must be a string"))),
    }
}

/// A string, or `None` for `null` or an empty string.
fn optional_string(name: &str, v: &Value) -> Result<Option<String>, ScimError> {
    match v {
        Value::Null => Ok(None),
        _ => Ok(Some(string_value(name, v)?).filter(|s| !s.is_empty())),
    }
}

/// `true`/`false`, also as the strings `"True"`/`"False"` some clients send.
pub fn bool_value(name: &str, v: &Value) -> Result<bool, ScimError> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::String(s) if s.eq_ignore_ascii_case("true") => Ok(true),
        Value::String(s) if s.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err(ScimError::invalid_value(format!(
            "{name} must be a boolean"
        ))),
    }
}

/// The e-mail to keep from a list of SCIM emails: the primary one, else the
/// work one, else the first.
fn pick_email(v: &Value) -> Result<Option<String>, ScimError> {
    let items: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![v],
        Value::Null => vec![],
        _ => return Err(ScimError::invalid_value("emails must be a list")),
    };
    let value_of = |e: &Value| -> Result<Option<String>, ScimError> {
        match get_ci(e, "value") {
            Some(x) => optional_string("emails.value", x),
            None => Ok(None),
        }
    };
    if items.iter().any(|e| !e.is_object()) {
        return Err(ScimError::invalid_value("emails must hold objects"));
    }
    let primary = items
        .iter()
        .find(|e| get_ci(e, "primary").is_some_and(|x| bool_value("primary", x).unwrap_or(false)));
    let work = items.iter().find(|e| {
        get_ci(e, "type")
            .and_then(Value::as_str)
            .is_some_and(|t| t.eq_ignore_ascii_case("work"))
    });
    for e in primary.into_iter().chain(work).chain(items.iter()) {
        if let Some(v) = value_of(e)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// The synthetic e-mail element the host holds, for value filters.
fn email_element(attrs: &UserAttrs) -> Value {
    json!({"value": attrs.email.clone().unwrap_or_default(), "type": "work", "primary": true})
}

/// Whether an `emails[...]` filter designates the e-mail the host keeps
/// (other e-mail types are not stored).
fn targets_kept_email(attrs: &UserAttrs, p: &PatchPath) -> bool {
    match &p.filter {
        None => true,
        Some(f) => attrs.email.is_none() || super::filter::matches(f, &email_element(attrs)),
    }
}

/// Operation kind for [`set_user_path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetMode {
    Add,
    Replace,
}

fn set_name_sub(attrs: &mut UserAttrs, sub: &str, v: &Value) -> Result<(), ScimError> {
    let value = optional_string("name", v)?;
    match sub {
        "givenname" => attrs.given_name = value,
        "familyname" => attrs.family_name = value,
        "formatted" => attrs.formatted_name = value,
        _ => {} // middleName, honorificPrefix, ...: not stored
    }
    Ok(())
}

/// Sets one attribute of a user (POST/PUT bodies, PATCH add and replace).
pub fn set_user_path(
    attrs: &mut UserAttrs,
    p: &PatchPath,
    v: &Value,
    mode: SetMode,
) -> Result<(), ScimError> {
    if p.extension {
        return Ok(());
    }
    match p.attr.as_str() {
        "username" => {
            let s = string_value("userName", v)?;
            if s.is_empty() {
                return Err(ScimError::invalid_value("userName must not be empty"));
            }
            attrs.user_name = s;
        }
        "externalid" => attrs.external_id = optional_string("externalId", v)?,
        "displayname" => attrs.display_name = optional_string("displayName", v)?,
        "active" => attrs.active = bool_value("active", v)?,
        "name" => match &p.sub {
            Some(sub) => set_name_sub(attrs, sub, v)?,
            None => match v {
                Value::Object(o) => {
                    for (k, x) in o {
                        set_name_sub(attrs, &k.to_ascii_lowercase(), x)?;
                    }
                }
                Value::Null => {
                    attrs.given_name = None;
                    attrs.family_name = None;
                    attrs.formatted_name = None;
                }
                _ => return Err(ScimError::invalid_value("name must be an object")),
            },
        },
        "emails" => {
            if p.filter.is_some() || p.sub.is_some() {
                let sub = p.sub.as_deref().unwrap_or("value");
                if sub == "value" && targets_kept_email(attrs, p) {
                    attrs.email = match v {
                        Value::Object(_) | Value::Array(_) => pick_email(v)?,
                        _ => optional_string("emails.value", v)?,
                    };
                }
            } else {
                let picked = pick_email(v)?;
                match mode {
                    SetMode::Replace => attrs.email = picked,
                    SetMode::Add => {
                        if picked.is_some() {
                            attrs.email = picked;
                        }
                    }
                }
            }
        }
        "groups" => {
            return Err(ScimError::mutability(
                "groups is read-only: change memberships through the Groups resource",
            ))
        }
        "id" | "meta" => {
            return Err(ScimError::mutability(format!("{} is read-only", p.attr)));
        }
        _ => {} // not stored (title, phoneNumbers, password, ...)
    }
    Ok(())
}

/// Removes one attribute of a user (PATCH remove).
pub fn remove_user_path(attrs: &mut UserAttrs, p: &PatchPath) -> Result<(), ScimError> {
    if p.extension {
        return Ok(());
    }
    match p.attr.as_str() {
        "username" => return Err(ScimError::mutability("userName is required")),
        "externalid" => attrs.external_id = None,
        "displayname" => attrs.display_name = None,
        "name" => match p.sub.as_deref() {
            Some(sub) => set_name_sub(attrs, sub, &Value::Null)?,
            None => {
                attrs.given_name = None;
                attrs.family_name = None;
                attrs.formatted_name = None;
            }
        },
        "emails" => {
            if targets_kept_email(attrs, p) && matches!(p.sub.as_deref(), None | Some("value")) {
                attrs.email = None;
            }
        }
        "active" => {
            return Err(ScimError::invalid_value(
                "active cannot be removed; replace it with false",
            ))
        }
        "groups" | "id" | "meta" => {
            return Err(ScimError::mutability(format!("{} is read-only", p.attr)));
        }
        _ => {}
    }
    Ok(())
}

/// Applies every attribute of a JSON object (a POST/PUT body or a PATCH
/// value without path). Keys may be paths such as `name.givenName`.
pub fn set_user_object(
    attrs: &mut UserAttrs,
    obj: &Map<String, Value>,
    mode: SetMode,
    ignore_read_only: bool,
) -> Result<(), ScimError> {
    for (k, v) in obj {
        let lower = normalize_attr(k);
        if matches!(lower.as_str(), "schemas" | "password")
            || (ignore_read_only && matches!(lower.as_str(), "id" | "meta" | "groups"))
        {
            continue;
        }
        let p = parse_path(k)?;
        set_user_path(attrs, &p, v, mode)?;
    }
    Ok(())
}

/// User attributes from a POST or PUT body. A missing `active` keeps
/// `active_default`; every other attribute that is missing is cleared.
pub fn user_from_body(body: &Value, active_default: bool) -> Result<UserAttrs, ScimError> {
    let obj = body
        .as_object()
        .ok_or_else(|| ScimError::invalid_syntax("The body must be a JSON object"))?;
    if get_ci(body, "userName").is_none() {
        return Err(ScimError::invalid_value("userName is required"));
    }
    let mut attrs = UserAttrs {
        active: active_default,
        ..Default::default()
    };
    set_user_object(&mut attrs, obj, SetMode::Replace, true)?;
    attrs.validate()?;
    Ok(attrs)
}

/// One member value (`{"value": "<user id>"}`); nested groups are refused.
pub fn member_id(v: &Value) -> Result<String, ScimError> {
    if let Some(t) = get_ci(v, "type").and_then(Value::as_str) {
        if !t.eq_ignore_ascii_case("user") {
            return Err(ScimError::invalid_value(
                "Only users can be group members (nested groups are not supported)",
            ));
        }
    }
    let raw = get_ci(v, "value")
        .and_then(Value::as_str)
        .ok_or_else(|| ScimError::invalid_value("Each member needs a value"))?;
    uuid::Uuid::parse_str(raw.trim())
        .map(|u| u.to_string())
        .map_err(|_| ScimError::invalid_value(format!("Unknown member '{}'", short(raw))))
}

pub fn member_ids(v: &Value) -> Result<Vec<String>, ScimError> {
    match v {
        Value::Array(a) => a.iter().map(member_id).collect(),
        Value::Object(_) => Ok(vec![member_id(v)?]),
        Value::Null => Ok(vec![]),
        _ => Err(ScimError::invalid_value("members must be a list")),
    }
}

pub fn short(s: &str) -> String {
    s.chars().take(64).collect()
}

/// Group attributes and members from a POST or PUT body.
pub fn group_from_body(body: &Value) -> Result<(GroupAttrs, Vec<String>), ScimError> {
    if !body.is_object() {
        return Err(ScimError::invalid_syntax("The body must be a JSON object"));
    }
    let display_name = get_ci(body, "displayName")
        .map(|v| string_value("displayName", v))
        .transpose()?
        .unwrap_or_default();
    let external_id = match get_ci(body, "externalId") {
        Some(v) => optional_string("externalId", v)?,
        None => None,
    };
    let members = match get_ci(body, "members") {
        Some(v) => member_ids(v)?,
        None => vec![],
    };
    let attrs = GroupAttrs {
        display_name,
        external_id,
    };
    attrs.validate()?;
    Ok((attrs, members))
}

// ---------------------------------------------------------------- output

fn meta(kind: &str, location: String, created: &str, modified: &str, version: i64) -> Value {
    json!({
        "resourceType": kind,
        "created": created,
        "lastModified": modified,
        "location": location,
        "version": etag(version),
    })
}

pub fn user_json(row: &UserRow, groups: &[GroupRef], base: &str) -> Value {
    let mut v = json!({
        "schemas": [USER_SCHEMA],
        "id": row.id,
        "userName": row.username,
        "active": row.deactivated_at.is_none(),
        "meta": meta("User", format!("{base}/Users/{}", row.id), &row.created_at, &row.updated_at, row.version),
    });
    if let Some(x) = &row.external_id {
        v["externalId"] = json!(x);
    }
    if let Some(x) = &row.display_name {
        v["displayName"] = json!(x);
    }
    let mut name = Map::new();
    for (k, x) in [
        ("givenName", &row.given_name),
        ("familyName", &row.family_name),
        ("formatted", &row.formatted_name),
    ] {
        if let Some(x) = x {
            name.insert(k.into(), json!(x));
        }
    }
    if !name.is_empty() {
        v["name"] = Value::Object(name);
    }
    if !is_placeholder_email(&row.email) {
        v["emails"] = json!([{"value": row.email, "type": "work", "primary": true}]);
    }
    if !groups.is_empty() {
        v["groups"] = Value::Array(
            groups
                .iter()
                .map(|g| {
                    json!({"value": g.id, "display": g.display_name,
                           "$ref": format!("{base}/Groups/{}", g.id), "type": "direct"})
                })
                .collect(),
        );
    }
    v
}

/// `members`: `(user id, userName)`.
pub fn group_json(row: &GroupRow, members: &[(String, String)], base: &str) -> Value {
    let mut v = json!({
        "schemas": [GROUP_SCHEMA],
        "id": row.id,
        "displayName": row.display_name,
        "members": members
            .iter()
            .map(|(id, name)| json!({"value": id, "display": name,
                                     "$ref": format!("{base}/Users/{id}"), "type": "User"}))
            .collect::<Vec<_>>(),
        "meta": meta("Group", format!("{base}/Groups/{}", row.id), &row.created_at, &row.updated_at, row.version),
    });
    if let Some(x) = &row.external_id {
        v["externalId"] = json!(x);
    }
    v
}

fn attr_list(s: Option<&str>) -> Vec<String> {
    s.unwrap_or("")
        .split(',')
        .map(|a| normalize_attr(a.trim()))
        .filter(|a| !a.is_empty())
        .map(|a| a.split('.').next().unwrap_or("").to_string())
        .collect()
}

/// Applies `attributes` / `excludedAttributes` (top-level names; a dotted
/// name selects its parent). `schemas`, `id` and `meta` are always kept.
pub fn project(mut resource: Value, attributes: Option<&str>, excluded: Option<&str>) -> Value {
    let keep = attr_list(attributes);
    let drop = attr_list(excluded);
    if let Some(obj) = resource.as_object_mut() {
        obj.retain(|k, _| {
            let l = k.to_ascii_lowercase();
            if matches!(l.as_str(), "schemas" | "id" | "meta") {
                return true;
            }
            if !keep.is_empty() {
                return keep.contains(&l);
            }
            !drop.contains(&l)
        });
    }
    resource
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> UserRow {
        UserRow {
            id: "u1".into(),
            username: "alice@example.com".into(),
            email: "alice@example.com".into(),
            deactivated_at: None,
            external_id: Some("ext-1".into()),
            display_name: Some("Alice".into()),
            given_name: Some("Alice".into()),
            family_name: Some("Doe".into()),
            formatted_name: None,
            version: 3,
            created_at: "2026-10-30T10:00:00Z".into(),
            updated_at: "2026-10-30T11:00:00Z".into(),
        }
    }

    #[test]
    fn bodies_parse_with_case_insensitive_names_and_string_booleans() {
        let a = user_from_body(
            &json!({
                "schemas": [USER_SCHEMA],
                "UserName": "alice@example.com",
                "externalId": "ext-1",
                "name": {"givenName": "Alice", "familyName": "Doe", "middleName": "Q"},
                "emails": [
                    {"value": "home@example.org", "type": "home"},
                    {"value": "alice@example.com", "type": "work", "primary": true}
                ],
                "active": "False",
                "password": "never stored",
                "title": "ignored",
                "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User": {"department": "x"}
            }),
            true,
        )
        .unwrap();
        assert_eq!(a.user_name, "alice@example.com");
        assert_eq!(a.email.as_deref(), Some("alice@example.com"));
        assert_eq!(a.given_name.as_deref(), Some("Alice"));
        assert!(!a.active);
        assert!(user_from_body(&json!({"active": true}), true).is_err());
        assert!(user_from_body(&json!({"userName": ""}), true).is_err());
        assert!(user_from_body(&json!({"userName": 3}), true).is_err());
        assert!(user_from_body(&json!({"userName": "a\u{0}b"}), true).is_err());
        assert!(user_from_body(
            &json!({"userName": "a", "emails": [{"value": "nope"}]}),
            true
        )
        .is_err());
        assert!(user_from_body(
            &json!({"userName": "a", "emails": [{"value": "x@scim.invalid"}]}),
            true
        )
        .is_err());
        assert!(
            user_from_body(&json!({"userName": "a", "groups": [{"value": "g"}]}), true).is_ok()
        );
        assert!(user_from_body(&json!([]), true).is_err());
    }

    #[test]
    fn email_filters_only_touch_the_kept_address() {
        let mut a = UserAttrs::from_row(&row());
        let work = parse_path(r#"emails[type eq "work"].value"#).unwrap();
        set_user_path(&mut a, &work, &json!("new@example.com"), SetMode::Replace).unwrap();
        assert_eq!(a.email.as_deref(), Some("new@example.com"));
        let home = parse_path(r#"emails[type eq "home"].value"#).unwrap();
        set_user_path(&mut a, &home, &json!("home@example.org"), SetMode::Replace).unwrap();
        assert_eq!(a.email.as_deref(), Some("new@example.com"));
        remove_user_path(&mut a, &home).unwrap();
        assert!(a.email.is_some());
        remove_user_path(&mut a, &parse_path("emails").unwrap()).unwrap();
        assert!(a.email.is_none());
    }

    #[test]
    fn read_only_and_required_attributes_are_protected() {
        let mut a = UserAttrs::from_row(&row());
        assert_eq!(
            remove_user_path(&mut a, &parse_path("userName").unwrap())
                .unwrap_err()
                .scim_type,
            Some("mutability")
        );
        assert_eq!(
            set_user_path(
                &mut a,
                &parse_path("groups").unwrap(),
                &json!([]),
                SetMode::Add
            )
            .unwrap_err()
            .scim_type,
            Some("mutability")
        );
        assert!(set_user_path(
            &mut a,
            &parse_path("id").unwrap(),
            &json!("x"),
            SetMode::Replace
        )
        .is_err());
        set_user_path(
            &mut a,
            &parse_path("title").unwrap(),
            &json!("CTO"),
            SetMode::Replace,
        )
        .unwrap();
        assert_eq!(a, UserAttrs::from_row(&row()));
    }

    #[test]
    fn json_output_hides_placeholders_and_carries_meta() {
        let mut r = row();
        let g = GroupRef {
            id: "g1".into(),
            display_name: "Ops".into(),
            external_id: None,
        };
        let v = user_json(&r, &[g], "/api/v1/scim/v2");
        assert_eq!(v["meta"]["version"], "W/\"3\"");
        assert_eq!(v["meta"]["location"], "/api/v1/scim/v2/Users/u1");
        assert_eq!(v["groups"][0]["display"], "Ops");
        assert_eq!(v["emails"][0]["value"], "alice@example.com");
        r.email = placeholder_email("u1");
        r.deactivated_at = Some("2026-10-30T12:00:00Z".into());
        let v = user_json(&r, &[], "");
        assert!(v.get("emails").is_none());
        assert_eq!(v["active"], false);
        let p = project(v.clone(), Some("userName"), None);
        assert!(p.get("userName").is_some() && p.get("id").is_some() && p.get("active").is_none());
        let p = project(v, None, Some("name, displayName"));
        assert!(p.get("name").is_none() && p.get("userName").is_some());
    }

    #[test]
    fn members_must_be_users_with_uuids() {
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            member_ids(&json!([{"value": id}])).unwrap(),
            vec![id.clone()]
        );
        assert_eq!(
            member_ids(&json!({"value": id.to_uppercase()})).unwrap(),
            vec![id]
        );
        assert!(member_ids(&json!([{"value": "not-a-uuid"}])).is_err());
        assert!(
            member_ids(&json!([{"value": uuid::Uuid::new_v4().to_string(), "type": "Group"}]))
                .is_err()
        );
        assert!(member_ids(&json!("x")).is_err());
        let (g, m) = group_from_body(&json!({"displayName": "Ops", "externalId": ""})).unwrap();
        assert_eq!(g.display_name, "Ops");
        assert!(g.external_id.is_none() && m.is_empty());
        assert!(group_from_body(&json!({"externalId": "x"})).is_err());
    }
}
