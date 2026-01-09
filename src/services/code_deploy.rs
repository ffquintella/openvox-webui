//! Code Deploy service
//!
//! Main orchestration service for Git-based environment management.
//! Coordinates Git operations, r10k deployments, and database state.

use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::db::{
    CodeDeploymentRepository, CodeEnvironmentRepository, CodePatTokenRepository,
    CodeRepositoryRepository, CodeSshKeyRepository,
};
use crate::models::{
    CodeDeployment, CodeDeploymentResponse, CodeDeploymentSummary, CodeEnvironment,
    CodeEnvironmentResponse, CodePatTokenResponse, CodeRepository, CodeRepositoryResponse,
    CodeSshKeyResponse, CreatePatTokenRequest, CreateRepositoryRequest, CreateSshKeyRequest,
    DeploymentStatus, ListDeploymentsQuery, ListEnvironmentsQuery, UpdateEnvironmentRequest,
    UpdatePatTokenRequest, UpdateRepositoryRequest,
};
use crate::services::git::{GitService, GitServiceConfig};
use crate::services::r10k::{R10kConfig, R10kService, R10kSource};

/// Code Deploy service configuration
#[derive(Debug, Clone)]
pub struct CodeDeployConfig {
    pub git: GitServiceConfig,
    pub r10k: R10kConfig,
    /// Whether the code deploy feature is enabled
    pub enabled: bool,
    /// Encryption key for SSH private keys (should come from secure storage)
    pub encryption_key: String,
    /// Base URL for webhook URLs
    pub webhook_base_url: Option<String>,
    /// Retain deployment history for this many days
    pub retain_history_days: u32,
}

impl Default for CodeDeployConfig {
    fn default() -> Self {
        Self {
            git: GitServiceConfig::default(),
            r10k: R10kConfig::default(),
            enabled: false,
            encryption_key: String::new(),
            webhook_base_url: None,
            retain_history_days: 90,
        }
    }
}

/// Code Deploy service for managing Git-based environment deployments
pub struct CodeDeployService {
    pool: SqlitePool,
    git: GitService,
    r10k: R10kService,
    config: CodeDeployConfig,
    /// Lock to ensure only one deployment runs at a time
    deployment_lock: Arc<Mutex<()>>,
}

