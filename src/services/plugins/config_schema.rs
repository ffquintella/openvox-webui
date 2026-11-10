//! Validation of plugin settings against the manifest `config_schema`, so a
//! plugin that needs settings cannot be enabled without them.
//!
//! JSON Schema subset: `type`, `required`, `properties`,
//! `additionalProperties: false`, `enum`, `minimum`/`maximum`,
//! `minLength`/`maxLength`, `pattern`, `minItems`/`maxItems`, `uniqueItems`
//! and `items`. Other keywords are ignored.

use serde_json::{Map, Value};

const MAX_DEPTH: usize = 8;

/// Validates the stored settings against `schema`: one message per problem,
/// naming the dotted path of the setting.
pub fn validate_config(schema: &Value, config: &Map<String, Value>) -> Vec<String> {
    let mut errors = Vec::new();
    check(schema, &Value::Object(config.clone()), "", 0, &mut errors);
    errors
}

fn label(path: &str) -> String {
    if path.is_empty() {
        "settings".to_string()
    } else {
        format!("'{}'", path)
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", path, key)
    }
}

fn check(schema: &Value, value: &Value, path: &str, depth: usize, errors: &mut Vec<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    let at = label(path);
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        if !values.contains(value) {
            errors.push(format!("{} is not one of the allowed values", at));
            return;
        }
    }
    let Some(t) = schema.get("type").and_then(Value::as_str) else {
        return;
    };
    match t {
        "object" => {
            let Value::Object(obj) = value else {
                errors.push(format!("{} must be an object", at));
                return;
            };
            let props = schema.get("properties").and_then(Value::as_object);
            if let Some(req) = schema.get("required").and_then(Value::as_array) {
                for name in req.iter().filter_map(Value::as_str) {
                    if obj.get(name).is_none_or(Value::is_null) {
                        errors.push(format!("'{}' is required", join(path, name)));
                    }
                }
            }
            for (k, v) in obj {
                match props.and_then(|p| p.get(k)) {
                    Some(sub) => check(sub, v, &join(path, k), depth + 1, errors),
                    None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                        errors.push(format!("'{}' is not a known setting", join(path, k)))
                    }
                    None => {}
                }
            }
        }
        "string" => {
            let Some(s) = value.as_str() else {
                errors.push(format!("{} must be a string", at));
                return;
            };
            let len = s.chars().count() as u64;
            if schema
                .get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|m| len < m)
            {
                errors.push(format!("{} is too short", at));
            }
            if schema
                .get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|m| len > m)
            {
                errors.push(format!("{} is too long", at));
            }
            // An unparsable pattern is the plugin's bug: it must not block
            // the administrator.
            if let Some(p) = schema.get("pattern").and_then(Value::as_str) {
                if regex::Regex::new(p).is_ok_and(|re| !re.is_match(s)) {
                    errors.push(format!("{} has an invalid format", at));
                }
            }
        }
        "integer" | "number" => {
            let ok = if t == "integer" {
                value.is_i64() || value.is_u64()
            } else {
                value.is_number()
            };
            if !ok {
                errors.push(format!("{} must be a {}", at, t));
                return;
            }
            let n = value.as_f64().unwrap_or(0.0);
            if schema
                .get("minimum")
                .and_then(Value::as_f64)
                .is_some_and(|m| n < m)
            {
                errors.push(format!("{} is below its minimum", at));
            }
            if schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|m| n > m)
            {
                errors.push(format!("{} is above its maximum", at));
            }
        }
        "boolean" => {
            if !value.is_boolean() {
                errors.push(format!("{} must be true or false", at));
            }
        }
        "array" => {
            let Some(items) = value.as_array() else {
                errors.push(format!("{} must be a list", at));
                return;
            };
            let n = items.len() as u64;
            if let Some(m) = schema.get("minItems").and_then(Value::as_u64) {
                if n < m {
                    errors.push(format!("{} needs at least {} entries", at, m));
                }
            }
            if schema
                .get("maxItems")
                .and_then(Value::as_u64)
                .is_some_and(|m| n > m)
            {
                errors.push(format!("{} has too many entries", at));
            }
            if schema.get("uniqueItems") == Some(&Value::Bool(true))
                && items
                    .iter()
                    .enumerate()
                    .any(|(i, a)| items[..i].contains(a))
            {
                errors.push(format!("{} has a repeated entry", at));
            }
            if let Some(sub) = schema.get("items") {
                for (i, v) in items.iter().enumerate() {
                    check(sub, v, &format!("{}[{}]", path, i), depth + 1, errors);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["rp"],
            "properties": {
                "rp": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "origins"],
                    "properties": {
                        "id": {"type": "string", "pattern": "^(localhost|[a-z0-9.-]+\\.[a-z]{2,})$"},
                        "name": {"type": "string", "minLength": 1, "maxLength": 8},
                        "origins": {
                            "type": "array", "minItems": 1, "uniqueItems": true,
                            "items": {"type": "string", "pattern": "^https://[a-z0-9.-]+$"}
                        }
                    }
                },
                "mode": {"type": "string", "enum": ["a", "b"]},
                "n": {"type": "integer", "minimum": 1, "maximum": 3}
            }
        })
    }

    fn cfg(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn an_empty_config_misses_the_required_settings() {
        let e = validate_config(&schema(), &Map::new());
        assert_eq!(e, vec!["'rp' is required".to_string()]);
    }

    #[test]
    fn a_complete_config_passes() {
        let c = cfg(json!({"rp": {"id": "puppet.example.com", "name": "OpenVox",
            "origins": ["https://puppet.example.com"]}, "mode": "a", "n": 2}));
        assert!(validate_config(&schema(), &c).is_empty());
    }

    #[test]
    fn nested_problems_carry_their_path() {
        let c = cfg(json!({"rp": {"id": "NOPE", "name": "", "origins": ["http://x", "http://x"]}}));
        let e = validate_config(&schema(), &c);
        assert!(
            e.contains(&"'rp.id' has an invalid format".to_string()),
            "{e:?}"
        );
        assert!(e.contains(&"'rp.name' is too short".to_string()), "{e:?}");
        assert!(
            e.contains(&"'rp.origins' has a repeated entry".to_string()),
            "{e:?}"
        );
        assert!(
            e.contains(&"'rp.origins[0]' has an invalid format".to_string()),
            "{e:?}"
        );
    }

    #[test]
    fn missing_nested_fields_unknown_keys_and_bad_values_are_reported() {
        let c = cfg(json!({"rp": {"id": "localhost", "extra": 1}, "mode": "z", "n": 9}));
        let e = validate_config(&schema(), &c);
        assert!(e.contains(&"'rp.origins' is required".to_string()), "{e:?}");
        assert!(
            e.contains(&"'rp.extra' is not a known setting".to_string()),
            "{e:?}"
        );
        assert!(
            e.contains(&"'mode' is not one of the allowed values".to_string()),
            "{e:?}"
        );
        assert!(e.contains(&"'n' is above its maximum".to_string()), "{e:?}");
    }

    #[test]
    fn a_schema_with_no_required_settings_accepts_an_empty_config() {
        let s = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        assert!(validate_config(&s, &Map::new()).is_empty());
    }
}
