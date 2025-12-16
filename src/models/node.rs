//! Node data model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a node in the infrastructure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Certificate name (unique identifier)
    pub certname: String,

    /// Whether the node is deactivated
    pub deactivated: Option<DateTime<Utc>>,

    /// Whether the node catalog has expired
    pub expired: Option<DateTime<Utc>>,

    /// Timestamp of the most recent catalog
    pub catalog_timestamp: Option<DateTime<Utc>>,

    /// Timestamp of the most recent facts
    pub facts_timestamp: Option<DateTime<Utc>>,

    /// Timestamp of the most recent report
    pub report_timestamp: Option<DateTime<Utc>>,

    /// Environment the node belongs to
    pub catalog_environment: Option<String>,

    /// Environment from facts
    pub facts_environment: Option<String>,

    /// Environment from report
    pub report_environment: Option<String>,

    /// Latest report status
    pub latest_report_status: Option<String>,

    /// Whether the latest report has corrective changes
    pub latest_report_corrective_change: Option<bool>,

    /// Whether the node is cached
    pub cached_catalog_status: Option<String>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            certname: String::new(),
            deactivated: None,
            expired: None,
            catalog_timestamp: None,
            facts_timestamp: None,
            report_timestamp: None,
            catalog_environment: None,
            facts_environment: None,
            report_environment: None,
            latest_report_status: None,
            latest_report_corrective_change: None,
            cached_catalog_status: None,
        }
    }
}

/// Node summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSummary {
    pub certname: String,
    pub status: NodeStatus,
    pub last_report: Option<DateTime<Utc>>,
    pub environment: Option<String>,
}

/// Status of a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Changed,
    Unchanged,
    Failed,
    Unreported,
    Unknown,
}

impl From<Option<&str>> for NodeStatus {
    fn from(s: Option<&str>) -> Self {
        match s {
            Some("changed") => NodeStatus::Changed,
            Some("unchanged") => NodeStatus::Unchanged,
            Some("failed") => NodeStatus::Failed,
            Some("unreported") => NodeStatus::Unreported,
            _ => NodeStatus::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_status_from_string() {
        assert_eq!(NodeStatus::from(Some("changed")), NodeStatus::Changed);
        assert_eq!(NodeStatus::from(Some("failed")), NodeStatus::Failed);
        assert_eq!(NodeStatus::from(None), NodeStatus::Unknown);
    }

    #[test]
    fn test_node_default() {
        let node = Node::default();
        assert!(node.certname.is_empty());
        assert!(node.deactivated.is_none());
    }
}
