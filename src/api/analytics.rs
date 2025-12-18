//! Analytics and Reporting API endpoints

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::repository::{
    ComplianceBaselineRepository, DriftBaselineRepository, ReportExecutionRepository,
    ReportScheduleRepository, ReportTemplateRepository, SavedReportRepository,
};
use crate::middleware::auth::AuthUser;
use crate::models::{
    ComplianceBaseline, CreateComplianceBaselineRequest, CreateDriftBaselineRequest,
    CreateSavedReportRequest, CreateScheduleRequest, DriftBaseline, ExecuteReportRequest,
    OutputFormat, ReportExecution, ReportQueryConfig, ReportResult, ReportSchedule, ReportTemplate,
    ReportType, SavedReport, UpdateSavedReportRequest, UpdateScheduleRequest,
};
use crate::services::ReportingService;
use crate::utils::error::{AppError, AppResult};
use crate::AppState;

/// Create the analytics API router
pub fn routes() -> Router<AppState> {
    Router::new()
        // Saved Reports
        .route("/saved-reports", get(list_saved_reports).post(create_saved_report))
        .route(
            "/saved-reports/{id}",
            get(get_saved_report)
                .put(update_saved_report)
                .delete(delete_saved_report),
        )
        .route("/saved-reports/{id}/execute", post(execute_saved_report))
        .route("/saved-reports/{id}/executions", get(list_report_executions))
        // Report Templates
        .route("/templates", get(list_report_templates))
        .route("/templates/{id}", get(get_report_template))
        // Schedules
        .route("/schedules", get(list_schedules).post(create_schedule))
        .route(
            "/schedules/{id}",
            get(get_schedule)
                .put(update_schedule)
                .delete(delete_schedule),
        )
        // Generate reports on-demand (without saving)
        .route("/generate", post(generate_report))
        .route("/generate/{report_type}", post(generate_report_by_type))
        // Compliance Baselines
        .route("/compliance-baselines", get(list_compliance_baselines).post(create_compliance_baseline))
        .route(
            "/compliance-baselines/{id}",
            get(get_compliance_baseline).delete(delete_compliance_baseline),
        )
        // Drift Baselines
        .route("/drift-baselines", get(list_drift_baselines).post(create_drift_baseline))
        .route(
            "/drift-baselines/{id}",
            get(get_drift_baseline).delete(delete_drift_baseline),
        )
        // Export
        .route("/executions/{id}/export", get(export_execution))
}

// ==================== Query Parameters ====================

#[derive(Debug, Deserialize)]
pub struct ListReportsQuery {
    pub report_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExecutionsQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,
}

// ==================== Saved Reports ====================

/// List all saved reports (user's own + public)
async fn list_saved_reports(
    State(state): State<AppState>,
    Query(query): Query<ListReportsQuery>,
    auth_user: AuthUser,
) -> AppResult<Json<Vec<SavedReport>>> {
    let repo = SavedReportRepository::new(&state.db);

    let reports = if let Some(type_str) = query.report_type {
        let report_type = ReportType::from_str(&type_str)
            .ok_or_else(|| AppError::bad_request("Invalid report type"))?;
        repo.get_by_type(report_type).await?
    } else {
        repo.get_by_user(auth_user.user_id()).await?
    };

    Ok(Json(reports))
}

/// Get a saved report by ID
async fn get_saved_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<SavedReport>> {
    let repo = SavedReportRepository::new(&state.db);
    let report = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("Saved report not found"))?;

    Ok(Json(report))
}

/// Create a new saved report
async fn create_saved_report(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(req): Json<CreateSavedReportRequest>,
) -> AppResult<Json<SavedReport>> {
    let repo = SavedReportRepository::new(&state.db);
    let report = repo.create(&req, auth_user.user_id()).await?;
    Ok(Json(report))
}

/// Update a saved report
async fn update_saved_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSavedReportRequest>,
) -> AppResult<Json<SavedReport>> {
    let repo = SavedReportRepository::new(&state.db);
    let report = repo
        .update(id, &req)
        .await?
        .ok_or_else(|| AppError::not_found("Saved report not found"))?;
    Ok(Json(report))
}

