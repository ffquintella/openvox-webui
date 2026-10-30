//! SCIM filters (RFC 7644 §3.4.2.2) and PATCH paths (§3.5.2).
//!
//! The whole grammar is parsed (`eq ne co sw ew gt ge lt le pr`, `and`, `or`,
//! `not`, grouping and value paths) and evaluated in memory against the JSON
//! of a resource. Attribute names are case-insensitive; the core schema URN
//! prefix is accepted and dropped.

use serde_json::Value;

use super::{ScimError, GROUP_SCHEMA, USER_SCHEMA};

/// Longest filter accepted, in bytes.
pub const MAX_FILTER_LEN: usize = 4096;
/// Deepest nesting of parentheses, `not` and value paths.
const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Co,
    Sw,
    Ew,
    Gt,
    Ge,
    Lt,
    Le,
}

impl CompareOp {
    fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_lowercase().as_str() {
            "eq" => CompareOp::Eq,
            "ne" => CompareOp::Ne,
            "co" => CompareOp::Co,
            "sw" => CompareOp::Sw,
            "ew" => CompareOp::Ew,
            "gt" => CompareOp::Gt,
            "ge" => CompareOp::Ge,
            "lt" => CompareOp::Lt,
            "le" => CompareOp::Le,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Str(String),
    Bool(bool),
    Num(f64),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    /// `attr op value`; `attr` is normalized (lowercase, no core URN).
    Compare {
        attr: String,
        op: CompareOp,
        value: FilterValue,
    },
    /// `attr pr`
    Present(String),
    And(Box<Filter>, Box<Filter>),
    Or(Box<Filter>, Box<Filter>),
    Not(Box<Filter>),
    /// `attr[filter]`: some value of a multi-valued attribute matches.
    ValuePath(String, Box<Filter>),
}

impl Filter {
    /// `(attribute, value)` when the filter is exactly `attribute eq "value"`.
    pub fn simple_eq(&self) -> Option<(&str, &str)> {
        match self {
            Filter::Compare {
                attr,
                op: CompareOp::Eq,
                value: FilterValue::Str(v),
            } => Some((attr.as_str(), v.as_str())),
            _ => None,
        }
    }

    /// An `attr eq "value"` that every match must satisfy (the filter itself,
    /// or one side of a top-level `and`); used to narrow the database query.
    pub fn required_eq(&self) -> Option<(&str, &str)> {
        match self {
            Filter::And(a, b) => a.required_eq().or_else(|| b.required_eq()),
            _ => self.simple_eq(),
        }
    }
}

/// Lowercases an attribute path and drops a core schema URN prefix.
pub fn normalize_attr(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    for schema in [USER_SCHEMA, GROUP_SCHEMA] {
        let prefix = format!("{}:", schema.to_ascii_lowercase());
        if let Some(rest) = lower.strip_prefix(&prefix) {
            return rest.to_string();
        }
    }
    lower
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open,
    Close,
    OpenBracket,
    CloseBracket,
    Str(String),
    Word(String),
}

fn tokenize(input: &str) -> Result<Vec<Token>, ScimError> {
    let invalid = |m: &str| ScimError::bad("invalidFilter", m.to_string());
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '(' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' => {
                out.push(Token::Close);
                i += 1;
            }
            '[' => {
                out.push(Token::OpenBracket);
                i += 1;
            }
            ']' => {
                out.push(Token::CloseBracket);
                i += 1;
            }
            '"' => {
                let start = i;
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    match chars[i] {
                        '\\' => i += 2,
                        '"' => {
                            closed = true;
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                if !closed || i > chars.len() {
                    return Err(invalid("Unterminated string in filter"));
                }
                let raw: String = chars[start..i].iter().collect();
                let s: String = serde_json::from_str(&raw)
                    .map_err(|_| invalid("Invalid string literal in filter"))?;
                out.push(Token::Str(s));
            }
            c if c.is_control() => return Err(invalid("Control character in filter")),
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], '(' | ')' | '[' | ']' | '"')
                {
                    i += 1;
                }
                out.push(Token::Word(chars[start..i].iter().collect()));
            }
        }
    }
    Ok(out)
}

