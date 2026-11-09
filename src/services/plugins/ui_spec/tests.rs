//! Unit tests of the declarative UI spec (H-6 / OQ-8): the mfa-fido2
//! example specs, the layouts, route references, the sandbox rules and the
//! limits.

use super::*;
use crate::models::{ApiRouteDecl, Requires};
use serde_json::json;

const PROFILE: &str =
    include_str!("../../../../tests/fixtures/plugins/mfa-fido2-ui/profile-security.json");
const ADMIN: &str = include_str!("../../../../tests/fixtures/plugins/mfa-fido2-ui/admin-auth.json");

fn decl(method: &str, path: &str) -> ApiRouteDecl {
    ApiRouteDecl {
        method: method.into(),
        path: path.into(),
        handler: None,
        requires: None,
        rate_limit: None,
        step_up: false,
    }
}

/// The api_routes of the mfa-fido2 manifest (SPEC 12.2).
fn fido_routes() -> Vec<ApiRouteDecl> {
    let admin = |m: &str, p: &str, a: &str| {
        let mut d = decl(m, p);
        d.requires = Some(Requires {
            resource: "plugin.mfa-fido2.admin".into(),
            action: a.into(),
        });
        d
    };
    vec![
        decl("GET", "/me/credentials"),
        decl("PATCH", "/me/credentials/{credential_ref}"),
        admin("GET", "/admin/credentials", "read"),
        admin("GET", "/admin/summary", "read"),
        admin("GET", "/admin/authenticators", "read"),
        admin(
            "POST",
            "/admin/credentials/{credential_ref}/suspend",
            "write",
        ),
        ApiRouteDecl {
            step_up: true,
            ..admin(
                "POST",
                "/admin/credentials/{credential_ref}/unsuspend",
                "write",
            )
        },
        admin("POST", "/admin/policy/preview", "read"),
    ]
}

fn check_with(
    spec: &Value,
    routes: &[ApiRouteDecl],
    context: &[&str],
) -> Result<ValidSpec, Vec<String>> {
    let ctx = SpecContext {
        routes,
        slot_context: context,
    };
    validate_spec(spec.to_string().as_bytes(), &ctx)
}

fn check(spec: Value) -> Result<ValidSpec, Vec<String>> {
    check_with(&spec, &fido_routes(), &["user"])
}

/// Asserts the spec is refused with a problem containing `needle`.
fn refused(spec: Value, needle: &str) {
    match check(spec) {
        Ok(_) => panic!("spec accepted, expected a problem with '{needle}'"),
        Err(list) => assert!(
            list.iter().any(|p| p.contains(needle)),
            "no problem contains '{needle}': {list:?}"
        ),
    }
}

fn table(extra: Value) -> Value {
    let mut t = json!({
        "layout": "table",
        "source": {"route": "/me/credentials", "path": "credentials"},
        "columns": [{"key": "nickname", "label": "Nickname"}]
    });
    for (k, v) in extra.as_object().unwrap() {
        t[k] = v.clone();
    }
    t
}

fn rename(extra: Value) -> Value {
    let mut a = json!({
        "id": "rename",
        "label": "Rename",
        "method": "PATCH",
        "route": "/me/credentials/{credential_ref}",
        "fields": [{"name": "nickname", "label": "Nickname", "type": "text", "initial": "nickname"}]
    });
    for (k, v) in extra.as_object().unwrap() {
        a[k] = v.clone();
    }
    a
}

// ---- the mfa-fido2 example specs ----------------------------------------------

#[test]
fn the_mfa_fido2_example_specs_are_valid_and_list_their_routes() {
    let profile = validate_spec(
        PROFILE.as_bytes(),
        &SpecContext {
            routes: &fido_routes(),
            slot_context: &["user"],
        },
    )
    .expect("profile-security.json");
    assert_eq!(
        profile.routes,
        vec![
            ("GET".to_string(), "/me/credentials".to_string()),
            (
                "PATCH".to_string(),
                "/me/credentials/{credential_ref}".to_string()
            ),
        ]
    );
    let admin = validate_spec(
        ADMIN.as_bytes(),
        &SpecContext {
            routes: &fido_routes(),
            slot_context: &[],
        },
    )
    .expect("admin-auth.json");
    assert_eq!(admin.routes.len(), 6);
    assert!(admin.routes.contains(&(
        "POST".into(),
        "/admin/credentials/{credential_ref}/suspend".into()
    )));
    assert_eq!(admin.value["layout"], "stack");
}

