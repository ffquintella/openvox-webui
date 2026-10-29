//! Parsing and validation of `plugin.yaml` (T68).
//!
//! Validation is strict and fails closed: any problem is a reason to refuse
//! the package. Every problem found is reported, not just the first.

use std::collections::BTreeSet;
use std::net::IpAddr;

use serde_json::Value;

use crate::models::{
    Capability, CapabilityDecl, CapabilityInfo, MenuEntry, PluginManifest, RenderKind, RiskLevel,
    RuntimeKind, PLUGIN_API_MIN_VERSION, PLUGIN_API_VERSION,
};

pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

pub const MENU_SECTIONS: &[&str] = &["main", "admin", "user-menu"];

/// Icons a plugin menu item may use (lucide-react names; the frontend keeps
/// the matching allowlist).
pub const ALLOWED_ICONS: &[&str] = &[
    "KeyRound",
    "Puzzle",
    "Boxes",
    "Database",
    "FileText",
    "Globe",
    "Link",
    "Shield",
    "Server",
    "Settings",
    "BarChart3",
    "Bell",
    "Cloud",
    "Cpu",
    "Activity",
    "Plug",
];

/// Core menu items plugins may hide or rename. `core:settings` and
/// `core:plugins` are deliberately absent (lock-out protection).
pub const PROTECTED_MENU_TARGETS: &[&str] = &["core:settings", "core:plugins"];

/// Parses and validates a manifest. `files` is the set of package paths.
pub fn parse_and_validate(
    yaml: &[u8],
    files: &BTreeSet<String>,
) -> Result<PluginManifest, Vec<String>> {
    if yaml.len() > MAX_MANIFEST_BYTES {
        return Err(vec!["plugin.yaml is too large".to_string()]);
    }
    let manifest: PluginManifest = serde_norway::from_slice(yaml)
        .map_err(|e| vec![format!("plugin.yaml is not a valid manifest: {}", e)])?;
    let problems = validate(&manifest, files);
    if problems.is_empty() {
        Ok(manifest)
    } else {
        Err(problems)
    }
}

pub fn validate(m: &PluginManifest, files: &BTreeSet<String>) -> Vec<String> {
    let mut p: Vec<String> = Vec::new();

    if !valid_plugin_id(&m.id) {
        p.push(format!(
            "id '{}' is invalid: use 2 to 64 characters of a-z, 0-9 and '-', starting with a letter",
            m.id
        ));
    }
    if m.api_version < PLUGIN_API_MIN_VERSION || m.api_version > PLUGIN_API_VERSION {
        p.push(format!(
            "api_version {} is not supported (this host supports {} to {})",
            m.api_version, PLUGIN_API_MIN_VERSION, PLUGIN_API_VERSION
        ));
    }
    check_text(&mut p, "name", &m.name, 80);
    check_text(&mut p, "publisher", &m.publisher, 80);
    if let Some(l) = &m.license {
        check_text(&mut p, "license", l, 64);
    }
    if semver::Version::parse(&m.version).is_err() {
        p.push(format!("version '{}' is not valid semver", m.version));
    }
    if let Some(min) = &m.min_webui_version {
        if semver::Version::parse(min).is_err() {
            p.push(format!("min_webui_version '{}' is not valid semver", min));
        }
    }

    validate_runtime(&mut p, m, files);

    // Capabilities
    let mut names = BTreeSet::new();
    for decl in &m.capabilities {
        match normalize_capability(decl) {
            Ok(cap) => {
                if let Err(e) = validate_capability(&cap) {
                    p.push(e);
                }
                if !names.insert(cap.name.clone()) {
                    p.push(format!("capability '{}' is declared twice", cap.name));
                }
            }
            Err(e) => p.push(e),
        }
    }

    validate_contributions(&mut p, m, files, &names);
    validate_hooks(&mut p, m, &names);
    validate_facts(&mut p, m, &names);
    validate_reports(&mut p, m, &names);

    if let Some(mig) = &m.migrations {
        if !safe_package_path(&mig.dir) {
            p.push(format!("migrations.dir '{}' is not a safe path", mig.dir));
        }
        if !names.contains("db:own") {
            p.push("migrations need the capability db:own".to_string());
        }
    }
    p
}

fn validate_runtime(p: &mut Vec<String>, m: &PluginManifest, files: &BTreeSet<String>) {
    let r = &m.runtime;
    match r.kind {
        RuntimeKind::Wasm => match &r.entry {
            Some(e) if safe_package_path(e) && e.ends_with(".wasm") => {
                if !files.contains(e) {
                    p.push(format!("runtime.entry '{}' is not in the package", e));
                }
            }
            _ => p.push("runtime.entry must be a .wasm file inside the package".to_string()),
        },
        RuntimeKind::Builtin => match &r.entry {
            Some(e) if valid_plugin_id(e) => {}
            _ => p.push("runtime.entry must name a built-in plugin".to_string()),
        },
        RuntimeKind::Remote => match &r.endpoint {
            Some(u) if u.starts_with("https://") && u.len() > 8 && u.len() <= 512 => {}
            _ => p.push("runtime.endpoint must be an https:// URL".to_string()),
        },
    }
    if let Some(l) = &r.limits {
        if l.memory_mb.is_some_and(|v| v == 0 || v > 1024) {
            p.push("runtime.limits.memory_mb must be between 1 and 1024".to_string());
        }
        if l.timeout_ms.is_some_and(|v| v == 0 || v > 60_000) {
            p.push("runtime.limits.timeout_ms must be between 1 and 60000".to_string());
        }
        if l.invocations_per_minute.is_some_and(|v| v == 0) {
            p.push("runtime.limits.invocations_per_minute must be positive".to_string());
        }
        if l.fuel_per_call.is_some_and(|v| v == 0) {
            p.push("runtime.limits.fuel_per_call must be positive".to_string());
        }
    }
}

