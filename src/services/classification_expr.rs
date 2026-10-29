//! Expression engine for classification rules (M26 / T91).
//!
//! Canonical JSON form: see `docs/architecture/PLUGIN_PLATFORM.md` section 13.3.
//! Evaluation is bounded and pure; `now` comes from the host. A fact that is
//! not set makes the comparison reading it not match, whatever the operator.

use std::cmp::Ordering;
use std::net::IpAddr;

use chrono::{DateTime, Datelike, SecondsFormat, Timelike, Utc};
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::models::{ExprTrace, RuleOperator};

/// Maximum nesting depth of an expression.
pub const MAX_DEPTH: usize = 16;
/// Maximum number of expression and operand nodes.
pub const MAX_NODES: usize = 200;
/// Maximum size of the serialized expression.
pub const MAX_JSON_BYTES: usize = 16 * 1024;
/// Compiled regex size limit (the engine is linear time; this bounds memory).
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// What a function returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Returns {
    Bool,
    Value,
}

/// A pure function of the core vocabulary.
#[derive(Debug)]
pub struct FnSpec {
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: usize,
    pub returns: Returns,
    pub description: &'static str,
    pub example: &'static str,
}

const FUNCTIONS: &[FnSpec] = &[
    FnSpec {
        name: "lower",
        min_args: 1,
        max_args: 1,
        returns: Returns::Value,
        description: "Lower-cases a string",
        example: r#"{"fn":"lower","args":[{"fact":"hostname"}]}"#,
    },
    FnSpec {
        name: "upper",
        min_args: 1,
        max_args: 1,
        returns: Returns::Value,
        description: "Upper-cases a string",
        example: r#"{"fn":"upper","args":[{"fact":"hostname"}]}"#,
    },
    FnSpec {
        name: "len",
        min_args: 1,
        max_args: 1,
        returns: Returns::Value,
        description: "Length of a string (characters), list or object",
        example: r#"{"fn":"len","args":[{"fact":"hostname"}]}"#,
    },
    FnSpec {
        name: "exists",
        min_args: 1,
        max_args: 1,
        returns: Returns::Bool,
        description: "True when the fact is set (and not null)",
        example: r#"{"fn":"exists","args":[{"fact":"acme_cmdb.owner"}]}"#,
    },
    FnSpec {
        name: "contains",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "Substring of a string, or element of a list",
        example: r#"{"fn":"contains","args":[{"fact":"hostname"},"web"]}"#,
    },
    FnSpec {
        name: "starts_with",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "String prefix",
        example: r#"{"fn":"starts_with","args":[{"fact":"hostname"},"db-"]}"#,
    },
    FnSpec {
        name: "ends_with",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "String suffix",
        example: r#"{"fn":"ends_with","args":[{"fact":"fqdn"},".example.com"]}"#,
    },
    FnSpec {
        name: "semver_gte",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "Version >= version (missing components count as 0)",
        example: r#"{"fn":"semver_gte","args":[{"fact":"os.release.full"},"9.0"]}"#,
    },
    FnSpec {
        name: "semver_gt",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "Version > version",
        example: r#"{"fn":"semver_gt","args":[{"fact":"os.release.full"},"9.0"]}"#,
    },
    FnSpec {
        name: "semver_lte",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "Version <= version",
        example: r#"{"fn":"semver_lte","args":[{"fact":"os.release.full"},"9.0"]}"#,
    },
    FnSpec {
        name: "semver_lt",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "Version < version",
        example: r#"{"fn":"semver_lt","args":[{"fact":"os.release.full"},"9.0"]}"#,
    },
    FnSpec {
        name: "in_cidr",
        min_args: 2,
        max_args: 2,
        returns: Returns::Bool,
        description: "IPv4 or IPv6 address inside a CIDR block",
        example: r#"{"fn":"in_cidr","args":[{"fact":"networking.ip"},"10.20.0.0/16"]}"#,
    },
    FnSpec {
        name: "percentage",
        min_args: 2,
        max_args: 3,
        returns: Returns::Bool,
        description: "Stable hash bucket: true for n percent of the values, always the same ones for the same salt (canary and wave rollouts)",
        example: r#"{"fn":"percentage","args":[{"fact":"certname"},10,"ring-1"]}"#,
    },
    FnSpec {
        name: "hour_utc",
        min_args: 0,
        max_args: 0,
        returns: Returns::Value,
        description: "Current hour 0-23 UTC, supplied by the host",
        example: r#"{"fn":"hour_utc","args":[]}"#,
    },
    FnSpec {
        name: "weekday",
        min_args: 0,
        max_args: 0,
        returns: Returns::Value,
        description: "Current weekday, 0 = Monday to 6 = Sunday, UTC, supplied by the host",
        example: r#"{"fn":"weekday","args":[]}"#,
    },
];

