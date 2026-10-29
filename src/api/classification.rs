//! Classification catalogs (M26): the vocabulary the group editor offers.

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    middleware::AuthUser,
    services::classification_expr::{
        function_catalog, operator_catalog, FunctionInfo, OperatorInfo,
    },
    AppState,
};

/// Create routes for classification catalog endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/functions", get(functions))
        .route("/operators", get(operators))
        .route("/behaviors", get(behaviors))
        .route("/merge-strategies", get(merge_strategies))
}

fn tagged<T: Serialize>(items: Vec<T>) -> Vec<Value> {
    items
        .into_iter()
        .filter_map(|i| serde_json::to_value(i).ok())
        .map(|mut v| {
            if let Some(o) = v.as_object_mut() {
                o.insert("source".into(), json!("core"));
            }
            v
        })
        .collect()
}

/// Core functions plus those of the enabled plugins (M27).
async fn functions(State(state): State<AppState>, _auth: AuthUser) -> Json<Vec<Value>> {
    let mut out = tagged::<FunctionInfo>(function_catalog());
    for f in state.plugin_manager.extension_catalog().functions {
        out.push(json!({
            "name": f.id,
            "min_args": f.args.len(),
            "max_args": f.args.len(),
            "returns": if f.returns == Some(crate::models::ValueType::Bool) { "bool" } else { "value" },
            "description": f.description.unwrap_or_default(),
            "example": json!({"fn": f.id, "args": f.args.iter().map(|_| json!(null)).collect::<Vec<_>>()}).to_string(),
            "source": "plugin",
            "plugin_id": f.plugin_id,
            "arg_types": f.args,
        }));
    }
    Json(out)
}

/// Core operators plus those of the enabled plugins (M27).
async fn operators(State(state): State<AppState>, _auth: AuthUser) -> Json<Vec<Value>> {
    let mut out = tagged::<OperatorInfo>(operator_catalog());
    for o in state.plugin_manager.extension_catalog().operators {
        out.push(json!({
            "op": o.id,
            "description": o.description.unwrap_or_default(),
            "source": "plugin",
            "plugin_id": o.plugin_id,
            "arg_types": o.args,
        }));
    }
    Json(out)
}

#[derive(Serialize)]
struct Described {
    key: &'static str,
    description: &'static str,
    values: Vec<&'static str>,
}

async fn behaviors(_auth: AuthUser) -> Json<Vec<Described>> {
    let d = |key, description, values: &[&'static str]| Described {
        key,
        description,
        values: values.to_vec(),
    };
    Json(vec![
        d(
            "inheritance",
            "How the group relates to its ancestors: inherit (default), independent (evaluated even if the parent did not match), block (inherits no classes or variables) or select (inherits only the listed keys)",
            &["inherit", "independent", "block", "select"],
        ),
        d(
            "exclusive_set",
            "Among matching siblings of the same parent and set, only the highest priority stays",
            &[],
        ),
        d(
            "match_mode",
            "How the rules combine, overriding rule_match_type: all, any or n_of (at least k)",
            &["all", "any", "n_of"],
        ),
        d(
            "merge",
            "How the group's classes and variables merge into what it inherited",
            &["deep", "replace", "union", "fail_on_conflict"],
        ),
        d(
            "active_from",
            "The group does not match before this instant (RFC 3339)",
            &[],
        ),
        d(
            "expires_at",
            "The group does not match from this instant on (RFC 3339)",
            &[],
        ),
    ])
}

#[derive(Serialize)]
struct Strategy {
    name: &'static str,
    description: &'static str,
}

async fn merge_strategies(State(state): State<AppState>, _auth: AuthUser) -> Json<Vec<Value>> {
    let s = |name, description| Strategy { name, description };
    let mut out = tagged(vec![
        s(
            "deep",
            "Deep merge of class parameters; the group's value wins (default)",
        ),
        s(
            "replace",
            "A class the group declares replaces the inherited entry as a whole",
        ),
        s(
            "union",
            "Like deep, but lists are united without duplicates",
        ),
        s(
            "fail_on_conflict",
            "A different value for an existing key is an error; the first value is kept",
        ),
    ]);
    for m in state.plugin_manager.extension_catalog().merge_strategies {
        out.push(json!({
            "name": m.id,
            "description": m.description.unwrap_or_default(),
            "source": "plugin",
            "plugin_id": m.plugin_id,
        }));
    }
    Json(out)
}
