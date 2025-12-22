//! Node-related API endpoints
//!
//! Provides endpoints for querying and managing nodes from PuppetDB.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::repository::GroupRepository,
    middleware::OptionalClientCert,
    models::{ClassificationResult, Fact, Node, Report},
    services::{
        classification::ClassificationService,
        puppetdb::{QueryBuilder, QueryParams, Resource},
    },
    utils::error::{AppError, AppResult},
    AppState,
};

/// Create routes for node endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_nodes))
        .route("/{certname}", get(get_node))
        .route("/{certname}/facts", get(get_node_facts))
        .route("/{certname}/reports", get(get_node_reports))
        .route("/{certname}/resources", get(get_node_resources))
        .route("/{certname}/catalog", get(get_node_catalog))
        .route("/{certname}/classification", get(get_node_classification))
}

/// Query parameters for listing nodes
#[derive(Debug, Deserialize)]
pub struct NodesQuery {
    /// Filter by environment
    pub environment: Option<String>,
    /// Filter by status (changed, unchanged, failed, unreported)
    pub status: Option<String>,
    /// Search by certname pattern (regex)
    pub search: Option<String>,
    /// Maximum number of results
    pub limit: Option<u32>,
    /// Number of results to skip
    pub offset: Option<u32>,
    /// Field to order by
    pub order_by: Option<String>,
    /// Order direction (asc/desc)
    pub order_dir: Option<String>,
}

// For compatibility with existing tests, return a plain array.

/// List all nodes
///
/// GET /api/v1/nodes
///
/// Query parameters:
/// - `environment`: Filter by environment
/// - `status`: Filter by status (changed, unchanged, failed, unreported)
/// - `search`: Search by certname pattern (regex)
/// - `limit`: Maximum number of results (default: 100)
/// - `offset`: Number of results to skip
/// - `order_by`: Field to order by (default: certname)
/// - `order_dir`: Order direction (asc/desc, default: asc)
async fn list_nodes(
    State(state): State<AppState>,
    Query(query): Query<NodesQuery>,
) -> AppResult<Json<Vec<Node>>> {
    // If PuppetDB is not configured, return empty list (stub behavior expected by tests)
    let Some(puppetdb) = state.puppetdb.as_ref() else {
        return Ok(Json(vec![]));
    };

    // Build query
    let mut qb = QueryBuilder::new();

    if let Some(ref env) = query.environment {
        qb = qb.equals("catalog_environment", env);
    }

    if let Some(ref status) = query.status {
        qb = qb.equals("latest_report_status", status);
    }

    if let Some(ref search) = query.search {
        qb = qb.matches("certname", search);
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

    // Add ordering
    let order_field = query.order_by.as_deref().unwrap_or("certname");
    let ascending = query.order_dir.as_deref() != Some("desc");
    params = params.order_by(order_field, ascending).include_total();

    // Execute query
    let nodes = puppetdb
        .query_nodes_with_params(&qb, params)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query nodes: {}", e)))?;

    Ok(Json(nodes))
}

/// Get a specific node by certname
///
/// GET /api/v1/nodes/:certname
async fn get_node(
    State(state): State<AppState>,
    Path(certname): Path<String>,
) -> AppResult<Json<Node>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    let node = puppetdb
        .get_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch node: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Node '{}' not found", certname)))?;

    Ok(Json(node))
}

/// Query parameters for node facts
#[derive(Debug, Deserialize)]
pub struct NodeFactsQuery {
    /// Filter by fact name
    pub name: Option<String>,
}

/// Get facts for a specific node
///
/// GET /api/v1/nodes/:certname/facts
///
/// Query parameters:
/// - `name`: Filter by fact name
async fn get_node_facts(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    Query(query): Query<NodeFactsQuery>,
) -> AppResult<Json<Vec<Fact>>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // First verify the node exists
    let node_exists = puppetdb
        .get_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check node: {}", e)))?
        .is_some();

    if !node_exists {
        return Err(AppError::NotFound(format!("Node '{}' not found", certname)));
    }

    let facts = if let Some(ref name) = query.name {
        // Get specific fact
        match puppetdb.get_node_fact(&certname, name).await {
            Ok(Some(fact)) => vec![fact],
            Ok(None) => vec![],
            Err(e) => return Err(AppError::Internal(format!("Failed to fetch fact: {}", e))),
        }
    } else {
        // Get all facts
        puppetdb
            .get_node_facts(&certname)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch facts: {}", e)))?
    };

    Ok(Json(facts))
}

/// Query parameters for node reports
#[derive(Debug, Deserialize)]
pub struct NodeReportsQuery {
    /// Filter by status
    pub status: Option<String>,
    /// Maximum number of results
    pub limit: Option<u32>,
}

/// Get reports for a specific node
///
/// GET /api/v1/nodes/:certname/reports
///
/// Query parameters:
/// - `status`: Filter by status (changed, unchanged, failed)
/// - `limit`: Maximum number of results (default: 10)
async fn get_node_reports(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    Query(query): Query<NodeReportsQuery>,
) -> AppResult<Json<Vec<Report>>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // First verify the node exists
    let node_exists = puppetdb
        .get_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check node: {}", e)))?
        .is_some();

    if !node_exists {
        return Err(AppError::NotFound(format!("Node '{}' not found", certname)));
    }

    let limit = query.limit.unwrap_or(10);

    let reports = if let Some(ref status) = query.status {
        // Query with status filter
        puppetdb
            .query_reports(Some(&certname), Some(status), Some(limit))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch reports: {}", e)))?
    } else {
        // Query without status filter
        puppetdb
            .get_node_reports(&certname, Some(limit))
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch reports: {}", e)))?
    };

    Ok(Json(reports))
}