#[test]
fn static_layouts_of_earlier_versions_stay_valid() {
    for spec in [
        json!({"layout": "markdown", "content": "# Title\n\n- one\n- two [nodes](/nodes)"}),
        json!({"layout": "table", "columns": [{"key": "host", "label": "Host"}], "rows": [{"host": "a"}]}),
        json!({"layout": "stats", "items": [{"label": "Nodes", "value": 42, "hint": "last hour"}]}),
        json!({"layout": "detail", "title": "Info", "fields": [{"label": "Owner", "value": "ops"}]}),
        json!({"layout": "stack", "blocks": [{"layout": "markdown", "content": "x"}]}),
    ] {
        let ok = check_with(&spec, &[], &[]);
        assert!(ok.is_ok(), "{spec}: {ok:?}");
        assert!(ok.unwrap().routes.is_empty());
    }
}

// ---- shape -----------------------------------------------------------------------

#[test]
fn unknown_keys_layouts_and_formats_are_refused() {
    refused(table(json!({"html": "<b>x</b>"})), "unknown key 'html'");
    refused(
        json!({"layout": "carousel"}),
        "layout 'carousel' is not supported",
    );
    refused(json!({"content": "x"}), "layout is required");
    refused(
        table(json!({"columns": [{"key": "a", "label": "A", "format": "html"}]})),
        "format 'html' is not supported",
    );
    refused(
        table(json!({"columns": [{"key": "a", "label": "A", "onclick": "x"}]})),
        "columns[0]: unknown key 'onclick'",
    );
    refused(
        table(json!({"data_source": {"fn": "list"}})),
        "unknown key 'data_source'",
    );
    refused(json!([1, 2]), "must be a JSON object");
}

#[test]
fn oversized_invalid_or_too_deep_specs_are_refused() {
    let ctx = SpecContext {
        routes: &[],
        slot_context: &[],
    };
    let big = format!(
        "{{\"layout\":\"markdown\",\"content\":\"{}\"}}",
        "a".repeat(MAX_SPEC_BYTES)
    );
    let e = validate_spec(big.as_bytes(), &ctx).unwrap_err();
    assert!(e[0].contains("larger than"), "{e:?}");
    let e = validate_spec(b"{not json", &ctx).unwrap_err();
    assert!(e[0].contains("not valid JSON"), "{e:?}");

    let mut deep = json!({"layout": "markdown", "content": "x"});
    for _ in 0..=MAX_DEPTH {
        deep = json!({"layout": "stack", "blocks": [deep]});
    }
    refused(deep, "nested deeper than");

    let many: Vec<Value> = (0..MAX_BLOCKS)
        .map(|_| json!({"layout": "markdown", "content": "x"}))
        .collect();
    refused(
        json!({"layout": "stack", "blocks": [{"layout": "stack", "blocks": many.clone()}, {"layout": "stack", "blocks": many}]}),
        "more than",
    );
}

#[test]
fn text_must_be_plain_and_bounded() {
    refused(
        table(json!({"title": "a".repeat(200)})),
        "title must be 1 to 120 characters",
    );
    refused(
        table(json!({"title": "bad\u{0007}bell"})),
        "control characters",
    );
    refused(table(json!({"title": 5})), "title must be a string");
    refused(
        table(json!({"columns": [{"key": "a", "label": ""}]})),
        "label must be 1 to 80 characters",
    );
}

#[test]
fn data_paths_are_checked() {
    refused(
        table(json!({"columns": [{"key": "a..b", "label": "A"}]})),
        "is not a valid data path",
    );
    refused(
        table(json!({"columns": [{"key": "__proto__.x", "label": "A"}]})),
        "is not a valid data path",
    );
    refused(
        table(json!({"source": {"route": "/me/credentials", "path": "a/b"}})),
        "is not a valid data path",
    );
}

// ---- sandbox rules ----------------------------------------------------------------

#[test]
fn markdown_links_must_point_to_host_routes() {
    for bad in [
        "[x](https://evil.example)",
        "[x](javascript:alert(1))",
        "[x](//evil.example/a)",
        "[x](/api/v1/users)",
        "[x](data:text/html,a)",
        "[x](nodes)",
    ] {
        refused(
            json!({"layout": "markdown", "content": bad}),
            "links may only point to host routes",
        );
    }
    assert!(
        check(json!({"layout": "markdown", "content": "See [nodes](/nodes?status=failed)."}))
            .is_ok()
    );
}