fn find_fn(name: &str) -> Option<&'static FnSpec> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// Function catalog entry served by the API.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionInfo {
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: usize,
    pub returns: Returns,
    pub description: &'static str,
    pub example: &'static str,
}

/// All core functions.
pub fn function_catalog() -> Vec<FunctionInfo> {
    FUNCTIONS
        .iter()
        .map(|f| FunctionInfo {
            name: f.name,
            min_args: f.min_args,
            max_args: f.max_args,
            returns: f.returns,
            description: f.description,
            example: f.example,
        })
        .collect()
}

/// Operator catalog entry served by the API.
#[derive(Debug, Clone, Serialize)]
pub struct OperatorInfo {
    pub op: &'static str,
    pub description: &'static str,
}

/// All comparison operators usable in rules and expressions.
pub fn operator_catalog() -> Vec<OperatorInfo> {
    [
        ("=", "Equal (numbers compare by value)"),
        ("!=", "Not equal"),
        ("~", "Regular expression match (strings only)"),
        ("!~", "Regular expression does not match (strings only)"),
        (">", "Greater than (numbers or strings of the same type)"),
        (">=", "Greater than or equal"),
        ("<", "Less than"),
        ("<=", "Less than or equal"),
        ("in", "Value is an element of the list"),
        ("not_in", "Value is not an element of the list"),
    ]
    .into_iter()
    .map(|(op, description)| OperatorInfo { op, description })
    .collect()
}

/// Inputs of an evaluation. Everything an expression can read is here.
pub struct EvalContext<'a> {
    pub facts: &'a Value,
    /// Supplied by the host, minute granularity.
    pub now: DateTime<Utc>,
}

/// A value an expression reads or computes.
#[derive(Debug, Clone)]
enum Operand {
    Literal(Value),
    Fact(String),
    Now,
    Call(&'static FnSpec, Vec<Operand>),
}

/// A parsed, validated expression.
#[derive(Debug, Clone)]
pub struct Expr(Node);

#[derive(Debug, Clone)]
enum Node {
    All(Vec<Node>),
    Any(Vec<Node>),
    Not(Box<Node>),
    Compare {
        left: Operand,
        op: RuleOperator,
        right: Operand,
        /// Compiled once when the right side is a literal pattern.
        regex: Option<Regex>,
    },
    Call(&'static FnSpec, Vec<Operand>),
}

struct Budget {
    nodes: usize,
}

impl Budget {
    fn take(&mut self, path: &str) -> Result<(), String> {
        if self.nodes == 0 {
            return Err(format!("{path}: expression exceeds {MAX_NODES} nodes"));
        }
        self.nodes -= 1;
        Ok(())
    }
}

impl Expr {
    /// Parse and validate the canonical JSON form.
    pub fn parse(value: &Value) -> Result<Expr, String> {
        let size = serde_json::to_string(value).map(|s| s.len()).unwrap_or(0);
        if size > MAX_JSON_BYTES {
            return Err(format!("expression is larger than {MAX_JSON_BYTES} bytes"));
        }
        let mut budget = Budget { nodes: MAX_NODES };
        parse_node(value, "expression", 1, &mut budget).map(Expr)
    }

    /// Evaluate without a trace (the hot path).
    pub fn matches(&self, ctx: &EvalContext) -> bool {
        eval_node(&self.0, ctx, false).0
    }

    /// Evaluate and report every sub-expression with the values it saw.
    pub fn trace(&self, ctx: &EvalContext) -> ExprTrace {
        let (matched, trace) = eval_node(&self.0, ctx, true);
        let mut trace = trace.expect("trace requested");
        trace.matched = matched;
        trace
    }
}

/// Validate an expression and return its error, for the API.
pub fn validate(value: &Value) -> Result<(), String> {
    Expr::parse(value).map(|_| ())
}

fn object<'a>(
    v: &'a Value,
    path: &str,
    allowed: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| format!("{path}: expected an object"))?;
    if let Some(extra) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(format!("{path}: unexpected key '{extra}'"));
    }
    Ok(obj)
}

