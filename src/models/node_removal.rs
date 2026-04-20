//! Node removal tracking models
//!
//! Tracks nodes that are pending removal due to revoked or missing certificates.
//! Nodes are automatically removed after a configurable retention period (default 10 days).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Reason why a node is pending removal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalReason {
    /// Certificate was revoked in Puppet CA
    RevokedCertificate,
    /// Node has no certificate in Puppet CA
    NoCertificate,
    /// Manually marked for removal by an administrator
    Manual,
}

impl RemovalReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemovalReason::RevokedCertificate => "revoked_certificate",
            RemovalReason::NoCertificate => "no_certificate",
            RemovalReason::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "revoked_certificate" => Some(RemovalReason::RevokedCertificate),
            "no_certificate" => Some(RemovalReason::NoCertificate),
            "manual" => Some(RemovalReason::Manual),
            _ => None,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            RemovalReason::RevokedCertificate => "Certificate was revoked",
            RemovalReason::NoCertificate => "No certificate found",
            RemovalReason::Manual => "Manually marked for removal",
        }
    }
}

impl std::fmt::Display for RemovalReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A node that is pending removal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingNodeRemoval {
    pub id: String,
    pub certname: String,
    pub removal_reason: RemovalReason,
    pub marked_at: DateTime<Utc>,
    pub scheduled_removal_at: DateTime<Utc>,
    pub removed_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub marked_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Audit log action for node removal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalAuditAction {
    /// Node was marked for removal
    Marked,
    /// Node was unmarked (removal cancelled)
    Unmarked,
    /// Node was actually removed
    Removed,
    /// Removal deadline was extended
    Extended,
}

impl RemovalAuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemovalAuditAction::Marked => "marked",
            RemovalAuditAction::Unmarked => "unmarked",
            RemovalAuditAction::Removed => "removed",
            RemovalAuditAction::Extended => "extended",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "marked" => Some(RemovalAuditAction::Marked),
            "unmarked" => Some(RemovalAuditAction::Unmarked),
            "removed" => Some(RemovalAuditAction::Removed),
            "extended" => Some(RemovalAuditAction::Extended),
            _ => None,
        }
    }
}

impl std::fmt::Display for RemovalAuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Audit log entry for node removal events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRemovalAudit {
    pub id: String,
    pub certname: String,
    pub action: RemovalAuditAction,
    pub reason: Option<String>,
    pub performed_by: Option<String>,
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Request to mark a node for removal
#[derive(Debug, Clone, Deserialize)]
pub struct MarkNodeForRemovalRequest {
    pub certname: String,
    pub reason: Option<RemovalReason>,
    pub notes: Option<String>,
}

/// Request to unmark a node (cancel pending removal)
#[derive(Debug, Clone, Deserialize)]
pub struct UnmarkNodeRemovalRequest {
    pub certname: String,
    pub notes: Option<String>,
}

/// Request to extend the removal deadline
#[derive(Debug, Clone, Deserialize)]
pub struct ExtendRemovalDeadlineRequest {
    pub certname: String,
    /// Number of days to extend from now
    pub extend_days: u32,
    pub notes: Option<String>,
}

/// Response for pending node removal
#[derive(Debug, Clone, Serialize)]
pub struct PendingNodeRemovalResponse {
    pub id: String,
    pub certname: String,
    pub removal_reason: RemovalReason,
    pub removal_reason_description: String,
    pub marked_at: DateTime<Utc>,
    pub scheduled_removal_at: DateTime<Utc>,
    pub days_until_removal: i64,
    pub removed_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub marked_by: Option<String>,
}

impl From<PendingNodeRemoval> for PendingNodeRemovalResponse {
    fn from(removal: PendingNodeRemoval) -> Self {
        let now = Utc::now();
        let days_until_removal = (removal.scheduled_removal_at - now).num_days();

        Self {
            id: removal.id,
            certname: removal.certname,
            removal_reason_description: removal.removal_reason.description().to_string(),
            removal_reason: removal.removal_reason,
            marked_at: removal.marked_at,
            scheduled_removal_at: removal.scheduled_removal_at,
            days_until_removal,
            removed_at: removal.removed_at,
            notes: removal.notes,
            marked_by: removal.marked_by,
        }
    }
}

/// Summary statistics for pending removals
#[derive(Debug, Clone, Serialize)]
pub struct PendingRemovalStats {
    pub total_pending: i64,
    pub revoked_certificates: i64,
    pub no_certificates: i64,
    pub manual: i64,
    pub due_today: i64,
    pub due_this_week: i64,
}