// ---- route references -------------------------------------------------------------

#[test]
fn routes_must_be_the_plugins_own_with_the_right_method() {
    refused(
        table(json!({"source": {"route": "/admin/secrets"}})),
        "GET /admin/secrets is not one of the plugin's api_routes",
    );
    refused(
        table(json!({"source": {"route": "/admin/credentials/{credential_ref}/suspend"}})),
        "GET /admin/credentials/{credential_ref}/suspend is not one of the plugin's api_routes",
    );
    refused(
        table(json!({"source": {"route": "https://evil.example/x"}})),
        "is not one of the plugin's api_routes",
    );
    refused(
        table(json!({"row_actions": [rename(json!({"method": "PUT"}))]})),
        "PUT /me/credentials/{credential_ref} is not one of the plugin's api_routes",
    );
    refused(
        table(json!({"row_actions": [rename(json!({"method": "GET"}))]})),
        "method 'GET' is not supported",
    );
}

#[test]
fn route_parameters_need_a_value_from_the_row_or_the_slot_context() {
    // A row action fills {credential_ref} from the row by default.
    assert!(check(table(json!({"row_actions": [rename(json!({}))]}))).is_ok());
    // A source has no row: the parameter must come from the slot context.
    let routes = vec![decl("GET", "/nodes/{certname}/keys")];
    let spec = json!({"layout": "detail", "source": {"route": "/nodes/{certname}/keys"}, "fields": [{"label": "a", "key": "a"}]});
    assert!(check_with(&spec, &routes, &["certname"]).is_ok());
    let e = check_with(&spec, &routes, &["user"]).unwrap_err();
    assert!(
        e[0].contains("route parameter 'certname' needs a value"),
        "{e:?}"
    );
    let mapped = json!({"layout": "detail", "source": {"route": "/nodes/{certname}/keys", "params": {"certname": "context.user"}}, "fields": [{"label": "a", "key": "a"}]});
    assert!(check_with(&mapped, &routes, &["user"]).is_ok());
    refused(
        table(json!({"row_actions": [rename(json!({"params": {"other": "row.x"}}))]})),
        "params: 'other' is not a parameter of the route",
    );
    refused(
        table(json!({"row_actions": [rename(json!({"params": {"credential_ref": "https://x"}}))]})),
        "must be row.<path> or context.<key>",
    );
    refused(
        json!({"layout": "form", "fields": [{"name": "a", "label": "A"}],
               "submit": {"method": "PATCH", "route": "/me/credentials/{credential_ref}", "label": "Save",
                          "params": {"credential_ref": "row.credential_ref"}}}),
        "row values are only available to row actions",
    );
}

#[test]
fn destructive_actions_need_a_confirmation() {
    refused(
        table(json!({"row_actions": [rename(json!({"danger": true}))]})),
        "a danger or DELETE action needs a confirm",
    );
    let routes = vec![decl("GET", "/items"), decl("DELETE", "/items/{id}")];
    let spec = json!({"layout": "table", "source": {"route": "/items"}, "columns": [{"key": "id", "label": "Id"}],
        "row_actions": [{"id": "delete", "label": "Delete", "method": "DELETE", "route": "/items/{id}"}]});
    let e = check_with(&spec, &routes, &[]).unwrap_err();
    assert!(e.iter().any(|p| p.contains("needs a confirm")), "{e:?}");
    let mut ok = spec.clone();
    ok["row_actions"][0]["confirm"] = json!({"message": "Delete it?"});
    assert!(check_with(&ok, &routes, &[]).is_ok());
}

// ---- layouts ------------------------------------------------------------------------

#[test]
fn tables_take_either_rows_or_a_source() {
    refused(
        table(json!({"rows": [{"nickname": "a"}]})),
        "rows and source cannot be used together",
    );
    refused(
        json!({"layout": "table", "columns": [{"key": "a", "label": "A"}], "filters": [{"name": "q", "label": "Q"}]}),
        "filters need a source",
    );
    refused(
        json!({"layout": "table", "columns": [{"key": "a", "label": "A"}], "row_actions": [rename(json!({}))]}),
        "row_actions need a source",
    );
    refused(
        table(json!({"paging": {"cursor_param": "Cursor!", "next_key": "next"}})),
        "cursor_param",
    );
    refused(
        table(json!({"columns": []})),
        "columns must have 1 to 12 items",
    );
}

