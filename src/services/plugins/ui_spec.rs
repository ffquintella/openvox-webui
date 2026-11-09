//! Declarative UI specs (H-6 / OQ-8): the JSON a `render: declarative`
//! contribution points to, rendered by host components only. Data and
//! actions are bound to the plugin's own `api_routes` by declared method
//! and path; nothing else can be reached. Specs are validated whenever a
//! package is read and a problem refuses the package. The format is in
//! PLUGIN_PLATFORM.md section 17.18.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::routes::{parse_route_path, Segment, CREDENTIAL_QUERY_KEYS};
use super::slots;
use crate::models::{ApiRouteDecl, PluginManifest, RenderKind};

/// Largest spec file.
pub const MAX_SPEC_BYTES: usize = 64 * 1024;
/// Deepest nesting of `stack` blocks.
pub const MAX_DEPTH: usize = 4;
/// Most blocks in one spec, counting stacks.
pub const MAX_BLOCKS: usize = 32;

const MAX_COLUMNS: usize = 12;
const MAX_STATIC_ROWS: usize = 200;
const MAX_ACTIONS: usize = 4;
const MAX_FORM_FIELDS: usize = 20;
const MAX_ACTION_FIELDS: usize = 8;
const MAX_FILTERS: usize = 8;
const MAX_STAT_ITEMS: usize = 24;
const MAX_DETAIL_FIELDS: usize = 40;
const MAX_OPTIONS: usize = 50;
const MAX_MAP_ENTRIES: usize = 32;
const MAX_QUERY_ENTRIES: usize = 16;
const MAX_MARKDOWN_CHARS: usize = 16 * 1024;
const MAX_TEXT_INPUT: u64 = 65_536;
const MAX_PATH_SEGMENTS: usize = 8;

pub const LAYOUTS: &[&str] = &[
    "markdown", "notice", "stats", "table", "detail", "form", "stack",
];
pub const FORMATS: &[&str] = &[
    "text", "number", "boolean", "date", "datetime", "list", "status",
];
pub const TONES: &[&str] = &["neutral", "info", "success", "warning", "danger"];
pub const FIELD_TYPES: &[&str] = &["text", "textarea", "number", "checkbox", "select", "json"];
const FILTER_TYPES: &[&str] = &["text", "number", "checkbox", "select"];
/// Methods of actions and form submissions; data sources are always `GET`.
pub const ACTION_METHODS: &[&str] = &["POST", "PUT", "PATCH", "DELETE"];
/// Path segments that could reach an object's prototype in the browser.
const FORBIDDEN_SEGMENTS: &[&str] = &["__proto__", "prototype", "constructor"];

/// What a spec may refer to: the plugin's own routes and the context keys
/// of the slot it is contributed to.
pub struct SpecContext<'a> {
    pub routes: &'a [ApiRouteDecl],
    pub slot_context: &'a [&'a str],
}

/// A valid spec and the routes it refers to, as `(method, declared path)`
/// in order of first use.
#[derive(Debug, Clone)]
pub struct ValidSpec {
    pub value: Value,
    pub routes: Vec<(String, String)>,
}

/// Parses and validates one spec file. Every problem is reported, each with
/// the JSON location it was found at.
pub fn validate_spec(bytes: &[u8], ctx: &SpecContext) -> Result<ValidSpec, Vec<String>> {
    if bytes.len() > MAX_SPEC_BYTES {
        return Err(vec![format!(
            "the spec is larger than {} KiB",
            MAX_SPEC_BYTES / 1024
        )]);
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| vec![format!("the spec is not valid JSON: {}", e)])?;
    let mut c = Checker {
        ctx,
        problems: Vec::new(),
        routes: Vec::new(),
        blocks: 0,
    };
    c.block("spec", &value, 0);
    if c.blocks > MAX_BLOCKS {
        c.problems
            .push(format!("the spec has more than {} blocks", MAX_BLOCKS));
    }
    if c.problems.is_empty() {
        Ok(ValidSpec {
            value,
            routes: c.routes,
        })
    } else {
        Err(c.problems)
    }
}

