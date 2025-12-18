//! Facts API endpoints
//!
//! Provides endpoints for querying facts from PuppetDB.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    models::Fact,
    services::puppetdb::{QueryBuilder, QueryParams},
    utils::error::{AppError, AppResult},
    AppState,
};

/// Create routes for fact endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(query_facts))
        .route("/names", get(list_fact_names))
        .route("/paths", get(list_fact_paths))
}

/// Query parameters for facts query
#[derive(Debug, Deserialize)]
pub struct FactsQuery {
    /// Filter by fact name
    pub name: Option<String>,
    /// Filter by fact value (exact match)
    pub value: Option<String>,
    /// Filter by certname
    pub certname: Option<String>,
    /// Filter by environment
    pub environment: Option<String>,
    /// Maximum number of results
    pub limit: Option<u32>,
    /// Number of results to skip
    pub offset: Option<u32>,
}

/// Response for facts query
#[derive(Debug, Serialize)]
pub struct FactsResponse {
    pub facts: Vec<Fact>,
    pub total: Option<u64>,
}

/// Query facts across all nodes
///
/// GET /api/v1/facts
///
/// Query parameters:
/// - `name`: Filter by fact name
/// - `value`: Filter by fact value (exact match)
/// - `certname`: Filter by certname
/// - `environment`: Filter by environment
/// - `limit`: Maximum number of results (default: 100)
/// - `offset`: Number of results to skip
async fn query_facts(
    State(state): State<AppState>,
    Query(query): Query<FactsQuery>,
) -> AppResult<Json<FactsResponse>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // Build query
    let mut qb = QueryBuilder::new();

    if let Some(ref name) = query.name {
        qb = qb.equals("name", name);
    }

    if let Some(ref value) = query.value {
        qb = qb.equals("value", value);
    }

    if let Some(ref certname) = query.certname {
        qb = qb.equals("certname", certname);
    }

    if let Some(ref env) = query.environment {
        qb = qb.equals("environment", env);
    }

    // Build pagination params
    let mut params = QueryParams::new();
    if let Some(limit) = query.limit {
        params = params.limit(limit);
    } else {
        params = params.limit(100); // Default limit
    }
    if let Some(offset) = query.offset {
        params = params.offset(offset);
    }

    // Execute query
    let facts = puppetdb
        .query_facts_advanced(&qb, params)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query facts: {}", e)))?;

    Ok(Json(FactsResponse {
        total: Some(facts.len() as u64),
        facts,
    }))
}

/// List all unique fact names
///
/// GET /api/v1/facts/names
async fn list_fact_names(State(state): State<AppState>) -> AppResult<Json<Vec<String>>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    let names = puppetdb
        .get_fact_names()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch fact names: {}", e)))?;

    Ok(Json(names))
}

/// Fact path response
#[derive(Debug, Serialize)]
pub struct FactPathResponse {
    pub path: Vec<String>,
    #[serde(rename = "type")]
    pub fact_type: String,
}

/// List all unique fact paths (for structured facts)
///
/// GET /api/v1/facts/paths
async fn list_fact_paths(State(state): State<AppState>) -> AppResult<Json<Vec<FactPathResponse>>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    let paths = puppetdb
        .get_fact_paths()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch fact paths: {}", e)))?;

    let response: Vec<FactPathResponse> = paths
        .into_iter()
        .map(|p| FactPathResponse {
            path: p.path,
            fact_type: p.fact_type,
        })
        .collect();

    Ok(Json(response))
}
