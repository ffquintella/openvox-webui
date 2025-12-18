//! Report data model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a Puppet run report
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Report {
    /// Report hash (unique identifier)
    pub hash: String,

    /// Certificate name of the node
    pub certname: String,

    /// Puppet version
    pub puppet_version: Option<String>,

    /// Report format version
    pub report_format: Option<u32>,

    /// Configuration version
    pub configuration_version: Option<String>,

    /// Start time of the Puppet run
    pub start_time: Option<DateTime<Utc>>,

    /// End time of the Puppet run
    pub end_time: Option<DateTime<Utc>>,

    /// Producer timestamp
    pub producer_timestamp: Option<DateTime<Utc>>,

    /// Producer (Puppet server)
    pub producer: Option<String>,

    /// Transaction UUID
    pub transaction_uuid: Option<String>,

    /// Status of the report
    pub status: Option<ReportStatus>,

    /// Whether there were corrective changes
    pub corrective_change: Option<bool>,

    /// Whether this is a noop run
    pub noop: Option<bool>,

    /// Whether noop was pending
    pub noop_pending: Option<bool>,

    /// Environment
    pub environment: Option<String>,

    /// Catalog UUID
    pub catalog_uuid: Option<String>,

    /// Code ID
    pub code_id: Option<String>,

    /// Cached catalog status
    pub cached_catalog_status: Option<String>,

    /// Report type (e.g., "agent")
    #[serde(rename = "type")]
    pub report_type: Option<String>,

    /// Job ID
    pub job_id: Option<String>,

    /// Receive time
    pub receive_time: Option<DateTime<Utc>>,

    /// Report metrics (raw PuppetDB format)
    pub metrics: Option<PuppetDbDataRef>,

    /// Resource events (raw PuppetDB format)
    pub resource_events: Option<PuppetDbDataRef>,

    /// Logs (raw PuppetDB format)
    pub logs: Option<PuppetDbDataRef>,
}

/// Reference to PuppetDB data with href for lazy loading
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PuppetDbDataRef {
    /// Data can be null or an array in PuppetDB responses
    #[serde(default)]
    pub data: Option<Vec<serde_json::Value>>,
    pub href: Option<String>,
}

/// Report status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    #[default]
    Changed,
    Unchanged,
    Failed,
}

/// Report metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportMetrics {
    pub resources: ResourceMetrics,
    pub time: TimeMetrics,
    pub changes: u32,
    pub events: EventMetrics,
}

/// Resource metrics from a report
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceMetrics {
    pub total: u32,
    pub changed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub scheduled: u32,
    pub out_of_sync: u32,
    pub restarted: u32,
    pub failed_to_restart: u32,
    pub corrective_change: u32,
}

/// Time metrics from a report
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimeMetrics {
    pub total: f64,
    pub config_retrieval: f64,
    pub catalog_application: f64,
    pub fact_generation: f64,
}

/// Event metrics from a report
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventMetrics {
    pub total: u32,
    pub success: u32,
    pub failure: u32,
    pub noop: u32,
}

/// Resource event from a report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEvent {
    pub certname: String,
    pub report: String,
    pub resource_type: String,
    pub resource_title: String,
    pub property: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub message: Option<String>,
    pub status: EventStatus,
    pub timestamp: DateTime<Utc>,
    pub containment_path: Vec<String>,
    pub corrective_change: bool,
}

/// Event status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    Success,
    Failure,
    Noop,
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_default() {
        let report = Report::default();
        assert!(report.hash.is_empty());
        assert_eq!(report.status, None);
    }

    #[test]
    fn test_report_status_serialization() {
        let status = ReportStatus::Failed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"failed\"");
    }

    #[test]
    fn test_parse_puppetdb_report() {
        let json = r#"[{
            "catalog_uuid": "68722b66-3561-4c94-bc34-31bc91bd2e5b",
            "receive_time": "2025-12-18T11:49:52.463Z",
            "producer": "segdc1vpr0018.fgv.br",
            "hash": "3db4c351c30135c484b128f7a408222d2ad18e77",
            "transaction_uuid": "14958851-5cbf-483e-93a0-3b64f34296f8",
            "puppet_version": "8.24.1",
            "noop": false,
            "corrective_change": null,
            "logs": {"data": [], "href": "/pdb/query/v4/reports/xxx/logs"},
            "report_format": 12,
            "start_time": "2025-12-18T11:49:49.671Z",
            "producer_timestamp": "2025-12-18T11:49:52.451Z",
            "type": "agent",
            "cached_catalog_status": "not_used",
            "end_time": "2025-12-18T11:49:52.406Z",
            "resource_events": {"data": null, "href": "/pdb/query/v4/reports/xxx/events"},
            "status": "failed",
            "configuration_version": "1766058591",
            "environment": "pserver",
            "code_id": null,
            "noop_pending": false,
            "certname": "segdc1vpr0018.fgv.br",
            "metrics": {"data": [], "href": "/pdb/query/v4/reports/xxx/metrics"},
            "job_id": null
        }]"#;

        let reports: Vec<Report> = serde_json::from_str(json).expect("Failed to parse report");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].certname, "segdc1vpr0018.fgv.br");
        assert_eq!(reports[0].status, Some(ReportStatus::Failed));
        // Verify null data is handled correctly
        assert!(reports[0].resource_events.as_ref().unwrap().data.is_none());
    }
}
