//! Node-related API endpoints

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use crate::{models::Node, AppState};

/// Create routes for node endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_nodes))
        .route("/{certname}", get(get_node))
        .route("/{certname}/facts", get(get_node_facts))
        .route("/{certname}/reports", get(get_node_reports))
}

/// List all nodes
async fn list_nodes(State(_state): State<AppState>) -> Json<Vec<Node>> {
    // TODO: Implement PuppetDB query
    Json(vec![])
}

/// Get a specific node by certname
async fn get_node(
    State(_state): State<AppState>,
    Path(_certname): Path<String>,
) -> Json<Option<Node>> {
    // TODO: Implement PuppetDB query
    Json(None)
}

/// Get facts for a specific node
async fn get_node_facts(
    State(_state): State<AppState>,
    Path(_certname): Path<String>,
) -> Json<serde_json::Value> {
    // TODO: Implement PuppetDB query
    Json(serde_json::json!({}))
}

/// Get reports for a specific node
async fn get_node_reports(
    State(_state): State<AppState>,
    Path(_certname): Path<String>,
) -> Json<Vec<serde_json::Value>> {
    // TODO: Implement PuppetDB query
    Json(vec![])
}