fn validate_contributions(
    p: &mut Vec<String>,
    m: &PluginManifest,
    files: &BTreeSet<String>,
    caps: &BTreeSet<String>,
) {
    let page_prefix = format!("/p/{}/", m.id);
    let mut ids = BTreeSet::new();

    for s in &m.contributes.slots {
        if !super::slots::is_known(&s.slot) {
            p.push(format!("unknown slot '{}'", s.slot));
        }
        if !valid_local_id(&s.id) {
            p.push(format!("slot contribution id '{}' is invalid", s.id));
        }
        if !ids.insert(s.id.clone()) {
            p.push(format!("contribution id '{}' is used twice", s.id));
        }
        if !caps.contains(&format!("ui:slot:{}", s.slot)) {
            p.push(format!(
                "contribution '{}' needs the capability ui:slot:{}",
                s.id, s.slot
            ));
        }
        if let Some(t) = &s.title {
            check_text(p, "slot title", t, 80);
        }
        match s.render {
            RenderKind::Declarative => match &s.spec {
                Some(f) if safe_package_path(f) && f.ends_with(".json") => {
                    if !files.contains(f) {
                        p.push(format!("spec '{}' is not in the package", f));
                    }
                }
                _ => p.push(format!(
                    "contribution '{}' needs a .json spec inside the package",
                    s.id
                )),
            },
            RenderKind::Iframe => match &s.src {
                Some(f) if safe_package_path(f) && files.contains(f) => {}
                _ => p.push(format!(
                    "contribution '{}' needs an existing src inside the package",
                    s.id
                )),
            },
            RenderKind::Module => p.push(format!(
                "contribution '{}': trusted modules are not allowed",
                s.id
            )),
        }
        if s.slot == "page" {
            match &s.path {
                Some(path) if path.starts_with(&page_prefix) && safe_url_path(path) => {}
                _ => p.push(format!(
                    "page '{}' needs a path under {}",
                    s.id, page_prefix
                )),
            }
        } else if s.path.is_some() {
            p.push(format!(
                "contribution '{}': path is only valid for page",
                s.id
            ));
        }
        check_requires(p, &m.id, s.requires.as_ref());
    }

    for entry in &m.contributes.menus {
        match entry {
            MenuEntry::Item(i) => {
                if !valid_local_id(&i.id) {
                    p.push(format!("menu id '{}' is invalid", i.id));
                }
                if !ids.insert(format!("menu:{}", i.id)) {
                    p.push(format!("menu id '{}' is used twice", i.id));
                }
                if !MENU_SECTIONS.contains(&i.section.as_str()) {
                    p.push(format!("menu '{}': unknown section '{}'", i.id, i.section));
                } else if !caps.contains(&format!("ui:slot:nav.{}", i.section)) {
                    p.push(format!(
                        "menu '{}' needs the capability ui:slot:nav.{}",
                        i.id, i.section
                    ));
                }
                check_text(p, "menu label", &i.label, 40);
                if let Some(icon) = &i.icon {
                    if !ALLOWED_ICONS.contains(&icon.as_str()) {
                        p.push(format!("menu '{}': icon '{}' is not allowed", i.id, icon));
                    }
                }
                if !i.href.starts_with(&page_prefix) || !safe_url_path(&i.href) {
                    p.push(format!(
                        "menu '{}': href must be under {}",
                        i.id, page_prefix
                    ));
                }
                check_requires(p, &m.id, i.requires.as_ref());
            }
            MenuEntry::Op(o) => {
                if !o.target.starts_with("core:") || !safe_url_path(&o.target) {
                    p.push(format!("menu op target '{}' must be a core:<id>", o.target));
                }
                if PROTECTED_MENU_TARGETS.contains(&o.target.as_str()) {
                    p.push(format!("menu item '{}' cannot be changed", o.target));
                }
                match o.op.as_str() {
                    "hide" => {}
                    "rename" => match &o.label {
                        Some(l) => check_text(p, "menu op label", l, 40),
                        None => p.push("menu op 'rename' needs a label".to_string()),
                    },
                    other => p.push(format!("menu op '{}' is not supported", other)),
                }
                if !caps.contains("ui:slot:nav.main") && !caps.contains("ui:slot:nav.admin") {
                    p.push(
                        "changing core menu items needs ui:slot:nav.main or ui:slot:nav.admin"
                            .to_string(),
                    );
                }
            }
        }
    }
}

/// Converts a five-field cron expression to the seven-field form of the
/// `cron` crate (seconds fixed at 0, any year).
pub fn parse_cron(expr: &str) -> Result<cron::Schedule, String> {
    let fields = expr.split_whitespace().count();
    if fields != 5 {
        return Err(format!("cron '{}' must have 5 fields", expr));
    }
    format!("0 {}", expr)
        .parse::<cron::Schedule>()
        .map_err(|e| format!("cron '{}' is invalid: {}", expr, e))
}

