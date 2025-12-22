//! User model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::default_organization_uuid;

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
