//! PuppetDB client service
//!
//! Provides a comprehensive client for interacting with the PuppetDB API v4.
//! Supports SSL/TLS with client certificates, PQL queries, and all major endpoints.

use anyhow::{Context, Result};
use reqwest::{Certificate, Client, Identity};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::error::Error as StdError;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::config::PuppetDbConfig;
use crate::models::{Fact, Node, Report, ResourceEvent};

/// Check if an SSL file exists and is readable, logging the result
fn check_ssl_file_access(path: &Path, file_type: &str) -> Result<usize, String> {
    if !path.exists() {
        let msg = format!(
            "PuppetDB SSL ERROR: {} file does not exist: {}",
            file_type,
            path.display()
        );
        error!("{}", msg);
        return Err(msg);
    }

    // Try to read the file to check permissions
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_file() {
                // Log file permissions on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    let mode = metadata.mode();
                    let uid = metadata.uid();
                    let gid = metadata.gid();
                    debug!(
                        "{} file permissions: mode={:o}, uid={}, gid={}, path={}",
                        file_type,
                        mode & 0o777,
                        uid,
                        gid,
                        path.display()
                    );
                }

                // Try to actually read the file to verify read permission
                match fs::read(path) {
                    Ok(contents) => {
                        info!(
                            "PuppetDB SSL: {} loaded successfully ({} bytes): {}",
                            file_type,
                            contents.len(),
                            path.display()
                        );
                        Ok(contents.len())
                    }
                    Err(e) => {
                        let msg = format!(
                            "PuppetDB SSL ERROR: {} file exists but cannot be read (permission denied?): {} - {}",
                            file_type,
                            path.display(),
                            e
                        );
                        error!("{}", msg);
                        warn!(
                            "Check that the openvox-webui service user has read access to: {}",
                            path.display()
                        );
                        Err(msg)
                    }
                }
            } else {
                let msg = format!(
                    "PuppetDB SSL ERROR: {} path is not a file: {}",
                    file_type,
                    path.display()
                );
                error!("{}", msg);
                Err(msg)
            }
        }
        Err(e) => {
            let msg = format!(
                "PuppetDB SSL ERROR: Cannot access {} file metadata: {} - {}",
                file_type,
                path.display(),
                e
            );
            error!("{}", msg);
            Err(msg)
        }
    }
}

/// PuppetDB API client
#[derive(Clone)]
pub struct PuppetDbClient {
    client: Client,
    base_url: String,
}

/// Query parameters for paginated requests
#[derive(Debug, Clone, Default, Serialize)]
pub struct QueryParams {
    /// Maximum number of results to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Number of results to skip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    /// Field to order by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,
    /// Include total count in response headers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_total: Option<bool>,
}

impl QueryParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn order_by(mut self, field: &str, ascending: bool) -> Self {
        let direction = if ascending { "asc" } else { "desc" };
        self.order_by = Some(format!(
            "[{{\"field\":\"{}\",\"order\":\"{}\"}}]",
            field, direction
        ));
        self
    }

    pub fn include_total(mut self) -> Self {
        self.include_total = Some(true);
        self
    }

    fn to_query_string(&self) -> String {
        let mut params = vec![];
        if let Some(limit) = self.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = self.offset {
            params.push(format!("offset={}", offset));
        }
        if let Some(ref order_by) = self.order_by {
            params.push(format!("order_by={}", order_by));
        }
        if self.include_total == Some(true) {
            params.push("include_total=true".to_string());
        }
        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        }
    }

    fn append_to_query_string(&self, existing: &str) -> String {
        let mut params = vec![];
        if let Some(limit) = self.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = self.offset {
            params.push(format!("offset={}", offset));
        }
        if let Some(ref order_by) = self.order_by {
            params.push(format!("order_by={}", order_by));
        }
        if self.include_total == Some(true) {
            params.push("include_total=true".to_string());
        }
        if params.is_empty() {
            existing.to_string()
        } else if existing.is_empty() {
            format!("?{}", params.join("&"))
        } else {
            format!("{}&{}", existing, params.join("&"))
        }
    }
}

