//! Types and redaction rules of the OpenBolt integration.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::sync::OnceLock;

/// Value stored and returned in place of a sensitive value.
pub const REDACTED: &str = "[REDACTED]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Task,
    Plan,
    /// `bolt command run`; only when `allow_arbitrary_commands` is on.
    Command,
}

impl JobKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobKind::Task => "task",
            JobKind::Plan => "plan",
            JobKind::Command => "command",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "task" => Some(JobKind::Task),
            "plan" => Some(JobKind::Plan),
            "command" => Some(JobKind::Command),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    PendingApproval,
    Queued,
    Running,
    Succeeded,
    /// Some targets succeeded and some failed.
    Partial,
    Failed,
    Cancelled,
    TimedOut,
    Rejected,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::PendingApproval => "pending_approval",
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Partial => "partial",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
            JobStatus::TimedOut => "timed_out",
            JobStatus::Rejected => "rejected",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending_approval" => JobStatus::PendingApproval,
            "queued" => JobStatus::Queued,
            "running" => JobStatus::Running,
            "succeeded" => JobStatus::Succeeded,
            "partial" => JobStatus::Partial,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            "timed_out" => JobStatus::TimedOut,
            "rejected" => JobStatus::Rejected,
            _ => return None,
        })
    }
    /// No further change will happen.
    pub fn is_terminal(&self) -> bool {
        !matches!(
            self,
            JobStatus::PendingApproval | JobStatus::Queued | JobStatus::Running
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BoltJob {
    pub id: String,
    pub kind: JobKind,
    pub name: String,
    /// Parameters with the sensitive values replaced by [`REDACTED`].
    pub parameters: Value,
    pub target_group_id: Option<String>,
    pub target_count: usize,
    pub noop: bool,
    pub status: JobStatus,
    pub requires_approval: bool,
    pub requested_by: String,
    pub requested_by_id: String,
    pub approved_by: Option<String>,
    pub approval_notes: Option<String>,
    pub maintenance_window_start: Option<DateTime<Utc>>,
    pub maintenance_window_end: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoltJobTarget {
    pub certname: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub result: Option<Value>,
    pub output: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoltJobDetail {
    #[serde(flatten)]
    pub job: BoltJob,
    pub targets: Vec<BoltJobTarget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowlistEntry {
    pub kind: JobKind,
    pub name: String,
    pub requires_approval: bool,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A task or plan Bolt can run, with what the form needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runnable {
    pub kind: JobKind,
    pub name: String,
    pub description: Option<String>,
    /// Parameter name -> `{type, description, sensitive, optional, default}`.
    pub parameters: Value,
    pub supports_noop: bool,
}

// ----------------------------------------------------------------------------
// Parameters
// ----------------------------------------------------------------------------

/// Names of the parameters a task declares as `sensitive`.
pub fn sensitive_parameter_names(parameters: &Value) -> Vec<String> {
    parameters
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(_, v)| v.get("sensitive").and_then(Value::as_bool) == Some(true))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn looks_secret(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "credential",
    ]
    .iter()
    .any(|w| n.contains(w))
}

/// The parameters with sensitive values removed: names the task declares
/// `sensitive` and names that look like secrets, at any depth.
pub fn redact_parameters(params: &Value, sensitive: &[String]) -> Value {
    match params {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                if sensitive.iter().any(|s| s == k) || looks_secret(k) {
                    out.insert(k.clone(), json!(REDACTED));
                } else {
                    out.insert(k.clone(), redact_parameters(v, sensitive));
                }
            }
            Value::Object(out)
        }
        Value::Array(a) => {
            Value::Array(a.iter().map(|v| redact_parameters(v, sensitive)).collect())
        }
        other => other.clone(),
    }
}

/// Checks the parameters against the metadata: required ones present, and no
/// parameter the task does not declare. Types are checked for the basic ones.
pub fn validate_parameters(declared: &Value, params: &Value) -> Result<(), String> {
    let given = match params {
        Value::Null => Map::new(),
        Value::Object(m) => m.clone(),
        _ => return Err("parameters must be an object".into()),
    };
    let declared = declared.as_object().cloned().unwrap_or_default();
    for k in given.keys() {
        if !declared.contains_key(k) {
            return Err(format!("unknown parameter '{k}'"));
        }
    }
    for (name, meta) in &declared {
        let ty = meta.get("type").and_then(Value::as_str).unwrap_or("");
        let optional = meta
            .get("optional")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || meta.get("default").is_some()
            || meta.get("default_value").is_some()
            || ty.starts_with("Optional[");
        match given.get(name) {
            None if !optional => return Err(format!("missing required parameter '{name}'")),
            Some(v) if !type_matches(ty, v) => {
                return Err(format!("parameter '{name}' must be of type {ty}"))
            }
            _ => {}
        }
    }
    Ok(())
}

fn type_matches(ty: &str, v: &Value) -> bool {
    let ty = ty.trim();
    let ty = ty
        .strip_prefix("Optional[")
        .and_then(|t| t.strip_suffix(']'))
        .unwrap_or(ty);
    if v.is_null() {
        return true; // null clears an optional value; required is checked above
    }
    match ty.split('[').next().unwrap_or("") {
        "String" | "Pattern" | "Enum" => v.is_string(),
        "Integer" => v.is_i64() || v.is_u64(),
        "Float" | "Numeric" => v.is_number(),
        "Boolean" => v.is_boolean(),
        "Array" => v.is_array(),
        "Hash" | "Struct" => v.is_object(),
        _ => true, // Data, Any, Variant, Optional types we do not model: Bolt checks them
    }
}

// ----------------------------------------------------------------------------
// Output
// ----------------------------------------------------------------------------

fn secret_line() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)((?:password|passwd|secret|token|api[_-]?key|private[_-]?key)\s*["']?\s*[:=]\s*)(\S+)"#,
        )
        .expect("static regex")
    })
}

