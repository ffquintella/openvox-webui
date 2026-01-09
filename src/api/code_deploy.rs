//! Code Deploy API endpoints
//!
//! Provides REST API for managing Git repositories, environments, and deployments.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    middleware::AuthUser,
    models::{
        ApproveDeploymentRequest, CodeDeploymentResponse, CodeEnvironmentResponse,
        CodePatTokenResponse, CodeRepositoryResponse, CodeSshKeyResponse,
        CreatePatTokenRequest, CreateRepositoryRequest, CreateSshKeyRequest,
        ListDeploymentsQuery, ListEnvironmentsQuery, RejectDeploymentRequest,
        TriggerDeploymentRequest, UpdateEnvironmentRequest, UpdatePatTokenRequest,
        UpdateRepositoryRequest,
    },
    utils::AppError,
    AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Feature status (accessible to all authenticated users)
        .route("/status", get(get_feature_status))
        // SSH Keys
        .route("/ssh-keys", get(list_ssh_keys).post(create_ssh_key))
        .route("/ssh-keys/{id}", get(get_ssh_key).delete(delete_ssh_key))
        // PAT Tokens
        .route("/pat-tokens", get(list_pat_tokens).post(create_pat_token))
        .route(
            "/pat-tokens/{id}",
            get(get_pat_token).put(update_pat_token).delete(delete_pat_token),
        )
        .route("/pat-tokens/expiring", get(list_expiring_pat_tokens))
        // Repositories
        .route("/repositories", get(list_repositories).post(create_repository))
        .route(
            "/repositories/{id}",
            get(get_repository).put(update_repository).delete(delete_repository),
        )
        .route("/repositories/{id}/sync", post(sync_repository))
        // Environments
        .route("/environments", get(list_environments))
        .route(
            "/environments/{id}",
            get(get_environment).put(update_environment),
        )
        .route("/environments/{id}/deployments", get(list_environment_deployments))
        // Deployments
        .route("/deployments", get(list_deployments).post(trigger_deployment))
        .route("/deployments/{id}", get(get_deployment))
        .route("/deployments/{id}/approve", post(approve_deployment))
        .route("/deployments/{id}/reject", post(reject_deployment))
        .route("/deployments/{id}/retry", post(retry_deployment))
}

pub fn webhook_routes() -> Router<AppState> {
    Router::new()
        .route("/github/{repo_id}", post(handle_github_webhook))
        .route("/gitlab/{repo_id}", post(handle_gitlab_webhook))
        .route("/bitbucket/{repo_id}", post(handle_bitbucket_webhook))
}

// ============================================================================
// Feature Status
// ============================================================================

#[derive(serde::Serialize)]
pub struct CodeDeployFeatureStatus {
    pub enabled: bool,
    pub message: Option<String>,
}

/// Get Code Deploy feature status
///
/// Returns whether the feature is enabled and any relevant message.
/// This endpoint is accessible to all authenticated users and doesn't require
/// the feature to be enabled (unlike other endpoints).
async fn get_feature_status(
    State(state): State<AppState>,
    _auth_user: AuthUser,
) -> Json<CodeDeployFeatureStatus> {
    let enabled = state.code_deploy_config.as_ref().map_or(false, |c| c.enabled);

    let message = if !enabled {
        Some("Code Deploy feature is not enabled. To enable it, add the following to your config.yaml:\n\ncode_deploy:\n  enabled: true\n  repos_base_dir: /var/lib/openvox-webui/code-deploy/repos\n  ssh_keys_dir: /etc/openvox-webui/code-deploy/ssh-keys\n  r10k_path: /opt/puppetlabs/puppet/bin/r10k\n  encryption_key: <your-encryption-key>".to_string())
    } else {
        None
    };

    Json(CodeDeployFeatureStatus { enabled, message })
}

// ============================================================================
// SSH Key Handlers
// ============================================================================

