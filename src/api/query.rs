//! PQL Query API endpoint
//!
//! Provides an endpoint for executing raw PQL (Puppet Query Language) queries.

use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{
    utils::error::{AppError, AppResult},
    AppState,
};

/// Create routes for query endpoints
pub fn routes() -> Router<AppState> {
    Router::new().route("/", post(execute_pql))
}

/// PQL query request
#[derive(Debug, Deserialize)]
pub struct PqlRequest {
    /// The PQL query string
    pub query: String,
}

/// PQL query response
#[derive(Debug, Serialize)]
pub struct PqlResponse {
    /// Query results
    pub results: serde_json::Value,
    /// Number of results
    pub count: usize,
}

/// Execute a PQL query
///
/// POST /api/v1/query
///
/// Request body:
/// ```json
/// {
///   "query": "nodes { certname ~ 'web.*' }"
/// }
/// ```
///
/// Example queries:
/// - `nodes { }` - List all nodes
/// - `nodes { certname = 'web1.example.com' }` - Get specific node
/// - `facts { name = 'osfamily' }` - Get all osfamily facts
/// - `reports { status = 'failed' limit 10 }` - Get failed reports
/// - `resources { type = 'Package' and title = 'httpd' }` - Find package resources
async fn execute_pql(
    State(state): State<AppState>,
    Json(request): Json<PqlRequest>,
) -> AppResult<Json<PqlResponse>> {
    let puppetdb = state.puppetdb.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable("PuppetDB is not configured".to_string())
    })?;

    // Validate query is not empty
    if request.query.trim().is_empty() {
        return Err(AppError::BadRequest("Query cannot be empty".to_string()));
    }

    // Execute the PQL query
    let results: Vec<serde_json::Value> = puppetdb
        .query(&request.query)
        .await
        .map_err(|e| AppError::Internal(format!("Query failed: {}", e)))?;

    let count = results.len();

    Ok(Json(PqlResponse {
        results: serde_json::Value::Array(results),
        count,
    }))
}