fn is_attr_path(w: &str) -> bool {
    let first = w.chars().next();
    first.is_some_and(|c| c.is_ascii_alphabetic() || c == '$')
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '$' | ':' | '.'))
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn err(&self, m: &str) -> ScimError {
        ScimError::bad("invalidFilter", m.to_string())
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn peek_keyword(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(w)) if w.eq_ignore_ascii_case(kw))
    }

    fn enter(&mut self) -> Result<(), ScimError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err("Filter is nested too deeply"));
        }
        Ok(())
    }

    fn parse_or(&mut self) -> Result<Filter, ScimError> {
        let mut left = self.parse_and()?;
        while self.peek_keyword("or") {
            self.pos += 1;
            let right = self.parse_and()?;
            left = Filter::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Filter, ScimError> {
        let mut left = self.parse_not()?;
        while self.peek_keyword("and") {
            self.pos += 1;
            let right = self.parse_not()?;
            left = Filter::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Filter, ScimError> {
        if self.peek_keyword("not") && matches!(self.tokens.get(self.pos + 1), Some(Token::Open)) {
            self.pos += 2;
            self.enter()?;
            let inner = self.parse_or()?;
            self.depth -= 1;
            if self.next() != Some(Token::Close) {
                return Err(self.err("Expected ')' after not("));
            }
            return Ok(Filter::Not(Box::new(inner)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Filter, ScimError> {
        match self.next() {
            Some(Token::Open) => {
                self.enter()?;
                let inner = self.parse_or()?;
                self.depth -= 1;
                if self.next() != Some(Token::Close) {
                    return Err(self.err("Expected ')'"));
                }
                Ok(inner)
            }
            Some(Token::Word(w)) if is_attr_path(&w) => {
                let attr = normalize_attr(&w);
                if self.peek() == Some(&Token::OpenBracket) {
                    self.pos += 1;
                    self.enter()?;
                    let inner = self.parse_or()?;
                    self.depth -= 1;
                    if self.next() != Some(Token::CloseBracket) {
                        return Err(self.err("Expected ']'"));
                    }
                    return Ok(Filter::ValuePath(attr, Box::new(inner)));
                }
                let op_word = match self.next() {
                    Some(Token::Word(o)) => o,
                    _ => return Err(self.err("Expected an operator after the attribute")),
                };
                if op_word.eq_ignore_ascii_case("pr") {
                    return Ok(Filter::Present(attr));
                }
                let op = CompareOp::parse(&op_word)
                    .ok_or_else(|| self.err(&format!("Unknown operator '{op_word}'")))?;
                let value = match self.next() {
                    Some(Token::Str(s)) => FilterValue::Str(s),
                    Some(Token::Word(v)) => match v.to_ascii_lowercase().as_str() {
                        "true" => FilterValue::Bool(true),
                        "false" => FilterValue::Bool(false),
                        "null" => FilterValue::Null,
                        _ => FilterValue::Num(
                            v.parse::<f64>()
                                .ok()
                                .filter(|n| n.is_finite())
                                .ok_or_else(|| self.err("Invalid comparison value"))?,
                        ),
                    },
                    _ => return Err(self.err("Expected a comparison value")),
                };
                Ok(Filter::Compare { attr, op, value })
            }
            _ => Err(self.err("Expected an attribute, '(' or 'not'")),
        }
    }
}

/// Parses a `filter` query parameter.
pub fn parse_filter(input: &str) -> Result<Filter, ScimError> {
    if input.len() > MAX_FILTER_LEN {
        return Err(ScimError::bad("invalidFilter", "Filter is too long"));
    }
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(ScimError::bad("invalidFilter", "Empty filter"));
    }
    let mut p = Parser {
        tokens,
        pos: 0,
        depth: 0,
    };
    let f = p.parse_or()?;
    if p.pos != p.tokens.len() {
        return Err(ScimError::bad(
            "invalidFilter",
            "Unexpected text after the filter",
        ));
    }
    Ok(f)
}

// ------------------------------------------------------------ evaluation

fn get_ci<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}

/// Every value at `path` (dotted, normalized); multi-valued attributes are
/// flattened and a complex value without sub-attribute stands for `value`.
fn values_at<'a>(resource: &'a Value, path: &str) -> Vec<&'a Value> {
    if path.starts_with("urn:") {
        return Vec::new();
    }
    let mut current: Vec<&Value> = vec![resource];
    for part in path.split('.') {
        let mut next = Vec::new();
        for v in current {
            match v {
                Value::Array(items) => {
                    for i in items {
                        if let Some(x) = get_ci(i, part) {
                            next.push(x);
                        }
                    }
                }
                _ => {
                    if let Some(x) = get_ci(v, part) {
                        next.push(x);
                    }
                }
            }
        }
        current = next;
    }
    let mut out = Vec::new();
    for v in current {
        match v {
            Value::Array(items) => {
                for i in items {
                    if i.is_object() {
                        if let Some(x) = get_ci(i, "value") {
                            out.push(x);
                        }
                    } else {
                        out.push(i);
                    }
                }
            }
            Value::Object(_) => {
                if let Some(x) = get_ci(v, "value") {
                    out.push(x);
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn case_exact(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path);
    matches!(last, "id" | "externalid" | "version")
}

fn compare(actual: &Value, op: CompareOp, expected: &FilterValue, exact: bool) -> bool {
    use std::cmp::Ordering;
    match (actual, expected) {
        (Value::String(a), FilterValue::Str(e)) => {
            let (a, e) = if exact {
                (a.clone(), e.clone())
            } else {
                (a.to_lowercase(), e.to_lowercase())
            };
            match op {
                CompareOp::Eq => a == e,
                CompareOp::Ne => a != e,
                CompareOp::Co => a.contains(&e),
                CompareOp::Sw => a.starts_with(&e),
                CompareOp::Ew => a.ends_with(&e),
                CompareOp::Gt => a > e,
                CompareOp::Ge => a >= e,
                CompareOp::Lt => a < e,
                CompareOp::Le => a <= e,
            }
        }
        (Value::Bool(a), FilterValue::Bool(e)) => match op {
            CompareOp::Eq => a == e,
            CompareOp::Ne => a != e,
            _ => false,
        },
        (Value::Number(a), FilterValue::Num(e)) => {
            let Some(a) = a.as_f64() else { return false };
            match a.partial_cmp(e) {
                Some(Ordering::Equal) => {
                    matches!(op, CompareOp::Eq | CompareOp::Ge | CompareOp::Le)
                }
                Some(Ordering::Greater) => {
                    matches!(op, CompareOp::Ne | CompareOp::Gt | CompareOp::Ge)
                }
                Some(Ordering::Less) => matches!(op, CompareOp::Ne | CompareOp::Lt | CompareOp::Le),
                None => false,
            }
        }
        _ => false,
    }
}

fn present(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}

/// Whether `resource` (its SCIM JSON) matches the filter.
pub fn matches(filter: &Filter, resource: &Value) -> bool {
    match filter {
        Filter::Compare {
            attr,
            op,
            value: FilterValue::Null,
        } => {
            let any = values_at(resource, attr).into_iter().any(present);
            match op {
                CompareOp::Eq => !any,
                CompareOp::Ne => any,
                _ => false,
            }
        }
        Filter::Compare { attr, op, value } => {
            let exact = case_exact(attr);
            let values = values_at(resource, attr);
            if *op == CompareOp::Ne {
                // `ne` holds when no value equals.
                return !values
                    .iter()
                    .any(|v| compare(v, CompareOp::Eq, value, exact));
            }
            values.iter().any(|v| compare(v, *op, value, exact))
        }
        Filter::Present(attr) => values_at(resource, attr).into_iter().any(present),
        Filter::And(a, b) => matches(a, resource) && matches(b, resource),
        Filter::Or(a, b) => matches(a, resource) || matches(b, resource),
        Filter::Not(f) => !matches(f, resource),
        Filter::ValuePath(attr, f) => {
            let Some(container) = (if attr.starts_with("urn:") {
                None
            } else {
                attr.split('.')
                    .try_fold(resource, |v, part| get_ci(v, part))
            }) else {
                return false;
            };
            match container {
                Value::Array(items) => items.iter().any(|i| matches(f, i)),
                Value::Object(_) => matches(f, container),
                _ => false,
            }
        }
    }
}

// ------------------------------------------------------------ PATCH paths

/// A PATCH `path`: `attr`, `attr.sub` or `attr[filter].sub`.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchPath {
    /// Normalized top-level attribute (lowercase).
    pub attr: String,
    pub filter: Option<Filter>,
    /// Normalized sub-attribute.
    pub sub: Option<String>,
    /// The path names an extension schema the host does not support.
    pub extension: bool,
}

pub fn parse_path(input: &str) -> Result<PatchPath, ScimError> {
    let invalid = |m: &str| ScimError::bad("invalidPath", m.to_string());
    let s = input.trim();
    if s.is_empty() || s.len() > MAX_FILTER_LEN || s.chars().any(|c| c.is_control()) {
        return Err(invalid("Invalid PATCH path"));
    }
    let normalized = normalize_attr(s);
    if normalized.starts_with("urn:") {
        // Enterprise or other extension attributes are not stored.
        return Ok(PatchPath {
            attr: normalized,
            filter: None,
            sub: None,
            extension: true,
        });
    }
    // Keep the original case inside the filter (string literals).
    let offset = s.len() - normalized.len();
    let original = &s[offset..];
    let (head, filter, tail) = match original.find('[') {
        Some(open) => {
            let close = original
                .rfind(']')
                .filter(|c| *c > open)
                .ok_or_else(|| invalid("Unbalanced '[' in PATCH path"))?;
            let f = parse_filter(&original[open + 1..close])
                .map_err(|e| ScimError::bad("invalidPath", e.detail))?;
            let tail = &original[close + 1..];
            let tail = if tail.is_empty() {
                None
            } else {
                Some(
                    tail.strip_prefix('.')
                        .ok_or_else(|| invalid("Expected '.' after ']'"))?,
                )
            };
            (&original[..open], Some(f), tail)
        }
        None => match original.split_once('.') {
            Some((a, b)) => (a, None, Some(b)),
            None => (original, None, None),
        },
    };
    if !is_attr_path(head) || head.contains('.') {
        return Err(invalid("Invalid attribute in PATCH path"));
    }
    if let Some(t) = tail {
        if !is_attr_path(t) || t.contains('.') {
            return Err(invalid("Invalid sub-attribute in PATCH path"));
        }
    }
    Ok(PatchPath {
        attr: head.to_ascii_lowercase(),
        filter,
        sub: tail.map(|t| t.to_ascii_lowercase()),
        extension: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user() -> Value {
        json!({
            "id": "2819c223-7f76-453a-919d-413861904646",
            "externalId": "Ext-1",
            "userName": "Bjensen@Example.com",
            "name": {"givenName": "Barbara", "familyName": "Jensen"},
            "active": true,
            "emails": [
                {"value": "bjensen@example.com", "type": "work", "primary": true},
                {"value": "babs@jensen.org", "type": "home"}
            ],
            "groups": [{"value": "g1", "display": "Ops"}],
            "meta": {"lastModified": "2026-10-30T10:00:00Z"}
        })
    }

    fn m(f: &str) -> bool {
        matches(&parse_filter(f).unwrap(), &user())
    }

    #[test]
    fn user_name_eq_is_case_insensitive() {
        assert!(m(r#"userName eq "bjensen@example.com""#));
        assert!(m(r#"USERNAME EQ "BJENSEN@EXAMPLE.COM""#));
        assert!(m(
            r#"urn:ietf:params:scim:schemas:core:2.0:User:userName eq "bjensen@example.com""#
        ));
        assert!(!m(r#"userName eq "other""#));
    }

    #[test]
    fn ids_are_case_exact() {
        assert!(m(r#"externalId eq "Ext-1""#));
        assert!(!m(r#"externalId eq "ext-1""#));
        assert!(m(r#"id eq "2819c223-7f76-453a-919d-413861904646""#));
    }

    #[test]
    fn operators_logic_and_value_paths() {
        assert!(m(r#"userName sw "bjen""#));
        assert!(m(r#"userName co "example""#));
        assert!(m(r#"userName ew ".com""#));
        assert!(m("active eq true"));
        assert!(!m("active eq false"));
        assert!(m("name.familyName pr"));
        assert!(!m("title pr"));
        assert!(m(r#"emails[type eq "work" and value co "@example.com"]"#));
        assert!(!m(r#"emails[type eq "work" and value co "@jensen.org"]"#));
        assert!(m(r#"emails.value eq "babs@jensen.org""#));
        assert!(m(r#"emails eq "babs@jensen.org""#));
        assert!(m(r#"groups.value eq "g1""#));
        assert!(m(r#"meta.lastModified gt "2026-01-01T00:00:00Z""#));
        assert!(m(
            r#"userName eq "x" or (active eq true and not (userName eq "y"))"#
        ));
        assert!(!m(r#"not (active eq true)"#));
        assert!(m(r#"userName ne "x""#));
        assert!(m("title eq null"));
        assert!(!m(
            r#"urn:ietf:params:scim:schemas:extension:enterprise:2.0:User:employeeNumber eq "1""#
        ));
    }

    #[test]
    fn malformed_filters_are_invalid_filter_errors() {
        for f in [
            "",
            "userName",
            "userName eq",
            r#"userName xx "a""#,
            r#"userName eq "a"#,
            r#"(userName eq "a""#,
            r#"userName eq "a" garbage"#,
            r#"emails[type eq "work""#,
            "userName eq bogus",
        ] {
            let e = parse_filter(f).unwrap_err();
            assert_eq!(e.scim_type, Some("invalidFilter"), "{f}");
        }
        let deep = format!("{}active eq true{}", "(".repeat(40), ")".repeat(40));
        assert!(parse_filter(&deep).is_err());
        assert!(parse_filter(&"a".repeat(MAX_FILTER_LEN + 1)).is_err());
    }

    #[test]
    fn required_eq_narrows_queries() {
        let f = parse_filter(r#"userName eq "a""#).unwrap();
        assert_eq!(f.required_eq(), Some(("username", "a")));
        let f = parse_filter(r#"active eq true and externalId eq "x""#).unwrap();
        assert_eq!(f.required_eq(), Some(("externalid", "x")));
        let f = parse_filter(r#"userName eq "a" or userName eq "b""#).unwrap();
        assert_eq!(f.required_eq(), None);
    }

    #[test]
    fn patch_paths() {
        let p = parse_path("name.givenName").unwrap();
        assert_eq!(p.attr, "name");
        assert_eq!(p.sub.as_deref(), Some("givenname"));
        let p = parse_path(r#"emails[type eq "Work"].value"#).unwrap();
        assert_eq!(p.attr, "emails");
        assert_eq!(p.sub.as_deref(), Some("value"));
        assert!(matches!(
            p.filter,
            Some(Filter::Compare { ref value, .. }) if *value == FilterValue::Str("Work".into())
        ));
        let p = parse_path(r#"members[value eq "2819c223"]"#).unwrap();
        assert_eq!(p.attr, "members");
        assert!(p.sub.is_none() && p.filter.is_some());
        let p = parse_path("urn:ietf:params:scim:schemas:core:2.0:User:name.familyName").unwrap();
        assert_eq!(
            (p.attr.as_str(), p.sub.as_deref()),
            ("name", Some("familyname"))
        );
        let p = parse_path("urn:ietf:params:scim:schemas:extension:enterprise:2.0:User:manager")
            .unwrap();
        assert!(p.extension);
        for bad in [
            "",
            "a b",
            "emails[type eq \"x\"",
            "emails[type eq \"x\"]value",
            "a.b.c",
            "1abc",
        ] {
            assert!(parse_path(bad).is_err(), "{bad}");
        }
    }
}