async fn list_ssh_keys(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Vec<CodeSshKeyResponse>>, AppError> {
    require_permission(&auth_user, "code_ssh_key_view")?;

    let service = state.code_deploy_service()?;
    let keys = service.list_ssh_keys().await.map_err(|e| {
        tracing::error!("Failed to list SSH keys: {}", e);
        AppError::internal("Failed to list SSH keys")
    })?;

    Ok(Json(keys))
}

async fn get_ssh_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CodeSshKeyResponse>, AppError> {
    require_permission(&auth_user, "code_ssh_key_view")?;

    let service = state.code_deploy_service()?;
    let key = service
        .get_ssh_key(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get SSH key: {}", e);
            AppError::internal("Failed to get SSH key")
        })?
        .ok_or_else(|| AppError::not_found("SSH key not found"))?;

    Ok(Json(key))
}

async fn create_ssh_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateSshKeyRequest>,
) -> Result<(StatusCode, Json<CodeSshKeyResponse>), AppError> {
    require_permission(&auth_user, "code_ssh_key_manage")?;

    let service = state.code_deploy_service()?;
    let key = service.create_ssh_key(&payload).await.map_err(|e| {
        tracing::error!("Failed to create SSH key: {}", e);
        if e.to_string().contains("already exists") {
            AppError::conflict("SSH key with this name already exists")
        } else {
            AppError::internal("Failed to create SSH key")
        }
    })?;

    Ok((StatusCode::CREATED, Json(key)))
}