fn validate_hooks(p: &mut Vec<String>, m: &PluginManifest, caps: &BTreeSet<String>) {
    use super::filter::{pattern_matches, valid_event_pattern, valid_event_type, Filter};

    match m.events_decl() {
        Err(e) => p.push(e),
        Ok(ev) => {
            if !ev.subscribe.is_empty() && !caps.contains("events:subscribe") {
                p.push("events.subscribe needs the capability events:subscribe".to_string());
            }
            if ev.subscribe.len() > 64 {
                p.push("events.subscribe has more than 64 entries".to_string());
            }
            for s in &ev.subscribe {
                if !valid_event_pattern(&s.event_type) {
                    p.push(format!("event pattern '{}' is invalid", s.event_type));
                }
                if let Some(f) = &s.filter {
                    if let Err(e) = Filter::parse(f) {
                        p.push(format!("filter of '{}': {}", s.event_type, e));
                    }
                }
                if let Some(h) = &s.handler {
                    if !valid_local_id(h) {
                        p.push(format!("event handler '{}' is invalid", h));
                    }
                }
            }
            let own = format!("{}.", m.id);
            for name in &ev.publish {
                if !valid_event_type(name) || !name.starts_with(&own) {
                    p.push(format!(
                        "published event '{}' must be a valid type under '{}*'",
                        name, own
                    ));
                    continue;
                }
                let covered = caps.iter().any(|c| {
                    c.strip_prefix("events:publish:")
                        .is_some_and(|pat| pattern_matches(pat, name))
                });
                if !covered {
                    p.push(format!(
                        "published event '{}' needs a matching events:publish:<pattern> capability",
                        name
                    ));
                }
            }
        }
    }

    let mut seen = BTreeSet::new();
    match m.job_decls() {
        Err(e) => p.push(e),
        Ok(jobs) => {
            if jobs.len() > 32 {
                p.push("more than 32 jobs declared".to_string());
            }
            for j in &jobs {
                if !valid_local_id(&j.id) || !seen.insert(j.id.clone()) {
                    p.push(format!("job id '{}' is invalid or repeated", j.id));
                }
                if let Err(e) = parse_cron(&j.cron) {
                    p.push(e);
                }
                if !valid_local_id(&j.handler) {
                    p.push(format!("job handler '{}' is invalid", j.handler));
                }
            }
        }
    }

    let mut seen = BTreeSet::new();
    match m.webhook_decls() {
        Err(e) => p.push(e),
        Ok(hooks) => {
            if hooks.len() > 16 {
                p.push("more than 16 webhooks declared".to_string());
            }
            for h in &hooks {
                if !valid_local_id(&h.id) || !seen.insert(h.id.clone()) {
                    p.push(format!("webhook id '{}' is invalid or repeated", h.id));
                }
                if h.auth != "hmac-sha256" {
                    p.push(format!(
                        "webhook '{}': auth must be hmac-sha256 (unsigned webhooks are not allowed)",
                        h.id
                    ));
                }
                if !valid_local_id(&h.handler) {
                    p.push(format!("webhook handler '{}' is invalid", h.handler));
                }
            }
        }
    }
}

/// Top-level names a fact namespace may not take: they are injected by the
/// host or are core Puppet/Facter names.
pub const RESERVED_FACT_NAMESPACES: &[&str] = &[
    "certname",
    "clientcert",
    "catalog_environment",
    "environment",
    "trusted",
    "server_facts",
    "facts",
    "facterversion",
    "puppetversion",
    "openvox",
];

/// Lowercase identifier: a letter, then letters, digits and `_`.
pub fn valid_fact_identifier(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=64).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// `contributes.facts` (M25): mandatory namespace, typed items, `secret`
/// refused, export needs `facts:export:external`, pull needs a handler.
fn validate_facts(p: &mut Vec<String>, m: &PluginManifest, caps: &BTreeSet<String>) {
    use crate::models::{FactMode, PluginFactType, MAX_PLUGIN_FACTS_PER_NODE};
    let Some(f) = &m.contributes.facts else {
        if caps.contains("facts:export:external") {
            p.push("facts:export:external needs contributes.facts".to_string());
        }
        return;
    };
    if !caps.contains("facts:provide") {
        p.push("contributes.facts needs the capability facts:provide".to_string());
    }
    if !valid_fact_identifier(&f.namespace) || f.namespace.len() < 2 {
        p.push(format!(
            "facts.namespace '{}' is invalid: use 2 to 64 characters of a-z, 0-9 and '_', starting with a letter",
            f.namespace
        ));
    } else if RESERVED_FACT_NAMESPACES.contains(&f.namespace.as_str()) {
        p.push(format!("facts.namespace '{}' is reserved", f.namespace));
    }
    if f.items.is_empty() {
        p.push("facts.items must declare at least one fact".to_string());
    }
    if f.items.len() > MAX_PLUGIN_FACTS_PER_NODE {
        p.push(format!(
            "facts.items declares more than {} facts",
            MAX_PLUGIN_FACTS_PER_NODE
        ));
    }
    if f.ttl_s.is_some_and(|v| v == 0) {
        p.push("facts.ttl_s must be positive".to_string());
    }
    if let Some(ms) = f.max_stale_s {
        if ms == 0 {
            p.push("facts.max_stale_s must be positive".to_string());
        } else if f.ttl_s.is_some_and(|t| t > ms) {
            p.push("facts.max_stale_s must not be shorter than facts.ttl_s".to_string());
        }
    }
    match (f.mode, &f.handler) {
        (FactMode::Pull, Some(h)) if valid_local_id(h) => {}
        (FactMode::Pull, _) => p.push("facts in pull mode need a valid handler".to_string()),
        (FactMode::Push, Some(_)) => p.push("facts.handler is only valid in pull mode".to_string()),
        (FactMode::Push, None) => {}
    }
    let mut seen = BTreeSet::new();
    for item in &f.items {
        if !valid_fact_identifier(&item.name) {
            p.push(format!(
                "fact name '{}' is invalid: use a-z, 0-9 and '_', starting with a letter",
                item.name
            ));
        }
        if !seen.insert(item.name.clone()) {
            p.push(format!("fact '{}' is declared twice", item.name));
        }
        if let Some(d) = &item.description {
            check_text(p, "fact description", d, 200);
        }
        if item.ttl_s.is_some_and(|v| v == 0) {
            p.push(format!("fact '{}': ttl_s must be positive", item.name));
        }
        match item.sensitivity.as_str() {
            "none" | "pii" => {}
            "secret" => p.push(format!(
                "fact '{}': sensitivity 'secret' is not allowed (facts are visible to catalogs and to many users)",
                item.name
            )),
            other => p.push(format!(
                "fact '{}': sensitivity '{}' must be none or pii",
                item.name, other
            )),
        }
        if let Some(values) = &item.enum_values {
            let scalar = matches!(
                item.fact_type,
                PluginFactType::String | PluginFactType::Integer | PluginFactType::Float
            );
            if !scalar {
                p.push(format!(
                    "fact '{}': enum is only valid for string, integer and float",
                    item.name
                ));
            } else if values.is_empty() || values.len() > 100 {
                p.push(format!(
                    "fact '{}': enum must list 1 to 100 values",
                    item.name
                ));
            } else if values.iter().any(|v| !item.fact_type.matches(v)) {
                p.push(format!(
                    "fact '{}': every enum value must be a {}",
                    item.name,
                    item.fact_type.as_str()
                ));
            }
        }
        if item.exportable && !caps.contains("facts:export:external") {
            p.push(format!(
                "fact '{}' is exportable and needs the capability facts:export:external",
                item.name
            ));
        }
        check_requires(p, &m.id, item.requires.as_ref());
    }
}