/// Paginated response with total count
#[derive(Debug, Clone)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: Option<u64>,
}

/// Resource from PuppetDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub certname: String,
    pub resource: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    pub title: String,
    pub tags: Vec<String>,
    pub exported: bool,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub environment: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

/// Catalog from PuppetDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub certname: String,
    pub version: String,
    pub environment: String,
    pub transaction_uuid: Option<String>,
    pub catalog_uuid: Option<String>,
    pub code_id: Option<String>,
    pub producer_timestamp: String,
    pub hash: String,
    pub producer: Option<String>,
    pub resources: Option<Vec<CatalogResource>>,
    pub edges: Option<Vec<CatalogEdge>>,
}

/// Resource within a catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogResource {
    #[serde(rename = "type")]
    pub resource_type: String,
    pub title: String,
    pub exported: bool,
    pub tags: Vec<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub parameters: Option<serde_json::Value>,
}

/// Edge in a catalog (dependency relationship)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEdge {
    pub source: ResourceRef,
    pub target: ResourceRef,
    pub relationship: String,
}

/// Reference to a resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRef {
    #[serde(rename = "type")]
    pub resource_type: String,
    pub title: String,
}

/// Event from PuppetDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub certname: String,
    pub report: String,
    pub configuration_version: Option<String>,
    pub run_start_time: Option<String>,
    pub run_end_time: Option<String>,
    pub report_receive_time: Option<String>,
    pub status: String,
    pub timestamp: String,
    pub resource_type: String,
    pub resource_title: String,
    pub property: Option<String>,
    pub new_value: Option<String>,
    pub old_value: Option<String>,
    pub message: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub containment_path: Option<Vec<String>>,
    pub containing_class: Option<String>,
    pub environment: Option<String>,
}

