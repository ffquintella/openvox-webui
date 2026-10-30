//! PATCH requests (RFC 7644 §3.5.2): `add`, `replace` and `remove`, with or
//! without a path. Operation names are case-insensitive because some
//! clients send `Replace` or `Add`.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::filter::{matches, parse_path, PatchPath};
use super::resources::{
    get_ci, member_ids, remove_user_path, set_user_object, set_user_path, GroupAttrs, SetMode,
    UserAttrs,
};
use super::{ScimError, PATCH_SCHEMA};

/// Most operations accepted in one request.
pub const MAX_OPERATIONS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Add,
    Replace,
    Remove,
}

#[derive(Debug, Clone)]
pub struct PatchOperation {
    pub op: OpKind,
    pub path: Option<PatchPath>,
    pub value: Option<Value>,
}

pub fn parse_patch(body: &Value) -> Result<Vec<PatchOperation>, ScimError> {
    if !body.is_object() {
        return Err(ScimError::invalid_syntax("The body must be a JSON object"));
    }
    if let Some(s) = get_ci(body, "schemas") {
        let ok = s
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(PATCH_SCHEMA)));
        if !ok {
            return Err(ScimError::invalid_syntax(format!(
                "schemas must contain {PATCH_SCHEMA}"
            )));
        }
    }
    let ops = get_ci(body, "Operations")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ScimError::invalid_syntax("Operations must be a non-empty list"))?;
    if ops.len() > MAX_OPERATIONS {
        return Err(ScimError::invalid_syntax(format!(
            "At most {MAX_OPERATIONS} operations per request"
        )));
    }
    ops.iter()
        .map(|o| {
            let op = match get_ci(o, "op")
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("add") => OpKind::Add,
                Some("replace") => OpKind::Replace,
                Some("remove") => OpKind::Remove,
                _ => {
                    return Err(ScimError::invalid_syntax(
                        "op must be add, replace or remove",
                    ))
                }
            };
            let path = match get_ci(o, "path") {
                None | Some(Value::Null) => None,
                Some(Value::String(p)) => Some(parse_path(p)?),
                Some(_) => return Err(ScimError::bad("invalidPath", "path must be a string")),
            };
            let value = get_ci(o, "value").cloned();
            if op == OpKind::Remove && path.is_none() {
                return Err(ScimError::bad("noTarget", "remove needs a path"));
            }
            if op != OpKind::Remove && value.is_none() {
                return Err(ScimError::invalid_value("add and replace need a value"));
            }
            Ok(PatchOperation { op, path, value })
        })
        .collect()
}

fn mode(op: OpKind) -> SetMode {
    if op == OpKind::Add {
        SetMode::Add
    } else {
        SetMode::Replace
    }
}

/// Applies the operations to a copy of the user's attributes.
pub fn apply_user_patch(attrs: &mut UserAttrs, ops: &[PatchOperation]) -> Result<(), ScimError> {
    for o in ops {
        let value = o.value.as_ref().unwrap_or(&Value::Null);
        match (&o.path, o.op) {
            (Some(p), OpKind::Remove) => remove_user_path(attrs, p)?,
            (Some(p), op) => set_user_path(attrs, p, value, mode(op))?,
            (None, op) => {
                // Some clients wrap the object in a one-element list.
                let obj = match value {
                    Value::Object(m) => m,
                    Value::Array(a) if a.len() == 1 && a[0].is_object() => {
                        a[0].as_object().expect("checked")
                    }
                    _ => {
                        return Err(ScimError::invalid_value(
                            "A value without path must be an object",
                        ))
                    }
                };
                set_user_object(attrs, obj, mode(op), true)?;
            }
        }
    }
    attrs.validate()
}

fn set_group_attr(
    attrs: &mut GroupAttrs,
    members: &mut BTreeSet<String>,
    p: &PatchPath,
    op: OpKind,
    v: &Value,
) -> Result<(), ScimError> {
    if p.extension {
        return Ok(());
    }
    match p.attr.as_str() {
        "displayname" => match v {
            Value::String(s) if !s.trim().is_empty() => attrs.display_name = s.trim().to_string(),
            _ => {
                return Err(ScimError::invalid_value(
                    "displayName must be a non-empty string",
                ))
            }
        },
        "externalid" => {
            attrs.external_id = match v {
                Value::Null => None,
                Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
                _ => return Err(ScimError::invalid_value("externalId must be a string")),
            }
        }
        "members" => {
            if p.filter.is_some() || p.sub.is_some() {
                return Err(ScimError::bad(
                    "invalidPath",
                    "members can only be added or replaced as a whole list",
                ));
            }
            let ids = member_ids(v)?;
            if op == OpKind::Replace {
                members.clear();
            }
            members.extend(ids);
        }
        "id" | "meta" => {
            return Err(ScimError::mutability(format!("{} is read-only", p.attr)));
        }
        _ => {}
    }
    Ok(())
}

