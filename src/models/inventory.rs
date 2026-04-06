//! Inventory data models for Phase 10.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryPayload {
    pub collector_version: String,
    pub collected_at: Option<DateTime<Utc>>,
    #[serde(default = "default_full_snapshot")]
    pub is_full_snapshot: bool,
    pub os: HostOsInventory,
    #[serde(default)]
    pub packages: Vec<HostPackageInventoryItem>,
    #[serde(default)]
    pub applications: Vec<HostApplicationInventoryItem>,
    #[serde(default)]
    pub websites: Vec<HostWebInventoryItem>,
    #[serde(default)]
    pub runtimes: Vec<HostRuntimeInventoryItem>,
    #[serde(default)]
    pub containers: Vec<HostContainerInventoryItem>,
    #[serde(default)]
    pub users: Vec<HostUserInventoryItem>,
    #[serde(default)]
    pub repositories: Vec<HostRepositoryConfig>,
}

fn default_full_snapshot() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostOsInventory {
    pub os_family: String,
    pub distribution: String,
    pub edition: Option<String>,
    pub architecture: Option<String>,
    pub kernel_version: Option<String>,
    pub os_version: String,
    pub patch_level: Option<String>,
    pub package_manager: Option<String>,
    pub update_channel: Option<String>,
    pub last_inventory_at: Option<DateTime<Utc>>,
    pub last_successful_update_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPackageInventoryItem {
    pub name: String,
    pub epoch: Option<String>,
    pub version: String,
    pub release: Option<String>,
    pub architecture: Option<String>,
    pub repository_source: Option<String>,
    pub install_path: Option<String>,
    pub install_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostApplicationInventoryItem {
    pub name: String,
    pub publisher: Option<String>,
    pub version: String,
    pub architecture: Option<String>,
    pub install_scope: Option<String>,
    pub install_path: Option<String>,
    pub application_type: Option<String>,
    pub bundle_identifier: Option<String>,
    pub uninstall_identity: Option<String>,
    pub install_date: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostWebInventoryItem {
    pub server_type: String,
    pub site_name: String,
    #[serde(default)]
    pub bindings: Vec<String>,
    pub document_root: Option<String>,
    pub application_pool: Option<String>,
    pub tls_certificate_reference: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRuntimeInventoryItem {
    pub runtime_type: String,
    pub runtime_name: String,
    pub runtime_version: Option<String>,
    pub install_path: Option<String>,
    pub management_endpoint: Option<String>,
    #[serde(default)]
    pub deployed_units: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostContainerInventoryItem {
    pub container_id: String,
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
    pub status: String,
    pub status_detail: Option<String>,
    pub created_at: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub mounts: Vec<String>,
    pub runtime_type: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostUserInventoryItem {
    pub username: String,
    pub uid: Option<i64>,
    pub sid: Option<String>,
    pub gid: Option<i64>,
    pub home_directory: Option<String>,
    pub shell: Option<String>,
    pub user_type: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    pub last_login: Option<String>,
    pub locked: Option<bool>,
    pub gecos: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRepositoryConfig {
    pub repo_id: String,
    pub repo_name: Option<String>,
    pub repo_type: String,
    pub base_url: Option<String>,
    pub mirror_list_url: Option<String>,
    pub distribution_path: Option<String>,
    pub components: Option<String>,
    pub architectures: Option<String>,
    #[serde(default = "default_repo_enabled")]
    pub enabled: bool,
    pub gpg_check: Option<bool>,
}

fn default_repo_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetRepositoryConfig {
    pub id: String,
    pub os_family: String,
    pub distribution: String,
    pub os_version_pattern: String,
    pub package_manager: String,
    pub repo_id: String,
    pub repo_name: Option<String>,
    pub repo_type: String,
    pub base_url: Option<String>,
    pub mirror_list_url: Option<String>,
    pub distribution_path: Option<String>,
    pub components: Option<String>,
    pub architectures: Option<String>,
    pub enabled: bool,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_check_status: Option<String>,
    pub last_check_error: Option<String>,
    pub reporting_nodes: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySnapshotSummary {
    pub id: String,
    pub certname: String,
    pub collector_version: String,
    pub collected_at: DateTime<Utc>,
    pub is_full_snapshot: bool,
    pub os_family: String,
    pub distribution: String,
    pub os_version: String,
    pub package_count: usize,
    pub application_count: usize,
    pub website_count: usize,
    pub runtime_count: usize,
    pub container_count: usize,
    pub user_count: usize,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySummary {
    pub certname: String,
    pub os_family: String,
    pub distribution: String,
    pub os_version: String,
    pub patch_level: Option<String>,
    pub architecture: Option<String>,
    pub package_manager: Option<String>,
    pub update_channel: Option<String>,
    pub last_inventory_at: Option<DateTime<Utc>>,
    pub last_successful_update_at: Option<DateTime<Utc>>,
    pub package_count: usize,
    pub application_count: usize,
    pub website_count: usize,
    pub runtime_count: usize,
    pub container_count: usize,
    pub user_count: usize,
    pub collected_at: DateTime<Utc>,
    pub collector_version: String,
    pub is_stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryVersionCatalogEntry {
    pub id: String,
    pub platform_family: String,
    pub distribution: String,
    pub os_version_pattern: Option<String>,
    pub package_manager: Option<String>,
    pub software_type: String,
    pub software_name: String,
    pub repository_source: Option<String>,
    pub latest_version: String,
    pub latest_release: Option<String>,
    pub source_kind: String,
    pub observed_nodes: usize,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutdatedInventoryItem {
    pub software_type: String,
    pub name: String,
    pub installed_version: String,
    pub installed_release: Option<String>,
    pub latest_version: String,
    pub latest_release: Option<String>,
    pub repository_source: Option<String>,
    #[serde(default)]
    pub source_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostUpdateStatus {
    pub certname: String,
    pub snapshot_id: Option<String>,
    pub is_stale: bool,
    pub stale_reason: Option<String>,
    pub outdated_packages: usize,
    pub outdated_applications: usize,
    pub total_packages: usize,
    pub total_applications: usize,
    pub checked_at: DateTime<Utc>,
    pub outdated_items: Vec<OutdatedInventoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryFleetStatusSummary {
    pub total_nodes: usize,
    pub stale_nodes: usize,
    pub nodes_with_inventory: usize,
    pub nodes_without_inventory: usize,
    pub outdated_nodes: usize,
    pub outdated_packages: usize,
    pub outdated_applications: usize,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryDistributionPoint {
    pub label: String,
    pub value: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchAgeBucket {
    pub label: String,
    pub value: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopOutdatedSoftwareItem {
    pub software_type: String,
    pub name: String,
    pub affected_nodes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryDashboardReport {
    pub summary: InventoryFleetStatusSummary,
    pub platform_distribution: Vec<InventoryDistributionPoint>,
    pub os_distribution: Vec<InventoryDistributionPoint>,
    pub update_compliance: Vec<InventoryDistributionPoint>,
    pub patch_age_buckets: Vec<PatchAgeBucket>,
    pub top_outdated_software: Vec<TopOutdatedSoftwareItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutdatedSoftwareNodeDetail {
    pub certname: String,
    pub installed_version: String,
    pub latest_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCategoryNode {
    pub certname: String,
    pub is_stale: bool,
    pub outdated_packages: i64,
    pub outdated_applications: i64,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInventory {
    pub snapshot: InventorySnapshotSummary,
    pub summary: InventorySummary,
    pub update_status: Option<HostUpdateStatus>,
    pub os: HostOsInventory,
    pub packages: Vec<HostPackageInventoryItem>,
    pub applications: Vec<HostApplicationInventoryItem>,
    pub websites: Vec<HostWebInventoryItem>,
    pub runtimes: Vec<HostRuntimeInventoryItem>,
    pub containers: Vec<HostContainerInventoryItem>,
    pub users: Vec<HostUserInventoryItem>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateJobStatus {
    #[default]
    PendingApproval,
    Approved,
    Rejected,
    InProgress,
    Completed,
    CompletedWithFailures,
    Cancelled,
}

impl UpdateJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingApproval => "pending_approval",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::CompletedWithFailures => "completed_with_failures",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending_approval" => Some(Self::PendingApproval),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "completed_with_failures" => Some(Self::CompletedWithFailures),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateTargetStatus {
    #[default]
    PendingApproval,
    Queued,
    Dispatched,
    Succeeded,
    Failed,
    Cancelled,
    Rejected,
}

impl UpdateTargetStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingApproval => "pending_approval",
            Self::Queued => "queued",
            Self::Dispatched => "dispatched",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Rejected => "rejected",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending_approval" => Some(Self::PendingApproval),
            "queued" => Some(Self::Queued),
            "dispatched" => Some(Self::Dispatched),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOperationType {
    #[default]
    PackageUpdate,
    PackageInstall,
    PackageRemove,
    SystemPatch,
    SecurityPatch,
}

impl UpdateOperationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PackageUpdate => "package_update",
            Self::PackageInstall => "package_install",
            Self::PackageRemove => "package_remove",
            Self::SystemPatch => "system_patch",
            Self::SecurityPatch => "security_patch",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "package_update" => Some(Self::PackageUpdate),
            "package_install" => Some(Self::PackageInstall),
            "package_remove" => Some(Self::PackageRemove),
            "system_patch" => Some(Self::SystemPatch),
            "security_patch" => Some(Self::SecurityPatch),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobTarget {
    pub id: String,
    pub certname: String,
    pub status: UpdateTargetStatus,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobResult {
    pub id: String,
    pub target_id: String,
    pub certname: String,
    pub status: UpdateTargetStatus,
    pub summary: Option<String>,
    pub output: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJob {
    pub id: String,
    pub status: UpdateJobStatus,
    pub operation_type: UpdateOperationType,
    pub package_names: Vec<String>,
    pub target_group_id: Option<String>,
    pub target_nodes: Vec<String>,
    pub requires_approval: bool,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub maintenance_window_start: Option<DateTime<Utc>>,
    pub maintenance_window_end: Option<DateTime<Utc>>,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub approval_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub targets: Vec<UpdateJobTarget>,
    pub results: Vec<UpdateJobResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUpdateJobRequest {
    pub operation_type: UpdateOperationType,
    #[serde(default)]
    pub package_names: Vec<String>,
    #[serde(default)]
    pub certnames: Vec<String>,
    pub group_id: Option<String>,
    #[serde(default)]
    pub requires_approval: bool,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub maintenance_window_start: Option<DateTime<Utc>>,
    pub maintenance_window_end: Option<DateTime<Utc>>,
    pub approval_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveUpdateJobRequest {
    pub approved: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePendingUpdateJob {
    pub job_id: String,
    pub target_id: String,
    pub operation_type: UpdateOperationType,
    pub package_names: Vec<String>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub maintenance_window_start: Option<DateTime<Utc>>,
    pub maintenance_window_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitUpdateJobResultRequest {
    pub status: UpdateTargetStatus,
    pub summary: Option<String>,
    pub output: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

// ── Group Update Schedules ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUpdateSchedule {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub description: Option<String>,
    pub schedule_type: String,
    pub cron_expression: Option<String>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub operation_type: UpdateOperationType,
    pub package_names: Vec<String>,
    pub requires_approval: bool,
    pub maintenance_window_start: Option<String>,
    pub maintenance_window_end: Option<String>,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_job_id: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupUpdateScheduleRequest {
    pub name: String,
    pub description: Option<String>,
    pub schedule_type: String,
    pub cron_expression: Option<String>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub operation_type: UpdateOperationType,
    #[serde(default)]
    pub package_names: Vec<String>,
    #[serde(default)]
    pub requires_approval: bool,
    pub maintenance_window_start: Option<String>,
    pub maintenance_window_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGroupUpdateScheduleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cron_expression: Option<String>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub operation_type: Option<UpdateOperationType>,
    pub package_names: Option<Vec<String>>,
    pub requires_approval: Option<bool>,
    pub maintenance_window_start: Option<String>,
    pub maintenance_window_end: Option<String>,
    pub enabled: Option<bool>,
}