fn parse_node(v: &Value, path: &str, depth: usize, budget: &mut Budget) -> Result<Node, String> {
    if depth > MAX_DEPTH {
        return Err(format!("{path}: nesting deeper than {MAX_DEPTH}"));
    }
    budget.take(path)?;
    let obj = v
        .as_object()
        .ok_or_else(|| format!("{path}: expected an object"))?;

    if obj.contains_key("all") || obj.contains_key("any") {
        let key = if obj.contains_key("all") {
            "all"
        } else {
            "any"
        };
        let obj = object(v, path, &[key])?;
        let items = obj[key]
            .as_array()
            .ok_or_else(|| format!("{path}.{key}: expected a list"))?;
        if items.is_empty() {
            return Err(format!("{path}.{key}: the list is empty"));
        }
        let children = items
            .iter()
            .enumerate()
            .map(|(i, item)| parse_node(item, &format!("{path}.{key}[{i}]"), depth + 1, budget))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(if key == "all" {
            Node::All(children)
        } else {
            Node::Any(children)
        });
    }
    if obj.contains_key("not") {
        let obj = object(v, path, &["not"])?;
        let inner = parse_node(&obj["not"], &format!("{path}.not"), depth + 1, budget)?;
        return Ok(Node::Not(Box::new(inner)));
    }
    if obj.contains_key("fn") {
        let (spec, args) = parse_call(v, path, depth, budget)?;
        if spec.returns != Returns::Bool {
            return Err(format!(
                "{path}: function '{}' returns a value; compare it with an operator",
                spec.name
            ));
        }
        return Ok(Node::Call(spec, args));
    }
    if obj.contains_key("op") {
        if obj.contains_key("fact") {
            let obj = object(v, path, &["fact", "op", "value"])?;
            let fact = obj["fact"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{path}.fact: expected a fact path"))?;
            let value = obj
                .get("value")
                .ok_or_else(|| format!("{path}: missing 'value'"))?;
            let op = parse_op(&obj["op"], path)?;
            budget.take(path)?;
            budget.take(path)?;
            return compare(
                Operand::Fact(fact.to_string()),
                op,
                Operand::Literal(value.clone()),
                path,
            );
        }
        let obj = object(v, path, &["op", "left", "right"])?;
        let op = parse_op(&obj["op"], path)?;
        let left = obj
            .get("left")
            .ok_or_else(|| format!("{path}: missing 'left'"))?;
        let right = obj
            .get("right")
            .ok_or_else(|| format!("{path}: missing 'right'"))?;
        let left = parse_operand(left, &format!("{path}.left"), depth + 1, budget)?;
        let right = parse_operand(right, &format!("{path}.right"), depth + 1, budget)?;
        return compare(left, op, right, path);
    }
    Err(format!(
        "{path}: expected one of all, any, not, fn or a comparison (op)"
    ))
}

fn parse_op(v: &Value, path: &str) -> Result<RuleOperator, String> {
    serde_json::from_value::<RuleOperator>(v.clone())
        .map_err(|_| format!("{path}.op: unknown operator {v}"))
}

fn compare(left: Operand, op: RuleOperator, right: Operand, path: &str) -> Result<Node, String> {
    let regex = match (op, &right) {
        (RuleOperator::Regex | RuleOperator::NotRegex, Operand::Literal(Value::String(p))) => {
            Some(build_regex(p).map_err(|e| format!("{path}: invalid regular expression: {e}"))?)
        }
        _ => None,
    };
    if matches!(op, RuleOperator::In | RuleOperator::NotIn) {
        if let Operand::Literal(v) = &right {
            if !v.is_array() {
                return Err(format!(
                    "{path}: '{}' needs a list on the right side",
                    op_symbol(op)
                ));
            }
        }
    }
    Ok(Node::Compare {
        left,
        op,
        right,
        regex,
    })
}

fn build_regex(pattern: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
}

fn parse_call(
    v: &Value,
    path: &str,
    depth: usize,
    budget: &mut Budget,
) -> Result<(&'static FnSpec, Vec<Operand>), String> {
    let obj = object(v, path, &["fn", "args"])?;
    let name = obj["fn"]
        .as_str()
        .ok_or_else(|| format!("{path}.fn: expected a function name"))?;
    let spec = find_fn(name).ok_or_else(|| format!("{path}: unknown function '{name}'"))?;
    let raw = match obj.get("args") {
        Some(a) => a
            .as_array()
            .ok_or_else(|| format!("{path}.args: expected a list"))?
            .as_slice(),
        None => &[],
    };
    if raw.len() < spec.min_args || raw.len() > spec.max_args {
        return Err(format!(
            "{path}: '{name}' takes {} to {} arguments, got {}",
            spec.min_args,
            spec.max_args,
            raw.len()
        ));
    }
    let args = raw
        .iter()
        .enumerate()
        .map(|(i, a)| parse_operand(a, &format!("{path}.args[{i}]"), depth + 1, budget))
        .collect::<Result<Vec<_>, _>>()?;
    if spec.name == "exists" && !matches!(args[0], Operand::Fact(_)) {
        return Err(format!("{path}: 'exists' takes a fact reference"));
    }
    if spec.name == "percentage" {
        if let Operand::Literal(n) = &args[1] {
            match n.as_f64() {
                Some(n) if (0.0..=100.0).contains(&n) => {}
                _ => return Err(format!("{path}: percentage must be a number from 0 to 100")),
            }
        }
    }
    Ok((spec, args))
}

fn parse_operand(
    v: &Value,
    path: &str,
    depth: usize,
    budget: &mut Budget,
) -> Result<Operand, String> {
    if depth > MAX_DEPTH {
        return Err(format!("{path}: nesting deeper than {MAX_DEPTH}"));
    }
    budget.take(path)?;
    match v {
        Value::Object(obj) => {
            if obj.contains_key("fact") {
                let obj = object(v, path, &["fact"])?;
                let fact = obj["fact"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| format!("{path}.fact: expected a fact path"))?;
                Ok(Operand::Fact(fact.to_string()))
            } else if obj.contains_key("fn") {
                let (spec, args) = parse_call(v, path, depth, budget)?;
                Ok(Operand::Call(spec, args))
            } else if obj.contains_key("now") {
                let obj = object(v, path, &["now"])?;
                if obj["now"] != Value::Bool(true) {
                    return Err(format!("{path}: write {{\"now\": true}}"));
                }
                Ok(Operand::Now)
            } else {
                Err(format!(
                    "{path}: an object operand is {{\"fact\"}}, {{\"fn\"}} or {{\"now\"}}"
                ))
            }
        }
        other => Ok(Operand::Literal(other.clone())),
    }
}

fn eval_node(node: &Node, ctx: &EvalContext, trace: bool) -> (bool, Option<ExprTrace>) {
    match node {
        Node::All(children) | Node::Any(children) => {
            let all = matches!(node, Node::All(_));
            let mut result = all;
            let mut traces = Vec::new();
            for child in children {
                let (m, t) = eval_node(child, ctx, trace);
                if all {
                    result &= m;
                } else {
                    result |= m;
                }
                if let Some(t) = t {
                    traces.push(t);
                } else if (all && !m) || (!all && m) {
                    // Pure evaluation: the rest cannot change the result.
                    break;
                }
            }
            let t = trace.then(|| ExprTrace {
                kind: if all { "all" } else { "any" }.into(),
                label: if all { "all of" } else { "any of" }.into(),
                matched: result,
                left: None,
                right: None,
                value: None,
                error: None,
                children: traces,
            });
            (result, t)
        }
        Node::Not(inner) => {
            let (m, t) = eval_node(inner, ctx, trace);
            let result = !m;
            let t = t.map(|child| ExprTrace {
                kind: "not".into(),
                label: "not".into(),
                matched: result,
                left: None,
                right: None,
                value: None,
                error: None,
                children: vec![child],
            });
            (result, t)
        }
        Node::Compare {
            left,
            op,
            right,
            regex,
        } => {
            let l = eval_operand(left, ctx);
            let r = eval_operand(right, ctx);
            let result = match (&l, &r) {
                (Some(l), Some(r)) => compare_values(*op, l, r, regex.as_ref()),
                _ => false,
            };
            let t = trace.then(|| {
                let mut missing = Vec::new();
                if l.is_none() {
                    missing.push(operand_label(left));
                }
                if r.is_none() {
                    missing.push(operand_label(right));
                }
                ExprTrace {
                    kind: "compare".into(),
                    label: format!(
                        "{} {} {}",
                        operand_label(left),
                        op_symbol(*op),
                        operand_label(right)
                    ),
                    matched: result,
                    left: l.clone(),
                    right: r.clone(),
                    value: None,
                    error: (!missing.is_empty())
                        .then(|| format!("no value for {}", missing.join(", "))),
                    children: Vec::new(),
                }
            });
            (result, t)
        }
        Node::Call(spec, args) => {
            let value = call(spec, args, ctx);
            let result = value == Some(Value::Bool(true));
            let t = trace.then(|| ExprTrace {
                kind: "fn".into(),
                label: call_label(spec, args),
                matched: result,
                left: None,
                right: None,
                value: value.clone(),
                error: value
                    .is_none()
                    .then(|| "an argument has no value".to_string()),
                children: Vec::new(),
            });
            (result, t)
        }
    }
}

fn eval_operand(op: &Operand, ctx: &EvalContext) -> Option<Value> {
    match op {
        Operand::Literal(v) => Some(v.clone()),
        Operand::Fact(path) => {
            let mut current = ctx.facts;
            for part in path.split('.') {
                current = current.get(part)?;
            }
            Some(current.clone())
        }
        Operand::Now => Some(Value::String(
            ctx.now.to_rfc3339_opts(SecondsFormat::Secs, true),
        )),
        Operand::Call(spec, args) => call(spec, args, ctx),
    }
}

fn call(spec: &FnSpec, args: &[Operand], ctx: &EvalContext) -> Option<Value> {
    match spec.name {
        "exists" => {
            let present = eval_operand(&args[0], ctx).is_some_and(|v| !v.is_null());
            return Some(Value::Bool(present));
        }
        "hour_utc" => return Some(Value::from(ctx.now.hour())),
        "weekday" => return Some(Value::from(ctx.now.weekday().num_days_from_monday())),
        _ => {}
    }
    let mut values = Vec::with_capacity(args.len());
    for a in args {
        values.push(eval_operand(a, ctx)?);
    }
    match spec.name {
        "lower" => values[0].as_str().map(|s| Value::String(s.to_lowercase())),
        "upper" => values[0].as_str().map(|s| Value::String(s.to_uppercase())),
        "len" => match &values[0] {
            Value::String(s) => Some(Value::from(s.chars().count())),
            Value::Array(a) => Some(Value::from(a.len())),
            Value::Object(o) => Some(Value::from(o.len())),
            _ => None,
        },
        "contains" => Some(Value::Bool(match (&values[0], &values[1]) {
            (Value::String(h), Value::String(n)) => h.contains(n.as_str()),
            (Value::Array(items), needle) => items.iter().any(|i| values_equal(i, needle)),
            _ => false,
        })),
        "starts_with" => Some(Value::Bool(match (&values[0], &values[1]) {
            (Value::String(h), Value::String(n)) => h.starts_with(n.as_str()),
            _ => false,
        })),
        "ends_with" => Some(Value::Bool(match (&values[0], &values[1]) {
            (Value::String(h), Value::String(n)) => h.ends_with(n.as_str()),
            _ => false,
        })),
        name @ ("semver_gte" | "semver_gt" | "semver_lte" | "semver_lt") => {
            let ordering = match (&values[0], &values[1]) {
                (Value::String(a), Value::String(b)) => parse_version(a)
                    .zip(parse_version(b))
                    .map(|(a, b)| a.cmp(&b)),
                _ => None,
            };
            Some(Value::Bool(match (name, ordering) {
                (_, None) => false,
                ("semver_gte", Some(o)) => o != Ordering::Less,
                ("semver_gt", Some(o)) => o == Ordering::Greater,
                ("semver_lte", Some(o)) => o != Ordering::Greater,
                (_, Some(o)) => o == Ordering::Less,
            }))
        }
        "in_cidr" => Some(Value::Bool(match (&values[0], &values[1]) {
            (Value::String(ip), Value::String(cidr)) => cidr_contains(cidr, ip).unwrap_or(false),
            _ => false,
        })),
        "percentage" => {
            let subject = match &values[0] {
                Value::String(s) => s.clone(),
                Value::Null => return None,
                other => other.to_string(),
            };
            let n = values[1].as_f64()?;
            let salt = values.get(2).and_then(Value::as_str).unwrap_or("");
            Some(Value::Bool(
                bucket(salt, &subject) < (n * 100.0).round() as u64,
            ))
        }
        _ => None,
    }
}

/// Stable bucket in 0..10_000 for a subject and a salt.
fn bucket(salt: &str, subject: &str) -> u64 {
    let digest = Sha256::digest(format!("{salt}\u{0}{subject}").as_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(first) % 10_000
}

/// "9.3", "v9.3.1-rc1" and "9" compare as 9.3.0, 9.3.1 and 9.0.0.
fn parse_version(s: &str) -> Option<semver::Version> {
    let s = s.trim().trim_start_matches('v');
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let mut parts = s[..end].split('.');
    let major: u64 = match parts.next() {
        Some("") | None => return None,
        Some(p) => p.parse().ok()?,
    };
    let mut component = || -> Option<u64> {
        match parts.next() {
            Some("") => None,
            Some(p) => p.parse().ok(),
            None => Some(0),
        }
    };
    let minor = component()?;
    let patch = component()?;
    Some(semver::Version::new(major, minor, patch))
}

fn cidr_contains(cidr: &str, ip: &str) -> Option<bool> {
    let ip: IpAddr = ip.trim().parse().ok()?;
    let (net, len) = match cidr.split_once('/') {
        Some((n, l)) => (
            n.trim().parse::<IpAddr>().ok()?,
            Some(l.trim().parse::<u32>().ok()?),
        ),
        None => (cidr.trim().parse::<IpAddr>().ok()?, None),
    };
    match (net, ip) {
        (IpAddr::V4(net), IpAddr::V4(ip)) => {
            let len = len.unwrap_or(32);
            if len > 32 {
                return None;
            }
            let mask = if len == 0 { 0 } else { u32::MAX << (32 - len) };
            Some(u32::from(net) & mask == u32::from(ip) & mask)
        }
        (IpAddr::V6(net), IpAddr::V6(ip)) => {
            let len = len.unwrap_or(128);
            if len > 128 {
                return None;
            }
            let mask = if len == 0 {
                0
            } else {
                u128::MAX << (128 - len)
            };
            Some(u128::from(net) & mask == u128::from(ip) & mask)
        }
        _ => Some(false),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Order two values of the same kind; anything else is not comparable.
fn ordering(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64()?.partial_cmp(&y.as_f64()?),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn compare_values(op: RuleOperator, l: &Value, r: &Value, regex: Option<&Regex>) -> bool {
    match op {
        RuleOperator::Equals => values_equal(l, r),
        RuleOperator::NotEquals => !values_equal(l, r),
        RuleOperator::Regex | RuleOperator::NotRegex => {
            let (Some(text), Some(pattern)) = (l.as_str(), r.as_str()) else {
                return false;
            };
            let matched = match regex {
                Some(re) => re.is_match(text),
                None => match build_regex(pattern) {
                    Ok(re) => re.is_match(text),
                    Err(_) => return false,
                },
            };
            matched == (op == RuleOperator::Regex)
        }
        RuleOperator::GreaterThan => ordering(l, r) == Some(Ordering::Greater),
        RuleOperator::GreaterThanOrEqual => {
            matches!(ordering(l, r), Some(Ordering::Greater | Ordering::Equal))
        }
        RuleOperator::LessThan => ordering(l, r) == Some(Ordering::Less),
        RuleOperator::LessThanOrEqual => {
            matches!(ordering(l, r), Some(Ordering::Less | Ordering::Equal))
        }
        RuleOperator::In => r
            .as_array()
            .is_some_and(|items| items.iter().any(|i| values_equal(i, l))),
        RuleOperator::NotIn => r
            .as_array()
            .is_some_and(|items| !items.iter().any(|i| values_equal(i, l))),
    }
}

fn op_symbol(op: RuleOperator) -> &'static str {
    match op {
        RuleOperator::Equals => "=",
        RuleOperator::NotEquals => "!=",
        RuleOperator::Regex => "~",
        RuleOperator::NotRegex => "!~",
        RuleOperator::GreaterThan => ">",
        RuleOperator::GreaterThanOrEqual => ">=",
        RuleOperator::LessThan => "<",
        RuleOperator::LessThanOrEqual => "<=",
        RuleOperator::In => "in",
        RuleOperator::NotIn => "not_in",
    }
}

fn operand_label(op: &Operand) -> String {
    match op {
        Operand::Literal(v) => v.to_string(),
        Operand::Fact(p) => p.clone(),
        Operand::Now => "now".into(),
        Operand::Call(spec, args) => call_label(spec, args),
    }
}

fn call_label(spec: &FnSpec, args: &[Operand]) -> String {
    let args: Vec<String> = args.iter().map(operand_label).collect();
    format!("{}({})", spec.name, args.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn ctx(facts: &Value) -> EvalContext<'_> {
        EvalContext {
            facts,
            // Wednesday 2026-10-28 03:15 UTC
            now: Utc.with_ymd_and_hms(2026, 10, 28, 3, 15, 0).unwrap(),
        }
    }

    fn facts() -> Value {
        json!({
            "certname": "web01.example.com",
            "hostname": "Web01",
            "os": {"family": "RedHat", "release": {"full": "9.3"}},
            "virtual": "kvm",
            "networking": {"ip": "10.20.4.7"},
            "memory": {"system": {"total_bytes": 8_000_000_000u64}},
            "acme_cmdb": {"min_memory": 4_000_000_000u64, "owner": "ops"},
            "tags": ["a", "b"],
        })
    }

    fn eval(expr: Value) -> bool {
        let f = facts();
        Expr::parse(&expr).expect("valid").matches(&ctx(&f))
    }

    #[test]
    fn spec_example_expression_evaluates() {
        let expr = json!({ "all": [
            { "fact": "os.family", "op": "=", "value": "RedHat" },
            { "any": [
                { "fn": "semver_gte", "args": [{ "fact": "os.release.full" }, "9.0"] },
                { "fn": "in_cidr", "args": [{ "fact": "networking.ip" }, "192.168.0.0/16"] } ] },
            { "not": { "fact": "virtual", "op": "=", "value": "physical" } },
            { "op": ">", "left": { "fact": "memory.system.total_bytes" },
                         "right": { "fact": "acme_cmdb.min_memory" } },
            { "fn": "percentage", "args": [{ "fact": "certname" }, 100, "ring-1"] }
        ] });
        assert!(eval(expr));
    }

    #[test]
    fn composites_and_negation() {
        assert!(eval(json!({"any": [
            {"fact": "os.family", "op": "=", "value": "Debian"},
            {"fact": "virtual", "op": "=", "value": "kvm"}]})));
        assert!(!eval(json!({"all": [
            {"fact": "os.family", "op": "=", "value": "Debian"},
            {"fact": "virtual", "op": "=", "value": "kvm"}]})));
        assert!(eval(
            json!({"not": {"fact": "os.family", "op": "=", "value": "Debian"}})
        ));
    }

    #[test]
    fn missing_fact_never_matches_not_even_not_equals() {
        assert!(!eval(json!({"fact": "nope", "op": "!=", "value": "x"})));
        assert!(!eval(json!({"fact": "nope", "op": "=", "value": "x"})));
        // `not` over a missing fact does match: it is the explicit way to ask
        assert!(eval(
            json!({"not": {"fact": "nope", "op": "=", "value": "x"}})
        ));
        assert!(eval(
            json!({"not": {"fn": "exists", "args": [{"fact": "nope"}]}})
        ));
        assert!(eval(
            json!({"fn": "exists", "args": [{"fact": "os.family"}]})
        ));
    }

    #[test]
    fn compares_fact_with_fact_and_numbers_by_value() {
        assert!(eval(
            json!({"op": ">=", "left": {"fact": "memory.system.total_bytes"},
                            "right": {"fact": "acme_cmdb.min_memory"}})
        ));
        assert!(eval(
            json!({"fact": "acme_cmdb.min_memory", "op": "=", "value": 4e9})
        ));
        // different types do not order
        assert!(!eval(json!({"fact": "hostname", "op": ">", "value": 3})));
        assert!(!eval(json!({"fact": "hostname", "op": ">=", "value": 3})));
    }

    #[test]
    fn regex_in_and_not_in() {
        assert!(eval(
            json!({"fact": "hostname", "op": "~", "value": "^Web\\d+$"})
        ));
        assert!(!eval(
            json!({"fact": "hostname", "op": "!~", "value": "^Web\\d+$"})
        ));
        assert!(eval(
            json!({"fact": "os.family", "op": "in", "value": ["Debian", "RedHat"]})
        ));
        assert!(eval(
            json!({"fact": "os.family", "op": "not_in", "value": ["Debian"]})
        ));
    }

    #[test]
    fn functions() {
        assert!(eval(
            json!({"op": "=", "left": {"fn": "lower", "args": [{"fact": "hostname"}]}, "right": "web01"})
        ));
        assert!(eval(
            json!({"op": "=", "left": {"fn": "len", "args": [{"fact": "tags"}]}, "right": 2})
        ));
        assert!(eval(
            json!({"fn": "contains", "args": [{"fact": "tags"}, "b"]})
        ));
        assert!(eval(
            json!({"fn": "contains", "args": [{"fact": "hostname"}, "eb0"]})
        ));
        assert!(eval(
            json!({"fn": "starts_with", "args": [{"fact": "certname"}, "web"]})
        ));
        assert!(eval(
            json!({"fn": "ends_with", "args": [{"fact": "certname"}, ".example.com"]})
        ));
        assert!(eval(
            json!({"fn": "semver_gt", "args": [{"fact": "os.release.full"}, "9.2.9"]})
        ));
        assert!(eval(
            json!({"fn": "semver_lt", "args": [{"fact": "os.release.full"}, "10"]})
        ));
        assert!(eval(
            json!({"fn": "semver_lte", "args": ["v9.3", {"fact": "os.release.full"}]})
        ));
        assert!(!eval(
            json!({"fn": "semver_gte", "args": ["not a version", "9"]})
        ));
    }

    #[test]
    fn cidr_blocks() {
        assert!(cidr_contains("10.20.0.0/16", "10.20.4.7").unwrap());
        assert!(!cidr_contains("10.20.0.0/16", "10.21.4.7").unwrap());
        assert!(cidr_contains("0.0.0.0/0", "8.8.8.8").unwrap());
        assert!(cidr_contains("10.0.0.1", "10.0.0.1").unwrap());
        assert!(cidr_contains("fd00::/8", "fd12::1").unwrap());
        assert!(!cidr_contains("fd00::/8", "10.0.0.1").unwrap());
        assert!(cidr_contains("10.0.0.0/33", "10.0.0.1").is_none());
        assert!(cidr_contains("garbage", "10.0.0.1").is_none());
    }

    #[test]
    fn percentage_is_stable_and_roughly_proportional() {
        assert_eq!(bucket("s", "n1"), bucket("s", "n1"));
        assert_ne!(bucket("s", "n1"), bucket("other", "n1"));
        let in_ring = (0..2000)
            .filter(|i| bucket("ring-1", &format!("node{i}.example.com")) < 1000)
            .count();
        assert!(
            (100..=300).contains(&in_ring),
            "{in_ring} of 2000 in a 10% ring"
        );
        assert!(!eval(
            json!({"fn": "percentage", "args": [{"fact": "certname"}, 0, "x"]})
        ));
        assert!(eval(
            json!({"fn": "percentage", "args": [{"fact": "certname"}, 100, "x"]})
        ));
    }

    #[test]
    fn now_comes_from_the_host_not_a_clock() {
        assert!(eval(
            json!({"op": "in", "left": {"fn": "hour_utc", "args": []}, "right": [2, 3, 4]})
        ));
        assert!(eval(
            json!({"op": "<", "left": {"fn": "weekday", "args": []}, "right": 5})
        ));
        assert!(eval(
            json!({"op": ">=", "left": {"now": true}, "right": "2026-10-28T00:00:00Z"})
        ));
        assert!(!eval(
            json!({"op": ">=", "left": {"now": true}, "right": "2026-11-01T00:00:00Z"})
        ));
    }

    #[test]
    fn trace_reports_values_and_missing_facts() {
        let f = facts();
        let expr = Expr::parse(&json!({"all": [
            {"fact": "os.family", "op": "=", "value": "Debian"},
            {"fact": "nope", "op": "=", "value": 1}]}))
        .unwrap();
        let t = expr.trace(&ctx(&f));
        assert!(!t.matched);
        assert_eq!(t.kind, "all");
        assert_eq!(t.children.len(), 2, "the trace evaluates every child");
        assert_eq!(t.children[0].left, Some(json!("RedHat")));
        assert_eq!(t.children[0].right, Some(json!("Debian")));
        assert_eq!(t.children[0].label, "os.family = \"Debian\"");
        assert!(t.children[1].error.as_deref().unwrap().contains("nope"));
        // The trace and the plain evaluation always agree.
        assert_eq!(expr.matches(&ctx(&f)), t.matched);
    }

    #[test]
    fn rejects_malformed_expressions_with_a_location() {
        let err = |v: Value| Expr::parse(&v).unwrap_err();
        assert!(err(json!({"all": []})).contains("empty"));
        assert!(err(json!({"all": [1]})).contains("expression.all[0]"));
        assert!(err(json!({"fact": "x", "op": "??", "value": 1})).contains("unknown operator"));
        assert!(err(json!({"fact": "x", "op": "="})).contains("value"));
        assert!(err(json!({"fn": "nope", "args": []})).contains("unknown function"));
        assert!(err(json!({"fn": "lower", "args": [{"fact": "x"}]})).contains("returns a value"));
        assert!(err(json!({"fn": "starts_with", "args": [{"fact": "x"}]})).contains("takes"));
        assert!(err(json!({"fn": "exists", "args": ["x"]})).contains("fact reference"));
        assert!(
            err(json!({"fn": "percentage", "args": [{"fact": "c"}, 150]})).contains("0 to 100")
        );
        assert!(err(json!({"fact": "x", "op": "~", "value": "("})).contains("regular expression"));
        assert!(err(json!({"fact": "x", "op": "in", "value": 1})).contains("list"));
        assert!(err(json!({"all": [], "any": []})).contains("unexpected key"));
        assert!(err(json!({"op": "=", "left": {"x": 1}, "right": 1})).contains("operand"));
        assert!(err(json!("text")).contains("expected an object"));
        assert!(err(json!({})).contains("expected one of"));
    }

    #[test]
    fn enforces_depth_and_node_limits() {
        let mut deep = json!({"fact": "x", "op": "=", "value": 1});
        for _ in 0..MAX_DEPTH {
            deep = json!({"not": deep});
        }
        assert!(Expr::parse(&deep).unwrap_err().contains("nesting"));

        let wide = json!({"any": (0..MAX_NODES)
            .map(|i| json!({"fact": "x", "op": "=", "value": i}))
            .collect::<Vec<_>>()});
        assert!(Expr::parse(&wide).unwrap_err().contains("nodes"));

        let mut ok = json!({"fact": "x", "op": "=", "value": 1});
        for _ in 0..(MAX_DEPTH - 1) {
            ok = json!({"not": ok});
        }
        assert!(Expr::parse(&ok).is_ok());
    }

    #[test]
    fn catalogs_cover_every_function_and_operator() {
        let names: Vec<_> = function_catalog().iter().map(|f| f.name).collect();
        for f in FUNCTIONS {
            assert!(names.contains(&f.name));
            let example: Value = serde_json::from_str(f.example).unwrap();
            assert!(
                parse_operand(&example, "x", 1, &mut Budget { nodes: 50 }).is_ok(),
                "{}",
                f.name
            );
        }
        assert_eq!(operator_catalog().len(), 10);
    }
}