/// Delete a saved report
async fn delete_saved_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let repo = SavedReportRepository::new(&state.db);
    let deleted = repo.delete(id).await?;

    if deleted {
        Ok(Json(serde_json::json!({"message": "Report deleted successfully"})))
    } else {
        Err(AppError::not_found("Saved report not found"))
    }
}

/// Execute a saved report
async fn execute_saved_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth_user: AuthUser,
    Json(req): Json<ExecuteReportRequest>,
) -> AppResult<Json<ReportExecution>> {
    let repo = SavedReportRepository::new(&state.db);
    let report = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("Saved report not found"))?;

    let service = ReportingService::new(state.db.clone(), state.puppetdb.clone());
    let execution = service
        .execute_report(&report, &req, Some(auth_user.user_id()))
        .await?;

    Ok(Json(execution))
}

/// List executions for a saved report
async fn list_report_executions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExecutionsQuery>,
) -> AppResult<Json<Vec<ReportExecution>>> {
    let repo = ReportExecutionRepository::new(&state.db);
    let executions = repo.get_by_report(id, query.limit).await?;
    Ok(Json(executions))
}

// ==================== Report Templates ====================

/// List all report templates
async fn list_report_templates(
    State(state): State<AppState>,
    Query(query): Query<ListReportsQuery>,
) -> AppResult<Json<Vec<ReportTemplate>>> {
    let repo = ReportTemplateRepository::new(&state.db);

    let templates = if let Some(type_str) = query.report_type {
        let report_type = ReportType::from_str(&type_str)
            .ok_or_else(|| AppError::bad_request("Invalid report type"))?;
        repo.get_by_type(report_type).await?
    } else {
        repo.get_all().await?
    };

    Ok(Json(templates))
}

/// Get a report template by ID
async fn get_report_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ReportTemplate>> {
    let repo = ReportTemplateRepository::new(&state.db);
    let template = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("Report template not found"))?;

    Ok(Json(template))
}

// ==================== Schedules ====================

/// List all schedules
async fn list_schedules(State(state): State<AppState>) -> AppResult<Json<Vec<ReportSchedule>>> {
    let repo = ReportScheduleRepository::new(&state.db);
    let schedules = repo.get_all().await?;
    Ok(Json(schedules))
}

/// Get a schedule by ID
async fn get_schedule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ReportSchedule>> {
    let repo = ReportScheduleRepository::new(&state.db);
    let schedule = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("Schedule not found"))?;

    Ok(Json(schedule))
}

/// Create a new schedule
async fn create_schedule(
    State(state): State<AppState>,
    Json(req): Json<CreateScheduleRequest>,
) -> AppResult<Json<ReportSchedule>> {
    // Validate that the report exists
    let report_repo = SavedReportRepository::new(&state.db);
    report_repo
        .get_by_id(req.report_id)
        .await?
        .ok_or_else(|| AppError::bad_request("Report not found"))?;

    let repo = ReportScheduleRepository::new(&state.db);
    let schedule = repo.create(&req).await?;
    Ok(Json(schedule))
}

/// Update a schedule
async fn update_schedule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateScheduleRequest>,
) -> AppResult<Json<ReportSchedule>> {
    let repo = ReportScheduleRepository::new(&state.db);
    let schedule = repo
        .update(id, &req)
        .await?
        .ok_or_else(|| AppError::not_found("Schedule not found"))?;
    Ok(Json(schedule))
}

/// Delete a schedule
async fn delete_schedule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let repo = ReportScheduleRepository::new(&state.db);
    let deleted = repo.delete(id).await?;

    if deleted {
        Ok(Json(serde_json::json!({"message": "Schedule deleted successfully"})))
    } else {
        Err(AppError::not_found("Schedule not found"))
    }
}

// ==================== Generate Reports On-Demand ====================

/// Request body for on-demand report generation
#[derive(Debug, Deserialize)]
pub struct GenerateReportRequest {
    pub report_type: ReportType,
    #[serde(default)]
    pub config: ReportQueryConfig,
    #[serde(default)]
    pub output_format: OutputFormat,
}

/// Generate a report on-demand (without saving)
async fn generate_report(
    State(state): State<AppState>,
    Json(req): Json<GenerateReportRequest>,
) -> AppResult<Json<ReportResult>> {
    let service = ReportingService::new(state.db.clone(), state.puppetdb.clone());
    let (result, _) = service.generate_report(req.report_type, &req.config).await?;
    Ok(Json(result))
}

