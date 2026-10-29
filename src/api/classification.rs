//! Classification catalogs (M26): the vocabulary the group editor offers.

use axum::{routing::get, Json, Router};
use serde::Serialize;

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

async fn functions(_auth: AuthUser) -> Json<Vec<FunctionInfo>> {
    Json(function_catalog())
}

async fn operators(_auth: AuthUser) -> Json<Vec<OperatorInfo>> {
    Json(operator_catalog())
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

async fn merge_strategies(_auth: AuthUser) -> Json<Vec<Strategy>> {
    let s = |name, description| Strategy { name, description };
    Json(vec![
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
    ])
}