/// Builder for AST queries
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    conditions: Vec<String>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self { conditions: vec![] }
    }

    /// Add an equality condition: ["=", field, value]
    pub fn equals(mut self, field: &str, value: &str) -> Self {
        self.conditions
            .push(format!("[\"=\",\"{}\",\"{}\"]", field, value));
        self
    }

    /// Add a regex match condition: ["~", field, pattern]
    pub fn matches(mut self, field: &str, pattern: &str) -> Self {
        self.conditions
            .push(format!("[\"~\",\"{}\",\"{}\"]", field, pattern));
        self
    }

    /// Add a greater-than condition: [">", field, value]
    pub fn greater_than(mut self, field: &str, value: &str) -> Self {
        self.conditions
            .push(format!("[\">\",\"{}\",\"{}\"]", field, value));
        self
    }

    /// Add a less-than condition: ["<", field, value]
    pub fn less_than(mut self, field: &str, value: &str) -> Self {
        self.conditions
            .push(format!("[\"<\",\"{}\",\"{}\"]", field, value));
        self
    }

    /// Add a greater-than-or-equal condition: [">=", field, value]
    pub fn gte(mut self, field: &str, value: &str) -> Self {
        self.conditions
            .push(format!("[\">=\",\"{}\",\"{}\"]", field, value));
        self
    }

    /// Add a less-than-or-equal condition: ["<=", field, value]
    pub fn lte(mut self, field: &str, value: &str) -> Self {
        self.conditions
            .push(format!("[\"<=\",\"{}\",\"{}\"]", field, value));
        self
    }

    /// Add a null check: ["null?", field, true/false]
    pub fn is_null(mut self, field: &str, is_null: bool) -> Self {
        self.conditions
            .push(format!("[\"null?\",\"{}\",{}]", field, is_null));
        self
    }

    /// Add an IN condition: ["in", field, ["array", values...]]
    pub fn in_array(mut self, field: &str, values: &[&str]) -> Self {
        let values_str = values
            .iter()
            .map(|v| format!("\"{}\"", v))
            .collect::<Vec<_>>()
            .join(",");
        self.conditions.push(format!(
            "[\"in\",\"{}\", [\"array\",{}]]",
            field, values_str
        ));
        self
    }

    /// Negate the entire query: ["not", query]
    pub fn not(mut self, subquery: QueryBuilder) -> Self {
        if let Some(q) = subquery.build_inner() {
            self.conditions.push(format!("[\"not\",{}]", q));
        }
        self
    }

    /// Add a raw condition string
    pub fn raw(mut self, condition: &str) -> Self {
        self.conditions.push(condition.to_string());
        self
    }

    fn build_inner(&self) -> Option<String> {
        match self.conditions.len() {
            0 => None,
            1 => Some(self.conditions[0].clone()),
            _ => Some(format!("[\"and\",{}]", self.conditions.join(","))),
        }
    }

    /// Build the query string
    pub fn build(&self) -> Option<String> {
        self.build_inner().map(|q| format!("query={}", q))
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PuppetDbClient {
    /// Create a new PuppetDB client with optional SSL/TLS configuration
    pub fn new(config: &PuppetDbConfig) -> Result<Self> {
        info!("Initializing PuppetDB client for {}", config.url);

        let mut builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .use_rustls_tls();

        // Check and load CA certificate if provided (must be done before identity for rustls)
        if let Some(ref ca_path) = config.ssl_ca {
            let ca_path_ref = Path::new(ca_path);

            // Check file access - this logs success/failure
            if let Err(e) = check_ssl_file_access(ca_path_ref, "CA certificate") {
                return Err(anyhow::anyhow!("{}", e));
            }

            let ca_cert = fs::read(ca_path)
                .with_context(|| format!("Failed to read CA certificate: {:?}", ca_path))?;

            // Parse all certificates from the CA file (may contain a chain)
            let certs = Certificate::from_pem_bundle(&ca_cert)
                .context("Failed to parse CA certificate(s) as PEM")?;

            info!(
                "PuppetDB SSL: Parsed {} certificate(s) from CA bundle",
                certs.len()
            );

            // Disable built-in root certs when using custom CA
            // This ensures we only trust our Puppet CA, not system CAs
            builder = builder.tls_built_in_root_certs(false);

            for cert in certs {
                builder = builder.add_root_certificate(cert);
            }
        }

        // Check and load client certificate and key if provided
        if let (Some(ref cert_path), Some(ref key_path)) = (&config.ssl_cert, &config.ssl_key) {
            let cert_path_ref = Path::new(cert_path);
            let key_path_ref = Path::new(key_path);

            // Check file access for both files - these log success/failure
            if let Err(e) = check_ssl_file_access(cert_path_ref, "Client certificate") {
                return Err(anyhow::anyhow!("{}", e));
            }

            if let Err(e) = check_ssl_file_access(key_path_ref, "Client private key") {
                return Err(anyhow::anyhow!("{}", e));
            }

            let cert = fs::read(cert_path)
                .with_context(|| format!("Failed to read client certificate: {:?}", cert_path))?;
            let key = fs::read(key_path)
                .with_context(|| format!("Failed to read client key: {:?}", key_path))?;

            // Combine cert and key into a single PEM bundle for rustls
            let mut pem_bundle = cert.clone();
            pem_bundle.push(b'\n');
            pem_bundle.extend_from_slice(&key);

            let identity = Identity::from_pem(&pem_bundle)
                .context("Failed to create identity from certificate and key")?;
            builder = builder.identity(identity);
        } else if config.ssl_cert.is_some() || config.ssl_key.is_some() {
            warn!(
                "Partial SSL configuration: cert={:?}, key={:?}. Both must be provided for client authentication.",
                config.ssl_cert.is_some(),
                config.ssl_key.is_some()
            );
        }

        // Configure SSL verification (must be after identity for rustls compatibility)
        if !config.ssl_verify {
            warn!("SSL certificate verification is DISABLED - this is insecure!");
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build().context("Failed to create HTTP client")?;

        info!(
            "PuppetDB client initialized successfully for {}",
            config.url
        );

        Ok(Self {
            client,
            base_url: config.url.trim_end_matches('/').to_string(),
        })
    }

    /// Execute a raw PQL query
    pub async fn query<T: DeserializeOwned>(&self, query: &str) -> Result<Vec<T>> {
        let url = format!("{}/pdb/query/v4", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&PqlQuery {
                query: query.to_string(),
            })
            .send()
            .await
            .context("Failed to send PQL query")?;

        self.handle_response(response).await
    }

    /// Execute a PQL query with pagination
    pub async fn query_paginated<T: DeserializeOwned>(
        &self,
        query: &str,
        params: QueryParams,
    ) -> Result<PaginatedResponse<T>> {
        let url = format!("{}/pdb/query/v4{}", self.base_url, params.to_query_string());

        let response = self
            .client
            .post(&url)
            .json(&PqlQuery {
                query: query.to_string(),
            })
            .send()
            .await
            .context("Failed to send PQL query")?;

        let total = response
            .headers()
            .get("X-Records")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        let data = self.handle_response(response).await?;
        Ok(PaginatedResponse { data, total })
    }

    // ==================== Node Endpoints ====================

    /// Get all nodes
    pub async fn get_nodes(&self) -> Result<Vec<Node>> {
        self.get_nodes_with_params(QueryParams::default()).await
    }

    /// Get nodes with query parameters
    pub async fn get_nodes_with_params(&self, params: QueryParams) -> Result<Vec<Node>> {
        let url = format!(
            "{}/pdb/query/v4/nodes{}",
            self.base_url,
            params.to_query_string()
        );
        self.get(&url).await
    }

    /// Get nodes matching a query
    pub async fn query_nodes(&self, query: &QueryBuilder) -> Result<Vec<Node>> {
        self.query_nodes_with_params(query, QueryParams::default())
            .await
    }

    /// Get nodes matching a query with pagination
    pub async fn query_nodes_with_params(
        &self,
        query: &QueryBuilder,
        params: QueryParams,
    ) -> Result<Vec<Node>> {
        let mut url = format!("{}/pdb/query/v4/nodes", self.base_url);
        if let Some(q) = query.build() {
            url = format!("{}?{}", url, params.append_to_query_string(&q));
        } else {
            url = format!("{}{}", url, params.to_query_string());
        }
        self.get(&url).await
    }

    /// Get a specific node by certname
    pub async fn get_node(&self, certname: &str) -> Result<Option<Node>> {
        let url = format!(
            "{}/pdb/query/v4/nodes/{}",
            self.base_url,
            urlencoding::encode(certname)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch node")?;

        if response.status().is_success() {
            let node = response
                .json::<Node>()
                .await
                .context("Failed to parse node response")?;
            Ok(Some(node))
        } else if response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to fetch node: {} - {}", status, body);
        }
    }

    // ==================== Fact Endpoints ====================

    /// Get facts for a specific node
    pub async fn get_node_facts(&self, certname: &str) -> Result<Vec<Fact>> {
        // Use the facts endpoint with a certname query filter
        // The /nodes/{certname}/facts endpoint is not supported by all PuppetDB versions
        let query = QueryBuilder::new().equals("certname", certname);
        self.query_facts_advanced(&query, QueryParams::default())
            .await
    }

    /// Get a specific fact for a node
    pub async fn get_node_fact(&self, certname: &str, fact_name: &str) -> Result<Option<Fact>> {
        // Use the facts endpoint with certname and name query filters
        // The /nodes/{certname}/facts/{name} endpoint is not supported by all PuppetDB versions
        let query = QueryBuilder::new()
            .equals("certname", certname)
            .equals("name", fact_name);
        let facts: Vec<Fact> = self
            .query_facts_advanced(&query, QueryParams::default())
            .await?;
        Ok(facts.into_iter().next())
    }

    /// Query facts across all nodes
    pub async fn query_facts(&self, fact_name: Option<&str>) -> Result<Vec<Fact>> {
        self.query_facts_with_params(fact_name, QueryParams::default())
            .await
    }

    /// Query facts with pagination
    pub async fn query_facts_with_params(
        &self,
        fact_name: Option<&str>,
        params: QueryParams,
    ) -> Result<Vec<Fact>> {
        let mut url = format!("{}/pdb/query/v4/facts", self.base_url);

        if let Some(name) = fact_name {
            let query = format!("query=[\"=\",\"name\",\"{}\"]", name);
            url = format!("{}?{}", url, params.append_to_query_string(&query));
        } else {
            url = format!("{}{}", url, params.to_query_string());
        }

        self.get(&url).await
    }

    /// Query facts with a custom query builder
    pub async fn query_facts_advanced(
        &self,
        query: &QueryBuilder,
        params: QueryParams,
    ) -> Result<Vec<Fact>> {
        let mut url = format!("{}/pdb/query/v4/facts", self.base_url);
        if let Some(q) = query.build() {
            url = format!("{}?{}", url, params.append_to_query_string(&q));
        } else {
            url = format!("{}{}", url, params.to_query_string());
        }
        self.get(&url).await
    }

    /// Get all unique fact names
    pub async fn get_fact_names(&self) -> Result<Vec<String>> {
        let url = format!("{}/pdb/query/v4/fact-names", self.base_url);
        self.get(&url).await
    }

    /// Get all unique fact paths (for structured facts)
    pub async fn get_fact_paths(&self) -> Result<Vec<FactPath>> {
        let url = format!("{}/pdb/query/v4/fact-paths", self.base_url);
        self.get(&url).await
    }

    /// Get fact contents (for structured facts)
    pub async fn get_fact_contents(&self, fact_path: Option<&str>) -> Result<Vec<FactContent>> {
        let mut url = format!("{}/pdb/query/v4/fact-contents", self.base_url);

        if let Some(path) = fact_path {
            url = format!("{}?query=[\"=\",\"path\",[{}]]", url, path);
        }

        self.get(&url).await
    }

    // ==================== Report Endpoints ====================

    /// Get reports for a specific node
    pub async fn get_node_reports(
        &self,
        certname: &str,
        limit: Option<u32>,
    ) -> Result<Vec<Report>> {
        // Use the reports endpoint with a certname query filter
        // The /nodes/{certname}/reports endpoint is not supported by all PuppetDB versions
        self.query_reports(Some(certname), None, limit).await
    }

    /// Query reports
    pub async fn query_reports(
        &self,
        certname: Option<&str>,
        status: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<Report>> {
        let mut query = QueryBuilder::new();

        if let Some(cn) = certname {
            query = query.equals("certname", cn);
        }
        if let Some(st) = status {
            query = query.equals("status", st);
        }

        let params = if let Some(l) = limit {
            QueryParams::new().limit(l)
        } else {
            QueryParams::default()
        };

        self.query_reports_advanced(&query, params).await
    }

    /// Query reports with custom query and pagination
    pub async fn query_reports_advanced(
        &self,
        query: &QueryBuilder,
        params: QueryParams,
    ) -> Result<Vec<Report>> {
        let mut url = format!("{}/pdb/query/v4/reports", self.base_url);
        if let Some(q) = query.build() {
            url = format!("{}?{}", url, params.append_to_query_string(&q));
        } else {
            url = format!("{}{}", url, params.to_query_string());
        }
        self.get(&url).await
    }

    /// Get a specific report by hash
    pub async fn get_report(&self, hash: &str) -> Result<Option<Report>> {
        let url = format!(
            "{}/pdb/query/v4/reports/{}",
            self.base_url,
            urlencoding::encode(hash)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch report")?;

        if response.status().is_success() {
            let report = response
                .json::<Report>()
                .await
                .context("Failed to parse report response")?;
            Ok(Some(report))
        } else if response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to fetch report: {} - {}", status, body);
        }
    }

    // ==================== Resource Endpoints ====================

    /// Get resources for a specific node
    pub async fn get_node_resources(&self, certname: &str) -> Result<Vec<Resource>> {
        let url = format!(
            "{}/pdb/query/v4/nodes/{}/resources",
            self.base_url,
            urlencoding::encode(certname)
        );
        self.get(&url).await
    }

    /// Get resources of a specific type for a node
    pub async fn get_node_resources_by_type(
        &self,
        certname: &str,
        resource_type: &str,
    ) -> Result<Vec<Resource>> {
        let url = format!(
            "{}/pdb/query/v4/nodes/{}/resources/{}",
            self.base_url,
            urlencoding::encode(certname),
            urlencoding::encode(resource_type)
        );
        self.get(&url).await
    }

    /// Get a specific resource by type and title for a node
    pub async fn get_node_resource(
        &self,
        certname: &str,
        resource_type: &str,
        title: &str,
    ) -> Result<Option<Resource>> {
        let url = format!(
            "{}/pdb/query/v4/nodes/{}/resources/{}/{}",
            self.base_url,
            urlencoding::encode(certname),
            urlencoding::encode(resource_type),
            urlencoding::encode(title)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch resource")?;

        if response.status().is_success() {
            let resource = response
                .json::<Resource>()
                .await
                .context("Failed to parse resource response")?;
            Ok(Some(resource))
        } else if response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to fetch resource: {} - {}", status, body);
        }
    }

    /// Query resources across all nodes
    pub async fn query_resources(&self, query: &QueryBuilder) -> Result<Vec<Resource>> {
        self.query_resources_with_params(query, QueryParams::default())
            .await
    }

    /// Query resources with pagination
    pub async fn query_resources_with_params(
        &self,
        query: &QueryBuilder,
        params: QueryParams,
    ) -> Result<Vec<Resource>> {
        let mut url = format!("{}/pdb/query/v4/resources", self.base_url);
        if let Some(q) = query.build() {
            url = format!("{}?{}", url, params.append_to_query_string(&q));
        } else {
            url = format!("{}{}", url, params.to_query_string());
        }
        self.get(&url).await
    }

    /// Get all resources of a specific type
    pub async fn get_resources_by_type(&self, resource_type: &str) -> Result<Vec<Resource>> {
        let url = format!(
            "{}/pdb/query/v4/resources/{}",
            self.base_url,
            urlencoding::encode(resource_type)
        );
        self.get(&url).await
    }

    // ==================== Event Endpoints ====================

    /// Get events for a report
    pub async fn get_report_events(&self, report_hash: &str) -> Result<Vec<ResourceEvent>> {
        let query = QueryBuilder::new().equals("report", report_hash);
        self.query_events(&query).await
    }

    /// Query events
    pub async fn query_events(&self, query: &QueryBuilder) -> Result<Vec<ResourceEvent>> {
        self.query_events_with_params(query, QueryParams::default())
            .await
    }

    /// Query events with pagination
    pub async fn query_events_with_params(
        &self,
        query: &QueryBuilder,
        params: QueryParams,
    ) -> Result<Vec<ResourceEvent>> {
        let mut url = format!("{}/pdb/query/v4/events", self.base_url);
        if let Some(q) = query.build() {
            url = format!("{}?{}", url, params.append_to_query_string(&q));
        } else {
            url = format!("{}{}", url, params.to_query_string());
        }
        self.get(&url).await
    }

    /// Get events for a specific node
    pub async fn get_node_events(
        &self,
        certname: &str,
        params: QueryParams,
    ) -> Result<Vec<ResourceEvent>> {
        let query = QueryBuilder::new().equals("certname", certname);
        self.query_events_with_params(&query, params).await
    }

    // ==================== Catalog Endpoints ====================

    /// Get the catalog for a specific node
    pub async fn get_node_catalog(&self, certname: &str) -> Result<Option<Catalog>> {
        let url = format!(
            "{}/pdb/query/v4/catalogs/{}",
            self.base_url,
            urlencoding::encode(certname)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch catalog")?;

        if response.status().is_success() {
            let catalog = response
                .json::<Catalog>()
                .await
                .context("Failed to parse catalog response")?;
            Ok(Some(catalog))
        } else if response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to fetch catalog: {} - {}", status, body);
        }
    }

    /// Query catalogs
    pub async fn query_catalogs(&self, query: &QueryBuilder) -> Result<Vec<Catalog>> {
        self.query_catalogs_with_params(query, QueryParams::default())
            .await
    }

    /// Query catalogs with pagination
    pub async fn query_catalogs_with_params(
        &self,
        query: &QueryBuilder,
        params: QueryParams,
    ) -> Result<Vec<Catalog>> {
        let mut url = format!("{}/pdb/query/v4/catalogs", self.base_url);
        if let Some(q) = query.build() {
            url = format!("{}?{}", url, params.append_to_query_string(&q));
        } else {
            url = format!("{}{}", url, params.to_query_string());
        }
        self.get(&url).await
    }

    // ==================== Environment Endpoints ====================

    /// Get all environments
    pub async fn get_environments(&self) -> Result<Vec<Environment>> {
        let url = format!("{}/pdb/query/v4/environments", self.base_url);
        self.get(&url).await
    }

    /// Get a specific environment
    pub async fn get_environment(&self, name: &str) -> Result<Option<Environment>> {
        let url = format!(
            "{}/pdb/query/v4/environments/{}",
            self.base_url,
            urlencoding::encode(name)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch environment")?;

        if response.status().is_success() {
            let env = response
                .json::<Environment>()
                .await
                .context("Failed to parse environment response")?;
            Ok(Some(env))
        } else if response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to fetch environment: {} - {}", status, body);
        }
    }

    // ==================== Server Status Endpoints ====================

    /// Get PuppetDB server version
    pub async fn get_version(&self) -> Result<ServerVersion> {
        let url = format!("{}/pdb/meta/v1/version", self.base_url);
        self.get(&url).await
    }

    /// Get PuppetDB server status
    pub async fn get_status(&self) -> Result<serde_json::Value> {
        let url = format!("{}/status/v1/services", self.base_url);
        self.get(&url).await
    }

    // ==================== Helper Methods ====================

    /// Internal GET request handler
    async fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        debug!("PuppetDB: Sending GET request to {}", url);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                // Log detailed error information
                error!(
                    "PuppetDB ERROR: HTTP request failed to {}: {}",
                    url, e
                );
                error!(
                    "PuppetDB ERROR: Error flags - is_connect: {}, is_timeout: {}, is_request: {}",
                    e.is_connect(),
                    e.is_timeout(),
                    e.is_request()
                );

                // Provide helpful messages based on error type
                if e.is_connect() {
                    error!("PuppetDB ERROR: Connection failed. Check:");
                    error!("  - PuppetDB URL is correct and reachable");
                    error!("  - Network/firewall allows connection to PuppetDB port");
                    error!("  - SSL certificates are valid and trusted");
                }
                if e.is_timeout() {
                    error!("PuppetDB ERROR: Request timed out. Check:");
                    error!("  - PuppetDB server is responsive");
                    error!("  - Network latency is acceptable");
                    error!("  - Consider increasing puppetdb.timeout setting");
                }

                // Walk through error chain for root cause
                if let Some(source) = e.source() {
                    error!("PuppetDB ERROR: Underlying cause: {}", source);
                    let mut current: &dyn StdError = source;
                    while let Some(next) = current.source() {
                        error!("PuppetDB ERROR: Caused by: {}", next);
                        current = next;
                    }

                    // Check for common SSL errors
                    let error_str = format!("{}", source);
                    if error_str.contains("UnknownIssuer") {
                        error!("PuppetDB SSL ERROR: Server certificate not trusted!");
                        error!("  - Verify puppetdb.ssl.ca_path points to correct CA certificate");
                        error!("  - Ensure CA certificate matches the one that signed PuppetDB's cert");
                    } else if error_str.contains("certificate") || error_str.contains("Certificate") {
                        error!("PuppetDB SSL ERROR: Certificate validation failed");
                        error!("  - Check SSL certificate paths in configuration");
                        error!("  - Verify certificates are in PEM format");
                        error!("  - Ensure certificates are not expired");
                    }
                }
                anyhow::anyhow!("Failed to send request to {}: {}", url, e)
            })?;

        self.handle_response(response).await
    }

    /// Handle HTTP response and parse JSON
    async fn handle_response<T: DeserializeOwned>(&self, response: reqwest::Response) -> Result<T> {
        let status = response.status();

        if status.is_success() {
            let body = response
                .text()
                .await
                .context("Failed to read response body")?;
            serde_json::from_str::<T>(&body).with_context(|| {
                // Truncate body for logging if too long
                let truncated = if body.len() > 500 {
                    format!("{}... (truncated)", &body[..500])
                } else {
                    body
                };
                format!("Failed to parse response JSON: {}", truncated)
            })
        } else {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, body);
        }
    }
}