/// Applies the operations to a copy of a group's attributes and members.
pub fn apply_group_patch(
    attrs: &mut GroupAttrs,
    members: &mut BTreeSet<String>,
    ops: &[PatchOperation],
) -> Result<(), ScimError> {
    for o in ops {
        let value = o.value.as_ref().unwrap_or(&Value::Null);
        match (&o.path, o.op) {
            (Some(p), OpKind::Remove) => {
                if p.extension {
                    continue;
                }
                match p.attr.as_str() {
                    "members" => {
                        if let Some(f) = &p.filter {
                            members.retain(|m| !matches(f, &json!({"value": m, "type": "User"})));
                        } else if o.value.is_some() && !value.is_null() {
                            for id in member_ids(value)? {
                                members.remove(&id);
                            }
                        } else {
                            members.clear();
                        }
                    }
                    "externalid" => attrs.external_id = None,
                    "displayname" => return Err(ScimError::mutability("displayName is required")),
                    "id" | "meta" => {
                        return Err(ScimError::mutability(format!("{} is read-only", p.attr)))
                    }
                    _ => {}
                }
            }
            (Some(p), op) => set_group_attr(attrs, members, p, op, value)?,
            (None, op) => {
                let obj = value.as_object().ok_or_else(|| {
                    ScimError::invalid_value("A value without path must be an object")
                })?;
                for (k, v) in obj {
                    let p = parse_path(k)?;
                    if matches!(p.attr.as_str(), "id" | "schemas" | "meta") {
                        continue;
                    }
                    set_group_attr(attrs, members, &p, op, v)?;
                }
            }
        }
    }
    attrs.validate()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(v: Value) -> Vec<PatchOperation> {
        parse_patch(&json!({"schemas": [PATCH_SCHEMA], "Operations": v})).unwrap()
    }

    fn alice() -> UserAttrs {
        UserAttrs {
            user_name: "alice".into(),
            email: Some("alice@example.com".into()),
            active: true,
            ..Default::default()
        }
    }

    #[test]
    fn entra_style_operations_apply() {
        let mut a = alice();
        apply_user_patch(
            &mut a,
            &ops(json!([
                {"op": "Replace", "path": "active", "value": "False"},
                {"op": "Add", "path": "externalId", "value": "ext-9"},
                {"op": "Replace", "path": "name.givenName", "value": "Alicia"},
                {"op": "Replace", "path": "emails[type eq \"work\"].value", "value": "alicia@example.com"},
                {"op": "Add", "path": "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User:department", "value": "Ops"}
            ])),
        )
        .unwrap();
        assert!(!a.active);
        assert_eq!(a.external_id.as_deref(), Some("ext-9"));
        assert_eq!(a.given_name.as_deref(), Some("Alicia"));
        assert_eq!(a.email.as_deref(), Some("alicia@example.com"));
    }

    #[test]
    fn okta_style_operations_without_path_apply() {
        let mut a = alice();
        apply_user_patch(
            &mut a,
            &ops(json!([{"op": "replace", "value": {"active": false, "name.familyName": "Doe", "userName": "alice2"}}])),
        )
        .unwrap();
        assert!(!a.active);
        assert_eq!(a.family_name.as_deref(), Some("Doe"));
        assert_eq!(a.user_name, "alice2");
    }

    #[test]
    fn malformed_requests_are_refused() {
        for body in [
            json!({"Operations": []}),
            json!({"schemas": ["x"], "Operations": [{"op": "add", "path": "a", "value": 1}]}),
            json!({"Operations": [{"op": "move", "path": "a"}]}),
            json!({"Operations": [{"op": "remove"}]}),
            json!({"Operations": [{"op": "add", "path": "displayName"}]}),
            json!({"Operations": [{"op": "add", "path": 3, "value": 1}]}),
            json!([]),
        ] {
            assert!(parse_patch(&body).is_err(), "{body}");
        }
        let mut a = alice();
        assert!(apply_user_patch(&mut a, &ops(json!([{"op": "replace", "value": "x"}]))).is_err());
        assert!(apply_user_patch(
            &mut a,
            &ops(json!([{"op": "replace", "path": "userName", "value": ""}]))
        )
        .is_err());
        assert!(apply_user_patch(
            &mut a,
            &ops(json!([{"op": "replace", "path": "active", "value": "maybe"}]))
        )
        .is_err());
    }

    #[test]
    fn group_member_operations() {
        let (u1, u2, u3) = (
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        );
        let mut g = GroupAttrs {
            display_name: "Ops".into(),
            external_id: None,
        };
        let mut m = BTreeSet::new();
        apply_group_patch(
            &mut g,
            &mut m,
            &ops(json!([
                {"op": "add", "path": "members", "value": [{"value": u1}, {"value": u2}, {"value": u3}]},
                {"op": "remove", "path": format!("members[value eq \"{u1}\"]")},
                {"op": "Remove", "path": "members", "value": [{"value": u2}]},
                {"op": "replace", "value": {"displayName": "Operators", "externalId": "g-ext"}}
            ])),
        )
        .unwrap();
        assert_eq!(m.into_iter().collect::<Vec<_>>(), vec![u3.clone()]);
        assert_eq!(g.display_name, "Operators");
        assert_eq!(g.external_id.as_deref(), Some("g-ext"));

        let mut m: BTreeSet<String> = [u1.clone(), u2.clone()].into();
        apply_group_patch(
            &mut g,
            &mut m,
            &ops(json!([{"op": "replace", "path": "members", "value": [{"value": u3}]}])),
        )
        .unwrap();
        assert_eq!(m.len(), 1);
        apply_group_patch(
            &mut g,
            &mut m,
            &ops(json!([{"op": "remove", "path": "members"}])),
        )
        .unwrap();
        assert!(m.is_empty());
        assert!(apply_group_patch(
            &mut g,
            &mut m,
            &ops(json!([{"op": "remove", "path": "displayName"}]))
        )
        .is_err());
    }
}
