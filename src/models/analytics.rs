//! Analytics and Reporting models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Report types supported by the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    /// Node health and status reports
    #[default]
    NodeHealth,
    /// Compliance status reports
    Compliance,
    /// Change tracking reports
    ChangeTracking,
    /// Configuration drift detection
    DriftDetection,
    /// Custom user-defined reports
    Custom,
}

impl ReportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReportType::NodeHealth => "node_health",
            ReportType::Compliance => "compliance",
            ReportType::ChangeTracking => "change_tracking",
            ReportType::DriftDetection => "drift_detection",
            ReportType::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "node_health" => Some(ReportType::NodeHealth),
            "compliance" => Some(ReportType::Compliance),
            "change_tracking" => Some(ReportType::ChangeTracking),
            "drift_detection" => Some(ReportType::DriftDetection),
            "custom" => Some(ReportType::Custom),
            _ => None,
        }
    }
}

/// Output formats for report exports
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Json,
    Csv,
    Pdf,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
            OutputFormat::Pdf => "pdf",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "json" => Some(OutputFormat::Json),
            "csv" => Some(OutputFormat::Csv),
            "pdf" => Some(OutputFormat::Pdf),
            _ => None,
        }
    }

    pub fn content_type(&self) -> &'static str {
        match self {
            OutputFormat::Json => "application/json",
            OutputFormat::Csv => "text/csv",
            OutputFormat::Pdf => "application/pdf",
        }
    }

    pub fn file_extension(&self) -> &'static str {
        match self {
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
            OutputFormat::Pdf => "pdf",
        }
    }
}

/// Execution status for reports
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Pending => "pending",
            ExecutionStatus::Running => "running",
            ExecutionStatus::Completed => "completed",
            ExecutionStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ExecutionStatus::Pending),
            "running" => Some(ExecutionStatus::Running),
            "completed" => Some(ExecutionStatus::Completed),
            "failed" => Some(ExecutionStatus::Failed),
            _ => None,
        }
    }
}

/// Severity levels for compliance rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SeverityLevel {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

impl SeverityLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SeverityLevel::Low => "low",
            SeverityLevel::Medium => "medium",
            SeverityLevel::High => "high",
            SeverityLevel::Critical => "critical",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(SeverityLevel::Low),
            "medium" => Some(SeverityLevel::Medium),
            "high" => Some(SeverityLevel::High),
            "critical" => Some(SeverityLevel::Critical),
            _ => None,
        }
    }
}

/// A saved report definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedReport {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub report_type: ReportType,
    pub query_config: ReportQueryConfig,
    pub created_by: Uuid,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Configuration for report queries
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportQueryConfig {
    /// Time range for the report (e.g., "24h", "7d", "30d")
    #[serde(default)]
    pub time_range: Option<String>,
    /// Filter by node status
    #[serde(default)]
    pub status_filter: Option<Vec<String>>,
    /// Filter by environment
    #[serde(default)]
    pub environment_filter: Option<Vec<String>>,
    /// Filter by node group
    #[serde(default)]
    pub node_group_filter: Option<Vec<Uuid>>,
    /// Filter by certname pattern
    #[serde(default)]
    pub certname_pattern: Option<String>,
    /// Group results by field
    #[serde(default)]
    pub group_by: Option<String>,
    /// Include detailed resource information
    #[serde(default)]
    pub include_resources: bool,
    /// Include error details for failed runs
    #[serde(default)]
    pub include_error_details: bool,
    /// Metrics to include in the report
    #[serde(default)]
    pub metrics: Option<Vec<String>>,
    /// Severity filter for compliance reports
    #[serde(default)]
    pub severity_filter: Option<Vec<String>>,
    /// Comparison mode for drift detection
    #[serde(default)]
    pub compare_mode: Option<String>,
    /// Ignore volatile facts in drift detection
    #[serde(default)]
    pub ignore_volatile_facts: bool,
    /// Custom query parameters
    #[serde(default)]
    pub custom_params: Option<serde_json::Value>,
}

