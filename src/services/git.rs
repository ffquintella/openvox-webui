//! Git operations service for Code Deploy
//!
//! Provides Git repository operations using libgit2 via the git2 crate.
//! Supports SSH key authentication, branch discovery, and commit information retrieval.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use git2::{
    BranchType, Cred, FetchOptions, RemoteCallbacks, Repository, ResetType,
};
use tracing::{debug, info, warn};

/// Information about a Git commit
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub message: Option<String>,
    pub author: Option<String>,
    pub author_email: Option<String>,
    pub date: Option<DateTime<Utc>>,
}

/// Information about a Git branch
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub commit: CommitInfo,
    pub is_default: bool,
}

/// Git service configuration
#[derive(Debug, Clone)]
pub struct GitServiceConfig {
    /// Base directory for cloned repositories
    pub repos_base_dir: PathBuf,
    /// SSH keys directory
    pub ssh_keys_dir: PathBuf,
}

impl Default for GitServiceConfig {
    fn default() -> Self {
        Self {
            repos_base_dir: PathBuf::from("/var/lib/openvox-webui/repos"),
            ssh_keys_dir: PathBuf::from("/etc/openvox-webui/ssh-keys"),
        }
    }
}

/// Git operations service
pub struct GitService {
    config: GitServiceConfig,
}

impl GitService {
    /// Create a new Git service with the given configuration
    pub fn new(config: GitServiceConfig) -> Self {
        Self { config }
    }

    /// Get the local path for a repository
    pub fn repo_path(&self, repo_id: &str) -> PathBuf {
        self.config.repos_base_dir.join(repo_id)
    }

    /// Clone or open a repository
    ///
    /// If the repository already exists locally, it will be opened.
    /// Otherwise, it will be cloned from the remote URL.
    pub fn clone_or_open(
        &self,
        repo_id: &str,
        url: &str,
        ssh_private_key: Option<&str>,
    ) -> Result<Repository> {
        let repo_path = self.repo_path(repo_id);

        if repo_path.exists() {
            debug!("Opening existing repository at {:?}", repo_path);
            Repository::open(&repo_path).context("Failed to open existing repository")
        } else {
            info!("Cloning repository {} to {:?}", url, repo_path);
            self.clone_repository(url, &repo_path, ssh_private_key)
        }
    }

