//! Organization (tenant) model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEFAULT_ORGANIZATION_ID: &str = "00000000-0000-0000-0000-000000000010";

pub fn default_organization_uuid() -> Uuid {
    Uuid::parse_str(DEFAULT_ORGANIZATION_ID).expect("DEFAULT_ORGANIZATION_ID must be valid UUID")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrganizationRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
}