const SCHEMA_TYPES: &[&str] = &["string", "integer", "number", "boolean", "array"];
const SCALAR_TYPES: &[&str] = &["string", "integer", "float", "bool"];

/// The JSON Schema subset the host renders as a form: an object whose
/// properties are strings, integers, numbers, booleans or arrays.
fn check_params_schema(p: &mut Vec<String>, id: &str, schema: &Value) {
    let ok_type = schema.get("type").and_then(Value::as_str) == Some("object");
    let props = schema.get("properties").and_then(Value::as_object);
    match (ok_type, props) {
        (true, Some(props)) if props.len() <= 50 => {
            for (name, prop) in props {
                if !valid_fact_identifier(name) {
                    p.push(format!(
                        "report type '{}': parameter '{}' is invalid",
                        id, name
                    ));
                }
                let t = prop.get("type").and_then(Value::as_str).unwrap_or("");
                if !SCHEMA_TYPES.contains(&t) {
                    p.push(format!(
                        "report type '{}': parameter '{}' must have a type among {}",
                        id,
                        name,
                        SCHEMA_TYPES.join(", ")
                    ));
                }
            }
            if let Some(req) = schema.get("required") {
                let names_ok = req.as_array().is_some_and(|a| {
                    a.iter()
                        .all(|r| r.as_str().is_some_and(|r| props.contains_key(r)))
                });
                if !names_ok {
                    p.push(format!(
                        "report type '{}': required must list declared parameters",
                        id
                    ));
                }
            }
        }
        _ => p.push(format!(
            "report type '{}': params_schema must be an object schema with at most 50 properties",
            id
        )),
    }
}