/// Report schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSchedule {
    pub id: Uuid,
    pub report_id: Uuid,
    pub schedule_cron: String,
    pub timezone: String,
    pub is_enabled: bool,
    pub output_format: OutputFormat,
    pub email_recipients: Option<Vec<String>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Report execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportExecution {
    pub id: Uuid,
    pub report_id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub executed_by: Option<Uuid>,
    pub status: ExecutionStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub row_count: Option<i32>,
    pub output_format: OutputFormat,
    pub output_data: Option<serde_json::Value>,
    pub output_file_path: Option<String>,
    pub error_message: Option<String>,
    pub execution_time_ms: Option<i32>,
}

/// Compliance baseline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceBaseline {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<ComplianceRule>,
    pub severity_level: SeverityLevel,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single compliance rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub fact_name: String,
    pub operator: String,
    pub expected_value: serde_json::Value,
    pub severity: SeverityLevel,
}

/// Drift baseline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftBaseline {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub node_group_id: Option<Uuid>,
    pub baseline_facts: serde_json::Value,
    pub tolerance_config: Option<DriftToleranceConfig>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Configuration for acceptable drift tolerances
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DriftToleranceConfig {
    /// Facts to ignore during drift detection
    #[serde(default)]
    pub ignored_facts: Vec<String>,
    /// Numeric tolerance (percentage) for numeric facts
    #[serde(default)]
    pub numeric_tolerance_percent: Option<f64>,
    /// Allow minor version differences in version facts
    #[serde(default)]
    pub allow_minor_version_drift: bool,
}

/// Report template (predefined configurations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub report_type: ReportType,
    pub query_config: ReportQueryConfig,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
}

// ==================== Request/Response DTOs ====================

/// Request to create a saved report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSavedReportRequest {
    pub name: String,
    pub description: Option<String>,
    pub report_type: ReportType,
    pub query_config: ReportQueryConfig,
    #[serde(default)]
    pub is_public: bool,
}

/// Request to update a saved report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSavedReportRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub query_config: Option<ReportQueryConfig>,
    pub is_public: Option<bool>,
}

/// Request to create a report schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScheduleRequest {
    pub report_id: Uuid,
    pub schedule_cron: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub output_format: OutputFormat,
    pub email_recipients: Option<Vec<String>>,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_true() -> bool {
    true
}

/// Request to update a report schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScheduleRequest {
    pub schedule_cron: Option<String>,
    pub timezone: Option<String>,
    pub is_enabled: Option<bool>,
    pub output_format: Option<OutputFormat>,
    pub email_recipients: Option<Vec<String>>,
}

/// Request to execute a report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteReportRequest {
    #[serde(default)]
    pub output_format: OutputFormat,
    /// Override query config for this execution
    pub query_config_override: Option<ReportQueryConfig>,
}

/// Request to create a compliance baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateComplianceBaselineRequest {
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<ComplianceRule>,
    #[serde(default)]
    pub severity_level: SeverityLevel,
}

/// Request to create a drift baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDriftBaselineRequest {
    pub name: String,
    pub description: Option<String>,
    pub node_group_id: Option<Uuid>,
    pub baseline_facts: serde_json::Value,
    pub tolerance_config: Option<DriftToleranceConfig>,
}

// ==================== Report Results ====================

/// Node health report result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthReport {
    pub generated_at: DateTime<Utc>,
    pub time_range: String,
    pub summary: NodeHealthSummary,
    pub by_environment: Option<Vec<EnvironmentHealth>>,
    pub by_group: Option<Vec<GroupHealth>>,
    pub nodes: Option<Vec<NodeHealthDetail>>,
}

/// Summary of node health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthSummary {
    pub total_nodes: i64,
    pub changed_count: i64,
    pub unchanged_count: i64,
    pub failed_count: i64,
    pub noop_count: i64,
    pub unreported_count: i64,
    pub compliance_rate: f64,
}

/// Health breakdown by environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentHealth {
    pub environment: String,
    pub total_nodes: i64,
    pub changed_count: i64,
    pub unchanged_count: i64,
    pub failed_count: i64,
}

/// Health breakdown by node group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupHealth {
    pub group_id: Uuid,
    pub group_name: String,
    pub total_nodes: i64,
    pub changed_count: i64,
    pub unchanged_count: i64,
    pub failed_count: i64,
}

/// Detailed health info for a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthDetail {
    pub certname: String,
    pub environment: Option<String>,
    pub status: String,
    pub last_report_at: Option<DateTime<Utc>>,
    pub failed_resources: Option<i64>,
    pub changed_resources: Option<i64>,
}

