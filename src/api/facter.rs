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
    db::repository::{FactTemplateRepository, GroupRepository},
    middleware::AuthUser,
    models::{
        CreateFactTemplateRequest, ExportFormat, Fact, FactTemplate, GenerateFactsRequest,
        UpdateFactTemplateRequest,
    },
    services::{
        classification::ClassificationService,
        facter::{ExportFormat as ServiceExportFormat, FacterService, GeneratedFacts},
    },
    utils::{
        validation::{format_validation_errors, validate_fact_template},
        AppError,
    },
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
            get(get_template)
                .put(update_template)
                .delete(delete_template),
        )
        .route("/generate", post(generate_facts))
        .route("/export/{certname}", get(export_facts))
}

#[derive(Debug, Deserialize, Default)]
struct OrgQuery {
    organization_id: Option<Uuid>,
}

fn resolve_org(auth_user: &AuthUser, requested: Option<Uuid>) -> Result<Uuid, AppError> {
    match requested {
        Some(org_id) if !auth_user.is_super_admin() => Err(AppError::forbidden(
            "organization_id can only be specified by super_admin",
        )),
        Some(org_id) => Ok(org_id),
        None => Ok(auth_user.organization_id),
    }
}

/// List all fact templates
async fn list_templates(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
) -> Result<Json<Vec<FactTemplate>>, AppError> {
    let org_id = resolve_org(&auth_user, query.organization_id)?;
    let repo = FactTemplateRepository::new(&state.db);
    let templates = repo.get_all(org_id).await.map_err(|e| {
        tracing::error!("Failed to list fact templates: {}", e);
        AppError::internal("Failed to list fact templates")
    })?;
    Ok(Json(templates))
}

/// Create a new fact template
async fn create_template(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Json(payload): Json<CreateFactTemplateRequest>,
) -> Result<(StatusCode, Json<FactTemplate>), AppError> {
    let org_id = resolve_org(&auth_user, query.organization_id)?;
    // Validate the template before creating
    let template_for_validation = FactTemplate {
        id: None,
        organization_id: org_id,
        name: payload.name.clone(),
        description: payload.description.clone(),
        facts: payload.facts.clone(),
    };

    if let Err(errors) = validate_fact_template(&template_for_validation) {
        return Err(AppError::bad_request(format!(
            "Validation failed: {}",
            format_validation_errors(&errors)
        )));
    }

    let repo = FactTemplateRepository::new(&state.db);
    let template = repo
        .create(
            org_id,
            &payload.name,
            payload.description.as_deref(),
            &payload.facts,
        )
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<Json<FactTemplate>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid template ID"))?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = FactTemplateRepository::new(&state.db);
    let template = repo.get_by_id(org_id, uuid).await.map_err(|e| {
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateFactTemplateRequest>,
) -> Result<Json<FactTemplate>, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid template ID"))?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = FactTemplateRepository::new(&state.db);

    // Get existing template to merge with updates for validation
    let existing = repo.get_by_id(org_id, uuid).await.map_err(|e| {
        tracing::error!("Failed to get fact template: {}", e);
        AppError::internal("Failed to get fact template")
    })?;

    let existing = existing.ok_or_else(|| AppError::not_found("Fact template not found"))?;

    // Create merged template for validation
    let template_for_validation = FactTemplate {
        id: existing.id.clone(),
        organization_id: existing.organization_id,
        name: payload.name.clone().unwrap_or(existing.name.clone()),
        description: payload.description.clone().or(existing.description.clone()),
        facts: payload.facts.clone().unwrap_or(existing.facts.clone()),
    };

    if let Err(errors) = validate_fact_template(&template_for_validation) {
        return Err(AppError::bad_request(format!(
            "Validation failed: {}",
            format_validation_errors(&errors)
        )));
    }

    let template = repo
        .update(
            org_id,
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| AppError::bad_request("Invalid template ID"))?;
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    let repo = FactTemplateRepository::new(&state.db);
    let deleted = repo.delete(org_id, uuid).await.map_err(|e| {
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
    auth_user: AuthUser,
    Query(query): Query<OrgQuery>,
    Json(payload): Json<GenerateFactsRequest>,
) -> Result<Json<GeneratedFacts>, AppError> {
    let org_id = resolve_org(&auth_user, query.organization_id)?;
    // Get the template from the database
    let repo = FactTemplateRepository::new(&state.db);
    let template = repo
        .get_by_name(org_id, &payload.template)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get fact template: {}", e);
            AppError::internal("Failed to get fact template")
        })?;

    let template = template
        .ok_or_else(|| AppError::not_found(format!("Template '{}' not found", payload.template)))?;

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

    // Get all groups for classification
    let group_repo = GroupRepository::new(&state.db);
    let all_groups = group_repo.get_all(org_id).await.map_err(|e| {
        tracing::error!("Failed to get groups for classification: {}", e);
        AppError::internal("Failed to get groups for classification")
    })?;

    // Classify the node to get variables from matched groups
    let classification_service = ClassificationService::new(all_groups);
    let classification = classification_service.classify(&payload.certname, &existing_facts);

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
    /// Optional organization override (super_admin only)
    organization_id: Option<Uuid>,
}

/// Export facts for a node in the specified format
async fn export_facts(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(certname): Path<String>,
    Query(query): Query<ExportQuery>,
) -> Result<String, AppError> {
    let org_id = resolve_org(&auth_user, query.organization_id)?;

    // Get the template
    let repo = FactTemplateRepository::new(&state.db);
    let template = repo
        .get_by_name(org_id, &query.template)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get fact template: {}", e);
            AppError::internal("Failed to get fact template")
        })?;

    let template = template
        .ok_or_else(|| AppError::not_found(format!("Template '{}' not found", query.template)))?;

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

    // Get all groups for classification
    let group_repo = GroupRepository::new(&state.db);
    let all_groups = group_repo.get_all(org_id).await.map_err(|e| {
        tracing::error!("Failed to get groups for classification: {}", e);
        AppError::internal("Failed to get groups for classification")
    })?;

    // Classify the node to get variables from matched groups
    let classification_service = ClassificationService::new(all_groups);
    let classification = classification_service.classify(&certname, &existing_facts);

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
