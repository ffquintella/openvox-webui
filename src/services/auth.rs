//! Authentication service
//!
//! Provides password hashing with Argon2 and user authentication.

use anyhow::{Context, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::models::{default_organization_uuid, User, UserPublic};

/// Authentication service for user management
pub struct AuthService {
    pool: SqlitePool,
}

impl AuthService {
    /// Create a new auth service
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Hash a password using Argon2id
    pub fn hash_password(password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
            .to_string();
        Ok(password_hash)
    }

    /// Verify a password against a hash
    pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(password_hash)
            .map_err(|e| anyhow::anyhow!("Invalid password hash format: {}", e))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Authenticate a user by username and password
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<Option<User>> {
        let user = self.get_user_by_username(username).await?;

        match user {
            Some(user) => {
                if Self::verify_password(password, &user.password_hash)? {
                    Ok(Some(user))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Get a user by username
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let row = sqlx::query(
            "SELECT id, organization_id, username, email, password_hash, role, force_password_change, created_at, updated_at FROM users WHERE username = ?"
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user by username")?;

        Ok(row.map(|r| row_to_user(&r)))
    }

    /// Get a user by ID
    pub async fn get_user_by_id(&self, id: &Uuid) -> Result<Option<User>> {
        let id_str = id.to_string();
        let row = sqlx::query(
            "SELECT id, organization_id, username, email, password_hash, role, force_password_change, created_at, updated_at FROM users WHERE id = ?"
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user by ID")?;

        Ok(row.map(|r| row_to_user(&r)))
    }

    /// Get a user by email
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let row = sqlx::query(
            "SELECT id, organization_id, username, email, password_hash, role, force_password_change, created_at, updated_at FROM users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user by email")?;

        Ok(row.map(|r| row_to_user(&r)))
    }

    /// Create a new user
    pub async fn create_user(
        &self,
        username: &str,
        email: &str,
        password: &str,
        role: &str,
    ) -> Result<User> {
        self.create_user_in_org(username, email, password, role, default_organization_uuid())
            .await
    }

    pub async fn create_user_in_org(
        &self,
        username: &str,
        email: &str,
        password: &str,
        role: &str,
        organization_id: Uuid,
    ) -> Result<User> {
        // Check if username already exists
        if self.get_user_by_username(username).await?.is_some() {
            anyhow::bail!("Username already exists");
        }

        // Check if email already exists
        if self.get_user_by_email(email).await?.is_some() {
            anyhow::bail!("Email already exists");
        }

        let password_hash = Self::hash_password(password)?;
        let user = User::new_with_org(
            organization_id,
            username.to_string(),
            email.to_string(),
            password_hash,
            role.to_string(),
        );

        let id_str = user.id.to_string();
        let org_id_str = user.organization_id.to_string();
        let created_at = user.created_at.to_rfc3339();
        let updated_at = user.updated_at.to_rfc3339();

        sqlx::query(
            "INSERT INTO users (id, organization_id, username, email, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id_str)
        .bind(&org_id_str)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.role)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&self.pool)
        .await
        .context("Failed to create user")?;

        Ok(user)
    }

    /// Update a user
    pub async fn update_user(
        &self,
        id: &Uuid,
        username: Option<&str>,
        email: Option<&str>,
        password: Option<&str>,
        role: Option<&str>,
    ) -> Result<User> {
        let existing = self.get_user_by_id(id).await?.context("User not found")?;

        let new_username = username.unwrap_or(&existing.username);
        let new_email = email.unwrap_or(&existing.email);
        let new_role = role.unwrap_or(&existing.role);

        // Check username uniqueness if changed
        if new_username != existing.username
            && self.get_user_by_username(new_username).await?.is_some()
        {
            anyhow::bail!("Username already exists");
        }

        // Check email uniqueness if changed
        if new_email != existing.email && self.get_user_by_email(new_email).await?.is_some() {
            anyhow::bail!("Email already exists");
        }

        let new_password_hash = match password {
            Some(p) => Self::hash_password(p)?,
            None => existing.password_hash.clone(),
        };

        let id_str = id.to_string();
        let updated_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE users SET username = ?, email = ?, password_hash = ?, role = ?, updated_at = ? WHERE id = ?"
        )
        .bind(new_username)
        .bind(new_email)
        .bind(&new_password_hash)
        .bind(new_role)
        .bind(&updated_at)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .context("Failed to update user")?;

        self.get_user_by_id(id)
            .await?
            .context("User not found after update")
    }

    /// Delete a user
    pub async fn delete_user(&self, id: &Uuid) -> Result<bool> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .context("Failed to delete user")?;

        Ok(result.rows_affected() > 0)
    }

    /// List all users
    pub async fn list_users(&self) -> Result<Vec<UserPublic>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, username, email, password_hash, role, force_password_change, created_at, updated_at FROM users ORDER BY username"
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to list users")?;

        Ok(rows.iter().map(|r| row_to_user(r).into()).collect())
    }

    pub async fn list_users_in_org(&self, organization_id: Uuid) -> Result<Vec<UserPublic>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, username, email, password_hash, role, force_password_change, created_at, updated_at FROM users WHERE organization_id = ? ORDER BY username",
        )
        .bind(organization_id.to_string())
        .fetch_all(&self.pool)
        .await
        .context("Failed to list users for organization")?;

        Ok(rows.iter().map(|r| row_to_user(r).into()).collect())
    }

    pub async fn get_user_by_id_in_org(
        &self,
        organization_id: Uuid,
        id: &Uuid,
    ) -> Result<Option<User>> {
        let row = sqlx::query(
            "SELECT id, organization_id, username, email, password_hash, role, force_password_change, created_at, updated_at FROM users WHERE organization_id = ? AND id = ?",
        )
        .bind(organization_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user by ID (scoped)")?;

        Ok(row.map(|r| row_to_user(&r)))
    }

    /// Get user roles from the user_roles table
    pub async fn get_user_roles(&self, user_id: &Uuid) -> Result<Vec<String>> {
        let user_id_str = user_id.to_string();
        let rows = sqlx::query(
            "SELECT r.name FROM roles r INNER JOIN user_roles ur ON r.id = ur.role_id WHERE ur.user_id = ?"
        )
        .bind(&user_id_str)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch user roles")?;

        Ok(rows.iter().map(|r| r.get::<String, _>("name")).collect())
    }

    /// Assign a role to a user
    pub async fn assign_role(&self, user_id: &Uuid, role_id: &Uuid) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let user_id_str = user_id.to_string();
        let role_id_str = role_id.to_string();

        sqlx::query("INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(&user_id_str)
            .bind(&role_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to assign role")?;

        Ok(())
    }

    /// Remove a role from a user
    pub async fn remove_role(&self, user_id: &Uuid, role_id: &Uuid) -> Result<()> {
        let user_id_str = user_id.to_string();
        let role_id_str = role_id.to_string();

        sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_id = ?")
            .bind(&user_id_str)
            .bind(&role_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to remove role")?;

        Ok(())
    }

    /// Create a password reset token for a user
    ///
    /// Returns the reset token if the email exists, None otherwise.
    /// The token is valid for 1 hour.
    pub async fn create_password_reset_token(&self, email: &str) -> Result<Option<String>> {
        let user = self.get_user_by_email(email).await?;

        match user {
            Some(user) => {
                let token = Uuid::new_v4().to_string();
                let token_hash = Self::hash_reset_token(&token);
                let user_id_str = user.id.to_string();
                let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
                let created_at = chrono::Utc::now().to_rfc3339();

                // Delete any existing reset tokens for this user
                sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = ?")
                    .bind(&user_id_str)
                    .execute(&self.pool)
                    .await
                    .context("Failed to delete existing reset tokens")?;

                // Insert new reset token
                let id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)"
                )
                .bind(&id)
                .bind(&user_id_str)
                .bind(&token_hash)
                .bind(&expires_at)
                .bind(&created_at)
                .execute(&self.pool)
                .await
                .context("Failed to create reset token")?;

                Ok(Some(token))
            }
            None => Ok(None),
        }
    }

    /// Validate a password reset token and return the associated user ID
    pub async fn validate_reset_token(&self, token: &str) -> Result<Option<Uuid>> {
        let token_hash = Self::hash_reset_token(token);
        let now = chrono::Utc::now().to_rfc3339();

        let row = sqlx::query(
            "SELECT user_id FROM password_reset_tokens WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(&token_hash)
        .bind(&now)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to validate reset token")?;

        match row {
            Some(row) => {
                let user_id_str: String = row.get("user_id");
                Ok(Some(
                    Uuid::parse_str(&user_id_str)
                        .map_err(|_| anyhow::anyhow!("Invalid user ID"))?,
                ))
            }
            None => Ok(None),
        }
    }

    /// Reset a user's password using a valid reset token
    pub async fn reset_password(&self, token: &str, new_password: &str) -> Result<bool> {
        let user_id = self.validate_reset_token(token).await?;

        match user_id {
            Some(user_id) => {
                // Update the password
                let new_password_hash = Self::hash_password(new_password)?;
                let user_id_str = user_id.to_string();
                let updated_at = chrono::Utc::now().to_rfc3339();

                sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                    .bind(&new_password_hash)
                    .bind(&updated_at)
                    .bind(&user_id_str)
                    .execute(&self.pool)
                    .await
                    .context("Failed to update password")?;

                // Delete the used token
                let token_hash = Self::hash_reset_token(token);
                sqlx::query("DELETE FROM password_reset_tokens WHERE token_hash = ?")
                    .bind(&token_hash)
                    .execute(&self.pool)
                    .await
                    .context("Failed to delete used reset token")?;

                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Delete expired password reset tokens (cleanup)
    pub async fn cleanup_expired_tokens(&self) -> Result<u64> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query("DELETE FROM password_reset_tokens WHERE expires_at <= ?")
            .bind(&now)
            .execute(&self.pool)
            .await
            .context("Failed to cleanup expired tokens")?;

        Ok(result.rows_affected())
    }

    /// Change password for a user (requires current password verification)
    ///
    /// This also clears the force_password_change flag.
    pub async fn change_password(
        &self,
        user_id: &Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<bool> {
        // Get user to verify current password
        let user = self
            .get_user_by_id(user_id)
            .await?
            .context("User not found")?;

        // Verify current password
        if !Self::verify_password(current_password, &user.password_hash)? {
            return Ok(false);
        }

        // Update to new password and clear force_password_change flag
        let new_password_hash = Self::hash_password(new_password)?;
        let user_id_str = user_id.to_string();
        let updated_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE users SET password_hash = ?, force_password_change = 0, updated_at = ? WHERE id = ?",
        )
        .bind(&new_password_hash)
        .bind(&updated_at)
        .bind(&user_id_str)
        .execute(&self.pool)
        .await
        .context("Failed to update password")?;

        Ok(true)
    }

    /// Clear the force_password_change flag for a user
    pub async fn clear_force_password_change(&self, user_id: &Uuid) -> Result<()> {
        let user_id_str = user_id.to_string();
        let updated_at = chrono::Utc::now().to_rfc3339();

        sqlx::query("UPDATE users SET force_password_change = 0, updated_at = ? WHERE id = ?")
            .bind(&updated_at)
            .bind(&user_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to clear force_password_change flag")?;

        Ok(())
    }

    /// Set the force_password_change flag for a user
    pub async fn set_force_password_change(&self, user_id: &Uuid, force: bool) -> Result<()> {
        let user_id_str = user_id.to_string();
        let updated_at = chrono::Utc::now().to_rfc3339();

        sqlx::query("UPDATE users SET force_password_change = ?, updated_at = ? WHERE id = ?")
            .bind(force)
            .bind(&updated_at)
            .bind(&user_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to update force_password_change flag")?;

        Ok(())
    }

    /// Hash a reset token using SHA-256 for storage
    fn hash_reset_token(token: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// Convert a database row to a User
fn row_to_user(row: &sqlx::sqlite::SqliteRow) -> User {
    let id_str: String = row.get("id");
    let org_id_str: String = row
        .try_get("organization_id")
        .unwrap_or_else(|_| default_organization_uuid().to_string());
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let force_password_change: bool = row.try_get("force_password_change").unwrap_or(false);

    User {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::nil()),
        organization_id: Uuid::parse_str(&org_id_str)
            .unwrap_or_else(|_| default_organization_uuid()),
        username: row.get("username"),
        email: row.get("email"),
        password_hash: row.get("password_hash"),
        role: row.get("role"),
        force_password_change,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let password = "my_secure_password";
        let hash = AuthService::hash_password(password).unwrap();

        assert!(AuthService::verify_password(password, &hash).unwrap());
        assert!(!AuthService::verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_hash_produces_different_hashes() {
        let password = "same_password";
        let hash1 = AuthService::hash_password(password).unwrap();
        let hash2 = AuthService::hash_password(password).unwrap();

        // Different salts should produce different hashes
        assert_ne!(hash1, hash2);

        // But both should verify correctly
        assert!(AuthService::verify_password(password, &hash1).unwrap());
        assert!(AuthService::verify_password(password, &hash2).unwrap());
    }

    #[test]
    fn test_verify_invalid_hash() {
        let result = AuthService::verify_password("password", "not_a_valid_hash");
        assert!(result.is_err());
    }
}
