//! Repository pattern implementation for code deploy database access

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{
    CodeDeployment, CodeEnvironment, CodeRepository, CodeSshKey, CreateRepositoryRequest,
    CreateSshKeyRequest, DeploymentStatus, ListDeploymentsQuery, ListEnvironmentsQuery,
    UpdateEnvironmentRequest, UpdateRepositoryRequest,
};

// ============================================================================
// SSH Key Repository
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct SshKeyRow {
    id: String,
    name: String,
    public_key: String,
    private_key_encrypted: String,
    created_at: String,
    updated_at: String,
}

/// Repository for SSH key operations
pub struct CodeSshKeyRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CodeSshKeyRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all SSH keys
    pub async fn get_all(&self) -> Result<Vec<CodeSshKey>> {
        let rows = sqlx::query_as::<_, SshKeyRow>(
            r#"
            SELECT id, name, public_key, private_key_encrypted, created_at, updated_at
            FROM code_ssh_keys
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch SSH keys")?;

        Ok(rows.into_iter().map(row_to_ssh_key).collect())
    }

    /// Get an SSH key by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<CodeSshKey>> {
        let row = sqlx::query_as::<_, SshKeyRow>(
            r#"
            SELECT id, name, public_key, private_key_encrypted, created_at, updated_at
            FROM code_ssh_keys
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch SSH key")?;

        Ok(row.map(row_to_ssh_key))
    }

    /// Get an SSH key by name
    pub async fn get_by_name(&self, name: &str) -> Result<Option<CodeSshKey>> {
        let row = sqlx::query_as::<_, SshKeyRow>(
            r#"
            SELECT id, name, public_key, private_key_encrypted, created_at, updated_at
            FROM code_ssh_keys
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch SSH key")?;

        Ok(row.map(row_to_ssh_key))
    }

    /// Create a new SSH key
    pub async fn create(&self, req: &CreateSshKeyRequest, encrypted_key: &str, public_key: &str) -> Result<CodeSshKey> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO code_ssh_keys (id, name, public_key, private_key_encrypted)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(public_key)
        .bind(encrypted_key)
        .execute(self.pool)
        .await
        .context("Failed to create SSH key")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created SSH key"))
    }

    /// Delete an SSH key
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM code_ssh_keys WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete SSH key")?;

        Ok(result.rows_affected() > 0)
    }
}