/// Report types, export formats, annotations and datasets (M25, T90).
fn validate_reports(p: &mut Vec<String>, m: &PluginManifest, caps: &BTreeSet<String>) {
    use crate::models::EXPORT_CONTENT_TYPES;
    let c = &m.contributes;
    let need = |p: &mut Vec<String>, what: &str, cap: &str| {
        if !caps.contains(cap) {
            p.push(format!("{} needs the capability {}", what, cap));
        }
    };
    if !c.report_types.is_empty() {
        need(p, "report_types", "reports:provide");
    }
    if c.report_types.len() > 32 {
        p.push("more than 32 report types declared".to_string());
    }
    let mut seen = BTreeSet::new();
    for t in &c.report_types {
        if !valid_plugin_id(&t.id) || !seen.insert(t.id.clone()) {
            p.push(format!("report type id '{}' is invalid or repeated", t.id));
        }
        check_text(p, "report type label", &t.label, 80);
        if let Some(d) = &t.description {
            check_text(p, "report type description", d, 200);
        }
        if let Some(schema) = &t.params_schema {
            check_params_schema(p, &t.id, schema);
        }
        if !valid_local_id(&t.handler) {
            p.push(format!("report type handler '{}' is invalid", t.handler));
        }
        check_requires(p, &m.id, t.requires.as_ref());
    }

    if !c.export_formats.is_empty() {
        need(p, "export_formats", "reports:export:format");
    }
    if c.export_formats.len() > 16 {
        p.push("more than 16 export formats declared".to_string());
    }
    let mut seen = BTreeSet::new();
    for f in &c.export_formats {
        if !valid_plugin_id(&f.id) || !seen.insert(f.id.clone()) {
            p.push(format!(
                "export format id '{}' is invalid or repeated",
                f.id
            ));
        }
        check_text(p, "export format label", &f.label, 80);
        if !EXPORT_CONTENT_TYPES.contains(&f.content_type.as_str()) {
            p.push(format!(
                "export format '{}': content type '{}' is not allowed",
                f.id, f.content_type
            ));
        }
        let ext_ok = (1..=10).contains(&f.extension.len())
            && f.extension
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        if !ext_ok {
            p.push(format!(
                "export format '{}': extension must be 1 to 10 characters of a-z and 0-9",
                f.id
            ));
        }
        if !valid_local_id(&f.handler) {
            p.push(format!("export format handler '{}' is invalid", f.handler));
        }
    }

    if !c.report_annotations.is_empty() {
        need(p, "report_annotations", "reports:annotate");
    }
    if c.report_annotations.len() > 32 {
        p.push("more than 32 report annotations declared".to_string());
    }
    let mut seen = BTreeSet::new();
    for a in &c.report_annotations {
        if !valid_fact_identifier(&a.key) || !seen.insert(a.key.clone()) {
            p.push(format!("annotation key '{}' is invalid or repeated", a.key));
        }
        if !SCALAR_TYPES.contains(&a.value_type.as_str()) {
            p.push(format!(
                "annotation '{}': type must be one of {}",
                a.key,
                SCALAR_TYPES.join(", ")
            ));
        }
        check_text(p, "annotation label", &a.label, 40);
        let places_ok = !a.show_in.is_empty()
            && a.show_in
                .iter()
                .all(|s| s == "list_column" || s == "detail_panel");
        if !places_ok {
            p.push(format!(
                "annotation '{}': show_in must list list_column and/or detail_panel",
                a.key
            ));
        }
        if (a.sortable || a.filterable) && !a.show_in.iter().any(|s| s == "list_column") {
            p.push(format!(
                "annotation '{}': sortable and filterable need show_in list_column",
                a.key
            ));
        }
    }

    if !c.datasets.is_empty() {
        need(p, "datasets", "reports:provide");
    }
    if c.datasets.len() > 16 {
        p.push("more than 16 datasets declared".to_string());
    }
    let mut seen = BTreeSet::new();
    for d in &c.datasets {
        if !valid_plugin_id(&d.id) || !seen.insert(d.id.clone()) {
            p.push(format!("dataset id '{}' is invalid or repeated", d.id));
        }
        if let Some(l) = &d.label {
            check_text(p, "dataset label", l, 80);
        }
        if let Some(t) = &d.description {
            check_text(p, "dataset description", t, 200);
        }
        let cols = &d.schema.columns;
        if cols.is_empty() || cols.len() > 64 {
            p.push(format!("dataset '{}' must declare 1 to 64 columns", d.id));
        }
        let mut names = BTreeSet::new();
        for col in cols {
            if !valid_fact_identifier(&col.name) || !names.insert(col.name.clone()) {
                p.push(format!(
                    "dataset '{}': column '{}' is invalid or repeated",
                    d.id, col.name
                ));
            }
            if !SCALAR_TYPES.contains(&col.column_type.as_str()) {
                p.push(format!(
                    "dataset '{}': column '{}' type must be one of {}",
                    d.id,
                    col.name,
                    SCALAR_TYPES.join(", ")
                ));
            }
            if let Some(l) = &col.label {
                check_text(p, "dataset column label", l, 80);
            }
        }
        if let Some(k) = &d.key {
            if !names.contains(k) {
                p.push(format!("dataset '{}': key '{}' is not a column", d.id, k));
            }
        }
        check_requires(p, &m.id, d.requires.as_ref());
    }
}

/// `requires` may name a core permission (`nodes`) or one of the plugin's own
/// (`plugin.<id>.<name>`); anything else could impersonate another plugin.
fn check_requires(p: &mut Vec<String>, plugin_id: &str, r: Option<&crate::models::Requires>) {
    let Some(r) = r else { return };
    let own = format!("plugin.{}.", plugin_id);
    let ok_resource = r.resource.starts_with(&own)
        || (!r.resource.starts_with("plugin") && valid_local_id(&r.resource.replace('_', "-")));
    if !ok_resource {
        p.push(format!(
            "requires.resource '{}' must be a core resource or start with {}",
            r.resource, own
        ));
    }
    if !valid_local_id(&r.action.replace('_', "-")) {
        p.push(format!("requires.action '{}' is invalid", r.action));
    }
}

pub fn valid_plugin_id(id: &str) -> bool {
    let b = id.as_bytes();
    (2..=64).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && !id.ends_with('-')
}

fn valid_local_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'.')
}

/// Human-readable text: no control characters and no markup characters.
fn check_text(p: &mut Vec<String>, field: &str, v: &str, max: usize) {
    if v.trim().is_empty() || v.chars().count() > max {
        p.push(format!("{} must be 1 to {} characters", field, max));
    } else if v.chars().any(|c| c.is_control() || c == '<' || c == '>') {
        p.push(format!("{} contains forbidden characters", field));
    }
}

/// Relative path inside a package: no traversal, no absolute paths.
pub fn safe_package_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 255
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

fn safe_url_path(path: &str) -> bool {
    path.len() <= 255
        && path
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/-_.:".contains(&c))
        && !path.contains("..")
}

// ============================================================================
// Capabilities
// ============================================================================

pub fn normalize_capability(decl: &CapabilityDecl) -> Result<Capability, String> {
    match decl {
        CapabilityDecl::Name(n) => Ok(Capability {
            name: n.clone(),
            scope: None,
            optional: false,
        }),
        CapabilityDecl::Scoped(map) if map.len() == 1 => {
            let (k, v) = map.iter().next().expect("len checked");
            // `optional` is a property of the declaration, not of the scope.
            let mut scope = v.clone();
            let mut optional = false;
            if let Some(obj) = scope.as_object_mut() {
                match obj.remove("optional") {
                    None => {}
                    Some(Value::Bool(b)) => optional = b,
                    Some(_) => {
                        return Err(format!("capability '{}': optional must be a boolean", k))
                    }
                }
                if obj.is_empty() {
                    scope = Value::Null;
                }
            }
            Ok(Capability {
                name: k.clone(),
                scope: (!scope.is_null()).then_some(scope),
                optional,
            })
        }
        CapabilityDecl::Scoped(_) => {
            Err("a scoped capability must have exactly one name".to_string())
        }
    }
}