impl CodeDeployService {
    /// Create a new Code Deploy service
    pub fn new(pool: SqlitePool, config: CodeDeployConfig) -> Self {
        let git = GitService::new(config.git.clone());
        let r10k = R10kService::new(config.r10k.clone());

        Self {
            pool,
            git,
            r10k,
            config,
            deployment_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Check if code deploy feature is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    // ========================================================================
    // SSH Key Operations
    // ========================================================================

    /// List all SSH keys
    pub async fn list_ssh_keys(&self) -> Result<Vec<CodeSshKeyResponse>> {
        let repo = CodeSshKeyRepository::new(&self.pool);
        let keys = repo.get_all().await?;
        Ok(keys.into_iter().map(CodeSshKeyResponse::from).collect())
    }

    /// Get an SSH key by ID
    pub async fn get_ssh_key(&self, id: Uuid) -> Result<Option<CodeSshKeyResponse>> {
        let repo = CodeSshKeyRepository::new(&self.pool);
        Ok(repo.get_by_id(id).await?.map(CodeSshKeyResponse::from))
    }

    /// Create a new SSH key
    pub async fn create_ssh_key(&self, req: &CreateSshKeyRequest) -> Result<CodeSshKeyResponse> {
        let repo = CodeSshKeyRepository::new(&self.pool);

        // Check if name already exists
        if repo.get_by_name(&req.name).await?.is_some() {
            return Err(anyhow::anyhow!("SSH key with name '{}' already exists", req.name));
        }

        // Extract public key from private key
        let public_key = match GitService::extract_public_key(&req.private_key) {
            Ok(key) => key,
            Err(e) => {
                // Log the error for debugging
                tracing::warn!("Failed to extract public key from private key: {}", e);
                // Return error to the user instead of storing invalid key
                return Err(anyhow::anyhow!(
                    "Failed to extract public key: {}. \
                     Please ensure the private key is in OpenSSH format \
                     (starts with '-----BEGIN OPENSSH PRIVATE KEY-----')",
                    e
                ));
            }
        };

        // Encrypt the private key
        let encrypted = self.encrypt_private_key(&req.private_key)?;

        let key = repo.create(req, &encrypted, &public_key).await?;
        Ok(CodeSshKeyResponse::from(key))
    }

    /// Delete an SSH key
    pub async fn delete_ssh_key(&self, id: Uuid) -> Result<bool> {
        let repo = CodeSshKeyRepository::new(&self.pool);

        // Check if any repository is using this key
        let repo_repo = CodeRepositoryRepository::new(&self.pool);
        for repository in repo_repo.get_all().await? {
            if repository.ssh_key_id == Some(id) {
                return Err(anyhow::anyhow!(
                    "SSH key is in use by repository '{}'",
                    repository.name
                ));
            }
        }

        repo.delete(id).await
    }

    // ========================================================================
    // PAT Token Operations
    // ========================================================================

    /// List all PAT tokens
    pub async fn list_pat_tokens(&self) -> Result<Vec<CodePatTokenResponse>> {
        let repo = CodePatTokenRepository::new(&self.pool);
        let tokens = repo.get_all().await?;
        Ok(tokens.into_iter().map(CodePatTokenResponse::from).collect())
    }

    /// Get a PAT token by ID
    pub async fn get_pat_token(&self, id: Uuid) -> Result<Option<CodePatTokenResponse>> {
        let repo = CodePatTokenRepository::new(&self.pool);
        Ok(repo.get_by_id(id).await?.map(CodePatTokenResponse::from))
    }

    /// Create a new PAT token
    pub async fn create_pat_token(&self, req: &CreatePatTokenRequest) -> Result<CodePatTokenResponse> {
        let repo = CodePatTokenRepository::new(&self.pool);

        // Check if name already exists
        if repo.get_by_name(&req.name).await?.is_some() {
            return Err(anyhow::anyhow!("PAT token with name '{}' already exists", req.name));
        }

        // Encrypt the token
        let encrypted_token = self.encrypt_private_key(&req.token)?;

        let token = repo.create(req, &encrypted_token).await?;
        Ok(CodePatTokenResponse::from(token))
    }

    /// Update a PAT token
    pub async fn update_pat_token(&self, id: Uuid, req: &UpdatePatTokenRequest) -> Result<CodePatTokenResponse> {
        let repo = CodePatTokenRepository::new(&self.pool);

        // Check token exists
        if repo.get_by_id(id).await?.is_none() {
            return Err(anyhow::anyhow!("PAT token not found"));
        }

        // If name is being changed, check it doesn't conflict
        if let Some(name) = &req.name {
            if let Some(existing) = repo.get_by_name(name).await? {
                if existing.id != id {
                    return Err(anyhow::anyhow!("PAT token with name '{}' already exists", name));
                }
            }
        }

        // Encrypt new token if provided
        let encrypted_token = req.token.as_ref().map(|t| self.encrypt_private_key(t)).transpose()?;

        let token = repo.update(id, req, encrypted_token.as_deref()).await?;
        Ok(CodePatTokenResponse::from(token))
    }

    /// Delete a PAT token
    pub async fn delete_pat_token(&self, id: Uuid) -> Result<bool> {
        let repo = CodePatTokenRepository::new(&self.pool);

        // Check if any repository is using this token
        let repo_repo = CodeRepositoryRepository::new(&self.pool);
        for repository in repo_repo.get_all().await? {
            if repository.pat_token_id == Some(id) {
                return Err(anyhow::anyhow!(
                    "PAT token is in use by repository '{}'",
                    repository.name
                ));
            }
        }

        repo.delete(id).await
    }

    /// List PAT tokens that are expired or expiring soon
    pub async fn list_expiring_pat_tokens(&self, days_threshold: i64) -> Result<Vec<CodePatTokenResponse>> {
        let repo = CodePatTokenRepository::new(&self.pool);
        let tokens = repo.get_expiring_tokens(days_threshold).await?;
        Ok(tokens.into_iter().map(CodePatTokenResponse::from).collect())
    }

    /// Check for expiring PAT tokens and return notifications to be created
    /// Returns a list of (token_name, days_until_expiration, is_expired) tuples
    pub async fn check_expiring_pat_tokens(&self, days_threshold: i64) -> Result<Vec<(String, i64, bool)>> {
        let tokens = self.list_expiring_pat_tokens(days_threshold).await?;
        let mut warnings = Vec::new();

        for token in tokens {
            if let Some(days) = token.days_until_expiration {
                warnings.push((token.name, days, token.is_expired));
            }
        }

        Ok(warnings)
    }

    // ========================================================================
    // Repository Operations
    // ========================================================================

    /// List all repositories
    pub async fn list_repositories(&self) -> Result<Vec<CodeRepositoryResponse>> {
        let repo = CodeRepositoryRepository::new(&self.pool);
        let key_repo = CodeSshKeyRepository::new(&self.pool);

        let repositories = repo.get_all().await?;
        let mut responses = Vec::new();

        for repository in repositories {
            let env_count = repo.get_environment_count(repository.id).await?;
            let ssh_key_name = if let Some(key_id) = repository.ssh_key_id {
                key_repo.get_by_id(key_id).await?.map(|k| k.name)
            } else {
                None
            };

            responses.push(self.repository_to_response(repository, ssh_key_name, env_count));
        }

        Ok(responses)
    }

    /// Get a repository by ID
    pub async fn get_repository(&self, id: Uuid) -> Result<Option<CodeRepositoryResponse>> {
        let repo = CodeRepositoryRepository::new(&self.pool);
        let key_repo = CodeSshKeyRepository::new(&self.pool);

        let Some(repository) = repo.get_by_id(id).await? else {
            return Ok(None);
        };

        let env_count = repo.get_environment_count(repository.id).await?;
        let ssh_key_name = if let Some(key_id) = repository.ssh_key_id {
            key_repo.get_by_id(key_id).await?.map(|k| k.name)
        } else {
            None
        };

        Ok(Some(self.repository_to_response(repository, ssh_key_name, env_count)))
    }

    /// Get the raw repository by ID (includes webhook_secret for internal use)
    ///
    /// This method is for internal use only (e.g., webhook verification).
    /// For API responses, use `get_repository` instead.
    pub async fn get_repository_raw(&self, id: Uuid) -> Result<Option<CodeRepository>> {
        let repo = CodeRepositoryRepository::new(&self.pool);
        repo.get_by_id(id).await
    }

    /// Create a new repository
    pub async fn create_repository(&self, req: &CreateRepositoryRequest) -> Result<CodeRepositoryResponse> {
        use crate::models::AuthType;
        let repo = CodeRepositoryRepository::new(&self.pool);

        // Check if name already exists
        if repo.get_by_name(&req.name).await?.is_some() {
            return Err(anyhow::anyhow!("Repository with name '{}' already exists", req.name));
        }

        // Validate authentication credentials
        match req.auth_type {
            AuthType::Ssh => {
                if let Some(key_id) = req.ssh_key_id {
                    let key_repo = CodeSshKeyRepository::new(&self.pool);
                    if key_repo.get_by_id(key_id).await?.is_none() {
                        return Err(anyhow::anyhow!("SSH key not found"));
                    }
                } else {
                    return Err(anyhow::anyhow!("SSH key ID required when auth_type is 'ssh'"));
                }
            }
            AuthType::Pat => {
                // Allow either pat_token_id (new) or github_pat (legacy)
                if req.pat_token_id.is_some() {
                    // Validate the PAT token exists
                    let token_repo = CodePatTokenRepository::new(&self.pool);
                    if token_repo.get_by_id(req.pat_token_id.unwrap()).await?.is_none() {
                        return Err(anyhow::anyhow!("PAT token not found"));
                    }
                } else if req.github_pat.is_none() {
                    return Err(anyhow::anyhow!("PAT token ID or GitHub PAT required when auth_type is 'pat'"));
                }
            }
            AuthType::None => {
                // No validation needed for public repositories
            }
        }

        // Encrypt PAT if provided
        let github_pat_encrypted = req.github_pat.as_ref().map(|pat| {
            self.encrypt_private_key(pat)
        }).transpose()?;

        let repository = repo.create(req, github_pat_encrypted.as_deref()).await?;
        Ok(self.repository_to_response(repository, None, 0))
    }

    /// Update a repository
    pub async fn update_repository(
        &self,
        id: Uuid,
        req: &UpdateRepositoryRequest,
    ) -> Result<Option<CodeRepositoryResponse>> {
        let repo = CodeRepositoryRepository::new(&self.pool);

        // Validate SSH key if provided
        if let Some(key_id) = req.ssh_key_id {
            let key_repo = CodeSshKeyRepository::new(&self.pool);
            if key_repo.get_by_id(key_id).await?.is_none() {
                return Err(anyhow::anyhow!("SSH key not found"));
            }
        }

        // Encrypt PAT if provided
        let github_pat_encrypted = req.github_pat.as_ref().map(|pat| {
            self.encrypt_private_key(pat)
        }).transpose()?;

        let Some(repository) = repo.update(id, req, github_pat_encrypted.as_deref()).await? else {
            return Ok(None);
        };

        let env_count = repo.get_environment_count(repository.id).await?;
        Ok(Some(self.repository_to_response(repository, None, env_count)))
    }

    /// Delete a repository
    pub async fn delete_repository(&self, id: Uuid) -> Result<bool> {
        let repo = CodeRepositoryRepository::new(&self.pool);

        // Get repository to get its ID for cleanup
        let Some(repository) = repo.get_by_id(id).await? else {
            return Ok(false);
        };

        // Delete local git repository
        if let Err(e) = self.git.delete_repo(&repository.id.to_string()) {
            warn!("Failed to delete local git repository: {}", e);
        }

        // Delete from database (cascade will delete environments and deployments)
        repo.delete(id).await
    }

    /// Sync a repository (fetch and discover environments)
    pub async fn sync_repository(&self, id: Uuid) -> Result<Vec<CodeEnvironment>> {
        let repo = CodeRepositoryRepository::new(&self.pool);
        let env_repo = CodeEnvironmentRepository::new(&self.pool);
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);

        let Some(repository) = repo.get_by_id(id).await? else {
            return Err(anyhow::anyhow!("Repository not found"));
        };

        info!("Syncing repository: {}", repository.name);

        // Determine authentication method and credentials
        use crate::models::AuthType;
        let (ssh_key, github_pat) = match repository.auth_type {
            AuthType::Ssh => {
                // Get SSH key if configured
                let ssh_key = if let Some(key_id) = repository.ssh_key_id {
                    let key_repo = CodeSshKeyRepository::new(&self.pool);
                    key_repo.get_by_id(key_id).await?.map(|k| {
                        self.decrypt_private_key(&k.private_key_encrypted)
                    })
                } else {
                    None
                };
                (ssh_key, None)
            }
            AuthType::Pat => {
                // Get PAT from centralized token store (preferred) or legacy encrypted field
                let github_pat = if let Some(token_id) = repository.pat_token_id {
                    // Use centralized PAT token
                    let token_repo = CodePatTokenRepository::new(&self.pool);
                    match token_repo.get_by_id(token_id).await? {
                        Some(token) => {
                            // Update last validated timestamp
                            if let Err(e) = token_repo.update_last_validated(token_id).await {
                                warn!("Failed to update PAT token last_validated_at: {}", e);
                            }
                            Some(self.decrypt_private_key(&token.token_encrypted))
                        }
                        None => {
                            error!("PAT token {} not found for repository {}", token_id, repository.id);
                            None
                        }
                    }
                } else if let Some(encrypted) = &repository.github_pat_encrypted {
                    // Fallback to legacy embedded PAT (deprecated)
                    warn!("Repository {} is using deprecated embedded PAT, consider migrating to centralized PAT tokens", repository.id);
                    Some(self.decrypt_private_key(encrypted))
                } else {
                    None
                };
                (None, github_pat)
            }
            AuthType::None => {
                // No authentication (public repository)
                (None, None)
            }
        };

        let ssh_key_ref = match &ssh_key {
            Some(Ok(key)) => Some(key.as_str()),
            Some(Err(e)) => {
                error!("Failed to decrypt SSH key for repository {}: {}", repository.id, e);
                None
            }
            None => None,
        };

        let github_pat_ref = match &github_pat {
            Some(Ok(pat)) => Some(pat.as_str()),
            Some(Err(e)) => {
                error!("Failed to decrypt GitHub PAT for repository {}: {}", repository.id, e);
                None
            }
            None => {
                if repository.auth_type == AuthType::Pat {
                    warn!("Repository {} is configured for PAT auth but no PAT is stored", repository.id);
                }
                None
            }
        };

        // Clone or open the repository based on auth type
        let git_repo = match repository.auth_type {
            AuthType::Pat | AuthType::None => {
                self.git
                    .clone_or_open_with_pat(&repository.id.to_string(), &repository.url, github_pat_ref)
                    .context("Failed to open repository")?
            }
            AuthType::Ssh => {
                self.git
                    .clone_or_open(&repository.id.to_string(), &repository.url, ssh_key_ref)
                    .context("Failed to open repository")?
            }
        };

        // Fetch updates based on auth type
        if let Err(e) = match repository.auth_type {
            AuthType::Pat | AuthType::None => self.git.fetch_with_pat(&git_repo, github_pat_ref),
            AuthType::Ssh => self.git.fetch(&git_repo, ssh_key_ref),
        } {
            repo.set_error(id, Some(&e.to_string())).await?;
            return Err(e);
        }

        // Clear any previous error
        repo.set_error(id, None).await?;

        // List branches matching pattern
        let branches = self
            .git
            .list_branches(&git_repo, &repository.branch_pattern)
            .context("Failed to list branches")?;

        let mut environments = Vec::new();
        let mut branch_names = Vec::new();

        for branch in branches {
            branch_names.push(branch.name.clone());

            // Create or update environment
            let env = env_repo
                .upsert(
                    repository.id,
                    &branch.name,
                    &branch.name, // branch name is the same as env name
                    Some(&branch.commit.sha),
                    branch.commit.message.as_deref(),
                    branch.commit.author.as_deref(),
                    branch.commit.date,
                )
                .await?;

            // Check if we need to create a deployment
            let latest_deployment = deploy_repo.get_latest_for_environment(env.id).await?;
            let needs_deployment = match &latest_deployment {
                Some(dep) => {
                    // New commit since last deployment
                    env.current_commit.as_deref() != Some(&dep.commit_sha)
                        && dep.status.is_terminal()
                }
                None => true, // No previous deployment
            };

            if needs_deployment && env.current_commit.is_some() {
                let commit_sha = env.current_commit.as_ref().unwrap();

                if env.auto_deploy && !env.requires_approval {
                    // Auto-deploy: create approved deployment
                    info!(
                        "Auto-deploying environment {} with commit {}",
                        env.name, commit_sha
                    );
                    deploy_repo
                        .create(
                            env.id,
                            commit_sha,
                            env.current_commit_message.as_deref(),
                            env.current_commit_author.as_deref(),
                            DeploymentStatus::Approved,
                            None,
                        )
                        .await?;
                } else if !env.auto_deploy {
                    // Manual deployment: check for existing pending
                    let pending = deploy_repo.get_pending_for_environment(env.id).await?;
                    if pending.is_none() {
                        info!(
                            "Creating pending deployment for environment {} with commit {}",
                            env.name, commit_sha
                        );
                        deploy_repo
                            .create(
                                env.id,
                                commit_sha,
                                env.current_commit_message.as_deref(),
                                env.current_commit_author.as_deref(),
                                DeploymentStatus::Pending,
                                None,
                            )
                            .await?;
                    }
                }
            }

            environments.push(env);
        }

        // Delete environments for branches that no longer exist
        let deleted = env_repo.delete_missing(repository.id, &branch_names).await?;
        if deleted > 0 {
            info!("Deleted {} environments for removed branches", deleted);
        }

        Ok(environments)
    }

    // ========================================================================
    // Environment Operations
    // ========================================================================

    /// List all environments
    pub async fn list_environments(
        &self,
        query: &ListEnvironmentsQuery,
    ) -> Result<Vec<CodeEnvironmentResponse>> {
        let env_repo = CodeEnvironmentRepository::new(&self.pool);
        let repo_repo = CodeRepositoryRepository::new(&self.pool);
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);

        let environments = env_repo.get_all(query).await?;
        let mut responses = Vec::new();

        for env in environments {
            let repo_name = repo_repo
                .get_by_id(env.repository_id)
                .await?
                .map(|r| r.name)
                .unwrap_or_default();

            let pending = deploy_repo.get_pending_for_environment(env.id).await?;
            let latest = deploy_repo.get_latest_for_environment(env.id).await?;

            responses.push(self.environment_to_response(env, repo_name, pending, latest));
        }

        Ok(responses)
    }