/// Validates the spec of every declarative slot contribution of a package.
/// A missing file is the manifest validation's to report.
pub fn check_specs(m: &PluginManifest, files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut out = Vec::new();
    for s in m
        .contributes
        .slots
        .iter()
        .filter(|s| s.render == RenderKind::Declarative)
    {
        let Some(file) = &s.spec else { continue };
        let Some(bytes) = files.get(file) else {
            continue;
        };
        let ctx = SpecContext {
            routes: &m.contributes.api_routes,
            slot_context: slots::find(&s.slot).map(|x| x.context).unwrap_or(&[]),
        };
        if let Err(list) = validate_spec(bytes, &ctx) {
            out.extend(
                list.into_iter()
                    .map(|e| format!("spec '{}' of contribution '{}': {}", file, s.id, e)),
            );
        }
    }
    out
}

/// A host route a markdown link may point to: an application path, never a
/// scheme, a protocol-relative URL or the API.
pub fn is_host_route(target: &str) -> bool {
    target.starts_with('/')
        && !target.starts_with("//")
        && target != "/api"
        && !target.starts_with("/api/")
        && !target.starts_with("/api?")
        && target.len() <= 512
        && !target
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
}

/// `a.b.0.c`: 1 to 8 segments of `[A-Za-z0-9_-]`, none that reaches a
/// prototype.
pub fn valid_data_path(p: &str) -> bool {
    let segs: Vec<&str> = p.split('.').collect();
    segs.len() <= MAX_PATH_SEGMENTS
        && segs.iter().all(|s| {
            (1..=64).contains(&s.len())
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                && !FORBIDDEN_SEGMENTS.contains(s)
        })
}

fn valid_ident(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    (1..=max).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn valid_action_id(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=32).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn is_scalar(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Number(_) | Value::Bool(_))
}

/// Where a binding is evaluated: whether a current record (`row.`) exists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// No record: route parameters come from the slot context.
    Block,
    /// A table row or the record of a bound detail block.
    Row,
}

struct Checker<'a> {
    ctx: &'a SpecContext<'a>,
    problems: Vec<String>,
    routes: Vec<(String, String)>,
    blocks: usize,
}

