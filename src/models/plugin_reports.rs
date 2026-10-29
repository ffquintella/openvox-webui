//! Plugin reports (M25, T90): report types, export formats, annotations on
//! Puppet run reports and datasets declared in `plugin.yaml`, the structured
//! result of a plugin report and the API payloads.
//!
//! See docs/architecture/PLUGIN_PLATFORM.md sections 12.3, 12.4 and 17.7.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Requires;

/// Content types a plugin export format may declare. Never HTML, SVG or
/// script: an export is always sent as an attachment with `nosniff`.
pub const EXPORT_CONTENT_TYPES: &[&str] = &[
    "application/json",
    "application/sarif+json",
    "application/pdf",
    "application/xml",
    "application/zip",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.oasis.opendocument.spreadsheet",
    "text/csv",
    "text/plain",
];

/// Severities a report annotation may carry.
pub const ANNOTATION_SEVERITIES: &[&str] = &["info", "low", "medium", "high", "critical"];

// ============================================================================
// Manifest
// ============================================================================

/// `contributes.report_types[]`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportTypeDecl {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema (subset) of the parameters, rendered by the host as a form.
    #[serde(default)]
    pub params_schema: Option<Value>,
    /// `generate` handler called with `{type_id, params, job_id, requested_by}`.
    pub handler: String,
    #[serde(default = "default_true")]
    pub schedulable: bool,
    #[serde(default)]
    pub requires: Option<Requires>,
}

/// `contributes.export_formats[]`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportFormatDecl {
    pub id: String,
    pub label: String,
    pub content_type: String,
    pub extension: String,
    /// `render` handler called with `{format, result, options}`.
    pub handler: String,
}

/// `contributes.report_annotations[]`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationDecl {
    pub key: String,
    /// `string`, `integer`, `float` or `bool`
    #[serde(rename = "type")]
    pub value_type: String,
    pub label: String,
    /// `list_column` and/or `detail_panel`
    #[serde(default = "default_show_in")]
    pub show_in: Vec<String>,
    #[serde(default)]
    pub sortable: bool,
    #[serde(default)]
    pub filterable: bool,
}

/// `contributes.datasets[]`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetDecl {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub schema: DatasetSchema,
    /// Column whose value identifies a row (default: the first column).
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub requires: Option<Requires>,
}

impl DatasetDecl {
    pub fn key_column(&self) -> Option<&str> {
        self.key
            .as_deref()
            .or_else(|| self.schema.columns.first().map(|c| c.name.as_str()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetSchema {
    pub columns: Vec<DatasetColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetColumn {
    pub name: String,
    /// `string`, `integer`, `float` or `bool`
    #[serde(rename = "type")]
    pub column_type: String,
    #[serde(default)]
    pub label: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_show_in() -> Vec<String> {
    vec!["detail_panel".to_string()]
}

/// Whether `v` fits a scalar type name used by annotations and datasets.
pub fn scalar_matches(type_name: &str, v: &Value) -> bool {
    match type_name {
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "float" => v.is_number(),
        "bool" => v.is_boolean(),
        _ => false,
    }
}

// ============================================================================
// Structured result
// ============================================================================

/// Result of a plugin report: typed sections the host renders and exports
/// itself, never HTML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginReportResult {
    /// `plugin:<plugin-id>:<type-id>`
    pub type_id: String,
    pub plugin_id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub params: Value,
    pub row_count: u64,
    /// Rows were cut at the row limit.
    #[serde(default)]
    pub truncated: bool,
    pub sections: Vec<ReportSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReportSection {
    Kpi {
        id: String,
        #[serde(default)]
        title: Option<String>,
        items: Vec<KpiItem>,
    },
    Table {
        id: String,
        #[serde(default)]
        title: Option<String>,
        columns: Vec<TableColumn>,
        #[serde(default)]
        rows: Vec<serde_json::Map<String, Value>>,
    },
    Chart {
        id: String,
        #[serde(default)]
        title: Option<String>,
        chart: String,
        series: Vec<ChartSeries>,
    },
    Text {
        id: String,
        #[serde(default)]
        title: Option<String>,
        text: String,
    },
}

impl ReportSection {
    pub fn id(&self) -> &str {
        match self {
            ReportSection::Kpi { id, .. }
            | ReportSection::Table { id, .. }
            | ReportSection::Chart { id, .. }
            | ReportSection::Text { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiItem {
    pub label: String,
    pub value: Value,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    pub key: String,
    pub label: String,
    #[serde(default, rename = "type")]
    pub column_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSeries {
    pub name: String,
    pub points: Vec<ChartPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartPoint {
    pub x: Value,
    pub y: f64,
}

// ============================================================================
// API payloads
// ============================================================================

/// A report type in the catalog: core or provided by an enabled plugin.
#[derive(Debug, Clone, Serialize)]
pub struct ReportTypeInfo {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    /// `core` or `plugin:<id>`
    pub source: String,
    pub params_schema: Option<Value>,
    pub schedulable: bool,
}

/// An export format in the catalog: core or provided by an enabled plugin.
#[derive(Debug, Clone, Serialize)]
pub struct ExportFormatInfo {
    pub id: String,
    pub label: String,
    pub content_type: String,
    pub extension: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnnotationColumn {
    pub plugin_id: String,
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub show_in: Vec<String>,
    pub sortable: bool,
    pub filterable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportAnnotation {
    pub plugin_id: String,
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub value: Value,
    pub severity: Option<String>,
    pub created_at: DateTime<Utc>,
    pub show_in: Vec<String>,
}

/// One stored annotation (`report_annotations`).
#[derive(Debug, Clone)]
pub struct AnnotationRecord {
    pub report_hash: String,
    pub plugin_id: String,
    pub key: String,
    pub value: Value,
    pub severity: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationLookupRequest {
    pub hashes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatasetInfo {
    /// `plugin:<plugin-id>:<dataset-id>`
    pub id: String,
    pub plugin_id: String,
    pub dataset_id: String,
    pub label: String,
    pub description: Option<String>,
    pub columns: Vec<DatasetColumn>,
    pub rows: u64,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DatasetQueryRequest {
    pub dataset: String,
    #[serde(default)]
    pub filter: serde_json::Map<String, Value>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatasetQueryResult {
    pub dataset: String,
    pub columns: Vec<DatasetColumn>,
    pub rows: Vec<Value>,
    pub total: u64,
}

/// `plugin:<plugin-id>:<local-id>` split into its parts.
pub fn parse_plugin_ref(s: &str) -> Option<(&str, &str)> {
    let rest = s.strip_prefix("plugin:")?;
    let (p, l) = rest.split_once(':')?;
    (!p.is_empty() && !l.is_empty() && !l.contains(':')).then_some((p, l))
}