    /// Clone a repository from a URL
    fn clone_repository(
        &self,
        url: &str,
        path: &Path,
        ssh_private_key: Option<&str>,
    ) -> Result<Repository> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create repos directory")?;
        }

        let mut callbacks = RemoteCallbacks::new();

        // Set up SSH authentication if key is provided
        if let Some(key) = ssh_private_key {
            let key_string = key.to_string();
            callbacks.credentials(move |_url, username_from_url, _allowed_types| {
                let username = username_from_url.unwrap_or("git");
                Cred::ssh_key_from_memory(username, None, &key_string, None)
            });
        }

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_options);

        builder
            .clone(url, path)
            .context("Failed to clone repository")
    }

    /// Fetch updates from the remote
    pub fn fetch(&self, repo: &Repository, ssh_private_key: Option<&str>) -> Result<()> {
        let mut remote = repo
            .find_remote("origin")
            .context("Failed to find origin remote")?;

        let mut callbacks = RemoteCallbacks::new();

        if let Some(key) = ssh_private_key {
            let key_string = key.to_string();
            callbacks.credentials(move |_url, username_from_url, _allowed_types| {
                let username = username_from_url.unwrap_or("git");
                Cred::ssh_key_from_memory(username, None, &key_string, None)
            });
        }

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        // Fetch all refs
        remote
            .fetch(&[] as &[&str], Some(&mut fetch_options), None)
            .context("Failed to fetch from remote")?;

        debug!("Fetched updates from remote");
        Ok(())
    }

    /// List all remote branches matching a pattern
    pub fn list_branches(
        &self,
        repo: &Repository,
        pattern: &str,
    ) -> Result<Vec<BranchInfo>> {
        let mut branches = Vec::new();

        // Get HEAD reference to determine default branch
        let head_ref = repo.head().ok();
        let default_branch = head_ref.as_ref().and_then(|h| {
            h.shorthand().map(|s| s.to_string())
        });

        // Iterate through remote branches
        for branch_result in repo.branches(Some(BranchType::Remote))? {
            let (branch, _branch_type) = branch_result?;

            let full_name = match branch.name()? {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Skip HEAD reference
            if full_name.ends_with("/HEAD") {
                continue;
            }

            // Extract branch name (remove "origin/" prefix)
            let branch_name = full_name
                .strip_prefix("origin/")
                .unwrap_or(&full_name)
                .to_string();

            // Check if branch matches pattern
            if !matches_pattern(&branch_name, pattern) {
                continue;
            }

            // Get the commit for this branch
            let commit = branch.get().peel_to_commit()?;
            let commit_info = commit_to_info(&commit);

            let is_default = default_branch.as_ref().map_or(false, |d| d == &branch_name);

            branches.push(BranchInfo {
                name: branch_name,
                commit: commit_info,
                is_default,
            });
        }

        // Sort branches: default first, then alphabetically
        branches.sort_by(|a, b| {
            if a.is_default && !b.is_default {
                std::cmp::Ordering::Less
            } else if !a.is_default && b.is_default {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });

        Ok(branches)
    }

    /// Get commit information for a specific branch
    pub fn get_branch_commit(
        &self,
        repo: &Repository,
        branch_name: &str,
    ) -> Result<Option<CommitInfo>> {
        let remote_ref = format!("refs/remotes/origin/{}", branch_name);

        match repo.find_reference(&remote_ref) {
            Ok(reference) => {
                let commit = reference.peel_to_commit()?;
                Ok(Some(commit_to_info(&commit)))
            }
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e).context("Failed to find branch reference"),
        }
    }

    /// Get the latest commit for a branch (from remote)
    pub fn get_latest_commit(
        &self,
        repo: &Repository,
        branch_name: &str,
    ) -> Result<Option<CommitInfo>> {
        // Try remote ref first (after fetch)
        let remote_ref = format!("refs/remotes/origin/{}", branch_name);

        if let Ok(reference) = repo.find_reference(&remote_ref) {
            let commit = reference.peel_to_commit()?;
            return Ok(Some(commit_to_info(&commit)));
        }

        // Fall back to local ref
        let local_ref = format!("refs/heads/{}", branch_name);

        match repo.find_reference(&local_ref) {
            Ok(reference) => {
                let commit = reference.peel_to_commit()?;
                Ok(Some(commit_to_info(&commit)))
            }
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e).context("Failed to find branch reference"),
        }
    }

    /// Checkout a specific branch/commit to the working directory
    pub fn checkout(&self, repo: &Repository, branch_name: &str) -> Result<()> {
        let remote_ref = format!("refs/remotes/origin/{}", branch_name);

        let reference = repo
            .find_reference(&remote_ref)
            .context("Failed to find branch reference")?;

        let commit = reference
            .peel_to_commit()
            .context("Failed to peel to commit")?;

        // Reset to the commit
        repo.reset(commit.as_object(), ResetType::Hard, None)
            .context("Failed to reset to commit")?;

        debug!("Checked out branch {} at {}", branch_name, commit.id());
        Ok(())
    }

    /// Delete a local repository
    pub fn delete_repo(&self, repo_id: &str) -> Result<()> {
        let repo_path = self.repo_path(repo_id);

        if repo_path.exists() {
            std::fs::remove_dir_all(&repo_path)
                .context("Failed to delete repository directory")?;
            info!("Deleted repository at {:?}", repo_path);
        }

        Ok(())
    }

    /// Check if a repository exists locally
    pub fn repo_exists(&self, repo_id: &str) -> bool {
        self.repo_path(repo_id).exists()
    }

    /// Extract public key from a private key in PEM format
    ///
    /// Note: Full public key extraction requires external tools like ssh-keygen.
    /// This method validates the key format and returns a placeholder.
    pub fn extract_public_key(private_key_pem: &str) -> Result<String> {
        // Parse the private key to validate it
        use std::io::BufReader;

        let mut reader = BufReader::new(private_key_pem.as_bytes());

        // Try to read as RSA key first
        let rsa_keys: Vec<_> = rustls_pemfile::rsa_private_keys(&mut reader)
            .filter_map(|r| r.ok())
            .collect();
        if !rsa_keys.is_empty() {
            // For RSA keys, we can extract the public key
            // This is a simplified implementation - in production you'd use a proper SSH key library
            warn!("Public key extraction from RSA keys requires ssh-keygen or similar tool");
            return Ok("(public key extraction not implemented - please provide separately)".to_string());
        }

        // Try EC key
        reader = BufReader::new(private_key_pem.as_bytes());
        let ec_keys: Vec<_> = rustls_pemfile::ec_private_keys(&mut reader)
            .filter_map(|r| r.ok())
            .collect();
        if !ec_keys.is_empty() {
            warn!("Public key extraction from EC keys requires ssh-keygen or similar tool");
            return Ok("(public key extraction not implemented - please provide separately)".to_string());
        }

        // Try PKCS8 key
        reader = BufReader::new(private_key_pem.as_bytes());
        let pkcs8_keys: Vec<_> = rustls_pemfile::pkcs8_private_keys(&mut reader)
            .filter_map(|r| r.ok())
            .collect();
        if !pkcs8_keys.is_empty() {
            warn!("Public key extraction from PKCS8 keys requires ssh-keygen or similar tool");
            return Ok("(public key extraction not implemented - please provide separately)".to_string());
        }

        Err(anyhow::anyhow!("Could not parse private key"))
    }
}