async fn delete_ssh_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_permission(&auth_user, "code_ssh_key_manage")?;

    let service = state.code_deploy_service()?;
    let deleted = service.delete_ssh_key(id).await.map_err(|e| {
        tracing::error!("Failed to delete SSH key: {}", e);
        if e.to_string().contains("in use") {
            AppError::conflict(&e.to_string())
        } else {
            AppError::internal("Failed to delete SSH key")
        }
    })?;

    if !deleted {
        return Err(AppError::not_found("SSH key not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// PAT Token Handlers
// ============================================================================

async fn list_pat_tokens(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Vec<CodePatTokenResponse>>, AppError> {
    require_permission(&auth_user, "code_pat_token_view")?;

    let service = state.code_deploy_service()?;
    let tokens = service.list_pat_tokens().await.map_err(|e| {
        tracing::error!("Failed to list PAT tokens: {}", e);
        AppError::internal("Failed to list PAT tokens")
    })?;

    Ok(Json(tokens))
}

async fn get_pat_token(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CodePatTokenResponse>, AppError> {
    require_permission(&auth_user, "code_pat_token_view")?;

    let service = state.code_deploy_service()?;
    let token = service
        .get_pat_token(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get PAT token: {}", e);
            AppError::internal("Failed to get PAT token")
        })?
        .ok_or_else(|| AppError::not_found("PAT token not found"))?;

    Ok(Json(token))
}

async fn create_pat_token(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreatePatTokenRequest>,
) -> Result<(StatusCode, Json<CodePatTokenResponse>), AppError> {
    require_permission(&auth_user, "code_pat_token_manage")?;

    let service = state.code_deploy_service()?;
    let token = service.create_pat_token(&payload).await.map_err(|e| {
        tracing::error!("Failed to create PAT token: {}", e);
        if e.to_string().contains("already exists") {
            AppError::conflict("PAT token with this name already exists")
        } else {
            AppError::internal("Failed to create PAT token")
        }
    })?;

    Ok((StatusCode::CREATED, Json(token)))
}

async fn update_pat_token(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdatePatTokenRequest>,
) -> Result<Json<CodePatTokenResponse>, AppError> {
    require_permission(&auth_user, "code_pat_token_manage")?;

    let service = state.code_deploy_service()?;
    let token = service.update_pat_token(id, &payload).await.map_err(|e| {
        tracing::error!("Failed to update PAT token: {}", e);
        if e.to_string().contains("not found") {
            AppError::not_found("PAT token not found")
        } else if e.to_string().contains("already exists") {
            AppError::conflict("PAT token with this name already exists")
        } else {
            AppError::internal("Failed to update PAT token")
        }
    })?;

    Ok(Json(token))
}

async fn delete_pat_token(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_permission(&auth_user, "code_pat_token_manage")?;

    let service = state.code_deploy_service()?;
    let deleted = service.delete_pat_token(id).await.map_err(|e| {
        tracing::error!("Failed to delete PAT token: {}", e);
        if e.to_string().contains("in use") {
            AppError::conflict(&e.to_string())
        } else {
            AppError::internal("Failed to delete PAT token")
        }
    })?;

    if !deleted {
        return Err(AppError::not_found("PAT token not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ExpiringTokensQuery {
    /// Number of days to look ahead (default: 30)
    #[serde(default = "default_expiring_days")]
    days: i64,
}

fn default_expiring_days() -> i64 {
    30
}

async fn list_expiring_pat_tokens(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ExpiringTokensQuery>,
) -> Result<Json<Vec<CodePatTokenResponse>>, AppError> {
    require_permission(&auth_user, "code_pat_token_view")?;

    let service = state.code_deploy_service()?;
    let tokens = service.list_expiring_pat_tokens(query.days).await.map_err(|e| {
        tracing::error!("Failed to list expiring PAT tokens: {}", e);
        AppError::internal("Failed to list expiring PAT tokens")
    })?;

    Ok(Json(tokens))
}

// ============================================================================
// Repository Handlers
// ============================================================================

async fn list_repositories(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<Vec<CodeRepositoryResponse>>, AppError> {
    require_permission(&auth_user, "code_repository_view")?;

    let service = state.code_deploy_service()?;
    let repos = service.list_repositories().await.map_err(|e| {
        tracing::error!("Failed to list repositories: {}", e);
        AppError::internal("Failed to list repositories")
    })?;

    Ok(Json(repos))
}

async fn get_repository(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CodeRepositoryResponse>, AppError> {
    require_permission(&auth_user, "code_repository_view")?;

    let service = state.code_deploy_service()?;
    let repo = service
        .get_repository(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get repository: {}", e);
            AppError::internal("Failed to get repository")
        })?
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    Ok(Json(repo))
}

async fn create_repository(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<CreateRepositoryRequest>,
) -> Result<(StatusCode, Json<CodeRepositoryResponse>), AppError> {
    require_permission(&auth_user, "code_repository_manage")?;

    let service = state.code_deploy_service()?;
    let repo = service.create_repository(&payload).await.map_err(|e| {
        tracing::error!("Failed to create repository: {}", e);
        if e.to_string().contains("already exists") {
            AppError::conflict("Repository with this name already exists")
        } else if e.to_string().contains("SSH key not found") {
            AppError::bad_request("SSH key not found")
        } else {
            AppError::internal("Failed to create repository")
        }
    })?;

    Ok((StatusCode::CREATED, Json(repo)))
}

async fn update_repository(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateRepositoryRequest>,
) -> Result<Json<CodeRepositoryResponse>, AppError> {
    require_permission(&auth_user, "code_repository_manage")?;

    let service = state.code_deploy_service()?;
    let repo = service
        .update_repository(id, &payload)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update repository: {}", e);
            if e.to_string().contains("SSH key not found") {
                AppError::bad_request("SSH key not found")
            } else {
                AppError::internal("Failed to update repository")
            }
        })?
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    Ok(Json(repo))
}

async fn delete_repository(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_permission(&auth_user, "code_repository_manage")?;

    let service = state.code_deploy_service()?;
    let deleted = service.delete_repository(id).await.map_err(|e| {
        tracing::error!("Failed to delete repository: {}", e);
        AppError::internal("Failed to delete repository")
    })?;

    if !deleted {
        return Err(AppError::not_found("Repository not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn sync_repository(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CodeEnvironmentResponse>>, AppError> {
    require_permission(&auth_user, "code_repository_manage")?;

    let service = state.code_deploy_service()?;

    // Sync returns the list of environments
    let _environments = service.sync_repository(id).await.map_err(|e| {
        tracing::error!("Failed to sync repository: {}", e);
        if e.to_string().contains("not found") {
            AppError::not_found("Repository not found")
        } else {
            AppError::internal(&format!("Failed to sync repository: {}", e))
        }
    })?;

    // Convert to responses
    let responses = service
        .list_environments(&ListEnvironmentsQuery {
            repository_id: Some(id),
            ..Default::default()
        })
        .await
        .map_err(|e| {
            tracing::error!("Failed to get environments: {}", e);
            AppError::internal("Failed to get environments")
        })?;

    Ok(Json(responses))
}

// ============================================================================
// Environment Handlers
// ============================================================================

async fn list_environments(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ListEnvironmentsQuery>,
) -> Result<Json<Vec<CodeEnvironmentResponse>>, AppError> {
    require_permission(&auth_user, "code_environment_view")?;

    let service = state.code_deploy_service()?;
    let envs = service.list_environments(&query).await.map_err(|e| {
        tracing::error!("Failed to list environments: {}", e);
        AppError::internal("Failed to list environments")
    })?;

    Ok(Json(envs))
}

async fn get_environment(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CodeEnvironmentResponse>, AppError> {
    require_permission(&auth_user, "code_environment_view")?;

    let service = state.code_deploy_service()?;
    let env = service
        .get_environment(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get environment: {}", e);
            AppError::internal("Failed to get environment")
        })?
        .ok_or_else(|| AppError::not_found("Environment not found"))?;

    Ok(Json(env))
}

async fn update_environment(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateEnvironmentRequest>,
) -> Result<Json<CodeEnvironmentResponse>, AppError> {
    require_permission(&auth_user, "code_environment_manage")?;

    let service = state.code_deploy_service()?;
    let env = service
        .update_environment(id, &payload)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update environment: {}", e);
            AppError::internal("Failed to update environment")
        })?
        .ok_or_else(|| AppError::not_found("Environment not found"))?;

    Ok(Json(env))
}

async fn list_environment_deployments(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CodeDeploymentResponse>>, AppError> {
    require_permission(&auth_user, "code_deployment_view")?;

    let service = state.code_deploy_service()?;
    let deployments = service
        .list_deployments(&ListDeploymentsQuery {
            environment_id: Some(id),
            ..Default::default()
        })
        .await
        .map_err(|e| {
            tracing::error!("Failed to list deployments: {}", e);
            AppError::internal("Failed to list deployments")
        })?;

    Ok(Json(deployments))
}

// ============================================================================
// Deployment Handlers
// ============================================================================

async fn list_deployments(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ListDeploymentsQuery>,
) -> Result<Json<Vec<CodeDeploymentResponse>>, AppError> {
    require_permission(&auth_user, "code_deployment_view")?;

    let service = state.code_deploy_service()?;
    let deployments = service.list_deployments(&query).await.map_err(|e| {
        tracing::error!("Failed to list deployments: {}", e);
        AppError::internal("Failed to list deployments")
    })?;

    Ok(Json(deployments))
}

async fn get_deployment(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CodeDeploymentResponse>, AppError> {
    require_permission(&auth_user, "code_deployment_view")?;

    let service = state.code_deploy_service()?;
    let deployment = service
        .get_deployment(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get deployment: {}", e);
            AppError::internal("Failed to get deployment")
        })?
        .ok_or_else(|| AppError::not_found("Deployment not found"))?;

    Ok(Json(deployment))
}

async fn trigger_deployment(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<TriggerDeploymentRequest>,
) -> Result<(StatusCode, Json<CodeDeploymentResponse>), AppError> {
    require_permission(&auth_user, "code_deployment_trigger")?;

    let service = state.code_deploy_service()?;
    let deployment = service
        .trigger_deployment(
            payload.environment_id,
            payload.commit_sha.as_deref(),
            Some(auth_user.user_id()),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to trigger deployment: {}", e);
            if e.to_string().contains("not found") {
                AppError::not_found("Environment not found")
            } else {
                AppError::internal(&format!("Failed to trigger deployment: {}", e))
            }
        })?;

    let response = service
        .get_deployment(deployment.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get deployment: {}", e);
            AppError::internal("Failed to get deployment")
        })?
        .ok_or_else(|| AppError::internal("Deployment created but not found"))?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn approve_deployment(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(_payload): Json<ApproveDeploymentRequest>,
) -> Result<Json<CodeDeploymentResponse>, AppError> {
    require_permission(&auth_user, "code_deployment_approve")?;

    let service = state.code_deploy_service()?;
    let _deployment = service
        .approve_deployment(id, auth_user.user_id())
        .await
        .map_err(|e| {
            tracing::error!("Failed to approve deployment: {}", e);
            AppError::internal("Failed to approve deployment")
        })?
        .ok_or_else(|| AppError::bad_request("Deployment not found or not pending"))?;

    let response = service
        .get_deployment(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get deployment: {}", e);
            AppError::internal("Failed to get deployment")
        })?
        .ok_or_else(|| AppError::internal("Deployment approved but not found"))?;

    Ok(Json(response))
}

async fn reject_deployment(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<RejectDeploymentRequest>,
) -> Result<Json<CodeDeploymentResponse>, AppError> {
    require_permission(&auth_user, "code_deployment_approve")?;

    let service = state.code_deploy_service()?;
    let _deployment = service
        .reject_deployment(id, auth_user.user_id(), &payload.reason)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reject deployment: {}", e);
            AppError::internal("Failed to reject deployment")
        })?
        .ok_or_else(|| AppError::bad_request("Deployment not found or not pending"))?;

    let response = service
        .get_deployment(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get deployment: {}", e);
            AppError::internal("Failed to get deployment")
        })?
        .ok_or_else(|| AppError::internal("Deployment rejected but not found"))?;

    Ok(Json(response))
}

async fn retry_deployment(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<CodeDeploymentResponse>), AppError> {
    require_permission(&auth_user, "code_deployment_trigger")?;

    let service = state.code_deploy_service()?;
    let deployment = service
        .retry_deployment(id, Some(auth_user.user_id()))
        .await
        .map_err(|e| {
            tracing::error!("Failed to retry deployment: {}", e);
            if e.to_string().contains("not found") {
                AppError::not_found("Deployment not found")
            } else if e.to_string().contains("cannot be retried") {
                AppError::bad_request(&e.to_string())
            } else {
                AppError::internal("Failed to retry deployment")
            }
        })?;

    let response = service
        .get_deployment(deployment.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get deployment: {}", e);
            AppError::internal("Failed to get deployment")
        })?
        .ok_or_else(|| AppError::internal("Deployment created but not found"))?;

    Ok((StatusCode::CREATED, Json(response)))
}

// ============================================================================
// Webhook Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubPushEvent {
    #[serde(rename = "ref")]
    ref_name: String,
    after: String,
    head_commit: Option<GitHubCommit>,
    sender: Option<GitHubSender>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubCommit {
    message: Option<String>,
    author: Option<GitHubAuthor>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubAuthor {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubSender {
    login: Option<String>,
}

async fn handle_github_webhook(
    State(state): State<AppState>,
    Path(repo_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, AppError> {
    let service = state.code_deploy_service()?;

    // Get raw repository to verify webhook secret
    let repo = service
        .get_repository_raw(repo_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get repository for webhook: {}", e);
            AppError::internal("Failed to process webhook")
        })?
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    // Verify signature
    if let Some(secret) = &repo.webhook_secret {
        let signature = headers
            .get("X-Hub-Signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !service.verify_github_signature(secret, &body, signature) {
            return Err(AppError::unauthorized("Invalid webhook signature"));
        }
    }

    // Parse event type
    let event_type = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if event_type != "push" {
        // Only process push events
        return Ok(StatusCode::OK);
    }

    // Parse payload
    let payload: GitHubPushEvent = serde_json::from_slice(&body).map_err(|e| {
        tracing::error!("Failed to parse GitHub webhook payload: {}", e);
        AppError::bad_request("Invalid webhook payload")
    })?;

    // Extract branch name from ref (refs/heads/main -> main)
    let branch = payload
        .ref_name
        .strip_prefix("refs/heads/")
        .unwrap_or(&payload.ref_name);

    tracing::info!(
        "Received GitHub push webhook for repository {} branch {}",
        repo_id,
        branch
    );

    // Trigger sync
    if let Err(e) = service.sync_repository(repo_id).await {
        tracing::error!("Failed to sync repository after webhook: {}", e);
    }

    Ok(StatusCode::OK)
}

async fn handle_gitlab_webhook(
    State(state): State<AppState>,
    Path(repo_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    _body: axum::body::Bytes,
) -> Result<StatusCode, AppError> {
    let service = state.code_deploy_service()?;

    // Get raw repository to verify webhook token
    let repo = service
        .get_repository_raw(repo_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get repository for webhook: {}", e);
            AppError::internal("Failed to process webhook")
        })?
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    // Verify token
    if let Some(secret) = &repo.webhook_secret {
        let token = headers
            .get("X-Gitlab-Token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !service.verify_gitlab_token(secret, token) {
            return Err(AppError::unauthorized("Invalid webhook token"));
        }
    }

    // Parse event type
    let event_type = headers
        .get("X-Gitlab-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if event_type != "Push Hook" {
        return Ok(StatusCode::OK);
    }

    tracing::info!("Received GitLab push webhook for repository {}", repo_id);

    // Trigger sync
    if let Err(e) = service.sync_repository(repo_id).await {
        tracing::error!("Failed to sync repository after webhook: {}", e);
    }

    Ok(StatusCode::OK)
}

async fn handle_bitbucket_webhook(
    State(state): State<AppState>,
    Path(repo_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, AppError> {
    let service = state.code_deploy_service()?;

    // Get raw repository
    let repo = service
        .get_repository_raw(repo_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get repository for webhook: {}", e);
            AppError::internal("Failed to process webhook")
        })?
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    // Bitbucket uses HMAC-SHA256 with the same header pattern as GitHub
    if let Some(secret) = &repo.webhook_secret {
        let signature = headers
            .get("X-Hub-Signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Bitbucket signature format is different
        let signature_256 = format!("sha256={}", signature.strip_prefix("sha256=").unwrap_or(signature));
        if !service.verify_github_signature(secret, &body, &signature_256) {
            return Err(AppError::unauthorized("Invalid webhook signature"));
        }
    }

    // Parse event type
    let event_type = headers
        .get("X-Event-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !event_type.starts_with("repo:push") {
        return Ok(StatusCode::OK);
    }

    tracing::info!("Received Bitbucket push webhook for repository {}", repo_id);

    // Trigger sync
    if let Err(e) = service.sync_repository(repo_id).await {
        tracing::error!("Failed to sync repository after webhook: {}", e);
    }

    Ok(StatusCode::OK)
}

// ============================================================================
// Helper Functions
// ============================================================================

fn require_permission(auth_user: &AuthUser, _permission: &str) -> Result<(), AppError> {
    // Super admins have all permissions
    if auth_user.is_super_admin() {
        return Ok(());
    }

    // Check if user is admin or operator (has code deploy permissions)
    // The actual permission checking is done via RBAC in the database
    // For code deploy, we allow admin and operator roles
    if auth_user.roles.iter().any(|r| r == "admin" || r == "operator") {
        return Ok(());
    }

    // Viewers have read-only access to some endpoints
    // This is a simplified check - full RBAC would query the database
    Err(AppError::forbidden(
        "Insufficient permissions for code deploy operations",
    ))
}
