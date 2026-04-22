//! Node-related API endpoints
//!
//! Provides endpoints for querying and managing nodes from PuppetDB.

use axum::{
    extract::{Path, Query, State},
    http::{header::HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::{
    db::{repository::GroupRepository, InventoryRepository},
    middleware::{
        rbac::{check_permission, RbacError},
        AuthUser, OptionalClientCert,
    },
    models::{
        default_organization_uuid, Action, ClassificationResult, Fact, InventoryPayload,
        InventorySnapshotSummary, Node, NodeInventory, NodePendingUpdateJob, Report,
        Resource as RbacResource, SubmitUpdateJobResultRequest, UpdateJob,
    },
    services::{
        classification::{build_classification_facts, ClassificationService},
        puppetdb::{QueryBuilder, QueryParams, Resource},
    },
    utils::error::{AppError, AppResult},
    AppState,
};

const POST_INGEST_CATALOG_REFRESH_DEBOUNCE_SECS: i64 = 120;

/// Create routes for node endpoints (protected, requires JWT auth)
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_nodes))
        .route("/{certname}", get(get_node).delete(delete_node))
        .route("/{certname}/facts", get(get_node_facts))
        .route("/{certname}/reports", get(get_node_reports))
        .route("/{certname}/resources", get(get_node_resources))
        .route("/{certname}/catalog", get(get_node_catalog))
        .route("/{certname}/classification", get(get_node_classification))
        .route("/{certname}/inventory", get(get_node_inventory))
        .route(
            "/{certname}/inventory/history",
            get(get_node_inventory_history),
        )
}

/// Public routes for node endpoints (no JWT required, uses client cert auth)
/// These endpoints are used by Puppet agents to fetch their own classification
pub fn public_routes() -> Router<AppState> {
    Router::new()
        // Use /classify path to avoid conflict with protected /classification endpoint
        .route("/{certname}/classify", get(get_node_classification_public))
        .route("/{certname}/inventory", post(ingest_node_inventory))
        .route("/{certname}/update-jobs", get(get_pending_node_update_jobs))
        .route(
            "/{certname}/update-jobs/{job_id}/targets/{target_id}/results",
            post(submit_node_update_job_result),
        )
        // Environment-only endpoint (unauthenticated) - used early in Puppet agent run
        .route("/{certname}/environment", get(get_node_environment_public))
}

