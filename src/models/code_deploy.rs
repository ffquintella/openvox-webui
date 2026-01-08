//! Code Deploy data models
//!
//! Models for Git-based environment management similar to Puppet Code Manager.
//! Supports multiple repositories, environment discovery from branches,
//! and integration with r10k for deployment.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Deployment status values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    /// Waiting for approval
    #[default]
    Pending,
    /// Approved, queued for deployment
    Approved,
    /// Rejected by approver
    Rejected,
    /// r10k deployment in progress
    Deploying,
    /// Deployment completed successfully
    Success,
    /// Deployment failed
    Failed,
    /// Deployment was cancelled
    Cancelled,
}

impl DeploymentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentStatus::Pending => "pending",
            DeploymentStatus::Approved => "approved",
            DeploymentStatus::Rejected => "rejected",
            DeploymentStatus::Deploying => "deploying",
            DeploymentStatus::Success => "success",
            DeploymentStatus::Failed => "failed",
            DeploymentStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(DeploymentStatus::Pending),
            "approved" => Some(DeploymentStatus::Approved),
            "rejected" => Some(DeploymentStatus::Rejected),
            "deploying" => Some(DeploymentStatus::Deploying),
            "success" => Some(DeploymentStatus::Success),
            "failed" => Some(DeploymentStatus::Failed),
            "cancelled" => Some(DeploymentStatus::Cancelled),
            _ => None,
        }
    }

    /// Check if this status represents a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DeploymentStatus::Success
                | DeploymentStatus::Failed
                | DeploymentStatus::Rejected
                | DeploymentStatus::Cancelled
        )
    }

    /// Check if this deployment can be retried
    pub fn can_retry(&self) -> bool {
        matches!(self, DeploymentStatus::Failed | DeploymentStatus::Rejected)
    }
}

/// SSH key for Git authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSshKey {
    pub id: Uuid,
    pub name: String,
    pub public_key: String,
    /// Private key is encrypted at rest - never serialize to API responses
    #[serde(skip_serializing)]
    pub private_key_encrypted: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// API response for SSH key (excludes private key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSshKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<CodeSshKey> for CodeSshKeyResponse {
    fn from(key: CodeSshKey) -> Self {
        Self {
            id: key.id,
            name: key.name,
            public_key: key.public_key,
            created_at: key.created_at,
            updated_at: key.updated_at,
        }
    }
}

/// Request to create a new SSH key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSshKeyRequest {
    pub name: String,
    /// Private key in PEM format
    pub private_key: String,
}

/// Authentication type for Git repositories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    /// SSH key authentication
    Ssh,
    /// Personal Access Token (for GitHub, GitLab, etc.)
    Pat,
    /// No authentication (public repositories)
    None,
}

impl AuthType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthType::Ssh => "ssh",
            AuthType::Pat => "pat",
            AuthType::None => "none",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ssh" => Some(AuthType::Ssh),
            "pat" => Some(AuthType::Pat),
            "none" => Some(AuthType::None),
            _ => None,
        }
    }
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::Ssh
    }
}

/// Git repository configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRepository {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    /// Glob pattern for branch filtering (e.g., "*", "feature/*", "production")
    pub branch_pattern: String,
    /// Authentication type
    #[serde(default)]
    pub auth_type: AuthType,
    /// SSH key ID (used when auth_type = ssh)
    pub ssh_key_id: Option<Uuid>,
    /// Encrypted GitHub PAT (used when auth_type = pat)
    #[serde(skip_serializing)]
    pub github_pat_encrypted: Option<String>,
    /// Webhook secret for verifying incoming webhooks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret: Option<String>,
    /// Polling interval in seconds (0 = disabled)
    pub poll_interval_seconds: i32,
    /// Whether this is the main control repository
    pub is_control_repo: bool,
    /// Last error message if sync failed
    pub last_error: Option<String>,
    /// When the last error occurred
    pub last_error_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// API response for repository (may include additional computed fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRepositoryResponse {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub branch_pattern: String,
    pub auth_type: AuthType,
    pub ssh_key_id: Option<Uuid>,
    pub ssh_key_name: Option<String>,
    /// Indicates if PAT is configured (true/false, never exposes actual token)
    pub has_pat: bool,
    /// Webhook URL for this repository
    pub webhook_url: Option<String>,
    pub poll_interval_seconds: i32,
    pub is_control_repo: bool,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub environment_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRepositoryRequest {
    pub name: String,
    pub url: String,
    #[serde(default = "default_branch_pattern")]
    pub branch_pattern: String,
    #[serde(default)]
    pub auth_type: AuthType,
    /// SSH key ID (required if auth_type = ssh)
    pub ssh_key_id: Option<Uuid>,
    /// GitHub Personal Access Token (required if auth_type = pat)
    pub github_pat: Option<String>,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: i32,
    #[serde(default)]
    pub is_control_repo: bool,
}

fn default_branch_pattern() -> String {
    "*".to_string()
}

fn default_poll_interval() -> i32 {
    300
}

/// Request to update a repository
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRepositoryRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub branch_pattern: Option<String>,
    pub auth_type: Option<AuthType>,
    pub ssh_key_id: Option<Uuid>,
    /// Set to true to clear the SSH key
    #[serde(default)]
    pub clear_ssh_key: bool,
    /// New GitHub PAT to set (if auth_type = pat)
    pub github_pat: Option<String>,
    /// Set to true to clear the PAT
    #[serde(default)]
    pub clear_github_pat: bool,
    pub poll_interval_seconds: Option<i32>,
    pub is_control_repo: Option<bool>,
    /// Regenerate the webhook secret
    #[serde(default)]
    pub regenerate_webhook_secret: bool,
}

