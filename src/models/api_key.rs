//! API key models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub role_ids: Vec<Uuid>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// Optional user override (super_admin only)
    pub user_id: Option<Uuid>,
    /// Optional organization override (super_admin only)
    pub organization_id: Option<Uuid>,
    /// Optional role scope; defaults to caller's roles
    pub role_ids: Option<Vec<Uuid>>,
    /// Optional expiry (RFC3339 timestamp)
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateApiKeyResponse {
    #[serde(flatten)]
    pub api_key: ApiKey,
    /// Plaintext API key (only returned on creation)
    pub key: String,
}