pub fn capabilities(m: &PluginManifest) -> Vec<Capability> {
    m.capabilities
        .iter()
        .filter_map(|d| normalize_capability(d).ok())
        .collect()
}

const EXACT_CAPABILITIES: &[&str] = &[
    "nodes:read",
    "facts:read",
    "reports:read",
    "users:read",
    "kv:rw",
    "db:own",
    "email:send",
    "events:subscribe",
    "http:egress",
    "audit:write",
    "auth:provider",
    "auth:factor",
    "auth:credentials",
    "users:provider",
    "facts:provide",
    "facts:export:external",
    "reports:provide",
    "reports:annotate",
    "reports:export:format",
    "datasets:read",
    "classification:operators",
    "classification:membership",
    "classification:templates",
    "classification:behaviors",
    "classification:inject",
    "groups:read",
    "groups:write:own",
];

const PREFIX_CAPABILITIES: &[&str] = &["events:publish:", "secrets:resolve:", "ui:slot:"];

fn validate_capability(c: &Capability) -> Result<(), String> {
    let known = EXACT_CAPABILITIES.contains(&c.name.as_str())
        || PREFIX_CAPABILITIES
            .iter()
            .any(|pre| c.name.starts_with(pre) && c.name.len() > pre.len());
    if !known {
        return Err(format!("unknown capability '{}'", c.name));
    }
    if let Some(slot) = c.name.strip_prefix("ui:slot:") {
        if !super::slots::is_known(slot) {
            return Err(format!("capability '{}' names an unknown slot", c.name));
        }
    }
    if c.name == "http:egress" {
        let hosts = c
            .scope
            .as_ref()
            .and_then(|s| s.get("hosts"))
            .and_then(Value::as_array)
            .filter(|h| !h.is_empty())
            .ok_or("http:egress needs a non-empty hosts allowlist")?;
        for h in hosts {
            let host = h.as_str().ok_or("http:egress hosts must be strings")?;
            validate_egress_host(host)?;
        }
    } else if c.scope.is_some() {
        return Err(format!("capability '{}' takes no scope", c.name));
    }
    Ok(())
}

/// Exact hostnames or `*.domain.tld`; never a bare wildcard, never a private,
/// loopback or link-local (metadata) address literal.
pub fn validate_egress_host(host: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("egress host '{}': {}", host, why));
    if host.is_empty() || host.len() > 253 || host != host.to_ascii_lowercase() {
        return bad("must be a lowercase hostname");
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        let blocked = match ip {
            IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
                    || v4.octets()[0] == 0
                    || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xFFC0) == 0xFE80
                    || (v6.segments()[0] & 0xFE00) == 0xFC00
                    || v6.to_ipv4_mapped().is_some()
            }
        };
        return if blocked {
            bad("private, loopback and link-local addresses are not allowed")
        } else {
            Ok(())
        };
    }
    let name = host.strip_prefix("*.").unwrap_or(host);
    let labels: Vec<&str> = name.split('.').collect();
    if host.starts_with("*.") && labels.len() < 2 {
        return bad("wildcards need a registrable domain");
    }
    if labels.len() < 2 && host != "localhost" {
        return bad("must be a fully qualified hostname");
    }
    if host == "localhost" || name.ends_with(".localhost") || name.ends_with(".internal") {
        return bad("local names are not allowed");
    }
    let label_ok = |l: &&str| {
        !l.is_empty()
            && l.len() <= 63
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    };
    if !labels.iter().all(label_ok) {
        return bad("contains an invalid label");
    }
    Ok(())
}