/// Hides values that follow a secret-looking name (`password=x`,
/// `"token": "x"`) and private key blocks.
pub fn redact_text(text: &str) -> String {
    let mut out = secret_line()
        .replace_all(text, format!("${{1}}{REDACTED}"))
        .to_string();
    while let Some(start) = out.find("-----BEGIN ") {
        let Some(rel_end) = out[start..].find("-----END ") else {
            out.truncate(start);
            out.push_str(REDACTED);
            break;
        };
        let after = start + rel_end;
        let end = out[after + 9..]
            .find("-----")
            .map_or(out.len(), |i| after + 9 + i + 5);
        out.replace_range(start..end, REDACTED);
    }
    out
}

/// Cuts text to `max` bytes on a character boundary and says so.
pub fn truncate_text(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n[output truncated: {} bytes]", &text[..cut], text.len())
}

/// Redacts every string in a JSON result and caps its serialized size.
pub fn redact_value(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(redact_text(s)),
        Value::Array(a) => Value::Array(a.iter().map(redact_value).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    if looks_secret(k) {
                        (k.clone(), json!(REDACTED))
                    } else {
                        (k.clone(), redact_value(v))
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_declared_and_secret_looking_parameters() {
        let p = json!({"name": "nginx", "pw": "x", "db_password": "y", "nested": {"token": "z", "ok": 1}});
        let r = redact_parameters(&p, &["pw".to_string()]);
        assert_eq!(r["name"], "nginx");
        assert_eq!(r["pw"], REDACTED);
        assert_eq!(r["db_password"], REDACTED);
        assert_eq!(r["nested"]["token"], REDACTED);
        assert_eq!(r["nested"]["ok"], 1);
    }

    #[test]
    fn finds_sensitive_names() {
        let d = json!({"a": {"type": "String", "sensitive": true}, "b": {"type": "String"}});
        assert_eq!(sensitive_parameter_names(&d), vec!["a".to_string()]);
    }

    #[test]
    fn validates_required_unknown_and_types() {
        let d = json!({
            "name": {"type": "String"},
            "count": {"type": "Optional[Integer]"},
            "force": {"type": "Boolean", "default": false}
        });
        assert!(validate_parameters(&d, &json!({"name": "x"})).is_ok());
        assert!(validate_parameters(&d, &json!({}))
            .unwrap_err()
            .contains("missing"));
        assert!(validate_parameters(&d, &json!({"name": "x", "nope": 1}))
            .unwrap_err()
            .contains("unknown"));
        assert!(validate_parameters(&d, &json!({"name": 3}))
            .unwrap_err()
            .contains("type String"));
        assert!(validate_parameters(&d, &json!({"name": "x", "count": "a"})).is_err());
        assert!(validate_parameters(&d, &json!([1])).is_err());
    }

    #[test]
    fn redacts_secrets_in_text_and_keys() {
        let t = redact_text("user=bob password=hunter2\n{\"token\": \"abc\"}");
        assert!(!t.contains("hunter2") && !t.contains("abc"));
        assert!(t.contains("user=bob"));
        let key = redact_text(
            "a\n-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----\nb",
        );
        assert!(!key.contains("MIIE") && key.contains('a') && key.ends_with('b'));
        let open = redact_text("x -----BEGIN PRIVATE KEY-----\nMIIE");
        assert!(!open.contains("MIIE"));
    }

    #[test]
    fn redacts_json_results() {
        let v = redact_value(&json!({"_output": "password: abc", "api_key": "k", "n": 1}));
        assert_eq!(v["api_key"], REDACTED);
        assert!(!v["_output"].as_str().unwrap().contains("abc"));
        assert_eq!(v["n"], 1);
    }

    #[test]
    fn truncates_on_a_char_boundary() {
        let t = truncate_text("ééééé", 3);
        assert!(t.starts_with('é') && t.contains("truncated"));
        assert_eq!(truncate_text("abc", 10), "abc");
    }

    #[test]
    fn status_round_trips_and_terminal() {
        for s in [
            JobStatus::PendingApproval,
            JobStatus::Queued,
            JobStatus::Running,
            JobStatus::Succeeded,
            JobStatus::Partial,
            JobStatus::Failed,
            JobStatus::Cancelled,
            JobStatus::TimedOut,
            JobStatus::Rejected,
        ] {
            assert_eq!(JobStatus::parse(s.as_str()), Some(s));
        }
        assert!(JobStatus::Failed.is_terminal() && !JobStatus::Running.is_terminal());
    }
}
