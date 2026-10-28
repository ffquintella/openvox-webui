//! Event subscription filters (PLUGIN_PLATFORM.md section 9.1).
//!
//! A small expression over the event payload, evaluated by the host before a
//! plugin is woken, e.g. `status == 'failed' && node.env in ['prod', 'qa']`.
//! Operators: `== != < <= > >= ~` (substring), `in [..]`, `exists`; `&&` binds
//! tighter than `||`; no parentheses or functions.

use serde_json::Value;

pub const MAX_FILTER_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// OR of ANDs.
    any: Vec<Vec<Clause>>,
}

#[derive(Debug, Clone, PartialEq)]
struct Clause {
    path: Vec<String>,
    test: Test,
}

#[derive(Debug, Clone, PartialEq)]
enum Test {
    Exists,
    Cmp(Op, Lit),
    In(Vec<Lit>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
}

#[derive(Debug, Clone, PartialEq)]
enum Lit {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Num(f64),
    Op(Op),
    And,
    Or,
    LBracket,
    RBracket,
    Comma,
}

impl Filter {
    pub fn parse(src: &str) -> Result<Filter, String> {
        if src.trim().is_empty() {
            return Err("filter is empty".into());
        }
        if src.len() > MAX_FILTER_LEN {
            return Err(format!(
                "filter is longer than {} characters",
                MAX_FILTER_LEN
            ));
        }
        let toks = tokenize(src)?;
        let mut p = Parser { toks, pos: 0 };
        let mut any = vec![vec![p.clause()?]];
        loop {
            match p.next() {
                None => break,
                Some(Tok::And) => any.last_mut().unwrap().push(p.clause()?),
                Some(Tok::Or) => any.push(vec![p.clause()?]),
                Some(t) => return Err(format!("unexpected {:?}, expected && or ||", t)),
            }
        }
        Ok(Filter { any })
    }

    pub fn matches(&self, payload: &Value) -> bool {
        self.any
            .iter()
            .any(|all| all.iter().all(|c| c.eval(payload)))
    }
}

impl Clause {
    fn eval(&self, payload: &Value) -> bool {
        let v = lookup(payload, &self.path);
        match &self.test {
            Test::Exists => v.is_some_and(|v| !v.is_null()),
            Test::Cmp(op, lit) => compare(v, *op, lit),
            Test::In(lits) => lits.iter().any(|l| compare(v, Op::Eq, l)),
        }
    }
}

fn lookup<'a>(v: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut cur = v;
    for seg in path {
        cur = match cur {
            Value::Object(m) => m.get(seg)?,
            Value::Array(a) => a.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

fn compare(v: Option<&Value>, op: Op, lit: &Lit) -> bool {
    let v = v.unwrap_or(&Value::Null);
    match (v, lit) {
        (Value::String(a), Lit::Str(b)) => match op {
            Op::Eq => a == b,
            Op::Ne => a != b,
            Op::Lt => a < b,
            Op::Le => a <= b,
            Op::Gt => a > b,
            Op::Ge => a >= b,
            Op::Contains => a.contains(b.as_str()),
        },
        (Value::Number(a), Lit::Num(b)) => {
            let Some(a) = a.as_f64() else { return false };
            match op {
                Op::Eq => a == *b,
                Op::Ne => a != *b,
                Op::Lt => a < *b,
                Op::Le => a <= *b,
                Op::Gt => a > *b,
                Op::Ge => a >= *b,
                Op::Contains => false,
            }
        }
        (Value::Bool(a), Lit::Bool(b)) => match op {
            Op::Eq => a == b,
            Op::Ne => a != b,
            _ => false,
        },
        (Value::Null, Lit::Null) => matches!(op, Op::Eq),
        _ => matches!(op, Op::Ne),
    }
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn clause(&mut self) -> Result<Clause, String> {
        let path = match self.next() {
            Some(Tok::Ident(p)) => p,
            other => return Err(format!("expected a payload field, found {:?}", other)),
        };
        let path: Vec<String> = path.split('.').map(str::to_string).collect();
        if path.iter().any(|s| s.is_empty()) {
            return Err("field path has an empty segment".into());
        }
        let test = match self.next() {
            Some(Tok::Ident(w)) if w == "exists" => Test::Exists,
            Some(Tok::Ident(w)) if w == "in" => {
                if self.next() != Some(Tok::LBracket) {
                    return Err("expected [ after in".into());
                }
                let mut items = Vec::new();
                loop {
                    items.push(self.lit()?);
                    match self.next() {
                        Some(Tok::Comma) => {}
                        Some(Tok::RBracket) => break,
                        other => return Err(format!("expected , or ], found {:?}", other)),
                    }
                    if items.len() > 32 {
                        return Err("in [..] accepts at most 32 values".into());
                    }
                }
                Test::In(items)
            }
            Some(Tok::Op(op)) => {
                let lit = self.lit()?;
                if matches!(op, Op::Contains) && !matches!(lit, Lit::Str(_)) {
                    return Err("~ needs a string".into());
                }
                Test::Cmp(op, lit)
            }
            other => return Err(format!("expected an operator, found {:?}", other)),
        };
        Ok(Clause { path, test })
    }

    fn lit(&mut self) -> Result<Lit, String> {
        match self.next() {
            Some(Tok::Str(s)) => Ok(Lit::Str(s)),
            Some(Tok::Num(n)) => Ok(Lit::Num(n)),
            Some(Tok::Ident(w)) if w == "true" => Ok(Lit::Bool(true)),
            Some(Tok::Ident(w)) if w == "false" => Ok(Lit::Bool(false)),
            Some(Tok::Ident(w)) if w == "null" => Ok(Lit::Null),
            other => Err(format!("expected a value, found {:?}", other)),
        }
    }
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            c if c.is_whitespace() => i += 1,
            '[' => {
                out.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                out.push(Tok::RBracket);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '~' => {
                out.push(Tok::Op(Op::Contains));
                i += 1;
            }
            '&' if next == Some('&') => {
                out.push(Tok::And);
                i += 2;
            }
            '|' if next == Some('|') => {
                out.push(Tok::Or);
                i += 2;
            }
            '=' if next == Some('=') => {
                out.push(Tok::Op(Op::Eq));
                i += 2;
            }
            '!' if next == Some('=') => {
                out.push(Tok::Op(Op::Ne));
                i += 2;
            }
            '<' | '>' => {
                let eq = next == Some('=');
                out.push(Tok::Op(match (c, eq) {
                    ('<', false) => Op::Lt,
                    ('<', true) => Op::Le,
                    ('>', false) => Op::Gt,
                    _ => Op::Ge,
                }));
                i += if eq { 2 } else { 1 };
            }
            '\'' | '"' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err("unterminated string".into()),
                        Some('\\') => {
                            match chars.get(i + 1) {
                                Some(&e) if e == quote || e == '\\' => s.push(e),
                                _ => return Err("invalid escape in string".into()),
                            }
                            i += 2;
                        }
                        Some(&q) if q == quote => {
                            i += 1;
                            break;
                        }
                        Some(&ch) => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Str(s));
            }
            c if c.is_ascii_digit() || (c == '-' && next.is_some_and(|d| d.is_ascii_digit())) => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let n = text
                    .parse::<f64>()
                    .map_err(|_| format!("'{}' is not a number", text))?;
                out.push(Tok::Num(n));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '.' | '-'))
                {
                    i += 1;
                }
                out.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(format!("unexpected character '{}'", other)),
        }
    }
    Ok(out)
}