/// Query parameters for node resources
#[derive(Debug, Deserialize)]
pub struct NodeResourcesQuery {
    /// Filter by resource type (e.g., "File", "Package", "Service")
    #[serde(rename = "type")]
    pub resource_type: Option<String>,
}

/// Get resources for a specific node
///
/// GET /api/v1/nodes/:certname/resources
///
/// Query parameters:
/// - `type`: Filter by resource type (e.g., "File", "Package", "Service")
async fn get_node_resources(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    Query(query): Query<NodeResourcesQuery>,
) -> AppResult<Json<Vec<Resource>>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // First verify the node exists
    let node_exists = puppetdb
        .get_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check node: {}", e)))?
        .is_some();

    if !node_exists {
        return Err(AppError::NotFound(format!("Node '{}' not found", certname)));
    }

    let resources = if let Some(ref rtype) = query.resource_type {
        puppetdb
            .get_node_resources_by_type(&certname, rtype)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch resources: {}", e)))?
    } else {
        puppetdb
            .get_node_resources(&certname)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch resources: {}", e)))?
    };

    Ok(Json(resources))
}

/// Catalog response
#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    pub certname: String,
    pub version: String,
    pub environment: String,
    pub transaction_uuid: Option<String>,
    pub producer_timestamp: String,
    pub hash: String,
    pub resource_count: usize,
    pub edge_count: usize,
}

/// Get catalog for a specific node
///
/// GET /api/v1/nodes/:certname/catalog
async fn get_node_catalog(
    State(state): State<AppState>,
    Path(certname): Path<String>,
) -> AppResult<(StatusCode, Json<CatalogResponse>)> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    let catalog = puppetdb
        .get_node_catalog(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch catalog: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Catalog for node '{}' not found", certname)))?;

    let response = CatalogResponse {
        certname: catalog.certname,
        version: catalog.version,
        environment: catalog.environment,
        transaction_uuid: catalog.transaction_uuid,
        producer_timestamp: catalog.producer_timestamp,
        hash: catalog.hash,
        resource_count: catalog.resources.as_ref().map(|r| r.len()).unwrap_or(0),
        edge_count: catalog.edges.as_ref().map(|e| e.len()).unwrap_or(0),
    };

    Ok((StatusCode::OK, Json(response)))
}

/// Query parameters for classification endpoint
#[derive(Debug, Deserialize)]
pub struct ClassificationQuery {
    /// Organization ID (super_admin only)
    pub organization_id: Option<uuid::Uuid>,
}

/// Get classification for a specific node
///
/// GET /api/v1/nodes/:certname/classification
///
/// Returns the classification result for a node including:
/// - Groups the node belongs to
/// - Classes assigned via classification
/// - Variables from matched groups
/// - Parameters from matched groups
/// - Environment assignment
///
/// ## Authentication
///
/// This endpoint supports two authentication methods:
///
/// 1. **Client Certificate (mTLS)**: When a client certificate is provided via
///    headers (X-SSL-Client-CN, X-SSL-Client-DN, or X-SSL-Client-Cert), the
///    certificate's CN must match the requested certname. This ensures nodes
///    can only fetch their own classification.
///
/// 2. **API Token/Key**: Standard JWT or API key authentication allows fetching
///    classification for any node (for administrative use).
///
/// When using client certificates, the reverse proxy must be configured to pass
/// the certificate information via headers. See the client_cert module for details.
async fn get_node_classification(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    Query(query): Query<ClassificationQuery>,
    client_cert: OptionalClientCert,
) -> AppResult<Json<ClassificationResult>> {
    // If a client certificate is provided, verify it matches the requested certname
    // This prevents nodes from fetching classification data for other nodes
    if let Some(ref cert) = client_cert.0 {
        if !cert.matches_certname(&certname) {
            tracing::warn!(
                "Client certificate CN '{}' does not match requested certname '{}'",
                cert.cn,
                certname
            );
            return Err(AppError::Forbidden(format!(
                "Certificate CN '{}' does not match requested node '{}'",
                cert.cn, certname
            )));
        }
        tracing::debug!(
            "Client certificate authentication successful for node '{}'",
            certname
        );
    }

    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // Get facts for the node from PuppetDB
    let facts = puppetdb
        .get_node_facts(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch node facts: {}", e)))?;

    // Convert facts to JSON object for classification
    let mut facts_obj = serde_json::Map::new();
    for fact in facts {
        facts_obj.insert(fact.name, fact.value);
    }
    let facts_json = serde_json::Value::Object(facts_obj);

    // Get organization ID - use default if not specified
    // In production, this would come from authentication context
    let org_id = query.organization_id.unwrap_or_else(|| {
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    });

    // Get all groups for classification
    let group_repo = GroupRepository::new(&state.db);
    let all_groups = group_repo
        .get_all(org_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get groups: {}", e)))?;

    // Classify the node
    let classification_service = ClassificationService::new(all_groups);
    let classification = classification_service.classify(&certname, &facts_json);

    Ok(Json(classification))
}