fn row_to_ssh_key(row: SshKeyRow) -> CodeSshKey {
    CodeSshKey {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        public_key: row.public_key,
        private_key_encrypted: row.private_key_encrypted,
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// PAT Token Repository
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct PatTokenRow {
    id: String,
    name: String,
    description: Option<String>,
    username: Option<String>,
    token_encrypted: String,
    expires_at: Option<String>,
    last_validated_at: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Repository for PAT token operations
pub struct CodePatTokenRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CodePatTokenRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all PAT tokens
    pub async fn get_all(&self) -> Result<Vec<crate::models::CodePatToken>> {
        let rows = sqlx::query_as::<_, PatTokenRow>(
            r#"
            SELECT id, name, description, username, token_encrypted, expires_at, last_validated_at, created_at, updated_at
            FROM code_pat_tokens
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch PAT tokens")?;

        Ok(rows.into_iter().map(row_to_pat_token).collect())
    }

    /// Get a PAT token by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<crate::models::CodePatToken>> {
        let row = sqlx::query_as::<_, PatTokenRow>(
            r#"
            SELECT id, name, description, username, token_encrypted, expires_at, last_validated_at, created_at, updated_at
            FROM code_pat_tokens
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch PAT token")?;

        Ok(row.map(row_to_pat_token))
    }

    /// Get a PAT token by name
    pub async fn get_by_name(&self, name: &str) -> Result<Option<crate::models::CodePatToken>> {
        let row = sqlx::query_as::<_, PatTokenRow>(
            r#"
            SELECT id, name, description, username, token_encrypted, expires_at, last_validated_at, created_at, updated_at
            FROM code_pat_tokens
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch PAT token")?;

        Ok(row.map(row_to_pat_token))
    }

    /// Create a new PAT token
    pub async fn create(
        &self,
        req: &crate::models::CreatePatTokenRequest,
        encrypted_token: &str,
    ) -> Result<crate::models::CodePatToken> {
        let id = Uuid::new_v4();
        let expires_at = req.expires_at.map(|dt| dt.to_rfc3339());

        sqlx::query(
            r#"
            INSERT INTO code_pat_tokens (id, name, description, username, token_encrypted, expires_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.username)
        .bind(encrypted_token)
        .bind(expires_at)
        .execute(self.pool)
        .await
        .context("Failed to create PAT token")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created PAT token"))
    }

    /// Update a PAT token
    pub async fn update(
        &self,
        id: Uuid,
        req: &crate::models::UpdatePatTokenRequest,
        encrypted_token: Option<&str>,
    ) -> Result<crate::models::CodePatToken> {
        // Build dynamic update query
        let mut updates = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(name) = &req.name {
            updates.push("name = ?");
            params.push(name.clone());
        }

        if req.description.is_some() {
            updates.push("description = ?");
            params.push(req.description.clone().unwrap_or_default());
        }

        if req.username.is_some() {
            updates.push("username = ?");
            params.push(req.username.clone().unwrap_or_default());
        }

        if let Some(token) = encrypted_token {
            updates.push("token_encrypted = ?");
            params.push(token.to_string());
        }

        if req.expires_at.is_some() {
            updates.push("expires_at = ?");
            params.push(req.expires_at.map(|dt| dt.to_rfc3339()).unwrap_or_default());
        }

        updates.push("updated_at = datetime('now')");

        if updates.is_empty() {
            return self
                .get_by_id(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("PAT token not found"));
        }

        let query_str = format!(
            "UPDATE code_pat_tokens SET {} WHERE id = ?",
            updates.join(", ")
        );

        let mut query = sqlx::query(&query_str);
        for param in params {
            query = query.bind(param);
        }
        query = query.bind(id.to_string());

        query
            .execute(self.pool)
            .await
            .context("Failed to update PAT token")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve updated PAT token"))
    }

    /// Update last_validated_at timestamp for a token
    pub async fn update_last_validated(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE code_pat_tokens
            SET last_validated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update last validated timestamp")?;

        Ok(())
    }

    /// Delete a PAT token
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM code_pat_tokens WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete PAT token")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get tokens that are expired or expiring soon (within 30 days)
    pub async fn get_expiring_tokens(&self, days_threshold: i64) -> Result<Vec<crate::models::CodePatToken>> {
        let now = Utc::now();
        let threshold_date = now + chrono::Duration::days(days_threshold);
        let threshold_str = threshold_date.to_rfc3339();

        let rows = sqlx::query_as::<_, PatTokenRow>(
            r#"
            SELECT id, name, description, token_encrypted, expires_at, last_validated_at, created_at, updated_at
            FROM code_pat_tokens
            WHERE expires_at IS NOT NULL AND expires_at <= ?
            ORDER BY expires_at ASC
            "#,
        )
        .bind(threshold_str)
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch expiring PAT tokens")?;

        Ok(rows.into_iter().map(row_to_pat_token).collect())
    }
}

fn row_to_pat_token(row: PatTokenRow) -> crate::models::CodePatToken {
    crate::models::CodePatToken {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        description: row.description,
        username: row.username,
        token_encrypted: row.token_encrypted,
        expires_at: row.expires_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
        last_validated_at: row.last_validated_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Code Repository Repository
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct RepositoryRow {
    id: String,
    name: String,
    url: String,
    branch_pattern: String,
    auth_type: Option<String>,
    ssh_key_id: Option<String>,
    pat_token_id: Option<String>,
    github_pat_encrypted: Option<String>,
    webhook_secret: Option<String>,
    poll_interval_seconds: i32,
    is_control_repo: bool,
    last_error: Option<String>,
    last_error_at: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Repository for code repository operations
pub struct CodeRepositoryRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CodeRepositoryRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all repositories
    pub async fn get_all(&self) -> Result<Vec<CodeRepository>> {
        let rows = sqlx::query_as::<_, RepositoryRow>(
            r#"
            SELECT id, name, url, branch_pattern, auth_type, ssh_key_id, pat_token_id, github_pat_encrypted,
                   webhook_secret, poll_interval_seconds, is_control_repo, last_error, last_error_at,
                   created_at, updated_at
            FROM code_repositories
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch repositories")?;

        Ok(rows.into_iter().map(row_to_repository).collect())
    }

    /// Get a repository by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<CodeRepository>> {
        let row = sqlx::query_as::<_, RepositoryRow>(
            r#"
            SELECT id, name, url, branch_pattern, auth_type, ssh_key_id, pat_token_id, github_pat_encrypted,
                   webhook_secret, poll_interval_seconds, is_control_repo, last_error, last_error_at,
                   created_at, updated_at
            FROM code_repositories
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch repository")?;

        Ok(row.map(row_to_repository))
    }

    /// Get a repository by name
    pub async fn get_by_name(&self, name: &str) -> Result<Option<CodeRepository>> {
        let row = sqlx::query_as::<_, RepositoryRow>(
            r#"
            SELECT id, name, url, branch_pattern, auth_type, ssh_key_id, pat_token_id, github_pat_encrypted,
                   webhook_secret, poll_interval_seconds, is_control_repo, last_error, last_error_at,
                   created_at, updated_at
            FROM code_repositories
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch repository")?;

        Ok(row.map(row_to_repository))
    }

    /// Get repositories that need polling
    pub async fn get_for_polling(&self) -> Result<Vec<CodeRepository>> {
        let rows = sqlx::query_as::<_, RepositoryRow>(
            r#"
            SELECT id, name, url, branch_pattern, auth_type, ssh_key_id, pat_token_id, github_pat_encrypted,
                   webhook_secret, poll_interval_seconds, is_control_repo, last_error, last_error_at,
                   created_at, updated_at
            FROM code_repositories
            WHERE poll_interval_seconds > 0
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch repositories for polling")?;

        Ok(rows.into_iter().map(row_to_repository).collect())
    }

    /// Create a new repository
    pub async fn create(&self, req: &CreateRepositoryRequest, github_pat_encrypted: Option<&str>) -> Result<CodeRepository> {
        let id = Uuid::new_v4();
        let webhook_secret = generate_webhook_secret();

        sqlx::query(
            r#"
            INSERT INTO code_repositories (id, name, url, branch_pattern, auth_type, ssh_key_id, pat_token_id,
                                          github_pat_encrypted, webhook_secret, poll_interval_seconds, is_control_repo)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.url)
        .bind(&req.branch_pattern)
        .bind(req.auth_type.as_str())
        .bind(req.ssh_key_id.map(|k| k.to_string()))
        .bind(req.pat_token_id.map(|k| k.to_string()))
        .bind(github_pat_encrypted)
        .bind(&webhook_secret)
        .bind(req.poll_interval_seconds)
        .bind(req.is_control_repo)
        .execute(self.pool)
        .await
        .context("Failed to create repository")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created repository"))
    }

    /// Update a repository
    pub async fn update(&self, id: Uuid, req: &UpdateRepositoryRequest, github_pat_encrypted: Option<&str>) -> Result<Option<CodeRepository>> {
        let existing = self.get_by_id(id).await?;
        if existing.is_none() {
            return Ok(None);
        }
        let existing = existing.unwrap();

        let name = req.name.as_ref().unwrap_or(&existing.name);
        let url = req.url.as_ref().unwrap_or(&existing.url);
        let branch_pattern = req.branch_pattern.as_ref().unwrap_or(&existing.branch_pattern);
        let auth_type = req.auth_type.unwrap_or(existing.auth_type);
        let ssh_key_id = if req.clear_ssh_key {
            None
        } else {
            req.ssh_key_id.or(existing.ssh_key_id)
        };
        let pat_token_id = if req.clear_pat_token {
            None
        } else {
            req.pat_token_id.or(existing.pat_token_id)
        };
        let github_pat = if req.clear_github_pat {
            None
        } else if github_pat_encrypted.is_some() {
            github_pat_encrypted.map(|s| s.to_string())
        } else {
            existing.github_pat_encrypted
        };
        let poll_interval = req.poll_interval_seconds.unwrap_or(existing.poll_interval_seconds);
        let is_control_repo = req.is_control_repo.unwrap_or(existing.is_control_repo);
        let webhook_secret = if req.regenerate_webhook_secret {
            Some(generate_webhook_secret())
        } else {
            existing.webhook_secret
        };

        sqlx::query(
            r#"
            UPDATE code_repositories
            SET name = ?, url = ?, branch_pattern = ?, auth_type = ?, ssh_key_id = ?, pat_token_id = ?,
                github_pat_encrypted = ?, webhook_secret = ?, poll_interval_seconds = ?, is_control_repo = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(name)
        .bind(url)
        .bind(branch_pattern)
        .bind(auth_type.as_str())
        .bind(ssh_key_id.map(|k| k.to_string()))
        .bind(pat_token_id.map(|k| k.to_string()))
        .bind(github_pat)
        .bind(&webhook_secret)
        .bind(poll_interval)
        .bind(is_control_repo)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update repository")?;

        self.get_by_id(id).await
    }

    /// Update repository error status
    pub async fn set_error(&self, id: Uuid, error: Option<&str>) -> Result<()> {
        if let Some(err) = error {
            sqlx::query(
                r#"
                UPDATE code_repositories
                SET last_error = ?, last_error_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                "#,
            )
            .bind(err)
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to update repository error")?;
        } else {
            sqlx::query(
                r#"
                UPDATE code_repositories
                SET last_error = NULL, last_error_at = NULL, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                "#,
            )
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to clear repository error")?;
        }
        Ok(())
    }

    /// Delete a repository
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM code_repositories WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete repository")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get environment count for a repository
    pub async fn get_environment_count(&self, id: Uuid) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM code_environments WHERE repository_id = ?",
        )
        .bind(id.to_string())
        .fetch_one(self.pool)
        .await
        .context("Failed to count environments")?;

        Ok(row.0)
    }
}

fn row_to_repository(row: RepositoryRow) -> CodeRepository {
    use crate::models::AuthType;

    let auth_type = row.auth_type
        .as_deref()
        .and_then(AuthType::from_str)
        .unwrap_or_default();

    CodeRepository {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        url: row.url,
        branch_pattern: row.branch_pattern,
        auth_type,
        ssh_key_id: row.ssh_key_id.and_then(|s| Uuid::parse_str(&s).ok()),
        pat_token_id: row.pat_token_id.and_then(|s| Uuid::parse_str(&s).ok()),
        github_pat_encrypted: row.github_pat_encrypted,
        webhook_secret: row.webhook_secret,
        poll_interval_seconds: row.poll_interval_seconds,
        is_control_repo: row.is_control_repo,
        last_error: row.last_error,
        last_error_at: row.last_error_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

fn generate_webhook_secret() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

// ============================================================================
// Code Environment Repository
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentRow {
    id: String,
    repository_id: String,
    name: String,
    branch: String,
    current_commit: Option<String>,
    current_commit_message: Option<String>,
    current_commit_author: Option<String>,
    current_commit_date: Option<String>,
    last_synced_at: Option<String>,
    auto_deploy: bool,
    requires_approval: bool,
    created_at: String,
    updated_at: String,
}

/// Repository for code environment operations
pub struct CodeEnvironmentRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CodeEnvironmentRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all environments
    pub async fn get_all(&self, query: &ListEnvironmentsQuery) -> Result<Vec<CodeEnvironment>> {
        let mut sql = String::from(
            r#"
            SELECT id, repository_id, name, branch, current_commit, current_commit_message,
                   current_commit_author, current_commit_date, last_synced_at,
                   auto_deploy, requires_approval, created_at, updated_at
            FROM code_environments
            WHERE 1=1
            "#,
        );

        if query.repository_id.is_some() {
            sql.push_str(" AND repository_id = ?");
        }
        if query.auto_deploy.is_some() {
            sql.push_str(" AND auto_deploy = ?");
        }

        sql.push_str(" ORDER BY name");

        let mut query_builder = sqlx::query_as::<_, EnvironmentRow>(&sql);

        if let Some(repo_id) = query.repository_id {
            query_builder = query_builder.bind(repo_id.to_string());
        }
        if let Some(auto_deploy) = query.auto_deploy {
            query_builder = query_builder.bind(auto_deploy);
        }

        let rows = query_builder
            .fetch_all(self.pool)
            .await
            .context("Failed to fetch environments")?;

        Ok(rows.into_iter().map(row_to_environment).collect())
    }

    /// Get environments by repository
    pub async fn get_by_repository(&self, repository_id: Uuid) -> Result<Vec<CodeEnvironment>> {
        let rows = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, repository_id, name, branch, current_commit, current_commit_message,
                   current_commit_author, current_commit_date, last_synced_at,
                   auto_deploy, requires_approval, created_at, updated_at
            FROM code_environments
            WHERE repository_id = ?
            ORDER BY name
            "#,
        )
        .bind(repository_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch environments")?;

        Ok(rows.into_iter().map(row_to_environment).collect())
    }

    /// Get an environment by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<CodeEnvironment>> {
        let row = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, repository_id, name, branch, current_commit, current_commit_message,
                   current_commit_author, current_commit_date, last_synced_at,
                   auto_deploy, requires_approval, created_at, updated_at
            FROM code_environments
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch environment")?;

        Ok(row.map(row_to_environment))
    }

    /// Get an environment by repository and name
    pub async fn get_by_repo_and_name(
        &self,
        repository_id: Uuid,
        name: &str,
    ) -> Result<Option<CodeEnvironment>> {
        let row = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, repository_id, name, branch, current_commit, current_commit_message,
                   current_commit_author, current_commit_date, last_synced_at,
                   auto_deploy, requires_approval, created_at, updated_at
            FROM code_environments
            WHERE repository_id = ? AND name = ?
            "#,
        )
        .bind(repository_id.to_string())
        .bind(name)
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch environment")?;

        Ok(row.map(row_to_environment))
    }

    /// Create or update an environment (upsert)
    pub async fn upsert(
        &self,
        repository_id: Uuid,
        name: &str,
        branch: &str,
        commit_sha: Option<&str>,
        commit_message: Option<&str>,
        commit_author: Option<&str>,
        commit_date: Option<DateTime<Utc>>,
    ) -> Result<CodeEnvironment> {
        let existing = self.get_by_repo_and_name(repository_id, name).await?;

        if let Some(env) = existing {
            // Update existing environment
            sqlx::query(
                r#"
                UPDATE code_environments
                SET branch = ?, current_commit = ?, current_commit_message = ?,
                    current_commit_author = ?, current_commit_date = ?,
                    last_synced_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                "#,
            )
            .bind(branch)
            .bind(commit_sha)
            .bind(commit_message)
            .bind(commit_author)
            .bind(commit_date.map(|d| d.to_rfc3339()))
            .bind(env.id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to update environment")?;

            self.get_by_id(env.id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Failed to retrieve updated environment"))
        } else {
            // Create new environment
            let id = Uuid::new_v4();

            sqlx::query(
                r#"
                INSERT INTO code_environments (id, repository_id, name, branch, current_commit,
                                              current_commit_message, current_commit_author,
                                              current_commit_date, last_synced_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
                "#,
            )
            .bind(id.to_string())
            .bind(repository_id.to_string())
            .bind(name)
            .bind(branch)
            .bind(commit_sha)
            .bind(commit_message)
            .bind(commit_author)
            .bind(commit_date.map(|d| d.to_rfc3339()))
            .execute(self.pool)
            .await
            .context("Failed to create environment")?;

            self.get_by_id(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created environment"))
        }
    }

    /// Update environment settings
    pub async fn update(
        &self,
        id: Uuid,
        req: &UpdateEnvironmentRequest,
    ) -> Result<Option<CodeEnvironment>> {
        let existing = self.get_by_id(id).await?;
        if existing.is_none() {
            return Ok(None);
        }
        let existing = existing.unwrap();

        let auto_deploy = req.auto_deploy.unwrap_or(existing.auto_deploy);
        let requires_approval = req.requires_approval.unwrap_or(existing.requires_approval);

        sqlx::query(
            r#"
            UPDATE code_environments
            SET auto_deploy = ?, requires_approval = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(auto_deploy)
        .bind(requires_approval)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update environment")?;

        self.get_by_id(id).await
    }

    /// Delete an environment
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM code_environments WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete environment")?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete environments that no longer exist in the repository
    pub async fn delete_missing(&self, repository_id: Uuid, existing_names: &[String]) -> Result<u64> {
        if existing_names.is_empty() {
            // Delete all environments for this repo
            let result = sqlx::query("DELETE FROM code_environments WHERE repository_id = ?")
                .bind(repository_id.to_string())
                .execute(self.pool)
                .await
                .context("Failed to delete environments")?;
            return Ok(result.rows_affected());
        }

        // Build placeholders for IN clause
        let placeholders: Vec<&str> = existing_names.iter().map(|_| "?").collect();
        let sql = format!(
            "DELETE FROM code_environments WHERE repository_id = ? AND name NOT IN ({})",
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&sql).bind(repository_id.to_string());
        for name in existing_names {
            query = query.bind(name);
        }

        let result = query
            .execute(self.pool)
            .await
            .context("Failed to delete missing environments")?;

        Ok(result.rows_affected())
    }
}

fn row_to_environment(row: EnvironmentRow) -> CodeEnvironment {
    CodeEnvironment {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        repository_id: Uuid::parse_str(&row.repository_id).unwrap_or_default(),
        name: row.name,
        branch: row.branch,
        current_commit: row.current_commit,
        current_commit_message: row.current_commit_message,
        current_commit_author: row.current_commit_author,
        current_commit_date: row.current_commit_date.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        last_synced_at: row.last_synced_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        auto_deploy: row.auto_deploy,
        requires_approval: row.requires_approval,
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Code Deployment Repository
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct DeploymentRow {
    id: String,
    environment_id: String,
    commit_sha: String,
    commit_message: Option<String>,
    commit_author: Option<String>,
    status: String,
    requested_by: Option<String>,
    approved_by: Option<String>,
    approved_at: Option<String>,
    rejected_at: Option<String>,
    rejection_reason: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    error_message: Option<String>,
    r10k_output: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Repository for code deployment operations
pub struct CodeDeploymentRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CodeDeploymentRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get deployments with optional filtering
    pub async fn get_all(&self, query: &ListDeploymentsQuery) -> Result<Vec<CodeDeployment>> {
        let mut sql = String::from(
            r#"
            SELECT d.id, d.environment_id, d.commit_sha, d.commit_message, d.commit_author,
                   d.status, d.requested_by, d.approved_by, d.approved_at, d.rejected_at,
                   d.rejection_reason, d.started_at, d.completed_at, d.error_message,
                   d.r10k_output, d.created_at, d.updated_at
            FROM code_deployments d
            "#,
        );

        let mut conditions = vec!["1=1".to_string()];

        if query.environment_id.is_some() {
            conditions.push("d.environment_id = ?".to_string());
        }
        if query.repository_id.is_some() {
            sql.push_str(" JOIN code_environments e ON d.environment_id = e.id");
            conditions.push("e.repository_id = ?".to_string());
        }
        if query.status.is_some() {
            conditions.push("d.status = ?".to_string());
        }

        sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
        sql.push_str(" ORDER BY d.created_at DESC");

        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let mut query_builder = sqlx::query_as::<_, DeploymentRow>(&sql);

        if let Some(env_id) = query.environment_id {
            query_builder = query_builder.bind(env_id.to_string());
        }
        if let Some(repo_id) = query.repository_id {
            query_builder = query_builder.bind(repo_id.to_string());
        }
        if let Some(status) = query.status {
            query_builder = query_builder.bind(status.as_str());
        }

        let rows = query_builder
            .fetch_all(self.pool)
            .await
            .context("Failed to fetch deployments")?;

        Ok(rows.into_iter().map(row_to_deployment).collect())
    }

    /// Get deployments for an environment
    pub async fn get_by_environment(
        &self,
        environment_id: Uuid,
        limit: Option<i64>,
    ) -> Result<Vec<CodeDeployment>> {
        let limit = limit.unwrap_or(50);
        let rows = sqlx::query_as::<_, DeploymentRow>(
            r#"
            SELECT id, environment_id, commit_sha, commit_message, commit_author,
                   status, requested_by, approved_by, approved_at, rejected_at,
                   rejection_reason, started_at, completed_at, error_message,
                   r10k_output, created_at, updated_at
            FROM code_deployments
            WHERE environment_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(environment_id.to_string())
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch deployments")?;

        Ok(rows.into_iter().map(row_to_deployment).collect())
    }

    /// Get a deployment by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<CodeDeployment>> {
        let row = sqlx::query_as::<_, DeploymentRow>(
            r#"
            SELECT id, environment_id, commit_sha, commit_message, commit_author,
                   status, requested_by, approved_by, approved_at, rejected_at,
                   rejection_reason, started_at, completed_at, error_message,
                   r10k_output, created_at, updated_at
            FROM code_deployments
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch deployment")?;

        Ok(row.map(row_to_deployment))
    }

    /// Get pending deployments that are approved and ready to run
    pub async fn get_ready_to_deploy(&self) -> Result<Vec<CodeDeployment>> {
        let rows = sqlx::query_as::<_, DeploymentRow>(
            r#"
            SELECT id, environment_id, commit_sha, commit_message, commit_author,
                   status, requested_by, approved_by, approved_at, rejected_at,
                   rejection_reason, started_at, completed_at, error_message,
                   r10k_output, created_at, updated_at
            FROM code_deployments
            WHERE status = 'approved'
            ORDER BY approved_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch ready deployments")?;

        Ok(rows.into_iter().map(row_to_deployment).collect())
    }

    /// Get pending deployment for an environment (if any)
    pub async fn get_pending_for_environment(
        &self,
        environment_id: Uuid,
    ) -> Result<Option<CodeDeployment>> {
        let row = sqlx::query_as::<_, DeploymentRow>(
            r#"
            SELECT id, environment_id, commit_sha, commit_message, commit_author,
                   status, requested_by, approved_by, approved_at, rejected_at,
                   rejection_reason, started_at, completed_at, error_message,
                   r10k_output, created_at, updated_at
            FROM code_deployments
            WHERE environment_id = ? AND status = 'pending'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(environment_id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch pending deployment")?;

        Ok(row.map(row_to_deployment))
    }

    /// Get latest deployment for an environment
    pub async fn get_latest_for_environment(
        &self,
        environment_id: Uuid,
    ) -> Result<Option<CodeDeployment>> {
        let row = sqlx::query_as::<_, DeploymentRow>(
            r#"
            SELECT id, environment_id, commit_sha, commit_message, commit_author,
                   status, requested_by, approved_by, approved_at, rejected_at,
                   rejection_reason, started_at, completed_at, error_message,
                   r10k_output, created_at, updated_at
            FROM code_deployments
            WHERE environment_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(environment_id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch latest deployment")?;

        Ok(row.map(row_to_deployment))
    }

    /// Create a new deployment
    pub async fn create(
        &self,
        environment_id: Uuid,
        commit_sha: &str,
        commit_message: Option<&str>,
        commit_author: Option<&str>,
        status: DeploymentStatus,
        requested_by: Option<Uuid>,
    ) -> Result<CodeDeployment> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO code_deployments (id, environment_id, commit_sha, commit_message,
                                         commit_author, status, requested_by, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(environment_id.to_string())
        .bind(commit_sha)
        .bind(commit_message)
        .bind(commit_author)
        .bind(status.as_str())
        .bind(requested_by.map(|u| u.to_string()))
        .bind(&now)
        .bind(&now)
        .execute(self.pool)
        .await
        .context("Failed to create deployment")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created deployment"))
    }

    /// Approve a deployment
    pub async fn approve(&self, id: Uuid, approved_by: Uuid) -> Result<Option<CodeDeployment>> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE code_deployments
            SET status = 'approved', approved_by = ?, approved_at = ?,
                updated_at = ?
            WHERE id = ? AND status = 'pending'
            "#,
        )
        .bind(approved_by.to_string())
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to approve deployment")?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get_by_id(id).await
    }

    /// Reject a deployment
    pub async fn reject(
        &self,
        id: Uuid,
        rejected_by: Uuid,
        reason: &str,
    ) -> Result<Option<CodeDeployment>> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE code_deployments
            SET status = 'rejected', approved_by = ?, rejected_at = ?,
                rejection_reason = ?, updated_at = ?
            WHERE id = ? AND status = 'pending'
            "#,
        )
        .bind(rejected_by.to_string())
        .bind(&now)
        .bind(reason)
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to reject deployment")?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get_by_id(id).await
    }

    /// Mark deployment as started
    pub async fn mark_deploying(&self, id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE code_deployments
            SET status = 'deploying', started_at = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to mark deployment as deploying")?;

        Ok(())
    }

    /// Mark deployment as succeeded
    pub async fn mark_success(&self, id: Uuid, r10k_output: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE code_deployments
            SET status = 'success', completed_at = ?, r10k_output = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(r10k_output)
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to mark deployment as success")?;

        Ok(())
    }

    /// Mark deployment as failed
    pub async fn mark_failed(
        &self,
        id: Uuid,
        error_message: &str,
        r10k_output: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE code_deployments
            SET status = 'failed', completed_at = ?, error_message = ?,
                r10k_output = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(error_message)
        .bind(r10k_output)
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to mark deployment as failed")?;

        Ok(())
    }

    /// Cancel a pending or approved deployment
    pub async fn cancel(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE code_deployments
            SET status = 'cancelled', updated_at = CURRENT_TIMESTAMP
            WHERE id = ? AND status IN ('pending', 'approved')
            "#,
        )
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to cancel deployment")?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete old deployments
    pub async fn delete_old(&self, older_than: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM code_deployments WHERE created_at < ? AND status IN ('success', 'failed', 'rejected', 'cancelled')",
        )
        .bind(older_than.to_rfc3339())
        .execute(self.pool)
        .await
        .context("Failed to delete old deployments")?;

        Ok(result.rows_affected())
    }
}

fn row_to_deployment(row: DeploymentRow) -> CodeDeployment {
    CodeDeployment {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        environment_id: Uuid::parse_str(&row.environment_id).unwrap_or_default(),
        commit_sha: row.commit_sha,
        commit_message: row.commit_message,
        commit_author: row.commit_author,
        status: DeploymentStatus::from_str(&row.status).unwrap_or_default(),
        requested_by: row.requested_by.and_then(|s| Uuid::parse_str(&s).ok()),
        approved_by: row.approved_by.and_then(|s| Uuid::parse_str(&s).ok()),
        approved_at: row.approved_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        rejected_at: row.rejected_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        rejection_reason: row.rejection_reason,
        started_at: row.started_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        completed_at: row.completed_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        error_message: row.error_message,
        r10k_output: row.r10k_output,
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_webhook_secret() {
        let secret = generate_webhook_secret();
        assert!(!secret.is_empty());
        // Should be URL-safe base64
        assert!(!secret.contains('+'));
        assert!(!secret.contains('/'));
    }
}
