//! Facter API endpoints for managing fact templates and generating external facts

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    db::repository::FactTemplateRepository,
    models::{
        CreateFactTemplateRequest, ExportFormat, Fact, FactTemplate, GenerateFactsRequest,
        UpdateFactTemplateRequest,
    },
    services::facter::{ExportFormat as ServiceExportFormat, FacterService, GeneratedFacts},
    utils::AppError,
    AppState,
};

/// Convert a Vec<Fact> to a JSON object for use with FacterService
fn facts_to_json(facts: Vec<Fact>) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for fact in facts {
        obj.insert(fact.name, fact.value);
    }
    serde_json::Value::Object(obj)
}

/// Create routes for facter endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/templates", get(list_templates).post(create_template))
        .route(
            "/templates/{id}",
            get(get_template).put(update_template).delete(delete_template),
        )
        .route("/generate", post(generate_facts))
        .route("/export/{certname}", get(export_facts))
}

/// List all fact templates
async fn list_templates(State(state): State<AppState>) -> Result<Json<Vec<FactTemplate>>, AppError> {
    let repo = FactTemplateRepository::new(&state.db);
    let templates = repo.get_all().await.map_err(|e| {
        tracing::error!("Failed to list fact templates: {}", e);
        AppError::internal("Failed to list fact templates")
    })?;
    Ok(Json(templates))
}

/// Create a new fact template
async fn create_template(
    State(state): State<AppState>,
    Json(payload): Json<CreateFactTemplateRequest>,
) -> Result<(StatusCode, Json<FactTemplate>), AppError> {
    let repo = FactTemplateRepository::new(&state.db);
    let template = repo
        .create(&payload.name, payload.description.as_deref(), &payload.facts)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create fact template: {}", e);
            if e.to_string().contains("UNIQUE constraint failed") {
                AppError::conflict("A template with this name already exists")
            } else {
                AppError::internal("Failed to create fact template")
            }
        })?;
    Ok((StatusCode::CREATED, Json(template)))
}

/// Get a specific fact template
async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FactTemplate>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid template ID"))?;

    let repo = FactTemplateRepository::new(&state.db);
    let template = repo.get_by_id(uuid).await.map_err(|e| {
        tracing::error!("Failed to get fact template: {}", e);
        AppError::internal("Failed to get fact template")
    })?;

    match template {
        Some(t) => Ok(Json(t)),
        None => Err(AppError::not_found("Fact template not found")),
    }
}

/// Update a fact template
async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateFactTemplateRequest>,
) -> Result<Json<FactTemplate>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid template ID"))?;

    let repo = FactTemplateRepository::new(&state.db);
    let template = repo
        .update(
            uuid,
            payload.name.as_deref(),
            payload.description.as_deref(),
            payload.facts.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to update fact template: {}", e);
            if e.to_string().contains("UNIQUE constraint failed") {
                AppError::conflict("A template with this name already exists")
            } else {
                AppError::internal("Failed to update fact template")
            }
        })?;

    match template {
        Some(t) => Ok(Json(t)),
        None => Err(AppError::not_found("Fact template not found")),
    }
}

/// Delete a fact template
async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid template ID"))?;

    let repo = FactTemplateRepository::new(&state.db);
    let deleted = repo.delete(uuid).await.map_err(|e| {
        tracing::error!("Failed to delete fact template: {}", e);
        AppError::internal("Failed to delete fact template")
    })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Fact template not found"))
    }
}

/// Generate facts for a node using a template
async fn generate_facts(
    State(state): State<AppState>,
    Json(payload): Json<GenerateFactsRequest>,
) -> Result<Json<GeneratedFacts>, AppError> {
    // Get the template from the database
    let repo = FactTemplateRepository::new(&state.db);
    let template = repo.get_by_name(&payload.template).await.map_err(|e| {
        tracing::error!("Failed to get fact template: {}", e);
        AppError::internal("Failed to get fact template")
    })?;

    let template = template.ok_or_else(|| {
        AppError::not_found(format!("Template '{}' not found", payload.template))
    })?;

    // Get existing facts for the node (from payload or PuppetDB)
    let existing_facts = match payload.existing_facts {
        Some(facts) => facts,
        None => {
            // Try to get from PuppetDB if configured
            if let Some(ref puppetdb) = state.puppetdb {
                let facts = puppetdb
                    .get_node_facts(&payload.certname)
                    .await
                    .map_err(|e| {
                        tracing::warn!("Failed to get facts from PuppetDB: {}", e);
                        AppError::internal("Failed to get facts from PuppetDB")
                    })?;
                facts_to_json(facts)
            } else {
                serde_json::json!({})
            }
        }
    };

    // Get classification for the node (mock for now if not classified)
    let classification = crate::models::ClassificationResult {
        certname: payload.certname.clone(),
        groups: vec![],
        classes: vec![],
        parameters: serde_json::json!({}),
        environment: None,
    };

    // Create facter service with the template
    let service = FacterService::new(vec![template]);

    // Generate facts
    let generated = service
        .generate_facts(&classification, &existing_facts, &payload.template)
        .map_err(|e| {
            tracing::error!("Failed to generate facts: {}", e);
            AppError::internal(format!("Failed to generate facts: {}", e))
        })?;

    Ok(Json(generated))
}

/// Query parameters for export endpoint
#[derive(Debug, Deserialize)]
struct ExportQuery {
    /// Export format (json, yaml, shell)
    #[serde(default)]
    format: ExportFormat,
    /// Template name
    template: String,
}

/// Export facts for a node in the specified format
async fn export_facts(
    State(state): State<AppState>,
    Path(certname): Path<String>,
    Query(query): Query<ExportQuery>,
) -> Result<String, AppError> {
    // Get the template
    let repo = FactTemplateRepository::new(&state.db);
    let template = repo.get_by_name(&query.template).await.map_err(|e| {
        tracing::error!("Failed to get fact template: {}", e);
        AppError::internal("Failed to get fact template")
    })?;

    let template = template.ok_or_else(|| {
        AppError::not_found(format!("Template '{}' not found", query.template))
    })?;

    // Get existing facts from PuppetDB if available
    let existing_facts = if let Some(ref puppetdb) = state.puppetdb {
        match puppetdb.get_node_facts(&certname).await {
            Ok(facts) => facts_to_json(facts),
            Err(e) => {
                tracing::warn!("Failed to get facts from PuppetDB: {}", e);
                serde_json::json!({})
            }
        }
    } else {
        serde_json::json!({})
    };

    // Get classification (mock for now)
    let classification = crate::models::ClassificationResult {
        certname: certname.clone(),
        groups: vec![],
        classes: vec![],
        parameters: serde_json::json!({}),
        environment: None,
    };

    // Generate facts
    let service = FacterService::new(vec![template]);
    let generated = service
        .generate_facts(&classification, &existing_facts, &query.template)
        .map_err(|e| {
            tracing::error!("Failed to generate facts: {}", e);
            AppError::internal(format!("Failed to generate facts: {}", e))
        })?;

    // Convert format
    let service_format = match query.format {
        ExportFormat::Json => ServiceExportFormat::Json,
        ExportFormat::Yaml => ServiceExportFormat::Yaml,
        ExportFormat::Shell => ServiceExportFormat::Shell,
    };

    // Export in requested format
    FacterService::export_facts(&generated, service_format).map_err(|e| {
        tracing::error!("Failed to export facts: {}", e);
        AppError::internal(format!("Failed to export facts: {}", e))
    })
}