impl Checker<'_> {
    fn bad(&mut self, msg: String) {
        self.problems.push(msg);
    }

    /// The object at `at` with only `allowed` keys and every `required` key.
    fn object<'v>(
        &mut self,
        at: &str,
        v: &'v Value,
        allowed: &[&str],
        required: &[&str],
    ) -> Option<&'v Map<String, Value>> {
        let Value::Object(m) = v else {
            self.bad(format!("{at} must be a JSON object"));
            return None;
        };
        for k in m.keys() {
            if !allowed.contains(&k.as_str()) {
                self.bad(format!("{at}: unknown key '{k}'"));
            }
        }
        for k in required {
            if !m.contains_key(*k) {
                self.bad(format!("{at}: {k} is required"));
            }
        }
        Some(m)
    }

    fn array<'v>(&mut self, at: &str, v: Option<&'v Value>, min: usize, max: usize) -> &'v [Value] {
        match v {
            None => &[],
            Some(Value::Array(a)) if (min..=max).contains(&a.len()) => a,
            Some(_) => {
                self.bad(format!("{at} must have {min} to {max} items"));
                &[]
            }
        }
    }

    /// Plain text of 1 to `max` characters; markdown may keep line breaks.
    fn text(&mut self, at: &str, v: Option<&Value>, max: usize, multiline: bool) {
        let Some(v) = v else { return };
        let Value::String(s) = v else {
            self.bad(format!("{at} must be a string"));
            return;
        };
        let n = s.chars().count();
        if n == 0 || n > max || s.trim().is_empty() {
            self.bad(format!("{at} must be 1 to {max} characters"));
        }
        if s.chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
        {
            self.bad(format!("{at} must not contain control characters"));
        }
    }

    fn path(&mut self, at: &str, v: Option<&Value>) {
        match v {
            None => {}
            Some(Value::String(s)) if valid_data_path(s) => {}
            Some(other) => self.bad(format!(
                "{at}: {} is not a valid data path (use dotted names of [A-Za-z0-9_-])",
                other
            )),
        }
    }

    fn boolean(&mut self, at: &str, v: Option<&Value>) {
        if v.is_some_and(|v| !v.is_boolean()) {
            self.bad(format!("{at} must be true or false"));
        }
    }

    fn one_of(&mut self, at: &str, v: Option<&Value>, allowed: &[&str], what: &str) {
        match v {
            None => {}
            Some(Value::String(s)) if allowed.contains(&s.as_str()) => {}
            Some(Value::String(s)) => self.bad(format!(
                "{at}: {what} '{s}' is not supported (use {})",
                allowed.join(", ")
            )),
            Some(_) => self.bad(format!("{at} must be a string")),
        }
    }

    // ---- blocks ------------------------------------------------------------

    fn block(&mut self, at: &str, v: &Value, depth: usize) {
        self.blocks += 1;
        let Value::Object(m) = v else {
            self.bad(format!("{at} must be a JSON object"));
            return;
        };
        let layout = match m.get("layout") {
            None => {
                self.bad(format!("{at}: layout is required"));
                return;
            }
            Some(Value::String(s)) if LAYOUTS.contains(&s.as_str()) => s.as_str(),
            Some(other) => {
                let shown = match other {
                    Value::String(s) => format!("'{s}'"),
                    o => o.to_string(),
                };
                self.bad(format!(
                    "{at}: layout {shown} is not supported (use {})",
                    LAYOUTS.join(", ")
                ));
                return;
            }
        };
        self.text(&format!("{at}.title"), m.get("title"), 120, false);
        self.text(
            &format!("{at}.description"),
            m.get("description"),
            500,
            false,
        );
        match layout {
            "markdown" => self.markdown(at, v),
            "notice" => self.notice(at, v),
            "stats" => self.stats(at, v),
            "table" => self.table(at, v),
            "detail" => self.detail(at, v),
            "form" => self.form(at, v),
            _ => self.stack(at, v, depth),
        }
    }

    fn stack(&mut self, at: &str, v: &Value, depth: usize) {
        let allowed = ["layout", "title", "description", "blocks"];
        let Some(m) = self.object(at, v, &allowed, &["blocks"]) else {
            return;
        };
        if depth >= MAX_DEPTH {
            self.bad(format!(
                "{at}: stacks are nested deeper than {MAX_DEPTH} levels"
            ));
            return;
        }
        let blocks = self.array(&format!("{at}.blocks"), m.get("blocks"), 1, MAX_BLOCKS);
        for (i, b) in blocks.iter().enumerate() {
            self.block(&format!("{at}.blocks[{i}]"), b, depth + 1);
        }
    }

    fn markdown(&mut self, at: &str, v: &Value) {
        let allowed = ["layout", "title", "description", "content"];
        let Some(m) = self.object(at, v, &allowed, &["content"]) else {
            return;
        };
        let content = m.get("content");
        self.text(&format!("{at}.content"), content, MAX_MARKDOWN_CHARS, true);
        if let Some(Value::String(s)) = content {
            for target in markdown_link_targets(s) {
                if !is_host_route(target) {
                    self.bad(format!(
                        "{at}.content: links may only point to host routes such as /nodes, not '{}'",
                        target.chars().take(80).collect::<String>()
                    ));
                }
            }
        }
    }

    fn notice(&mut self, at: &str, v: &Value) {
        let allowed = [
            "layout",
            "title",
            "description",
            "text",
            "tone",
            "source",
            "when",
        ];
        let Some(m) = self.object(at, v, &allowed, &["text"]) else {
            return;
        };
        self.text(&format!("{at}.text"), m.get("text"), 500, false);
        self.one_of(&format!("{at}.tone"), m.get("tone"), TONES, "tone");
        let bound = self.source(at, m.get("source"));
        if let Some(w) = m.get("when") {
            if bound {
                self.when(&format!("{at}.when"), w);
            } else {
                self.bad(format!("{at}: when needs a source"));
            }
        }
    }

    fn stats(&mut self, at: &str, v: &Value) {
        let allowed = ["layout", "title", "description", "source", "items"];
        let Some(m) = self.object(at, v, &allowed, &["items"]) else {
            return;
        };
        let bound = self.source(at, m.get("source"));
        let items = self.array(&format!("{at}.items"), m.get("items"), 1, MAX_STAT_ITEMS);
        for (i, item) in items.iter().enumerate() {
            self.display(&format!("{at}.items[{i}]"), item, bound, &["hint"]);
        }
    }

    fn table(&mut self, at: &str, v: &Value) {
        let allowed = [
            "layout",
            "title",
            "description",
            "columns",
            "rows",
            "source",
            "filters",
            "paging",
            "row_actions",
            "actions",
            "empty",
        ];
        let Some(m) = self.object(at, v, &allowed, &["columns"]) else {
            return;
        };
        let bound = self.source(at, m.get("source"));
        if bound && m.contains_key("rows") {
            self.bad(format!("{at}: rows and source cannot be used together"));
        }
        let columns = self.array(&format!("{at}.columns"), m.get("columns"), 1, MAX_COLUMNS);
        for (i, c) in columns.iter().enumerate() {
            let cat = format!("{at}.columns[{i}]");
            let keys = ["key", "label", "format", "labels", "tones", "empty"];
            if let Some(cm) = self.object(&cat, c, &keys, &["key", "label"]) {
                self.path(&format!("{cat}.key"), cm.get("key"));
                self.text(&format!("{cat}.label"), cm.get("label"), 80, false);
                self.formatting(&cat, cm);
            }
        }
        let rows = self.array(&format!("{at}.rows"), m.get("rows"), 0, MAX_STATIC_ROWS);
        for (i, r) in rows.iter().enumerate() {
            if !r.is_object() {
                self.bad(format!("{at}.rows[{i}] must be a JSON object"));
            }
        }
        self.text(&format!("{at}.empty"), m.get("empty"), 200, false);
        for key in ["filters", "paging", "row_actions"] {
            if m.contains_key(key) && !bound {
                self.bad(format!("{at}: {key} need a source"));
            }
        }
        let filters = self.array(&format!("{at}.filters"), m.get("filters"), 1, MAX_FILTERS);
        self.fields(
            &format!("{at}.filters"),
            filters,
            Scope::Block,
            FILTER_TYPES,
        );
        if let Some(p) = m.get("paging") {
            let pat = format!("{at}.paging");
            let keys = ["cursor_param", "next_key"];
            if let Some(pm) = self.object(&pat, p, &keys, &keys) {
                match pm.get("cursor_param") {
                    Some(Value::String(s)) if valid_ident(s, 64) => {}
                    _ => self.bad(format!("{pat}.cursor_param must be a name of [a-z0-9_]")),
                }
                self.path(&format!("{pat}.next_key"), pm.get("next_key"));
            }
        }
        let mut ids = BTreeSet::new();
        let row_actions = self.array(
            &format!("{at}.row_actions"),
            m.get("row_actions"),
            1,
            MAX_ACTIONS,
        );
        for (i, a) in row_actions.iter().enumerate() {
            self.action(&format!("{at}.row_actions[{i}]"), a, Scope::Row, &mut ids);
        }
        let actions = self.array(&format!("{at}.actions"), m.get("actions"), 1, MAX_ACTIONS);
        for (i, a) in actions.iter().enumerate() {
            self.action(&format!("{at}.actions[{i}]"), a, Scope::Block, &mut ids);
        }
    }

    fn detail(&mut self, at: &str, v: &Value) {
        let allowed = [
            "layout",
            "title",
            "description",
            "source",
            "fields",
            "actions",
        ];
        let Some(m) = self.object(at, v, &allowed, &["fields"]) else {
            return;
        };
        let bound = self.source(at, m.get("source"));
        let fields = self.array(
            &format!("{at}.fields"),
            m.get("fields"),
            1,
            MAX_DETAIL_FIELDS,
        );
        for (i, f) in fields.iter().enumerate() {
            self.display(&format!("{at}.fields[{i}]"), f, bound, &[]);
        }
        // A bound detail block shows one record, which its actions act on.
        let scope = if bound { Scope::Row } else { Scope::Block };
        let mut ids = BTreeSet::new();
        let actions = self.array(&format!("{at}.actions"), m.get("actions"), 1, MAX_ACTIONS);
        for (i, a) in actions.iter().enumerate() {
            self.action(&format!("{at}.actions[{i}]"), a, scope, &mut ids);
        }
    }

    fn form(&mut self, at: &str, v: &Value) {
        let allowed = [
            "layout",
            "title",
            "description",
            "fields",
            "submit",
            "result",
        ];
        let Some(m) = self.object(at, v, &allowed, &["fields", "submit"]) else {
            return;
        };
        let fields = self.array(&format!("{at}.fields"), m.get("fields"), 1, MAX_FORM_FIELDS);
        self.fields(&format!("{at}.fields"), fields, Scope::Block, FIELD_TYPES);
        if let Some(s) = m.get("submit") {
            let sat = format!("{at}.submit");
            let keys = [
                "method", "route", "params", "label", "confirm", "danger", "success",
            ];
            if let Some(sm) = self.object(&sat, s, &keys, &["method", "route", "label"]) {
                self.text(&format!("{sat}.label"), sm.get("label"), 40, false);
                self.mutation(&sat, sm, Scope::Block);
            }
        }
        if let Some(r) = m.get("result") {
            let rat = format!("{at}.result");
            if let Some(rm) = self.object(&rat, r, &["fields"], &["fields"]) {
                let fields = self.array(
                    &format!("{rat}.fields"),
                    rm.get("fields"),
                    1,
                    MAX_DETAIL_FIELDS,
                );
                for (i, f) in fields.iter().enumerate() {
                    self.display(&format!("{rat}.fields[{i}]"), f, true, &[]);
                }
            }
        }
    }

    // ---- parts -------------------------------------------------------------

    /// A stats item or detail field: a static `value` or a bound `key`.
    fn display(&mut self, at: &str, v: &Value, bound: bool, extra: &[&str]) {
        let mut allowed = vec![
            "label", "value", "key", "format", "labels", "tones", "empty",
        ];
        allowed.extend_from_slice(extra);
        let Some(m) = self.object(at, v, &allowed, &["label"]) else {
            return;
        };
        self.text(&format!("{at}.label"), m.get("label"), 80, false);
        self.text(&format!("{at}.hint"), m.get("hint"), 160, false);
        match (m.get("value"), m.get("key")) {
            (Some(_), Some(_)) => self.bad(format!("{at}: use either value or key")),
            (None, None) => self.bad(format!("{at}: value or key is required")),
            (Some(v), None) => {
                if !(is_scalar(v) || v.is_null()) {
                    self.bad(format!("{at}.value must be a string, number or boolean"));
                }
            }
            (None, Some(k)) => {
                if !bound {
                    self.bad(format!("{at}: key needs a source"));
                }
                self.path(&format!("{at}.key"), Some(k));
            }
        }
        self.formatting(at, m);
    }

    fn formatting(&mut self, at: &str, m: &Map<String, Value>) {
        self.one_of(&format!("{at}.format"), m.get("format"), FORMATS, "format");
        self.text(&format!("{at}.empty"), m.get("empty"), 40, false);
        if let Some(l) = m.get("labels") {
            self.text_map(&format!("{at}.labels"), l, |c, at, v| {
                c.text(at, Some(v), 80, false)
            });
        }
        if let Some(t) = m.get("tones") {
            if m.get("format").and_then(Value::as_str) != Some("status") {
                self.bad(format!("{at}: tones are only valid with format status"));
            }
            self.text_map(&format!("{at}.tones"), t, |c, at, v| {
                c.one_of(at, Some(v), TONES, "tone")
            });
        }
    }

    fn text_map(&mut self, at: &str, v: &Value, check: impl Fn(&mut Self, &str, &Value)) {
        match v {
            Value::Object(m) if m.len() <= MAX_MAP_ENTRIES => {
                for (k, val) in m {
                    if k.is_empty() || k.chars().count() > 64 || k.chars().any(char::is_control) {
                        self.bad(format!("{at}: key '{k}' must be 1 to 64 characters"));
                    }
                    check(self, &format!("{at}.{k}"), val);
                }
            }
            _ => self.bad(format!(
                "{at} must be an object of at most {MAX_MAP_ENTRIES} entries"
            )),
        }
    }

    /// `{route, path?, params?, query?}`: a `GET` route of the plugin.
    /// Returns whether a source is present (even an invalid one, so the
    /// bindings that depend on it are still checked).
    fn source(&mut self, at: &str, v: Option<&Value>) -> bool {
        let Some(v) = v else { return false };
        let sat = format!("{at}.source");
        let keys = ["route", "path", "params", "query"];
        let Some(m) = self.object(&sat, v, &keys, &["route"]) else {
            return true;
        };
        self.route(&sat, "GET", m.get("route"), m.get("params"), Scope::Block);
        self.path(&format!("{sat}.path"), m.get("path"));
        if let Some(q) = m.get("query") {
            self.query(&sat, q);
        }
        true
    }

    fn query(&mut self, sat: &str, q: &Value) {
        let Value::Object(qm) = q else {
            self.bad(format!("{sat}.query must be an object"));
            return;
        };
        if qm.len() > MAX_QUERY_ENTRIES {
            self.bad(format!(
                "{sat}.query has more than {MAX_QUERY_ENTRIES} entries"
            ));
        }
        for (k, val) in qm {
            if !valid_ident(k, 64) {
                self.bad(format!("{sat}.query: '{k}' must be a name of [a-z0-9_]"));
            } else if CREDENTIAL_QUERY_KEYS.contains(&k.as_str()) {
                self.bad(format!("{sat}.query: '{k}' is reserved for credentials"));
            }
            let ok = match val {
                Value::String(s) => s.chars().count() <= 256 && !s.chars().any(char::is_control),
                Value::Number(_) | Value::Bool(_) => true,
                _ => false,
            };
            if !ok {
                self.bad(format!(
                    "{sat}.query.{k} must be a string of at most 256 characters, a number or a boolean"
                ));
            }
        }
    }

    /// `route` must be declared by the plugin with `method`; every `{param}`
    /// of it needs a value: `row.<path>` (default in row scope) or
    /// `context.<key>` of the slot (default elsewhere).
    fn route(
        &mut self,
        at: &str,
        method: &str,
        route: Option<&Value>,
        params: Option<&Value>,
        scope: Scope,
    ) {
        let Some(route) = route else { return };
        let Value::String(path) = route else {
            self.bad(format!("{at}.route must be a string"));
            return;
        };
        let declared = self
            .ctx
            .routes
            .iter()
            .any(|d| d.method == method && &d.path == path);
        if !declared {
            self.bad(format!(
                "{at}.route: {method} {} is not one of the plugin's api_routes",
                path.chars().take(160).collect::<String>()
            ));
            return;
        }
        let key = (method.to_string(), path.clone());
        if !self.routes.contains(&key) {
            self.routes.push(key);
        }
        let names: Vec<String> = parse_route_path(path)
            .map(|segs| {
                segs.into_iter()
                    .filter_map(|s| match s {
                        Segment::Param(n) => Some(n),
                        Segment::Static(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut mapped: BTreeMap<String, String> = BTreeMap::new();
        match params {
            None => {}
            Some(Value::Object(pm)) => {
                for (k, v) in pm {
                    if !names.contains(k) {
                        self.bad(format!(
                            "{at}.params: '{k}' is not a parameter of the route"
                        ));
                    } else if let Value::String(s) = v {
                        mapped.insert(k.clone(), s.clone());
                    } else {
                        self.bad(format!(
                            "{at}.params.{k} must be row.<path> or context.<key>"
                        ));
                    }
                }
            }
            Some(_) => self.bad(format!("{at}.params must be an object")),
        }
        for name in names {
            let explicit = mapped.get(&name).cloned();
            let source = explicit.clone().unwrap_or_else(|| match scope {
                Scope::Row => format!("row.{name}"),
                Scope::Block => format!("context.{name}"),
            });
            self.param(at, &name, &source, explicit.is_some(), scope);
        }
    }

    fn param(&mut self, at: &str, name: &str, source: &str, explicit: bool, scope: Scope) {
        let have = context_list(self.ctx.slot_context);
        if let Some(p) = source.strip_prefix("row.") {
            if scope != Scope::Row {
                self.bad(format!(
                    "{at}.params.{name}: row values are only available to row actions and bound detail actions"
                ));
            } else if !valid_data_path(p) {
                self.bad(format!(
                    "{at}.params.{name}: '{p}' is not a valid data path"
                ));
            }
        } else if let Some(k) = source.strip_prefix("context.") {
            if self.ctx.slot_context.contains(&k) {
                return;
            }
            if explicit {
                self.bad(format!(
                    "{at}.params.{name}: the slot has no context key '{k}' (it has: {have})"
                ));
            } else {
                self.bad(format!(
                    "{at}: route parameter '{name}' needs a value: map it in params to context.<key> (the slot has: {have})"
                ));
            }
        } else {
            self.bad(format!(
                "{at}.params.{name} must be row.<path> or context.<key>"
            ));
        }
    }

    /// The method, route, confirmation and messages of an action or a form
    /// submission.
    fn mutation(&mut self, at: &str, m: &Map<String, Value>, scope: Scope) {
        let method = match m.get("method") {
            Some(Value::String(s)) if ACTION_METHODS.contains(&s.as_str()) => Some(s.as_str()),
            Some(Value::String(s)) => {
                self.bad(format!(
                    "{at}: method '{s}' is not supported (use {})",
                    ACTION_METHODS.join(", ")
                ));
                None
            }
            Some(_) => {
                self.bad(format!("{at}.method must be a string"));
                None
            }
            None => None,
        };
        if let Some(method) = method {
            self.route(at, method, m.get("route"), m.get("params"), scope);
        }
        self.boolean(&format!("{at}.danger"), m.get("danger"));
        self.text(&format!("{at}.success"), m.get("success"), 200, false);
        let danger = m.get("danger") == Some(&Value::Bool(true)) || method == Some("DELETE");
        match m.get("confirm") {
            Some(c) => {
                let cat = format!("{at}.confirm");
                let keys = ["title", "message", "confirm_label"];
                if let Some(cm) = self.object(&cat, c, &keys, &["message"]) {
                    self.text(&format!("{cat}.title"), cm.get("title"), 80, false);
                    self.text(&format!("{cat}.message"), cm.get("message"), 500, false);
                    self.text(
                        &format!("{cat}.confirm_label"),
                        cm.get("confirm_label"),
                        40,
                        false,
                    );
                }
            }
            None if danger => self.bad(format!("{at}: a danger or DELETE action needs a confirm")),
            None => {}
        }
    }

    fn action(&mut self, at: &str, v: &Value, scope: Scope, ids: &mut BTreeSet<String>) {
        let allowed = [
            "id", "label", "method", "route", "params", "fields", "confirm", "danger", "success",
            "when",
        ];
        let required = ["id", "label", "method", "route"];
        let Some(m) = self.object(at, v, &allowed, &required) else {
            return;
        };
        match m.get("id") {
            Some(Value::String(id)) if valid_action_id(id) => {
                if !ids.insert(id.clone()) {
                    self.bad(format!("{at}: action id '{id}' is used twice"));
                }
            }
            Some(_) => self.bad(format!(
                "{at}.id must be 1 to 32 characters of a-z, 0-9 and '-'"
            )),
            None => {}
        }
        self.text(&format!("{at}.label"), m.get("label"), 40, false);
        self.mutation(at, m, scope);
        let fields = self.array(
            &format!("{at}.fields"),
            m.get("fields"),
            1,
            MAX_ACTION_FIELDS,
        );
        self.fields(&format!("{at}.fields"), fields, scope, FIELD_TYPES);
        if let Some(w) = m.get("when") {
            if scope == Scope::Row {
                self.when(&format!("{at}.when"), w);
            } else {
                self.bad(format!("{at}: when is only valid for row actions"));
            }
        }
    }

    /// `{key, equals | not_equals | in}` over the record.
    fn when(&mut self, at: &str, v: &Value) {
        let keys = ["key", "equals", "not_equals", "in"];
        let Some(m) = self.object(at, v, &keys, &["key"]) else {
            return;
        };
        self.path(&format!("{at}.key"), m.get("key"));
        let given: Vec<&str> = ["equals", "not_equals", "in"]
            .into_iter()
            .filter(|k| m.contains_key(*k))
            .collect();
        if given.len() != 1 {
            self.bad(format!(
                "{at}: when needs exactly one of equals, not_equals or in"
            ));
            return;
        }
        let comparable = |x: &Value| is_scalar(x) || x.is_null();
        let ok = match given[0] {
            "in" => matches!(
                m.get("in"),
                Some(Value::Array(a)) if (1..=MAX_OPTIONS).contains(&a.len()) && a.iter().all(comparable)
            ),
            k => m.get(k).is_some_and(comparable),
        };
        if !ok {
            self.bad(format!(
                "{at}.{}: compare with a string, number, boolean or null (in: a list of them)",
                given[0]
            ));
        }
    }

    fn fields(&mut self, at: &str, list: &[Value], scope: Scope, types: &[&str]) {
        let mut names = BTreeSet::new();
        for (i, f) in list.iter().enumerate() {
            self.field(&format!("{at}[{i}]"), f, scope, types, &mut names);
        }
    }

    fn field(
        &mut self,
        fat: &str,
        f: &Value,
        scope: Scope,
        types: &[&str],
        names: &mut BTreeSet<String>,
    ) {
        let allowed = [
            "name",
            "label",
            "type",
            "required",
            "placeholder",
            "help",
            "default",
            "min_length",
            "max_length",
            "min",
            "max",
            "options",
            "initial",
        ];
        let Some(m) = self.object(fat, f, &allowed, &["name", "label"]) else {
            return;
        };
        match m.get("name") {
            Some(Value::String(n)) if valid_ident(n, 64) => {
                if !names.insert(n.clone()) {
                    self.bad(format!("{fat}: field '{n}' is used twice"));
                }
            }
            Some(Value::String(n)) => self.bad(format!(
                "{fat}: name '{n}' is invalid: use [a-z][a-z0-9_], at most 64 characters"
            )),
            Some(_) => self.bad(format!("{fat}.name must be a string")),
            None => {}
        }
        self.text(&format!("{fat}.label"), m.get("label"), 80, false);
        self.text(
            &format!("{fat}.placeholder"),
            m.get("placeholder"),
            120,
            false,
        );
        self.text(&format!("{fat}.help"), m.get("help"), 300, false);
        self.boolean(&format!("{fat}.required"), m.get("required"));
        let ty = match m.get("type") {
            None => "text",
            Some(Value::String(t)) if types.contains(&t.as_str()) => t.as_str(),
            Some(Value::String(t)) => {
                self.bad(format!(
                    "{fat}: type '{t}' is not supported here (use {})",
                    types.join(", ")
                ));
                return;
            }
            Some(_) => {
                self.bad(format!("{fat}.type must be a string"));
                return;
            }
        };
        self.field_limits(fat, m, ty);
        self.field_options(fat, m, ty);
        if let Some(d) = m.get("default") {
            let ok = match ty {
                "number" => d.is_number(),
                "checkbox" => d.is_boolean(),
                "select" => is_scalar(d),
                _ => d
                    .as_str()
                    .is_some_and(|s| s.chars().count() as u64 <= MAX_TEXT_INPUT),
            };
            if !ok {
                let want = match ty {
                    "number" => "a number",
                    "checkbox" => "true or false",
                    "select" => "one of the option values",
                    _ => "a string",
                };
                self.bad(format!("{fat}: default must be {want}"));
            }
        }
        if let Some(init) = m.get("initial") {
            if scope == Scope::Row {
                self.path(&format!("{fat}.initial"), Some(init));
            } else {
                self.bad(format!(
                    "{fat}: initial is only valid in row actions and bound detail actions"
                ));
            }
        }
    }

    fn field_limits(&mut self, fat: &str, m: &Map<String, Value>, ty: &str) {
        let texty = matches!(ty, "text" | "textarea" | "json");
        for k in ["min_length", "max_length"] {
            let Some(v) = m.get(k) else { continue };
            if !texty {
                self.bad(format!(
                    "{fat}: {k} is only valid for text, textarea and json"
                ));
            } else if !v.as_u64().is_some_and(|n| n <= MAX_TEXT_INPUT) {
                self.bad(format!(
                    "{fat}.{k} must be an integer from 0 to {MAX_TEXT_INPUT}"
                ));
            }
        }
        let lengths = (
            m.get("min_length").and_then(Value::as_u64),
            m.get("max_length").and_then(Value::as_u64),
        );
        if let (Some(min), Some(max)) = lengths {
            if min > max {
                self.bad(format!("{fat}: min_length is larger than max_length"));
            }
        }
        for k in ["min", "max"] {
            let Some(v) = m.get(k) else { continue };
            if ty != "number" {
                self.bad(format!("{fat}: {k} is only valid for number"));
            } else if !v.is_number() {
                self.bad(format!("{fat}.{k} must be a number"));
            }
        }
    }

    fn field_options(&mut self, fat: &str, m: &Map<String, Value>, ty: &str) {
        match (ty, m.get("options")) {
            ("select", Some(Value::Array(opts))) if (1..=MAX_OPTIONS).contains(&opts.len()) => {
                for (j, o) in opts.iter().enumerate() {
                    let oat = format!("{fat}.options[{j}]");
                    let keys = ["value", "label"];
                    if let Some(om) = self.object(&oat, o, &keys, &keys) {
                        if !om.get("value").is_some_and(is_scalar) {
                            self.bad(format!("{oat}.value must be a string, number or boolean"));
                        }
                        self.text(&format!("{oat}.label"), om.get("label"), 80, false);
                    }
                }
            }
            ("select", Some(_)) => {
                self.bad(format!("{fat}.options must have 1 to {MAX_OPTIONS} items"))
            }
            ("select", None) => self.bad(format!("{fat}: a select needs options")),
            (_, Some(_)) => self.bad(format!("{fat}: options are only valid for select")),
            (_, None) => {}
        }
    }
}

fn context_list(keys: &[&str]) -> String {
    if keys.is_empty() {
        "none".to_string()
    } else {
        keys.join(", ")
    }
}

/// Targets of `[text](target)` links, found the way the host renderer
/// finds them.
fn markdown_link_targets(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        match after.find(')') {
            Some(j) => {
                out.push(after[..j].trim());
                rest = &after[j + 1..];
            }
            None => {
                out.push(after.trim());
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