/// Valid event type: 1 to 8 dot-separated segments of `a-z 0-9 _ -`.
pub fn valid_event_type(t: &str) -> bool {
    !t.is_empty() && t.len() <= 128 && t.split('.').count() <= 8 && t.split('.').all(valid_segment)
}

fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}

/// Valid pattern: a valid type, a valid prefix followed by `.*`, or `*`.
pub fn valid_event_pattern(p: &str) -> bool {
    if p == "*" {
        return true;
    }
    match p.strip_suffix(".*") {
        Some(prefix) => valid_event_type(prefix),
        None => valid_event_type(p),
    }
}

/// Whether `pattern` (exact, `prefix.*` or `*`) covers `event_type`.
pub fn pattern_matches(pattern: &str, event_type: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.strip_suffix(".*") {
        Some(prefix) => {
            event_type.len() > prefix.len() + 1
                && event_type.starts_with(prefix)
                && event_type.as_bytes()[prefix.len()] == b'.'
        }
        None => pattern == event_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m(f: &str, v: Value) -> bool {
        Filter::parse(f).unwrap().matches(&v)
    }

    #[test]
    fn comparisons_and_paths() {
        let p = json!({"status": "failed", "count": 5, "node": {"env": "production"}, "tags": ["a", "b"], "ok": true});
        assert!(m("status == 'failed'", p.clone()));
        assert!(!m("status != 'failed'", p.clone()));
        assert!(m("count >= 5 && count < 6", p.clone()));
        assert!(m("node.env in ['staging', \"production\"]", p.clone()));
        assert!(m("tags.1 == 'b'", p.clone()));
        assert!(m("status ~ 'ail'", p.clone()));
        assert!(m("ok == true", p.clone()));
        assert!(m("missing == null", p.clone()));
        assert!(!m("missing exists", p.clone()));
        assert!(m("node exists", p));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let p = json!({"a": 1, "b": 2});
        assert!(m("a == 9 && b == 2 || a == 1", p.clone()));
        assert!(!m("a == 9 && b == 2 || a == 8", p));
    }

    #[test]
    fn type_mismatch_is_not_equal() {
        let p = json!({"count": "5"});
        assert!(!m("count == 5", p.clone()));
        assert!(m("count != 5", p));
    }

    #[test]
    fn rejects_bad_filters() {
        for bad in [
            "",
            "status ==",
            "== 'x'",
            "a == 'x' &&",
            "a == 'x' b == 'y'",
            "a ~ 5",
            "a in 'x'",
            "a == 'unterminated",
            "a = 'x'",
            "a..b == 1",
            &"a == 'x' && ".repeat(40),
        ] {
            assert!(Filter::parse(bad).is_err(), "{:?} should fail", bad);
        }
    }

    #[test]
    fn event_patterns() {
        assert!(valid_event_type("node.classified"));
        assert!(!valid_event_type("Node.Classified"));
        assert!(!valid_event_type("node..x"));
        assert!(valid_event_pattern("acme.*"));
        assert!(valid_event_pattern("*"));
        assert!(!valid_event_pattern("acme.*.x"));
        assert!(pattern_matches("acme.*", "acme.crm.synced"));
        assert!(!pattern_matches("acme.*", "acme"));
        assert!(!pattern_matches("acme.*", "acmex.y"));
        assert!(pattern_matches("node.classified", "node.classified"));
        assert!(!pattern_matches("node.classified", "node.classified.x"));
        assert!(pattern_matches("*", "anything.at.all"));
    }
}
