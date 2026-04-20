//! CVE vulnerability models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// -- Feed source types --

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CveFeedType {
    NvdJson,
    CisaKev,
    Custom,
}

impl CveFeedType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NvdJson => "nvd_json",
            Self::CisaKev => "cisa_kev",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "nvd_json" => Some(Self::NvdJson),
            "cisa_kev" => Some(Self::CisaKev),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveFeedSource {
    pub id: String,
    pub name: String,
    pub feed_url: String,
    pub feed_type: CveFeedType,
    pub enabled: bool,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_sync_status: String,
    pub last_sync_error: Option<String>,
    pub sync_interval_secs: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCveFeedSourceRequest {
    pub name: String,
    pub feed_url: String,
    pub feed_type: CveFeedType,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,
}

fn default_true() -> bool {
    true
}

fn default_sync_interval() -> u64 {
    3600
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCveFeedSourceRequest {
    pub name: Option<String>,
    pub feed_url: Option<String>,
    pub enabled: Option<bool>,
    pub sync_interval_secs: Option<u64>,
}

// -- CVE entry types --

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CveSeverity {
    Critical,
    High,
    Medium,
    Low,
    Unknown,
}

impl CveSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "critical" | "CRITICAL" => Self::Critical,
            "high" | "HIGH" => Self::High,
            "medium" | "MEDIUM" => Self::Medium,
            "low" | "LOW" => Self::Low,
            _ => Self::Unknown,
        }
    }

    pub fn from_cvss(score: f64) -> Self {
        if score >= 9.0 {
            Self::Critical
        } else if score >= 7.0 {
            Self::High
        } else if score >= 4.0 {
            Self::Medium
        } else if score > 0.0 {
            Self::Low
        } else {
            Self::Unknown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveEntry {
    pub id: String,
    pub feed_source_id: String,
    pub description: Option<String>,
    pub severity: CveSeverity,
    pub cvss_score: Option<f64>,
    pub cvss_vector: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub references: Vec<String>,
    pub affected_products: Vec<String>,
    pub is_kev: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvePackageMatch {
    pub id: String,
    pub cve_id: String,
    pub package_name: String,
    pub version_start: Option<String>,
    pub version_end: Option<String>,
    pub platform_family: Option<String>,
    pub created_at: DateTime<Utc>,
}

// -- Host vulnerability types --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostVulnerabilityMatch {
    pub id: String,
    pub certname: String,
    pub cve_id: String,
    pub package_name: String,
    pub installed_version: String,
    pub severity: CveSeverity,
    pub cvss_score: Option<f64>,
    pub is_kev: bool,
    pub matched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeVulnerabilitySummary {
    pub certname: String,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub kev_count: usize,
    pub total_count: usize,
    pub last_checked_at: Option<DateTime<Utc>>,
}

// -- Dashboard / reporting types --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityDashboardReport {
    pub total_vulnerable_nodes: usize,
    pub total_cves_matched: usize,
    pub severity_distribution: Vec<SeverityDistributionPoint>,
    pub top_cves: Vec<TopCveItem>,
    pub top_vulnerable_nodes: Vec<TopVulnerableNode>,
    pub kev_count: usize,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityDistributionPoint {
    pub severity: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopCveItem {
    pub cve_id: String,
    pub severity: CveSeverity,
    pub cvss_score: Option<f64>,
    pub affected_nodes: usize,
    pub description: Option<String>,
    pub is_kev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopVulnerableNode {
    pub certname: String,
    pub total_vulns: usize,
    pub critical_count: usize,
    pub kev_count: usize,
}

// -- CVE search / query types --

#[derive(Debug, Clone, Deserialize)]
pub struct CveSearchQuery {
    pub query: Option<String>,
    pub severity: Option<String>,
    pub is_kev: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VulnerableNodesQuery {
    pub severity: Option<String>,
    pub limit: Option<usize>,
}

// -- CVE detail with affected nodes --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveDetailResponse {
    pub entry: CveEntry,
    pub affected_nodes: Vec<CveAffectedNode>,
    pub package_matches: Vec<CvePackageMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveAffectedNode {
    pub certname: String,
    pub package_name: String,
    pub installed_version: String,
    pub matched_at: DateTime<Utc>,
}

// -- Feed sync result --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSyncResult {
    pub feed_id: String,
    pub entries_processed: usize,
    pub entries_new: usize,
    pub entries_updated: usize,
    pub package_matches_created: usize,
    pub errors: Vec<String>,
    pub synced_at: DateTime<Utc>,
}

// -- Update preview types --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePreviewRequest {
    pub operation_type: super::UpdateOperationType,
    #[serde(default)]
    pub package_names: Vec<String>,
    #[serde(default)]
    pub certnames: Vec<String>,
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePreviewResponse {
    pub targets: Vec<UpdatePreviewTarget>,
    pub total_packages: usize,
    pub total_nodes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePreviewTarget {
    pub certname: String,
    pub packages_to_update: Vec<UpdatePreviewPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePreviewPackage {
    pub name: String,
    pub from_version: String,
    pub to_version: String,
    pub cve_ids: Vec<String>,
}
