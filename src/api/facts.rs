//! Facts API endpoints

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::AppState;

/// Create routes for fact endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(query_facts))
        .route("/names", get(list_fact_names))
}

#[derive(Deserialize)]
pub struct FactsQuery {
    pub name: Option<String>,
    pub value: Option<String>,
    pub certname: Option<String>,
}

/// Query facts across all nodes
async fn query_facts(
    State(_state): State<AppState>,
    Query(_query): Query<FactsQuery>,
) -> Json<Vec<serde_json::Value>> {
    // TODO: Implement PuppetDB query
    Json(vec![])
}

/// List all unique fact names
async fn list_fact_names(State(_state): State<AppState>) -> Json<Vec<String>> {
    // TODO: Implement PuppetDB query
    Json(vec![])
}