/// Compliance report result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub generated_at: DateTime<Utc>,
    pub baseline_name: String,
    pub summary: ComplianceSummary,
    pub by_severity: Vec<SeverityBreakdown>,
    pub violations: Vec<ComplianceViolation>,
}

/// Summary of compliance status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceSummary {
    pub total_nodes: i64,
    pub compliant_nodes: i64,
    pub non_compliant_nodes: i64,
    pub compliance_rate: f64,
    pub total_violations: i64,
}

/// Breakdown by severity level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityBreakdown {
    pub severity: SeverityLevel,
    pub violation_count: i64,
    pub affected_nodes: i64,
}

/// A compliance violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceViolation {
    pub certname: String,
    pub rule_id: String,
    pub rule_name: String,
    pub fact_name: String,
    pub expected_value: serde_json::Value,
    pub actual_value: serde_json::Value,
    pub severity: SeverityLevel,
}

/// Change tracking report result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeTrackingReport {
    pub generated_at: DateTime<Utc>,
    pub time_range: String,
    pub summary: ChangeSummary,
    pub changes_by_type: Vec<ChangeTypeBreakdown>,
    pub changes: Vec<ChangeDetail>,
}

/// Summary of changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub total_changes: i64,
    pub nodes_affected: i64,
    pub resources_changed: i64,
    pub resources_failed: i64,
}

/// Breakdown by resource type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeTypeBreakdown {
    pub resource_type: String,
    pub change_count: i64,
}

/// Detail of a single change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeDetail {
    pub certname: String,
    pub report_time: DateTime<Utc>,
    pub resource_type: String,
    pub resource_title: String,
    pub property: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub status: String,
}

/// Drift detection report result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub generated_at: DateTime<Utc>,
    pub baseline_name: String,
    pub summary: DriftSummary,
    pub drifted_nodes: Vec<DriftedNode>,
}

/// Summary of drift detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftSummary {
    pub total_nodes: i64,
    pub nodes_with_drift: i64,
    pub nodes_without_drift: i64,
    pub drift_rate: f64,
    pub total_drifted_facts: i64,
}

/// A node with configuration drift
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftedNode {
    pub certname: String,
    pub drift_count: i64,
    pub drifted_facts: Vec<DriftedFact>,
}

/// A fact that has drifted from baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftedFact {
    pub fact_name: String,
    pub baseline_value: serde_json::Value,
    pub current_value: serde_json::Value,
    pub drift_severity: SeverityLevel,
}

/// Generic report result wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "report_type")]
pub enum ReportResult {
    #[serde(rename = "node_health")]
    NodeHealth(NodeHealthReport),
    #[serde(rename = "compliance")]
    Compliance(ComplianceReport),
    #[serde(rename = "change_tracking")]
    ChangeTracking(ChangeTrackingReport),
    #[serde(rename = "drift_detection")]
    DriftDetection(DriftReport),
    #[serde(rename = "custom")]
    Custom(serde_json::Value),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_type_serialization() {
        let report_type = ReportType::NodeHealth;
        let json = serde_json::to_string(&report_type).unwrap();
        assert_eq!(json, "\"node_health\"");

        let deserialized: ReportType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ReportType::NodeHealth);
    }

    #[test]
    fn test_output_format() {
        assert_eq!(OutputFormat::Json.content_type(), "application/json");
        assert_eq!(OutputFormat::Csv.content_type(), "text/csv");
        assert_eq!(OutputFormat::Pdf.content_type(), "application/pdf");
    }

    #[test]
    fn test_severity_level() {
        assert_eq!(SeverityLevel::Critical.as_str(), "critical");
        assert_eq!(SeverityLevel::from_str("high"), Some(SeverityLevel::High));
        assert_eq!(SeverityLevel::from_str("unknown"), None);
    }

    #[test]
    fn test_query_config_default() {
        let config = ReportQueryConfig::default();
        assert!(config.time_range.is_none());
        assert!(!config.include_resources);
        assert!(!config.ignore_volatile_facts);
    }

    #[test]
    fn test_query_config_serialization() {
        let config = ReportQueryConfig {
            time_range: Some("24h".to_string()),
            status_filter: Some(vec!["failed".to_string()]),
            include_error_details: true,
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"time_range\":\"24h\""));
        assert!(json.contains("\"include_error_details\":true"));
    }
}