/// Convert a git2 Commit to CommitInfo
fn commit_to_info(commit: &git2::Commit) -> CommitInfo {
    let author = commit.author();

    CommitInfo {
        sha: commit.id().to_string(),
        message: commit.message().map(|m| m.lines().next().unwrap_or(m).to_string()),
        author: author.name().map(|s| s.to_string()),
        author_email: author.email().map(|s| s.to_string()),
        date: {
            let time = commit.time();
            Utc.timestamp_opt(time.seconds(), 0).single()
        },
    }
}

/// Check if a branch name matches a glob pattern
fn matches_pattern(name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // Simple glob matching
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        name.starts_with(prefix)
    } else if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        name.ends_with(suffix)
    } else if pattern.contains('*') {
        // Handle patterns like "feature/*/test"
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            name.starts_with(parts[0]) && name.ends_with(parts[1])
        } else {
            // Complex patterns - fall back to exact match
            name == pattern
        }
    } else {
        // Exact match
        name == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern_wildcard() {
        assert!(matches_pattern("main", "*"));
        assert!(matches_pattern("feature/foo", "*"));
    }

    #[test]
    fn test_matches_pattern_prefix() {
        assert!(matches_pattern("feature/foo", "feature/*"));
        assert!(matches_pattern("feature/bar", "feature/*"));
        assert!(!matches_pattern("main", "feature/*"));
    }

    #[test]
    fn test_matches_pattern_suffix() {
        assert!(matches_pattern("foo-release", "*-release"));
        assert!(!matches_pattern("foo-dev", "*-release"));
    }

    #[test]
    fn test_matches_pattern_exact() {
        assert!(matches_pattern("main", "main"));
        assert!(!matches_pattern("master", "main"));
    }

    #[test]
    fn test_matches_pattern_middle() {
        assert!(matches_pattern("feature/foo/bar", "feature/*/bar"));
        assert!(!matches_pattern("feature/foo/baz", "feature/*/bar"));
    }
}
