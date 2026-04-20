//! User model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::default_organization_uuid;

/// Authentication provider type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthProvider {
    /// Password-based authentication only
    #[default]
    Local,
    /// SAML-only authentication
    Saml,
    /// Both local and SAML authentication allowed
    Both,
}

impl AuthProvider {
    /// Check if local password authentication is allowed
    pub fn allows_local(&self) -> bool {
        matches!(self, AuthProvider::Local | AuthProvider::Both)
    }

    /// Check if SAML authentication is allowed
    pub fn allows_saml(&self) -> bool {
        matches!(self, AuthProvider::Saml | AuthProvider::Both)
    }
}

impl std::fmt::Display for AuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthProvider::Local => write!(f, "local"),
            AuthProvider::Saml => write!(f, "saml"),
            AuthProvider::Both => write!(f, "both"),
        }
    }
}

impl std::str::FromStr for AuthProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(AuthProvider::Local),
            "saml" => Ok(AuthProvider::Saml),
            "both" => Ok(AuthProvider::Both),
            _ => Err(format!("Invalid auth provider: {}", s)),
        }
    }
}

/// User entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    #[serde(default = "default_organization_uuid")]
    pub organization_id: Uuid,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    #[serde(default)]
    pub force_password_change: bool,
    /// Authentication provider (local, saml, or both)
    #[serde(default)]
    pub auth_provider: AuthProvider,
    /// External identifier for SAML users (SAML NameID)
    #[serde(default)]
    pub external_id: Option<String>,
    /// IdP entity ID that authenticated this user
    #[serde(default)]
    pub idp_entity_id: Option<String>,
    /// Last SAML authentication timestamp
    #[serde(default)]
    pub last_saml_auth_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// Create a new user
    pub fn new(username: String, email: String, password_hash: String, role: String) -> Self {
        Self::new_with_org(
            default_organization_uuid(),
            username,
            email,
            password_hash,
            role,
        )
    }

    pub fn new_with_org(
        organization_id: Uuid,
        username: String,
        email: String,
        password_hash: String,
        role: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id,
            username,
            email,
            password_hash,
            role,
            force_password_change: false,
            auth_provider: AuthProvider::Local,
            external_id: None,
            idp_entity_id: None,
            last_saml_auth_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// User without password hash for safe serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPublic {
    pub id: Uuid,
    #[serde(default = "default_organization_uuid")]
    pub organization_id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    #[serde(default)]
    pub force_password_change: bool,
    /// Authentication provider (local, saml, or both)
    #[serde(default)]
    pub auth_provider: AuthProvider,
    /// RBAC roles assigned to the user (optional for backwards compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<UserRoleInfo>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Simplified role info for user responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRoleInfo {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
}

impl From<User> for UserPublic {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            organization_id: user.organization_id,
            username: user.username,
            email: user.email,
            role: user.role,
            force_password_change: user.force_password_change,
            auth_provider: user.auth_provider,
            roles: None,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

impl UserPublic {
    /// Set roles on a UserPublic instance
    pub fn with_roles(mut self, roles: Vec<UserRoleInfo>) -> Self {
        self.roles = Some(roles);
        self
    }
}

/// Request to create a new user
#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub organization_id: Option<Uuid>,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "viewer".to_string()
}

/// Request to update a user
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
}

/// Login request
#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Token refresh request
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// Authentication response with tokens
#[derive(Debug, Clone, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: UserPublic,
}

/// Token response for refresh
#[derive(Debug, Clone, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_new() {
        let user = User::new(
            "testuser".to_string(),
            "test@example.com".to_string(),
            "hash".to_string(),
            "viewer".to_string(),
        );

        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.role, "viewer");
        assert!(!user.id.is_nil());
    }

    #[test]
    fn test_user_public_from_user() {
        let user = User::new(
            "testuser".to_string(),
            "test@example.com".to_string(),
            "secret_hash".to_string(),
            "viewer".to_string(),
        );

        let public: UserPublic = user.clone().into();

        assert_eq!(public.id, user.id);
        assert_eq!(public.username, user.username);
        assert_eq!(public.email, user.email);
        // password_hash is not in UserPublic
    }

    #[test]
    fn test_default_role() {
        let json = r#"{"username": "test", "email": "test@test.com", "password": "pass"}"#;
        let req: CreateUserRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.role, "viewer");
    }
}