/// Risk level and approval-screen wording for a capability.
pub fn describe_capability(c: &Capability) -> CapabilityInfo {
    let (risk, text) = match c.name.as_str() {
        "nodes:read" => (
            RiskLevel::Medium,
            "Read nodes and their summary data".to_string(),
        ),
        "facts:read" => (RiskLevel::Medium, "Read node facts".to_string()),
        "reports:read" => (RiskLevel::Medium, "Read Puppet run reports".to_string()),
        "users:read" => (
            RiskLevel::High,
            "Read user accounts (names and emails)".to_string(),
        ),
        "kv:rw" => (
            RiskLevel::Low,
            "Store small key/value data of its own".to_string(),
        ),
        "db:own" => (RiskLevel::Low, "Use its own private database".to_string()),
        "email:send" => (RiskLevel::High, "Send email from this server".to_string()),
        "events:subscribe" => (RiskLevel::Medium, "Receive platform events".to_string()),
        "audit:write" => (RiskLevel::Low, "Write entries to the audit log".to_string()),
        "http:egress" => {
            let hosts = c
                .scope
                .as_ref()
                .and_then(|s| s.get("hosts"))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            (
                RiskLevel::High,
                format!("Send data to external hosts: {}", hosts),
            )
        }
        "auth:credentials" => (
            RiskLevel::Critical,
            "Receive user passwords in clear text".to_string(),
        ),
        "auth:provider" | "auth:factor" | "users:provider" => (
            RiskLevel::Critical,
            "Take part in user authentication".to_string(),
        ),
        "facts:export:external" | "classification:inject" => (
            RiskLevel::High,
            "Change what agents receive from this server".to_string(),
        ),
        "reports:provide" => (
            RiskLevel::Medium,
            "Provide report types and datasets (report jobs run with limits)".to_string(),
        ),
        "reports:annotate" => (
            RiskLevel::Low,
            "Add annotations to Puppet run reports (the reports are not changed)".to_string(),
        ),
        "reports:export:format" => (
            RiskLevel::Medium,
            "Render report results as downloadable files".to_string(),
        ),
        "datasets:read" => (
            RiskLevel::Medium,
            "Read datasets provided by other plugins".to_string(),
        ),
        "facts:provide" => (
            RiskLevel::Medium,
            "Provide node facts in its own namespace (can change classification once an administrator enables it)"
                .to_string(),
        ),
        n if n.starts_with("ui:slot:") => (
            RiskLevel::Low,
            format!("Add interface elements to '{}'", &n["ui:slot:".len()..]),
        ),
        n if n.starts_with("events:publish:") => (
            RiskLevel::Low,
            format!("Publish events named '{}'", &n["events:publish:".len()..]),
        ),
        n if n.starts_with("secrets:resolve:") => (
            RiskLevel::High,
            format!("Resolve secrets under '{}'", &n["secrets:resolve:".len()..]),
        ),
        n if n.starts_with("classification:") => (
            RiskLevel::High,
            "Take part in node classification".to_string(),
        ),
        _ => (RiskLevel::Medium, format!("Use capability {}", c.name)),
    };
    CapabilityInfo {
        name: c.name.clone(),
        scope: c.scope.clone(),
        optional: c.optional,
        risk,
        description: text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    const OK: &str = r#"
api_version: 2
id: acme-demo
name: ACME Demo
version: 1.0.0
publisher: ACME
runtime: { kind: builtin, entry: acme-demo }
capabilities:
  - ui:slot:page
  - ui:slot:nav.main
  - http:egress:
      hosts: [api.acme.com, "*.acme.io"]
contributes:
  slots:
    - { slot: page, id: home, render: declarative, spec: ui/home.json, path: /p/acme-demo/home }
  menus:
    - { id: m, section: main, label: Demo, icon: Puzzle, href: /p/acme-demo/home }
"#;

    #[test]
    fn accepts_a_valid_manifest() {
        let m = parse_and_validate(OK.as_bytes(), &files(&["ui/home.json"])).unwrap();
        assert_eq!(m.id, "acme-demo");
        assert_eq!(capabilities(&m).len(), 3);
    }

    #[test]
    fn rejects_unknown_fields_and_bad_ids() {
        let bad = OK
            .replace("id: acme-demo", "id: Bad_ID")
            .replace("name:", "nope: 1\nname:");
        assert!(parse_and_validate(bad.as_bytes(), &files(&["ui/home.json"])).is_err());
        let bad = OK.replace("id: acme-demo", "id: Bad_ID");
        let e = parse_and_validate(bad.as_bytes(), &files(&["ui/home.json"])).unwrap_err();
        assert!(e.iter().any(|m| m.contains("id 'Bad_ID'")));
    }

    #[test]
    fn contributions_need_their_capability_and_files() {
        let bad = OK.replace("  - ui:slot:page\n", "");
        let e = parse_and_validate(bad.as_bytes(), &files(&["ui/home.json"])).unwrap_err();
        assert!(e.iter().any(|m| m.contains("ui:slot:page")));
        let e = parse_and_validate(OK.as_bytes(), &files(&[])).unwrap_err();
        assert!(e.iter().any(|m| m.contains("not in the package")));
    }

    #[test]
    fn page_paths_and_hrefs_stay_under_the_plugin_prefix() {
        let bad = OK.replace("path: /p/acme-demo/home", "path: /p/other/home");
        assert!(parse_and_validate(bad.as_bytes(), &files(&["ui/home.json"])).is_err());
        let bad = OK.replace("href: /p/acme-demo/home", "href: /users");
        assert!(parse_and_validate(bad.as_bytes(), &files(&["ui/home.json"])).is_err());
    }

    #[test]
    fn egress_hosts_are_restricted() {
        for ok in ["api.acme.com", "*.acme.io", "8.8.8.8"] {
            assert!(validate_egress_host(ok).is_ok(), "{}", ok);
        }
        for bad in [
            "*",
            "*.com",
            "com",
            "localhost",
            "169.254.169.254",
            "10.0.0.1",
            "127.0.0.1",
            "::1",
            "fe80::1",
            "API.acme.com",
            "a b.com",
        ] {
            assert!(validate_egress_host(bad).is_err(), "{}", bad);
        }
    }

    #[test]
    fn protected_menu_items_cannot_be_hidden() {
        let m = OK.replace(
            "menus:\n",
            "menus:\n    - { target: \"core:settings\", op: hide }\n",
        );
        assert!(parse_and_validate(m.as_bytes(), &files(&["ui/home.json"])).is_err());
    }

    const FACTS: &str = r#"
api_version: 2
id: acme-cmdb
name: ACME CMDB
version: 1.0.0
publisher: ACME
runtime: { kind: builtin, entry: acme-cmdb }
capabilities:
  - facts:provide
  - facts:export:external
contributes:
  facts:
    namespace: acme_cmdb
    mode: push
    ttl_s: 3600
    max_stale_s: 86400
    items:
      - { name: owner, type: string, sensitivity: pii, description: "Asset owner" }
      - { name: criticality, type: integer, enum: [1, 2, 3, 4], usable_in_rules: true, exportable: true,
          requires: { resource: plugin.acme-cmdb.facts, action: read } }
"#;

    fn facts_problems(yaml: &str) -> Vec<String> {
        match parse_and_validate(yaml.as_bytes(), &files(&[])) {
            Ok(_) => Vec::new(),
            Err(e) => e,
        }
    }

    #[test]
    fn accepts_a_valid_facts_contribution() {
        let m = parse_and_validate(FACTS.as_bytes(), &files(&[])).unwrap();
        let f = m.contributes.facts.unwrap();
        assert_eq!(f.namespace, "acme_cmdb");
        assert_eq!(f.items.len(), 2);
        assert!(f.items[0].is_pii());
        assert_eq!(f.full_name(&f.items[1]), "acme_cmdb.criticality");
    }

    #[test]
    fn facts_need_their_capabilities_and_a_safe_schema() {
        let cases = [
            (FACTS.replace("  - facts:provide\n", ""), "facts:provide"),
            (
                FACTS.replace("  - facts:export:external\n", ""),
                "facts:export:external",
            ),
            (
                FACTS.replace("sensitivity: pii", "sensitivity: secret"),
                "'secret'",
            ),
            (
                FACTS.replace("namespace: acme_cmdb", "namespace: Acme-CMDB"),
                "namespace",
            ),
            (
                FACTS.replace("namespace: acme_cmdb", "namespace: trusted"),
                "reserved",
            ),
            (FACTS.replace("enum: [1, 2, 3, 4]", "enum: [a, b]"), "enum"),
            (FACTS.replace("name: owner", "name: owner.x"), "fact name"),
            (FACTS.replace("mode: push", "mode: pull"), "handler"),
            (
                FACTS.replace("mode: push", "mode: push\n    handler: resolve"),
                "only valid in pull",
            ),
            (
                FACTS.replace("max_stale_s: 86400", "max_stale_s: 60"),
                "max_stale_s",
            ),
            (FACTS.replace("type: string", "type: blob"), "manifest"),
            (
                FACTS.replace("name: criticality", "name: owner"),
                "declared twice",
            ),
        ];
        for (yaml, needle) in cases {
            let p = facts_problems(&yaml);
            assert!(
                p.iter().any(|m| m.contains(needle)),
                "expected '{needle}' in {p:?}"
            );
        }
        let pull = FACTS.replace("mode: push", "mode: pull\n    handler: resolve_facts");
        assert!(facts_problems(&pull).is_empty());
    }

    const REPORTS: &str = r#"
api_version: 2
id: acme-cmdb
name: ACME CMDB
version: 1.0.0
publisher: ACME
runtime: { kind: builtin, entry: acme-cmdb }
capabilities:
  - reports:provide
  - reports:annotate
  - reports:export:format
contributes:
  report_types:
    - id: asset-compliance
      label: Asset compliance
      params_schema: { type: object, properties: { since: { type: string, format: date } }, required: [since] }
      handler: generate_asset_compliance
      requires: { resource: plugin.acme-cmdb.reports, action: read }
  export_formats:
    - { id: xlsx, label: Excel, content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", extension: xlsx, handler: render_xlsx }
  report_annotations:
    - { key: risk_score, type: integer, label: Risk, show_in: [list_column, detail_panel], sortable: true, filterable: true }
  datasets:
    - id: assets
      schema: { columns: [ { name: certname, type: string }, { name: owner, type: string } ] }
"#;

    #[test]
    fn accepts_report_types_formats_annotations_and_datasets() {
        let m = parse_and_validate(REPORTS.as_bytes(), &files(&[])).unwrap();
        assert_eq!(m.contributes.report_types[0].id, "asset-compliance");
        assert_eq!(m.contributes.datasets[0].key_column(), Some("certname"));
    }

    #[test]
    fn report_contributions_need_capabilities_and_safe_declarations() {
        let cases = [
            (
                REPORTS.replace("  - reports:provide\n", ""),
                "reports:provide",
            ),
            (
                REPORTS.replace("  - reports:annotate\n", ""),
                "reports:annotate",
            ),
            (
                REPORTS.replace("  - reports:export:format\n", ""),
                "reports:export:format",
            ),
            (
                REPORTS.replace(
                    "\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"",
                    "text/html",
                ),
                "not allowed",
            ),
            (
                REPORTS.replace("extension: xlsx", "extension: \"x/y\""),
                "extension",
            ),
            (
                REPORTS.replace("type: object, properties", "type: array, properties"),
                "params_schema",
            ),
            (
                REPORTS.replace("required: [since]", "required: [until]"),
                "required",
            ),
            (
                REPORTS.replace("format: date", "format: date }, x: { type: blob"),
                "type among",
            ),
            (
                REPORTS.replace("type: integer, label: Risk", "type: object, label: Risk"),
                "type must be",
            ),
            (
                REPORTS.replace(
                    "show_in: [list_column, detail_panel]",
                    "show_in: [detail_panel]",
                ),
                "need show_in list_column",
            ),
            (
                REPORTS.replace(
                    "{ name: owner, type: string }",
                    "{ name: owner, type: blob }",
                ),
                "column 'owner' type",
            ),
            (
                REPORTS.replace("id: asset-compliance", "id: \"Bad:Id\""),
                "report type id",
            ),
        ];
        for (yaml, needle) in cases {
            let p = match parse_and_validate(yaml.as_bytes(), &files(&[])) {
                Ok(_) => Vec::new(),
                Err(e) => e,
            };
            assert!(
                p.iter().any(|m| m.contains(needle)),
                "expected '{needle}' in {p:?}"
            );
        }
    }

    #[test]
    fn package_paths_reject_traversal() {
        for bad in ["../x", "/x", "a//b", "a/./b", "a\\b", ""] {
            assert!(!safe_package_path(bad), "{}", bad);
        }
        assert!(safe_package_path("ui/home.json"));
    }
}