/// PQL query request body
#[derive(Serialize)]
struct PqlQuery {
    query: String,
}

/// Fact path for structured facts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactPath {
    pub path: Vec<String>,
    #[serde(rename = "type")]
    pub fact_type: String,
}

/// Fact content for structured facts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactContent {
    pub certname: String,
    pub path: Vec<String>,
    pub value: serde_json::Value,
    pub environment: Option<String>,
}

/// Environment from PuppetDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
}

/// Server version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerVersion {
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_url_construction() {
        let config = PuppetDbConfig {
            url: "http://localhost:8081/".to_string(),
            timeout_secs: 30,
            ssl_verify: true,
            ssl_cert: None,
            ssl_key: None,
            ssl_ca: None,
        };

        let client = PuppetDbClient::new(&config).unwrap();
        assert_eq!(client.base_url, "http://localhost:8081");
    }

    #[test]
    fn test_query_builder_single_condition() {
        let query = QueryBuilder::new().equals("certname", "node1.example.com");
        assert_eq!(
            query.build(),
            Some("query=[\"=\",\"certname\",\"node1.example.com\"]".to_string())
        );
    }

    #[test]
    fn test_query_builder_multiple_conditions() {
        let query = QueryBuilder::new()
            .equals("certname", "node1.example.com")
            .equals("status", "changed");
        assert_eq!(
            query.build(),
            Some("query=[\"and\",[\"=\",\"certname\",\"node1.example.com\"],[\"=\",\"status\",\"changed\"]]".to_string())
        );
    }

    #[test]
    fn test_query_builder_regex() {
        let query = QueryBuilder::new().matches("certname", ".*\\.example\\.com");
        assert_eq!(
            query.build(),
            Some("query=[\"~\",\"certname\",\".*\\.example\\.com\"]".to_string())
        );
    }

    #[test]
    fn test_query_builder_in_array() {
        let query = QueryBuilder::new().in_array("status", &["changed", "failed"]);
        assert_eq!(
            query.build(),
            Some("query=[\"in\",\"status\", [\"array\",\"changed\",\"failed\"]]".to_string())
        );
    }

    #[test]
    fn test_query_builder_empty() {
        let query = QueryBuilder::new();
        assert_eq!(query.build(), None);
    }

    #[test]
    fn test_query_params_construction() {
        let params = QueryParams::new()
            .limit(10)
            .offset(20)
            .order_by("timestamp", false);

        let qs = params.to_query_string();
        assert!(qs.contains("limit=10"));
        assert!(qs.contains("offset=20"));
        assert!(qs.contains("order_by="));
    }

    #[test]
    fn test_query_params_empty() {
        let params = QueryParams::new();
        assert_eq!(params.to_query_string(), "");
    }

    #[test]
    fn test_query_params_append() {
        let params = QueryParams::new().limit(10);
        let result = params.append_to_query_string("query=[\"=\",\"certname\",\"node1\"]");
        assert!(result.contains("query="));
        assert!(result.contains("limit=10"));
    }
}
