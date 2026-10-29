//! Facts API endpoints
//!
//! Provides endpoints for querying facts from PuppetDB, merged with the facts
//! of enabled plugin fact providers (M25). Plugin facts carry a `source`
//! (`plugin:<id>`); PuppetDB facts keep their original shape.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    middleware::AuthUser,
    models::{Fact, MergedFact},
    services::{
        plugins::facts::{agent_names_of, collides, NodePluginFact},
        puppetdb::{QueryBuilder, QueryParams},
    },
    utils::error::{AppError, AppResult},
    AppState,
};

use super::plugin_facts::fact_visibility;
use super::plugins::check;

/// Which facts a catalog or query covers (`?source=`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FactSource {
    /// PuppetDB merged with the plugin facts (default).
    All,
    PuppetDb,
    /// Every plugin fact provider.
    Plugins,
    Plugin(String),
}

impl FactSource {
    pub(super) fn parse(s: Option<&str>) -> Result<Self, AppError> {
        match s {
            None | Some("") | Some("all") => Ok(Self::All),
            Some("puppetdb") => Ok(Self::PuppetDb),
            Some("plugins") => Ok(Self::Plugins),
            Some(other) => match other.strip_prefix("plugin:") {
                Some(id) if !id.is_empty() => Ok(Self::Plugin(id.to_string())),
                _ => Err(AppError::BadRequest(format!(
                    "source '{}' must be puppetdb, plugins or plugin:<id>",
                    other
                ))),
            },
        }
    }

    fn includes_plugin(&self, plugin_id: &str) -> bool {
        match self {
            Self::All | Self::Plugins => true,
            Self::PuppetDb => false,
            Self::Plugin(id) => id == plugin_id,
        }
    }

    fn includes_puppetdb(&self) -> bool {
        matches!(self, Self::All | Self::PuppetDb)
    }
}

/// Declared plugin facts the user may see: `(plugin_id, full name, type)`.
async fn visible_plugin_facts(
    state: &AppState,
    user: &AuthUser,
    source: &FactSource,
) -> AppResult<Vec<(String, String, &'static str)>> {
    let declared = state.plugin_manager.declared_plugin_facts();
    if declared.is_empty() {
        return Ok(Vec::new());
    }
    let vis = fact_visibility(state, user).await?;
    let allow = check(&vis);
    Ok(declared
        .into_iter()
        .filter(|(p, _, d)| source.includes_plugin(p) && allow(&d.requires))
        .map(|(p, n, d)| (p, n, d.fact_type.catalog_type()))
        .collect())
}

/// Keeps the plugin facts whose `requires` the user satisfies.
pub(super) async fn filter_visible(
    state: &AppState,
    user: &AuthUser,
    facts: Vec<NodePluginFact>,
) -> AppResult<Vec<NodePluginFact>> {
    if facts.iter().all(|f| f.requires.is_none()) {
        return Ok(facts);
    }
    let vis = fact_visibility(state, user).await?;
    let allow = check(&vis);
    Ok(facts.into_iter().filter(|f| allow(&f.requires)).collect())
}

fn value_matches(v: &serde_json::Value, wanted: &str) -> bool {
    match v {
        serde_json::Value::String(s) => s == wanted,
        serde_json::Value::Number(n) => n.to_string() == wanted,
        serde_json::Value::Bool(b) => b.to_string() == wanted,
        _ => false,
    }
}

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
    /// `puppetdb`, `plugins` or `plugin:<id>`; default: merged
    pub source: Option<String>,
}

/// Response for facts query
#[derive(Debug, Serialize)]
pub struct FactsResponse {
    pub facts: Vec<MergedFact>,
    pub total: Option<u64>,
}

impl FactsResponse {
    fn of(facts: Vec<MergedFact>) -> Self {
        Self {
            total: Some(facts.len() as u64),
            facts,
        }
    }
}

/// Plugin values for `GET /facts` (source `plugins` or `plugin:<id>`, or a
/// plugin fact name that PuppetDB does not know).
async fn plugin_fact_values(
    state: &AppState,
    user: &AuthUser,
    query: &FactsQuery,
    source: &FactSource,
    limit: u32,
) -> AppResult<Vec<MergedFact>> {
    let mut plugin_ids: Vec<String> = state
        .plugin_manager
        .fact_providers()
        .into_iter()
        .map(|h| h.plugin_id)
        .filter(|p| source.includes_plugin(p))
        .collect();
    plugin_ids.dedup();
    let mut out = Vec::new();
    for id in plugin_ids {
        let rows = state
            .plugin_manager
            .query_plugin_facts(
                &id,
                query.name.as_deref(),
                query.certname.as_deref(),
                limit,
                query.offset.unwrap_or(0),
            )
            .await
            .map_err(|e| AppError::Internal(format!("Failed to query plugin facts: {}", e)))?;
        let rows = filter_visible(state, user, rows).await?;
        out.extend(
            rows.iter()
                .filter(|f| {
                    query
                        .value
                        .as_deref()
                        .is_none_or(|v| value_matches(&f.value, v))
                })
                .map(NodePluginFact::to_merged),
        );
    }
    out.truncate(limit as usize);
    Ok(out)
}