/// Generate a specific type of report
async fn generate_report_by_type(
    State(state): State<AppState>,
    Path(report_type): Path<String>,
    Json(config): Json<ReportQueryConfig>,
) -> AppResult<Json<ReportResult>> {
    let report_type = ReportType::from_str(&report_type)
        .ok_or_else(|| AppError::bad_request("Invalid report type"))?;

    let service = ReportingService::new(state.db.clone(), state.puppetdb.clone());
    let (result, _) = service.generate_report(report_type, &config).await?;
    Ok(Json(result))
}

// ==================== Compliance Baselines ====================

/// List all compliance baselines
async fn list_compliance_baselines(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<ComplianceBaseline>>> {
    let repo = ComplianceBaselineRepository::new(&state.db);
    let baselines = repo.get_all().await?;
    Ok(Json(baselines))
}

/// Get a compliance baseline by ID
async fn get_compliance_baseline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ComplianceBaseline>> {
    let repo = ComplianceBaselineRepository::new(&state.db);
    let baseline = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("Compliance baseline not found"))?;

    Ok(Json(baseline))
}

/// Create a new compliance baseline
async fn create_compliance_baseline(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(req): Json<CreateComplianceBaselineRequest>,
) -> AppResult<Json<ComplianceBaseline>> {
    let repo = ComplianceBaselineRepository::new(&state.db);
    let baseline = repo.create(&req, auth_user.user_id()).await?;
    Ok(Json(baseline))
}

/// Delete a compliance baseline
async fn delete_compliance_baseline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let repo = ComplianceBaselineRepository::new(&state.db);
    let deleted = repo.delete(id).await?;

    if deleted {
        Ok(Json(serde_json::json!({"message": "Baseline deleted successfully"})))
    } else {
        Err(AppError::not_found("Compliance baseline not found"))
    }
}

// ==================== Drift Baselines ====================

/// List all drift baselines
async fn list_drift_baselines(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<DriftBaseline>>> {
    let repo = DriftBaselineRepository::new(&state.db);
    let baselines = repo.get_all().await?;
    Ok(Json(baselines))
}

/// Get a drift baseline by ID
async fn get_drift_baseline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<DriftBaseline>> {
    let repo = DriftBaselineRepository::new(&state.db);
    let baseline = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("Drift baseline not found"))?;

    Ok(Json(baseline))
}

/// Create a new drift baseline
async fn create_drift_baseline(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(req): Json<CreateDriftBaselineRequest>,
) -> AppResult<Json<DriftBaseline>> {
    let repo = DriftBaselineRepository::new(&state.db);
    let baseline = repo.create(&req, auth_user.user_id()).await?;
    Ok(Json(baseline))
}

/// Delete a drift baseline
async fn delete_drift_baseline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let repo = DriftBaselineRepository::new(&state.db);
    let deleted = repo.delete(id).await?;

    if deleted {
        Ok(Json(serde_json::json!({"message": "Baseline deleted successfully"})))
    } else {
        Err(AppError::not_found("Drift baseline not found"))
    }
}

// ==================== Export ====================

/// Export an execution result in the specified format
async fn export_execution(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> AppResult<(axum::http::StatusCode, [(String, String); 2], Vec<u8>)> {
    let repo = ReportExecutionRepository::new(&state.db);
    let execution = repo
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("Execution not found"))?;

    let output_data = execution
        .output_data
        .ok_or_else(|| AppError::bad_request("Execution has no output data"))?;

    let format = query
        .format
        .and_then(|f| OutputFormat::from_str(&f))
        .unwrap_or(execution.output_format);

    // Parse the stored result
    let result: ReportResult = serde_json::from_value(output_data)
        .map_err(|_| AppError::internal("Failed to parse stored report data"))?;

    let service = ReportingService::new(state.db.clone(), state.puppetdb.clone());
    let data = service.export_report(&result, format)?;

    let content_type = format.content_type().to_string();
    let filename = format!("report-{}.{}", id, format.file_extension());

    Ok((
        axum::http::StatusCode::OK,
        [
            ("Content-Type".to_string(), content_type),
            (
                "Content-Disposition".to_string(),
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        data,
    ))
}