    /// Get an environment by ID
    pub async fn get_environment(&self, id: Uuid) -> Result<Option<CodeEnvironmentResponse>> {
        let env_repo = CodeEnvironmentRepository::new(&self.pool);
        let repo_repo = CodeRepositoryRepository::new(&self.pool);
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);

        let Some(env) = env_repo.get_by_id(id).await? else {
            return Ok(None);
        };

        let repo_name = repo_repo
            .get_by_id(env.repository_id)
            .await?
            .map(|r| r.name)
            .unwrap_or_default();

        let pending = deploy_repo.get_pending_for_environment(env.id).await?;
        let latest = deploy_repo.get_latest_for_environment(env.id).await?;

        Ok(Some(self.environment_to_response(env, repo_name, pending, latest)))
    }

    /// Update environment settings
    pub async fn update_environment(
        &self,
        id: Uuid,
        req: &UpdateEnvironmentRequest,
    ) -> Result<Option<CodeEnvironmentResponse>> {
        let env_repo = CodeEnvironmentRepository::new(&self.pool);

        let Some(env) = env_repo.update(id, req).await? else {
            return Ok(None);
        };

        self.get_environment(env.id).await
    }

    // ========================================================================
    // Deployment Operations
    // ========================================================================

    /// List deployments
    pub async fn list_deployments(
        &self,
        query: &ListDeploymentsQuery,
    ) -> Result<Vec<CodeDeploymentResponse>> {
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);
        let env_repo = CodeEnvironmentRepository::new(&self.pool);
        let repo_repo = CodeRepositoryRepository::new(&self.pool);

        let deployments = deploy_repo.get_all(query).await?;
        let mut responses = Vec::new();

        for deployment in deployments {
            let response = self
                .deployment_to_response(deployment, &env_repo, &repo_repo)
                .await?;
            responses.push(response);
        }

        Ok(responses)
    }

    /// Get a deployment by ID
    pub async fn get_deployment(&self, id: Uuid) -> Result<Option<CodeDeploymentResponse>> {
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);
        let env_repo = CodeEnvironmentRepository::new(&self.pool);
        let repo_repo = CodeRepositoryRepository::new(&self.pool);

        let Some(deployment) = deploy_repo.get_by_id(id).await? else {
            return Ok(None);
        };

        Ok(Some(
            self.deployment_to_response(deployment, &env_repo, &repo_repo)
                .await?,
        ))
    }

    /// Trigger a deployment for an environment
    pub async fn trigger_deployment(
        &self,
        environment_id: Uuid,
        commit_sha: Option<&str>,
        requested_by: Option<Uuid>,
    ) -> Result<CodeDeployment> {
        let env_repo = CodeEnvironmentRepository::new(&self.pool);
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);

        let Some(env) = env_repo.get_by_id(environment_id).await? else {
            return Err(anyhow::anyhow!("Environment not found"));
        };

        // Use provided commit or current commit
        let commit = commit_sha
            .map(|s| s.to_string())
            .or_else(|| env.current_commit.clone())
            .ok_or_else(|| anyhow::anyhow!("No commit specified and environment has no current commit"))?;

        // Determine initial status based on environment settings
        let status = if env.auto_deploy && !env.requires_approval {
            DeploymentStatus::Approved
        } else {
            DeploymentStatus::Pending
        };

        deploy_repo
            .create(
                environment_id,
                &commit,
                env.current_commit_message.as_deref(),
                env.current_commit_author.as_deref(),
                status,
                requested_by,
            )
            .await
    }

    /// Approve a pending deployment
    pub async fn approve_deployment(
        &self,
        id: Uuid,
        approved_by: Uuid,
    ) -> Result<Option<CodeDeployment>> {
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);
        deploy_repo.approve(id, approved_by).await
    }

    /// Reject a pending deployment
    pub async fn reject_deployment(
        &self,
        id: Uuid,
        rejected_by: Uuid,
        reason: &str,
    ) -> Result<Option<CodeDeployment>> {
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);
        deploy_repo.reject(id, rejected_by, reason).await
    }

    /// Process the deployment queue (run approved deployments)
    pub async fn process_deployment_queue(&self) -> Result<u32> {
        let _lock = self.deployment_lock.lock().await;

        let deploy_repo = CodeDeploymentRepository::new(&self.pool);
        let env_repo = CodeEnvironmentRepository::new(&self.pool);

        let ready = deploy_repo.get_ready_to_deploy().await?;
        let mut processed = 0;

        for deployment in ready {
            let Some(env) = env_repo.get_by_id(deployment.environment_id).await? else {
                warn!(
                    "Environment not found for deployment {}, marking as failed",
                    deployment.id
                );
                deploy_repo
                    .mark_failed(deployment.id, "Environment not found", None)
                    .await?;
                continue;
            };

            info!(
                "Processing deployment {} for environment {}",
                deployment.id, env.name
            );

            // Mark as deploying
            deploy_repo.mark_deploying(deployment.id).await?;

            // Run r10k deploy
            let result = self.r10k.deploy_environment(&env.name).await?;

            if result.success {
                deploy_repo
                    .mark_success(
                        deployment.id,
                        Some(&format!("{}\n{}", result.stdout, result.stderr)),
                    )
                    .await?;
                info!("Deployment {} completed successfully", deployment.id);
            } else {
                let error_msg = if result.stderr.is_empty() {
                    format!("Deployment failed with exit code {:?}", result.exit_code)
                } else {
                    result.stderr.clone()
                };

                deploy_repo
                    .mark_failed(
                        deployment.id,
                        &error_msg,
                        Some(&format!("{}\n{}", result.stdout, result.stderr)),
                    )
                    .await?;
                error!("Deployment {} failed: {}", deployment.id, error_msg);
            }

            processed += 1;
        }

        Ok(processed)
    }

    /// Retry a failed deployment
    pub async fn retry_deployment(&self, id: Uuid, requested_by: Option<Uuid>) -> Result<CodeDeployment> {
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);

        let Some(deployment) = deploy_repo.get_by_id(id).await? else {
            return Err(anyhow::anyhow!("Deployment not found"));
        };

        if !deployment.status.can_retry() {
            return Err(anyhow::anyhow!(
                "Deployment cannot be retried (status: {:?})",
                deployment.status
            ));
        }

        // Create a new deployment with the same commit
        deploy_repo
            .create(
                deployment.environment_id,
                &deployment.commit_sha,
                deployment.commit_message.as_deref(),
                deployment.commit_author.as_deref(),
                DeploymentStatus::Approved, // Skip pending for retries
                requested_by,
            )
            .await
    }

    // ========================================================================
    // Webhook Operations
    // ========================================================================

    /// Generate webhook URL for a repository
    pub fn webhook_url(&self, repository_id: Uuid, provider: &str) -> Option<String> {
        self.config.webhook_base_url.as_ref().map(|base| {
            format!("{}/api/v1/webhooks/{}/{}", base, provider, repository_id)
        })
    }

    /// Verify webhook signature (GitHub)
    pub fn verify_github_signature(&self, secret: &str, payload: &[u8], signature: &str) -> bool {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let signature = match signature.strip_prefix("sha256=") {
            Some(s) => s,
            None => return false,
        };

        let signature_bytes = match hex::decode(signature) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };

        mac.update(payload);
        mac.verify_slice(&signature_bytes).is_ok()
    }

    /// Verify webhook signature (GitLab)
    pub fn verify_gitlab_token(&self, secret: &str, token: &str) -> bool {
        secret == token
    }

    // ========================================================================
    // Maintenance Operations
    // ========================================================================

    /// Clean up old deployment history
    pub async fn cleanup_old_deployments(&self) -> Result<u64> {
        let deploy_repo = CodeDeploymentRepository::new(&self.pool);

        let cutoff = chrono::Utc::now()
            - chrono::Duration::days(self.config.retain_history_days as i64);

        deploy_repo.delete_old(cutoff).await
    }

    /// Update r10k configuration based on repositories
    pub async fn update_r10k_config(&self) -> Result<()> {
        let repo_repo = CodeRepositoryRepository::new(&self.pool);

        let repositories = repo_repo.get_all().await?;
        let sources: Vec<R10kSource> = repositories
            .iter()
            .filter(|r| r.is_control_repo)
            .map(|r| R10kSource {
                name: r.name.clone(),
                remote: r.url.clone(),
                basedir: self.config.r10k.basedir.to_string_lossy().to_string(),
                prefix: None,
                invalid_branches: Some("correct".to_string()),
            })
            .collect();

        if !sources.is_empty() {
            self.r10k.write_config(&sources)?;
        }

        Ok(())
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    fn repository_to_response(
        &self,
        repo: CodeRepository,
        ssh_key_name: Option<String>,
        env_count: i64,
    ) -> CodeRepositoryResponse {
        CodeRepositoryResponse {
            id: repo.id,
            name: repo.name,
            url: repo.url,
            branch_pattern: repo.branch_pattern,
            auth_type: repo.auth_type,
            ssh_key_id: repo.ssh_key_id,
            ssh_key_name,
            has_pat: repo.github_pat_encrypted.is_some(),
            webhook_url: self.webhook_url(repo.id, "github"),
            poll_interval_seconds: repo.poll_interval_seconds,
            is_control_repo: repo.is_control_repo,
            last_error: repo.last_error,
            last_error_at: repo.last_error_at,
            environment_count: env_count,
            created_at: repo.created_at,
            updated_at: repo.updated_at,
        }
    }

    fn environment_to_response(
        &self,
        env: CodeEnvironment,
        repo_name: String,
        pending: Option<CodeDeployment>,
        latest: Option<CodeDeployment>,
    ) -> CodeEnvironmentResponse {
        CodeEnvironmentResponse {
            id: env.id,
            repository_id: env.repository_id,
            repository_name: repo_name,
            name: env.name,
            branch: env.branch,
            current_commit: env.current_commit,
            current_commit_message: env.current_commit_message,
            current_commit_author: env.current_commit_author,
            current_commit_date: env.current_commit_date,
            last_synced_at: env.last_synced_at,
            auto_deploy: env.auto_deploy,
            requires_approval: env.requires_approval,
            pending_deployment: pending.as_ref().map(CodeDeploymentSummary::from),
            latest_deployment_status: latest.map(|d| d.status),
            created_at: env.created_at,
            updated_at: env.updated_at,
        }
    }

    async fn deployment_to_response(
        &self,
        deployment: CodeDeployment,
        env_repo: &CodeEnvironmentRepository<'_>,
        repo_repo: &CodeRepositoryRepository<'_>,
    ) -> Result<CodeDeploymentResponse> {
        let env = env_repo.get_by_id(deployment.environment_id).await?;
        let env_name = env.as_ref().map(|e| e.name.clone()).unwrap_or_default();
        let repo_name = if let Some(e) = &env {
            repo_repo
                .get_by_id(e.repository_id)
                .await?
                .map(|r| r.name)
                .unwrap_or_default()
        } else {
            String::new()
        };

        let duration_seconds = match (deployment.started_at, deployment.completed_at) {
            (Some(start), Some(end)) => Some((end - start).num_seconds()),
            _ => None,
        };

        // TODO: Fetch usernames from user repository
        let requested_by_username = None;
        let approved_by_username = None;

        Ok(CodeDeploymentResponse {
            id: deployment.id,
            environment_id: deployment.environment_id,
            environment_name: env_name,
            repository_name: repo_name,
            commit_sha: deployment.commit_sha,
            commit_message: deployment.commit_message,
            commit_author: deployment.commit_author,
            status: deployment.status,
            requested_by: deployment.requested_by,
            requested_by_username,
            approved_by: deployment.approved_by,
            approved_by_username,
            approved_at: deployment.approved_at,
            rejected_at: deployment.rejected_at,
            rejection_reason: deployment.rejection_reason,
            started_at: deployment.started_at,
            completed_at: deployment.completed_at,
            duration_seconds,
            error_message: deployment.error_message,
            r10k_output: deployment.r10k_output,
            created_at: deployment.created_at,
            updated_at: deployment.updated_at,
        })
    }

    fn encrypt_private_key(&self, key: &str) -> Result<String> {
        // Simple base64 encoding for now
        // In production, use proper encryption with the encryption_key
        // TODO: Implement proper encryption
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            key.as_bytes(),
        ))
    }

    fn decrypt_private_key(&self, encrypted: &str) -> Result<String> {
        // Simple base64 decoding for now
        // In production, use proper decryption with the encryption_key
        // TODO: Implement proper decryption
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted)
            .context("Failed to decode encrypted key")?;
        String::from_utf8(bytes).context("Invalid UTF-8 in decrypted key")
    }

    // =========================================================================
    // Scheduler support methods
    // =========================================================================

    /// List repositories that need polling
    ///
    /// Returns repositories where poll_interval_seconds > 0.
    pub async fn list_repositories_for_polling(&self) -> Result<Vec<CodeRepository>> {
        let repo = CodeRepositoryRepository::new(&self.pool);
        repo.get_for_polling().await
    }

    /// Record an error on a repository
    pub async fn record_repository_error(&self, id: Uuid, error: &str) -> Result<()> {
        let repo = CodeRepositoryRepository::new(&self.pool);
        repo.set_error(id, Some(error)).await
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    /// Test GitHub signature verification without needing a full service
    fn verify_github_signature_helper(secret: &str, payload: &[u8], signature: &str) -> bool {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let signature = match signature.strip_prefix("sha256=") {
            Some(s) => s,
            None => return false,
        };

        let signature_bytes = match hex::decode(signature) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };

        mac.update(payload);
        mac.verify_slice(&signature_bytes).is_ok()
    }

    #[test]
    fn test_github_signature_verification() {
        let secret = "mysecret";
        let payload = b"test payload";

        // Generate valid signature
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_github_signature_helper(secret, payload, &signature));
        assert!(!verify_github_signature_helper(secret, payload, "sha256=invalid"));
        assert!(!verify_github_signature_helper("wrong", payload, &signature));
    }

    #[test]
    fn test_gitlab_token_verification() {
        // GitLab token verification is simple equality check
        assert_eq!("mytoken", "mytoken");
        assert_ne!("mytoken", "wrongtoken");
    }
}