/// Query facts across all nodes
///
/// GET /api/v1/facts
///
/// Query parameters:
/// - `name`: Filter by fact name (supports dot-notation for structured facts like "os.family")
/// - `value`: Filter by fact value (exact match)
/// - `certname`: Filter by certname
/// - `environment`: Filter by environment
/// - `limit`: Maximum number of results (default: 100)
/// - `offset`: Number of results to skip
/// - `source`: `puppetdb`, `plugins` or `plugin:<id>` (default: PuppetDB, and
///   the plugin values when `name` is a plugin fact PuppetDB does not know)
async fn query_facts(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<FactsQuery>,
) -> AppResult<Json<FactsResponse>> {
    let source = FactSource::parse(query.source.as_deref())?;
    let limit = state.config.pagination.resolve_limit(query.limit);
    if !source.includes_puppetdb() {
        let facts = plugin_fact_values(&state, &user, &query, &source, limit).await?;
        return Ok(Json(FactsResponse::of(facts)));
    }
    let facts: Vec<MergedFact> = query_puppetdb_facts(&state, &query, limit)
        .await?
        .into_iter()
        .map(MergedFact::from)
        .collect();
    // The agent's fact wins: plugin values only fill a name PuppetDB lacks.
    if facts.is_empty() && source == FactSource::All && query.environment.is_none() {
        if let Some(name) = query.name.as_deref() {
            let is_plugin_fact = state
                .plugin_manager
                .declared_plugin_facts()
                .iter()
                .any(|(_, n, _)| n == name);
            if is_plugin_fact {
                let plugin = plugin_fact_values(&state, &user, &query, &source, limit).await?;
                return Ok(Json(FactsResponse::of(plugin)));
            }
        }
    }
    Ok(Json(FactsResponse::of(facts)))
}

