//! Configuration of the OpenBolt integration (`orchestration.bolt`).
//!
//! Secret values (`secret://` references) are resolved when the configuration
//! is loaded, so the fields below hold the resolved value. `Debug` never
//! prints a password.

use std::fmt;
use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// `orchestration` block of `config.yaml`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OrchestrationConfig {
    #[serde(default)]
    pub bolt: BoltConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BoltConfig {
    /// The feature is off until an administrator turns it on.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_binary_path")]
    pub binary_path: PathBuf,
    /// Bolt project directory (holds `bolt-project.yaml`).
    #[serde(default = "default_project_dir")]
    pub project_dir: PathBuf,
    /// Module path searched for tasks and plans. Empty means the project's own.
    #[serde(default)]
    pub modulepath: Vec<PathBuf>,
    #[serde(default)]
    pub ssh: SshTransport,
    #[serde(default)]
    pub winrm: WinrmTransport,
    /// Hard timeout of one run.
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    /// Bolt processes running at the same time.
    #[serde(default = "default_max_concurrent_runs")]
    pub max_concurrent_runs: usize,
    /// Largest number of targets in one run.
    #[serde(default = "default_max_targets_per_run")]
    pub max_targets_per_run: usize,
    /// Targets Bolt works on at the same time inside one run.
    #[serde(default = "default_target_concurrency")]
    pub target_concurrency: usize,
    /// Runs on more targets than this need an approval by another user.
    #[serde(default = "default_approval_target_threshold")]
    pub approval_target_threshold: usize,
    /// `bolt command run` and `bolt script run`: off unless set.
    #[serde(default)]
    pub allow_arbitrary_commands: bool,
    /// Largest stored output per target, in bytes.
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    /// Finished runs older than this are deleted.
    #[serde(default = "default_retention_days")]
    pub retention_days: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SshTransport {
    #[serde(default = "default_user")]
    pub user: String,
    /// Path of the private key file (the key itself is never in the config).
    #[serde(default)]
    pub private_key_path: Option<PathBuf>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub host_key_check: bool,
    #[serde(default)]
    pub run_as: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WinrmTransport {
    #[serde(default = "default_user")]
    pub user: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub ssl: bool,
    #[serde(default = "default_true")]
    pub ssl_verify: bool,
}

fn default_true() -> bool {
    true
}
fn default_user() -> String {
    "bolt".to_string()
}
fn default_binary_path() -> PathBuf {
    PathBuf::from("/opt/puppetlabs/bolt/bin/bolt")
}
fn default_project_dir() -> PathBuf {
    PathBuf::from("/etc/openvox-webui/bolt")
}
fn default_timeout_seconds() -> u64 {
    900
}
fn default_max_concurrent_runs() -> usize {
    4
}
fn default_max_targets_per_run() -> usize {
    200
}
fn default_target_concurrency() -> usize {
    20
}
fn default_approval_target_threshold() -> usize {
    10
}
fn default_max_output_bytes() -> usize {
    64 * 1024
}
fn default_retention_days() -> i64 {
    90
}

impl Default for SshTransport {
    fn default() -> Self {
        Self {
            user: default_user(),
            private_key_path: None,
            password: None,
            host_key_check: true,
            run_as: None,
        }
    }
}

impl Default for WinrmTransport {
    fn default() -> Self {
        Self {
            user: default_user(),
            password: None,
            ssl: true,
            ssl_verify: true,
        }
    }
}

impl Default for BoltConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            binary_path: default_binary_path(),
            project_dir: default_project_dir(),
            modulepath: Vec::new(),
            ssh: SshTransport::default(),
            winrm: WinrmTransport::default(),
            timeout_seconds: default_timeout_seconds(),
            max_concurrent_runs: default_max_concurrent_runs(),
            max_targets_per_run: default_max_targets_per_run(),
            target_concurrency: default_target_concurrency(),
            approval_target_threshold: default_approval_target_threshold(),
            allow_arbitrary_commands: false,
            max_output_bytes: default_max_output_bytes(),
            retention_days: default_retention_days(),
        }
    }
}

impl fmt::Debug for SshTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshTransport")
            .field("user", &self.user)
            .field("private_key_path", &self.private_key_path)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("host_key_check", &self.host_key_check)
            .field("run_as", &self.run_as)
            .finish()
    }
}

impl fmt::Debug for WinrmTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WinrmTransport")
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("ssl", &self.ssl)
            .field("ssl_verify", &self.ssl_verify)
            .finish()
    }
}

impl BoltConfig {
    /// Checks the limits. Only an enabled integration is validated, so a
    /// leftover block never stops the application from starting.
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.timeout_seconds == 0 {
            bail!("orchestration.bolt.timeout_seconds must be greater than 0");
        }
        if self.max_concurrent_runs == 0 {
            bail!("orchestration.bolt.max_concurrent_runs must be greater than 0");
        }
        if self.max_targets_per_run == 0 {
            bail!("orchestration.bolt.max_targets_per_run must be greater than 0");
        }
        if self.target_concurrency == 0 {
            bail!("orchestration.bolt.target_concurrency must be greater than 0");
        }
        if self.retention_days < 1 {
            bail!("orchestration.bolt.retention_days must be at least 1");
        }
        Ok(())
    }

    /// Settings that weaken the transport; the startup log warns about them.
    pub fn security_warnings(&self) -> Vec<&'static str> {
        let mut w = Vec::new();
        if !self.ssh.host_key_check {
            w.push("orchestration.bolt.ssh.host_key_check is off");
        }
        if !self.winrm.ssl {
            w.push("orchestration.bolt.winrm.ssl is off (credentials travel in clear text)");
        } else if !self.winrm.ssl_verify {
            w.push("orchestration.bolt.winrm.ssl_verify is off");
        }
        if self.allow_arbitrary_commands {
            w.push("orchestration.bolt.allow_arbitrary_commands is on");
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default_and_not_validated() {
        let c = BoltConfig {
            timeout_seconds: 0,
            ..Default::default()
        };
        assert!(!c.enabled);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn enabled_rejects_zero_limits() {
        let c = BoltConfig {
            enabled: true,
            max_concurrent_runs: 0,
            ..Default::default()
        };
        assert!(c.validate().is_err());
        assert!(BoltConfig {
            enabled: true,
            ..Default::default()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn debug_never_prints_a_password() {
        let c = BoltConfig {
            ssh: SshTransport {
                password: Some("hunter2".into()),
                ..Default::default()
            },
            winrm: WinrmTransport {
                password: Some("s3cret".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let s = format!("{c:?}");
        assert!(!s.contains("hunter2") && !s.contains("s3cret"));
        assert!(s.contains("[REDACTED]"));
    }

    #[test]
    fn weak_transport_settings_warn() {
        let mut c = BoltConfig::default();
        assert!(c.security_warnings().is_empty());
        c.ssh.host_key_check = false;
        c.allow_arbitrary_commands = true;
        assert_eq!(c.security_warnings().len(), 2);
    }

    #[test]
    fn parses_from_yaml_with_defaults() {
        let c: OrchestrationConfig =
            serde_norway::from_str("bolt:\n  enabled: true\n  max_targets_per_run: 5\n").unwrap();
        assert!(c.bolt.enabled);
        assert_eq!(c.bolt.max_targets_per_run, 5);
        assert_eq!(c.bolt.timeout_seconds, 900);
        assert!(c.bolt.ssh.host_key_check);
    }
}