#[test]
fn bound_values_need_a_source() {
    refused(
        json!({"layout": "stats", "items": [{"label": "A", "key": "a"}]}),
        "key needs a source",
    );
    refused(
        json!({"layout": "stats", "source": {"route": "/me/credentials"}, "items": [{"label": "A", "key": "a", "value": 1}]}),
        "use either value or key",
    );
    refused(
        json!({"layout": "notice", "text": "x", "when": {"key": "a", "equals": true}}),
        "when needs a source",
    );
}

#[test]
fn form_fields_are_typed_and_bounded() {
    let form = |field: Value| {
        json!({"layout": "form", "fields": [field],
               "submit": {"method": "POST", "route": "/admin/policy/preview", "label": "Preview"}})
    };
    assert!(check(form(
        json!({"name": "config", "label": "Config", "type": "json", "required": true})
    ))
    .is_ok());
    refused(
        form(json!({"name": "Config!", "label": "C"})),
        "name 'Config!' is invalid",
    );
    refused(
        form(json!({"name": "c", "label": "C", "type": "file"})),
        "type 'file' is not supported",
    );
    refused(
        form(json!({"name": "c", "label": "C", "type": "select"})),
        "a select needs options",
    );
    refused(
        form(json!({"name": "c", "label": "C", "type": "number", "max_length": 4})),
        "max_length is only valid for",
    );
    refused(
        form(json!({"name": "c", "label": "C", "type": "number", "default": "x"})),
        "default must be a number",
    );
    refused(
        form(json!({"name": "c", "label": "C", "initial": "x"})),
        "initial is only valid in row actions",
    );
    refused(
        json!({"layout": "form", "fields": [{"name": "a", "label": "A"}, {"name": "a", "label": "B"}],
               "submit": {"method": "POST", "route": "/admin/policy/preview", "label": "Go"}}),
        "field 'a' is used twice",
    );
}

#[test]
fn row_action_conditions_and_ids_are_checked() {
    refused(
        table(json!({"row_actions": [rename(json!({"when": {"key": "status"}}))]})),
        "when needs exactly one of equals, not_equals or in",
    );
    refused(
        table(json!({"row_actions": [rename(json!({})), rename(json!({}))]})),
        "action id 'rename' is used twice",
    );
    refused(
        table(json!({"columns": [{"key": "s", "label": "S", "tones": {"a": "danger"}}]})),
        "tones are only valid with format status",
    );
    refused(
        table(
            json!({"columns": [{"key": "s", "label": "S", "format": "status", "tones": {"a": "red"}}]}),
        ),
        "tone 'red' is not supported",
    );
}

// ---- manifest level ------------------------------------------------------------------

#[test]
fn check_specs_names_the_file_and_the_contribution() {
    use crate::models::{PluginManifest, RenderKind};
    let yaml = "api_version: 2\nid: mfa-fido2\nname: F\nversion: 1.0.0\npublisher: A\n\
        runtime: { kind: builtin, entry: mfa-fido2 }\ncapabilities: [ui:slot:profile.security, api:routes]\n\
        contributes:\n  slots:\n    - { slot: profile.security, id: keys, render: declarative, spec: ui/p.json }\n  \
        api_routes:\n    - { method: GET, path: /me/credentials }\n";
    let m: PluginManifest = serde_norway::from_str(yaml).unwrap();
    assert_eq!(m.contributes.slots[0].render, RenderKind::Declarative);
    let mut files = BTreeMap::new();
    files.insert(
        "ui/p.json".to_string(),
        br#"{"layout":"table","source":{"route":"/admin/summary"},"columns":[{"key":"a","label":"A"}]}"#.to_vec(),
    );
    let e = check_specs(&m, &files);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].starts_with("spec 'ui/p.json' of contribution 'keys': "),
        "{e:?}"
    );
    assert!(e[0].contains("GET /admin/summary is not one of the plugin's api_routes"));
    files.insert(
        "ui/p.json".to_string(),
        br#"{"layout":"table","source":{"route":"/me/credentials"},"columns":[{"key":"a","label":"A"}]}"#.to_vec(),
    );
    assert!(check_specs(&m, &files).is_empty());
}