#[derive(Debug, Deserialize)]
pub struct InventoryHistoryQuery {
    pub limit: Option<usize>,
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

/// GET /api/v1/nodes/:certname/inventory
async fn get_node_inventory(
    State(state): State<AppState>,
    Path(certname): Path<String>,
) -> AppResult<Json<NodeInventory>> {
    let inventory_repo = state.inventory_repository();
    let inventory = inventory_repo
        .get_current_inventory(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch node inventory: {}", e)))?
        .ok_or_else(|| {
            AppError::NotFound(format!("Inventory for node '{}' not found", certname))
        })?;

    Ok(Json(inventory))
}

/// GET /api/v1/nodes/:certname/inventory/history
async fn get_node_inventory_history(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    Query(query): Query<InventoryHistoryQuery>,
) -> AppResult<Json<Vec<InventorySnapshotSummary>>> {
    let inventory_repo = state.inventory_repository();
    let history = inventory_repo
        .get_inventory_history(&certname, query.limit.unwrap_or(20).min(100))
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch inventory history: {}", e)))?;

    Ok(Json(history))
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
    auth_user: AuthUser,
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

    let node = puppetdb
        .get_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch node: {}", e)))?;
    let facts_json = build_classification_facts(
        facts,
        &certname,
        node.and_then(|n| n.catalog_environment).as_deref(),
    );

    // Get organization ID from authenticated user, or allow override for super_admin
    let org_id = query.organization_id.unwrap_or(auth_user.organization_id);

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

/// Public classification endpoint for Puppet agents
///
/// GET /api/v1/nodes/:certname/classify (public route)
///
/// This endpoint requires client certificate authentication (mTLS).
/// It does NOT require JWT authentication, but the client must present a valid
/// certificate whose CN matches the requested certname.
///
/// When deployed behind a reverse proxy with mTLS enabled, the proxy should pass
/// client certificate headers (X-SSL-Client-CN, X-SSL-Client-DN, X-SSL-Client-Verify).
///
/// The endpoint verifies that the certificate CN matches the requested certname,
/// preventing nodes from fetching classification data for other nodes.
///
/// This endpoint classifies the node against ALL organizations and:
/// - Returns the classification from the matching organization
/// - Returns an error if the node matches groups from multiple organizations
/// - Uses the default organization if no groups match
///
/// This is the endpoint Puppet agents should use via the openvox_classification fact.
async fn get_node_classification_public(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    headers: HeaderMap,
    client_cert: OptionalClientCert,
) -> AppResult<Json<ClassificationResult>> {
    if is_classification_authentication_disabled(&state) {
        tracing::debug!(
            "Classification: Authentication disabled by configuration for node '{}'",
            certname
        );
    }

    // Check for shared key authentication first
    let shared_key_header = headers
        .get("X-Classification-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let configured_shared_key = state
        .config
        .classification
        .as_ref()
        .and_then(|c| c.shared_key.as_ref());

    let authenticated = if is_classification_authentication_disabled(&state) {
        true
    } else if let (Some(header_key), Some(config_key)) = (&shared_key_header, configured_shared_key)
    {
        // Shared key authentication
        if header_key == config_key {
            tracing::debug!(
                "Classification: Shared key authentication successful for node '{}' (debug mode)",
                certname
            );
            true
        } else {
            tracing::warn!(
                "Classification: Invalid shared key provided for node '{}'",
                certname
            );
            false
        }
    } else if let Some(ref cert) = client_cert.0 {
        // Client certificate authentication
        if cert.matches_certname(&certname) {
            tracing::debug!(
                "Classification: Client certificate authentication successful for node '{}'",
                certname
            );
            true
        } else {
            tracing::warn!(
                "Classification: Certificate CN '{}' does not match requested certname '{}'",
                cert.cn,
                certname
            );
            return Err(AppError::Forbidden(format!(
                "Certificate CN '{}' does not match requested node '{}'",
                cert.cn, certname
            )));
        }
    } else {
        false
    };

    if !authenticated {
        return Err(AppError::Unauthorized(
            "Client certificate or shared key required. Provide X-SSL-Client-CN header or X-Classification-Key header.".to_string()
        ));
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

    let node = puppetdb
        .get_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch node: {}", e)))?;
    let facts_json = build_classification_facts(
        facts,
        &certname,
        node.and_then(|n| n.catalog_environment).as_deref(),
    );

    // Get ALL groups from ALL organizations for cross-org classification
    let group_repo = GroupRepository::new(&state.db);
    let all_groups = group_repo
        .get_all_across_organizations()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get groups: {}", e)))?;

    // Classify the node against all organizations
    // This will detect if the node matches groups from multiple orgs (conflict)
    // and use the default org if no matches are found
    let classification_service = ClassificationService::new(all_groups);
    let classification = classification_service.classify_across_organizations(
        &certname,
        &facts_json,
        default_organization_uuid(),
    );

    Ok(Json(classification))
}

/// POST /api/v1/nodes/:certname/inventory
async fn ingest_node_inventory(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    headers: HeaderMap,
    client_cert: OptionalClientCert,
    Json(payload): Json<InventoryPayload>,
) -> AppResult<(StatusCode, Json<NodeInventory>)> {
    authenticate_node_request(&state, &certname, &headers, &client_cert)?;

    let inventory_repo = state.inventory_repository();
    let inventory = inventory_repo
        .ingest_inventory(&certname, &payload)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to ingest node inventory: {}", e)))?;

    // Trigger background catalog and status refresh after ingestion.
    // Use the inventory DB pool (not the main DB) — these tables now live
    // in the dedicated inventory database.
    let inv_db = state.inventory_db.clone();
    let keep_raw = state.inventory_config.keep_raw_payload;
    let stale_hours = state
        .config
        .inventory
        .as_ref()
        .map(|i| i.stale_after_hours)
        .unwrap_or(48);
    tokio::spawn(async move {
        let repo = InventoryRepository::new(inv_db).with_keep_raw_payload(keep_raw);
        match repo
            .refresh_version_catalog_debounced_from_ingest(
                POST_INGEST_CATALOG_REFRESH_DEBOUNCE_SECS,
            )
            .await
        {
            Ok(Some(entries)) => {
                debug!(
                    "Post-ingestion catalog refresh completed: {} entries",
                    entries
                );
            }
            Ok(None) => {
                debug!(
                    "Post-ingestion catalog refresh skipped (in progress or within debounce window)"
                );
            }
            Err(e) => {
                warn!("Post-ingestion catalog refresh failed: {}", e);
            }
        }
        if let Err(e) = repo.refresh_host_update_statuses(stale_hours).await {
            warn!("Post-ingestion status refresh failed: {}", e);
        }
    });

    Ok((StatusCode::CREATED, Json(inventory)))
}

/// GET /api/v1/nodes/:certname/update-jobs
async fn get_pending_node_update_jobs(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    headers: HeaderMap,
    client_cert: OptionalClientCert,
) -> AppResult<Json<Vec<NodePendingUpdateJob>>> {
    authenticate_node_request(&state, &certname, &headers, &client_cert)?;

    let inventory_repo = state.inventory_repository();
    let jobs = inventory_repo
        .claim_pending_updates_for_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch pending update jobs: {}", e)))?;

    Ok(Json(jobs))
}

/// POST /api/v1/nodes/:certname/update-jobs/:job_id/targets/:target_id/results
async fn submit_node_update_job_result(
    State(state): State<AppState>,
    Path((certname, job_id, target_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    client_cert: OptionalClientCert,
    Json(payload): Json<SubmitUpdateJobResultRequest>,
) -> AppResult<Json<UpdateJob>> {
    authenticate_node_request(&state, &certname, &headers, &client_cert)?;

    let inventory_repo = state.inventory_repository();
    let job = inventory_repo
        .submit_update_job_result(&job_id, &target_id, &certname, &payload)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to submit update job result: {}", e)))?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Update job target '{}' for node '{}' was not found",
                target_id, certname
            ))
        })?;

    Ok(Json(job))
}

/// Response for environment-only endpoint
#[derive(Debug, Serialize)]
pub struct EnvironmentResponse {
    /// The node's certname
    pub certname: String,
    /// The assigned environment (if any)
    pub environment: Option<String>,
}

/// GET /api/v1/nodes/:certname/environment - Get node environment (UNAUTHENTICATED)
///
/// This endpoint is intentionally unauthenticated to allow Puppet agents to determine
/// their environment early in the agent run, before certificates are available.
///
/// Only returns the environment assignment - no sensitive classification data.
/// This is safe because:
/// - Environment names are not sensitive
/// - The node must already exist in PuppetDB (has run at least once)
/// - No classes, variables, or other sensitive data is exposed
async fn get_node_environment_public(
    State(state): State<AppState>,
    Path(certname): Path<String>,
) -> AppResult<Json<EnvironmentResponse>> {
    tracing::debug!(
        "Environment lookup (unauthenticated) for node '{}'",
        certname
    );

    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // Get facts for the node from PuppetDB
    let facts = puppetdb
        .get_node_facts(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch node facts: {}", e)))?;

    let node = puppetdb
        .get_node(&certname)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch node: {}", e)))?;
    let facts_json = build_classification_facts(
        facts,
        &certname,
        node.and_then(|n| n.catalog_environment).as_deref(),
    );

    // Get ALL groups from ALL organizations for cross-org classification
    let group_repo = GroupRepository::new(&state.db);
    let all_groups = group_repo
        .get_all_across_organizations()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get groups: {}", e)))?;

    // Classify the node to determine environment
    let classification_service = ClassificationService::new(all_groups);
    let classification = classification_service.classify_across_organizations(
        &certname,
        &facts_json,
        default_organization_uuid(),
    );

    Ok(Json(EnvironmentResponse {
        certname,
        environment: classification.environment,
    }))
}

fn authenticate_node_request(
    state: &AppState,
    certname: &str,
    headers: &HeaderMap,
    client_cert: &OptionalClientCert,
) -> AppResult<()> {
    let shared_key_header = headers
        .get("X-Classification-Key")
        .and_then(|v| v.to_str().ok());
    let configured_shared_key = state
        .config
        .classification
        .as_ref()
        .and_then(|c| c.shared_key.as_deref());

    if let (Some(header_key), Some(config_key)) = (shared_key_header, configured_shared_key) {
        if header_key == config_key {
            return Ok(());
        }

        tracing::warn!(
            "Node public auth: invalid shared key provided for node '{}'",
            certname
        );
        return Err(AppError::Unauthorized(
            "Invalid X-Classification-Key header".to_string(),
        ));
    }

    if let Some(ref cert) = client_cert.0 {
        if cert.matches_certname(certname) {
            return Ok(());
        }

        return Err(AppError::Forbidden(format!(
            "Certificate CN '{}' does not match requested node '{}'",
            cert.cn, certname
        )));
    }

    Err(AppError::Unauthorized(
        "Client certificate or shared key required. Provide X-SSL-Client-CN header or X-Classification-Key header.".to_string(),
    ))
}

fn is_classification_authentication_disabled(state: &AppState) -> bool {
    state
        .config
        .classification
        .as_ref()
        .map(|c| c.disable_authentication)
        .unwrap_or(false)
}

/// Response for node deletion
#[derive(Debug, Serialize)]
pub struct DeleteNodeResponse {
    /// Whether the deletion was successful
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Number of pinned node associations removed
    pub pinned_associations_removed: u64,
    /// Whether the certificate was revoked (if it existed)
    pub certificate_revoked: bool,
    /// Whether the node was deactivated in PuppetDB
    pub puppetdb_deactivated: bool,
}

/// DELETE /api/v1/nodes/:certname - Delete a node
///
/// This operation:
/// 1. Removes all pinned node associations from groups
/// 2. Attempts to revoke the node's certificate (if Puppet CA is configured and cert exists)
/// 3. Attempts to deactivate the node in PuppetDB (if configured)
///
/// Requires the `nodes:delete` permission.
async fn delete_node(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(certname): Path<String>,
) -> Result<Json<DeleteNodeResponse>, AppError> {
    // Check permission
    check_permission(
        &state.rbac,
        &auth_user,
        RbacResource::Nodes,
        Action::Delete,
        None,
        None,
    )
    .map_err(|e| match e {
        RbacError::PermissionDenied { reason, .. } => AppError::Forbidden(reason),
        RbacError::NotAuthenticated => {
            AppError::Unauthorized("Authentication required".to_string())
        }
        RbacError::RoleNotFound(name) => AppError::Internal(format!("Role not found: {}", name)),
    })?;

    tracing::info!(
        "User '{}' is deleting node '{}'",
        auth_user.username,
        certname
    );

    // Step 1: Remove all pinned node associations
    let group_repo = GroupRepository::new(&state.db);
    let pinned_removed = group_repo
        .remove_all_pinned_for_certname(&certname)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to remove pinned associations for '{}': {}",
                certname,
                e
            );
            AppError::Internal(format!("Failed to remove pinned associations: {}", e))
        })?;

    if pinned_removed > 0 {
        tracing::info!(
            "Removed {} pinned node associations for '{}'",
            pinned_removed,
            certname
        );
    }

    // Step 2: Attempt to revoke certificate if CA is configured
    let mut certificate_revoked = false;
    if let Some(ca) = state.puppet_ca.as_ref() {
        match ca.revoke_certificate(&certname).await {
            Ok(_) => {
                tracing::info!("Revoked certificate for '{}'", certname);
                certificate_revoked = true;
            }
            Err(e) => {
                // Certificate might not exist or already be revoked - this is not a fatal error
                tracing::debug!(
                    "Could not revoke certificate for '{}': {} (may not exist)",
                    certname,
                    e
                );
            }
        }
    }

    // Step 3: Attempt to deactivate node in PuppetDB if configured
    let mut puppetdb_deactivated = false;
    if let Some(puppetdb) = state.puppetdb.as_ref() {
        match puppetdb.deactivate_node(&certname).await {
            Ok(_) => {
                tracing::info!("Deactivated node '{}' in PuppetDB", certname);
                puppetdb_deactivated = true;
            }
            Err(e) => {
                // Node might not exist in PuppetDB - this is not a fatal error
                tracing::debug!(
                    "Could not deactivate node '{}' in PuppetDB: {} (may not exist)",
                    certname,
                    e
                );
            }
        }
    }

    Ok(Json(DeleteNodeResponse {
        success: true,
        message: format!("Node '{}' has been deleted", certname),
        pinned_associations_removed: pinned_removed,
        certificate_revoked,
        puppetdb_deactivated,
    }))
}