/// Code environment discovered from a Git branch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEnvironment {
    pub id: Uuid,
    pub repository_id: Uuid,
    /// Environment name (usually matches branch name)
    pub name: String,
    /// Git branch name
    pub branch: String,
    /// Current deployed commit SHA
    pub current_commit: Option<String>,
    pub current_commit_message: Option<String>,
    pub current_commit_author: Option<String>,
    pub current_commit_date: Option<DateTime<Utc>>,
    /// When the environment was last synced from Git
    pub last_synced_at: Option<DateTime<Utc>>,
    /// Auto-deploy on push (no approval required)
    pub auto_deploy: bool,
    /// Requires manual approval before deployment
    pub requires_approval: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// API response for environment (may include additional computed fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEnvironmentResponse {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub repository_name: String,
    pub name: String,
    pub branch: String,
    pub current_commit: Option<String>,
    pub current_commit_message: Option<String>,
    pub current_commit_author: Option<String>,
    pub current_commit_date: Option<DateTime<Utc>>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub auto_deploy: bool,
    pub requires_approval: bool,
    /// Pending deployment awaiting approval (if any)
    pub pending_deployment: Option<CodeDeploymentSummary>,
    /// Latest deployment status
    pub latest_deployment_status: Option<DeploymentStatus>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to update environment settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateEnvironmentRequest {
    pub auto_deploy: Option<bool>,
    pub requires_approval: Option<bool>,
}

/// Code deployment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeDeployment {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub commit_sha: String,
    pub commit_message: Option<String>,
    pub commit_author: Option<String>,
    pub status: DeploymentStatus,
    /// User who triggered the deployment (None for auto-triggered)
    pub requested_by: Option<Uuid>,
    /// User who approved/rejected the deployment
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    /// When r10k started
    pub started_at: Option<DateTime<Utc>>,
    /// When r10k completed
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    /// Full r10k output
    pub r10k_output: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Summary view of a deployment (for embedding in other responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeDeploymentSummary {
    pub id: Uuid,
    pub commit_sha: String,
    pub commit_message: Option<String>,
    pub status: DeploymentStatus,
    pub created_at: DateTime<Utc>,
}

impl From<&CodeDeployment> for CodeDeploymentSummary {
    fn from(d: &CodeDeployment) -> Self {
        Self {
            id: d.id,
            commit_sha: d.commit_sha.clone(),
            commit_message: d.commit_message.clone(),
            status: d.status,
            created_at: d.created_at,
        }
    }
}

/// API response for deployment (includes related data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeDeploymentResponse {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub environment_name: String,
    pub repository_name: String,
    pub commit_sha: String,
    pub commit_message: Option<String>,
    pub commit_author: Option<String>,
    pub status: DeploymentStatus,
    pub requested_by: Option<Uuid>,
    pub requested_by_username: Option<String>,
    pub approved_by: Option<Uuid>,
    pub approved_by_username: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub error_message: Option<String>,
    pub r10k_output: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to trigger a new deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDeploymentRequest {
    pub environment_id: Uuid,
    /// Optional specific commit SHA (defaults to latest)
    pub commit_sha: Option<String>,
}

/// Request to approve a deployment
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApproveDeploymentRequest {
    /// Optional comment
    pub comment: Option<String>,
}

/// Request to reject a deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectDeploymentRequest {
    pub reason: String,
}

/// Query parameters for listing deployments
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListDeploymentsQuery {
    pub environment_id: Option<Uuid>,
    pub repository_id: Option<Uuid>,
    pub status: Option<DeploymentStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Query parameters for listing environments
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListEnvironmentsQuery {
    pub repository_id: Option<Uuid>,
    pub auto_deploy: Option<bool>,
    pub has_pending: Option<bool>,
}

/// Webhook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    Push,
    Create,
    Delete,
}

/// Parsed Git webhook payload (normalized across providers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitWebhookPayload {
    pub event_type: WebhookEventType,
    pub repository_url: String,
    pub branch: String,
    pub commit_sha: String,
    pub commit_message: Option<String>,
    pub commit_author: Option<String>,
    pub sender: Option<String>,
}

/// Webhook provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebhookProvider {
    GitHub,
    GitLab,
    Bitbucket,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_status_serialization() {
        let status = DeploymentStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending\"");

        let status: DeploymentStatus = serde_json::from_str("\"deploying\"").unwrap();
        assert_eq!(status, DeploymentStatus::Deploying);
    }

    #[test]
    fn test_deployment_status_is_terminal() {
        assert!(!DeploymentStatus::Pending.is_terminal());
        assert!(!DeploymentStatus::Approved.is_terminal());
        assert!(!DeploymentStatus::Deploying.is_terminal());
        assert!(DeploymentStatus::Success.is_terminal());
        assert!(DeploymentStatus::Failed.is_terminal());
        assert!(DeploymentStatus::Rejected.is_terminal());
        assert!(DeploymentStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_deployment_status_can_retry() {
        assert!(!DeploymentStatus::Pending.can_retry());
        assert!(!DeploymentStatus::Success.can_retry());
        assert!(DeploymentStatus::Failed.can_retry());
        assert!(DeploymentStatus::Rejected.can_retry());
    }

    #[test]
    fn test_ssh_key_response_excludes_private_key() {
        let key = CodeSshKey {
            id: Uuid::new_v4(),
            name: "test-key".to_string(),
            public_key: "ssh-rsa AAAA...".to_string(),
            private_key_encrypted: "encrypted-data".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let response = CodeSshKeyResponse::from(key);
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("encrypted-data"));
        assert!(!json.contains("private_key"));
    }
}