async fn query_puppetdb_facts(
    state: &AppState,
    query: &FactsQuery,
    limit: u32,
) -> AppResult<Vec<Fact>> {
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    // Helper function to query fact-contents endpoint and convert to Fact format
    // Tries multiple query strategies: by path first, then by name if no results
    async fn query_fact_contents_as_facts(
        puppetdb: &crate::services::puppetdb::PuppetDbClient,
        name: &str,
        certname: Option<&str>,
        value_filter: Option<&str>,
        env_filter: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<Fact>, AppError> {
        // First try by path (works for most facts)
        let mut fact_contents = puppetdb
            .query_fact_contents_by_path(name, certname, limit.or(Some(100)))
            .await
            .unwrap_or_default();

        // If no results, try by name (works for external facts)
        if fact_contents.is_empty() {
            fact_contents = puppetdb
                .query_fact_contents_by_name(name, certname, limit.or(Some(100)))
                .await
                .map_err(|e| AppError::Internal(format!("Failed to query fact contents: {}", e)))?;

            // Filter to only include entries where path length is 1 (top-level facts)
            // to avoid returning nested values for structured facts
            fact_contents.retain(|fc| fc.path.len() == 1);
        }

        // Convert FactContent to Fact format for consistent API response
        let facts: Vec<Fact> = fact_contents
            .into_iter()
            .filter(|fc| {
                // Apply value filter if specified
                if let Some(value) = value_filter {
                    match &fc.value {
                        serde_json::Value::String(s) => s == value,
                        serde_json::Value::Number(n) => n.to_string() == value,
                        serde_json::Value::Bool(b) => b.to_string() == value,
                        _ => false,
                    }
                } else {
                    true
                }
            })
            .filter(|fc| {
                // Apply environment filter if specified
                if let Some(env) = env_filter {
                    fc.environment.as_ref().map(|e| e == env).unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|fc| Fact {
                certname: fc.certname,
                name: name.to_string(),
                value: fc.value,
                environment: fc.environment,
            })
            .collect();

        Ok(facts)
    }

    // Check if the fact name contains a dot (structured fact path like "os.family")
    // If so, use the fact-contents endpoint instead
    if let Some(ref name) = query.name {
        if name.contains('.') {
            // Use fact-contents endpoint for structured facts with nested paths
            let facts = query_fact_contents_as_facts(
                puppetdb,
                name,
                query.certname.as_deref(),
                query.value.as_deref(),
                query.environment.as_deref(),
                Some(limit),
            )
            .await?;

            return Ok(facts);
        }
    }

    // Use standard facts endpoint for top-level facts
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

    // Build pagination params (limit resolved against config above)
    let mut params = QueryParams::new().limit(limit);
    if let Some(offset) = query.offset {
        params = params.offset(offset);
    }

    // Execute query
    let facts = puppetdb
        .query_facts_advanced(&qb, params)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query facts: {}", e)))?;

    // If querying by name and got no results, try fact-contents endpoint
    // This handles external facts and structured facts that don't appear in the standard facts endpoint
    if facts.is_empty() {
        if let Some(ref name) = query.name {
            let fallback_facts = query_fact_contents_as_facts(
                puppetdb,
                name,
                query.certname.as_deref(),
                query.value.as_deref(),
                query.environment.as_deref(),
                Some(limit),
            )
            .await?;

            if !fallback_facts.is_empty() {
                return Ok(fallback_facts);
            }
        }
    }

    Ok(facts)
}

#[derive(Debug, Default, Deserialize)]
pub struct CatalogQuery {
    /// `puppetdb`, `plugins` or `plugin:<id>`; default: merged
    pub source: Option<String>,
}

/// List all unique fact names
///
/// GET /api/v1/facts/names
///
/// Merged with the plugin facts the user may see; `?source=` narrows it.
/// A plugin fact that collides with an agent fact is left out.
async fn list_fact_names(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<CatalogQuery>,
) -> AppResult<Json<Vec<String>>> {
    let source = FactSource::parse(q.source.as_deref())?;
    let plugin = visible_plugin_facts(&state, &user, &source).await?;
    if !source.includes_puppetdb() {
        return Ok(Json(plugin.into_iter().map(|(_, n, _)| n).collect()));
    }
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    let mut names = puppetdb
        .get_fact_names()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch fact names: {}", e)))?;

    if source == FactSource::All && !plugin.is_empty() {
        let agent = agent_names_of(names.iter().map(String::as_str));
        names.extend(
            plugin
                .into_iter()
                .filter(|(_, n, _)| !collides(n, &agent))
                .map(|(_, n, _)| n),
        );
        names.sort();
        names.dedup();
    }

    Ok(Json(names))
}

/// Fact path response
#[derive(Debug, Serialize)]
pub struct FactPathResponse {
    pub path: Vec<String>,
    #[serde(rename = "type")]
    pub fact_type: String,
    /// `plugin:<id>` for plugin facts; absent for PuppetDB paths.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// List all unique fact paths (for structured facts)
///
/// GET /api/v1/facts/paths
///
/// Merged with the plugin facts the user may see (with `source`); `?source=`
/// narrows it and `?source=plugins` works without PuppetDB.
async fn list_fact_paths(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<CatalogQuery>,
) -> AppResult<Json<Vec<FactPathResponse>>> {
    let source = FactSource::parse(q.source.as_deref())?;
    let plugin = visible_plugin_facts(&state, &user, &source).await?;
    let to_entry = |(p, n, t): (String, String, &'static str)| FactPathResponse {
        path: n.split('.').map(str::to_string).collect(),
        fact_type: t.to_string(),
        source: Some(crate::models::plugin_source(&p)),
    };
    if !source.includes_puppetdb() {
        return Ok(Json(plugin.into_iter().map(to_entry).collect()));
    }
    let puppetdb = state
        .puppetdb
        .as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PuppetDB is not configured".to_string()))?;

    let paths = puppetdb
        .get_fact_paths()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch fact paths: {}", e)))?;

    let mut response: Vec<FactPathResponse> = paths
        .into_iter()
        .map(|p| FactPathResponse {
            path: p.path,
            fact_type: p.fact_type,
            source: None,
        })
        .collect();

    if source == FactSource::All && !plugin.is_empty() {
        let agent = agent_names_of(
            response
                .iter()
                .filter_map(|p| p.path.first().map(String::as_str)),
        );
        response.extend(
            plugin
                .into_iter()
                .filter(|(_, n, _)| !collides(n, &agent))
                .map(to_entry),
        );
    }

    Ok(Json(response))
}
