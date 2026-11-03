//! Rollback guidance for update jobs: per-node commands for the node's package
//! manager. Nothing is executed, and previous versions are not recorded, so
//! version-pinned commands carry a `<previous-version>` placeholder.

use std::collections::HashMap;

use serde::Serialize;

use crate::models::{UpdateJob, UpdateOperationType, UpdateTargetStatus};

const PREVIOUS_VERSION: &str = "<previous-version>";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RollbackStep {
    pub description: String,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetRollbackGuidance {
    pub certname: String,
    pub target_status: UpdateTargetStatus,
    pub package_manager: Option<String>,
    /// False when the node was never changed by the job.
    pub rollback_needed: bool,
    pub steps: Vec<RollbackStep>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollbackGuidance {
    pub job_id: String,
    /// True when at least one node may have been changed by the job.
    pub applicable: bool,
    pub summary: String,
    pub general_notes: Vec<String>,
    pub targets: Vec<TargetRollbackGuidance>,
}

/// Builds guidance for `job`. `package_managers` maps a certname to the
/// package manager reported by its latest inventory.
pub fn build_guidance(
    job: &UpdateJob,
    package_managers: &HashMap<String, String>,
) -> RollbackGuidance {
    let (safe_packages, skipped) = split_package_names(&job.package_names);

    let targets: Vec<TargetRollbackGuidance> = job
        .targets
        .iter()
        .map(|target| {
            let manager = package_managers
                .get(&target.certname)
                .map(|m| m.trim().to_lowercase())
                .filter(|m| !m.is_empty());
            let changed = matches!(
                target.status,
                UpdateTargetStatus::Succeeded
                    | UpdateTargetStatus::Failed
                    | UpdateTargetStatus::Dispatched
            );
            let mut warnings = Vec::new();
            let mut steps = Vec::new();

            match target.status {
                UpdateTargetStatus::Dispatched => warnings.push(
                    "The update is still running on this node; wait for the result before rolling back."
                        .to_string(),
                ),
                UpdateTargetStatus::Failed => warnings.push(
                    "The update failed and may have been applied partially; check the package manager state first."
                        .to_string(),
                ),
                _ => {}
            }

            if changed {
                steps = steps_for(job.operation_type, manager.as_deref(), &safe_packages);
                if !skipped.is_empty() {
                    warnings.push(format!(
                        "Package names with unsupported characters were left out of the commands: {}.",
                        skipped.join(", ")
                    ));
                }
            }

            TargetRollbackGuidance {
                certname: target.certname.clone(),
                target_status: target.status,
                package_manager: manager,
                rollback_needed: changed,
                steps,
                warnings,
            }
        })
        .collect();

    let applicable = targets.iter().any(|t| t.rollback_needed);
    let summary = if applicable {
        let count = targets.iter().filter(|t| t.rollback_needed).count();
        format!(
            "{} of {} node(s) may have been changed by this job.",
            count,
            targets.len()
        )
    } else {
        "No node was changed by this job, so there is nothing to roll back.".to_string()
    };

    RollbackGuidance {
        job_id: job.id.clone(),
        applicable,
        summary,
        general_notes: if applicable {
            general_notes(job.operation_type)
        } else {
            Vec::new()
        },
        targets,
    }
}

fn is_safe_package_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-' | ':' | '@'))
}

fn split_package_names(names: &[String]) -> (Vec<String>, Vec<String>) {
    let mut safe = Vec::new();
    let mut skipped = Vec::new();
    for name in names {
        if is_safe_package_name(name) {
            safe.push(name.clone());
        } else {
            skipped.push(name.clone());
        }
    }
    (safe, skipped)
}

fn step(description: &str, command: Option<String>) -> RollbackStep {
    RollbackStep {
        description: description.to_string(),
        command,
    }
}

fn general_notes(operation: UpdateOperationType) -> Vec<String> {
    let mut notes = vec![
        "Roll back one node first and verify the service before touching the rest.".to_string(),
        "Previous versions are not recorded by the job; read them from the package manager history or the node's inventory history.".to_string(),
        "Pause the group's update schedule while you roll back so the same update is not applied again.".to_string(),
    ];
    match operation {
        UpdateOperationType::SecurityPatch => notes.push(
            "Rolling back a security patch brings the vulnerability back; do it only as a short-term measure."
                .to_string(),
        ),
        UpdateOperationType::SystemPatch => notes.push(
            "System patches can include a kernel or core libraries; plan a reboot and boot the previous kernel if the new one fails."
                .to_string(),
        ),
        _ => {}
    }
    notes
}

fn steps_for(
    operation: UpdateOperationType,
    manager: Option<&str>,
    packages: &[String],
) -> Vec<RollbackStep> {
    let list = packages.join(" ");
    let targeted = !packages.is_empty();
    let Some(manager) = manager else {
        return vec![step(
            "The package manager of this node is unknown. Use its package manager history to revert the transaction made by the job.",
            None,
        )];
    };

    let sudo = if matches!(manager, "choco" | "winget") {
        ""
    } else {
        "sudo "
    };

    match (operation, manager) {
        (UpdateOperationType::PackageInstall, m) => match remove_command(m, &list, sudo) {
            Some(cmd) if targeted => vec![step("Remove the installed packages.", Some(cmd))],
            _ => vec![step(
                "Remove the packages the job installed using the package manager history.",
                None,
            )],
        },
        (UpdateOperationType::PackageRemove, m) => match install_command(m, &list, sudo) {
            Some(cmd) if targeted => vec![step("Install the removed packages again.", Some(cmd))],
            _ => vec![step(
                "Install again the packages the job removed, using the package manager history.",
                None,
            )],
        },
        (_, "dnf") | (_, "yum") => {
            let mut steps = vec![
                step(
                    "Find the transaction made by the job.",
                    Some(format!("{sudo}{manager} history list")),
                ),
                step(
                    "Undo that transaction (replace <id> with the transaction number).",
                    Some(format!("{sudo}{manager} history undo <id>")),
                ),
            ];
            if targeted {
                steps.push(step(
                    "Or downgrade only the affected packages to the previous available version.",
                    Some(format!("{sudo}{manager} downgrade {list}")),
                ));
            }
            steps
        }
        (_, "zypper") => {
            let mut steps = vec![step(
                "Find the previous versions in the zypper history.",
                Some("grep -E '\\|(install|remove)\\|' /var/log/zypp/history | tail -n 50".to_string()),
            )];
            for package in packages {
                steps.push(step(
                    "Install the previous version.",
                    Some(format!(
                        "{sudo}zypper install --oldpackage {package}={PREVIOUS_VERSION}"
                    )),
                ));
            }
            steps
        }
        (_, "apt") | (_, "apt-get") => {
            let mut steps = vec![step(
                "Find the previous versions in the apt history.",
                Some("grep -B2 -A3 'Upgrade:' /var/log/apt/history.log | tail -n 50".to_string()),
            )];
            for package in packages {
                steps.push(step(
                    "Install the previous version.",
                    Some(format!(
                        "{sudo}apt-get install --allow-downgrades {package}={PREVIOUS_VERSION}"
                    )),
                ));
            }
            steps
        }
        (_, "brew") => {
            let mut steps = Vec::new();
            for package in packages {
                steps.push(step(
                    "Show the installed versions.",
                    Some(format!("brew list --versions {package}")),
                ));
                steps.push(step(
                    "Install the previous version when the formula is versioned (for example name@major), then link it.",
                    Some(format!("brew install {package}@{PREVIOUS_VERSION}")),
                ));
            }
            if steps.is_empty() {
                steps.push(step(
                    "Show the installed versions.",
                    Some("brew list --versions".to_string()),
                ));
            }
            steps
        }
        (_, "choco") => packages
            .iter()
            .map(|package| {
                step(
                    "Install the previous version.",
                    Some(format!(
                        "choco install {package} --version {PREVIOUS_VERSION} --allow-downgrade -y"
                    )),
                )
            })
            .chain(std::iter::once(step(
                "Pin the package so the next run does not upgrade it again.",
                Some("choco pin add --name <package> --version <previous-version>".to_string()),
            )))
            .collect(),
        (_, "winget") => packages
            .iter()
            .map(|package| {
                step(
                    "Install the previous version.",
                    Some(format!(
                        "winget install --id {package} --version {PREVIOUS_VERSION} --force"
                    )),
                )
            })
            .collect(),
        _ => vec![step(
            "Use the package manager history of this node to revert the transaction made by the job.",
            None,
        )],
    }
}

fn remove_command(manager: &str, list: &str, sudo: &str) -> Option<String> {
    match manager {
        "dnf" | "yum" => Some(format!("{sudo}{manager} remove {list}")),
        "zypper" => Some(format!("{sudo}zypper remove {list}")),
        "apt" | "apt-get" => Some(format!("{sudo}apt-get remove {list}")),
        "brew" => Some(format!("brew uninstall {list}")),
        "choco" => Some(format!("choco uninstall {list} -y")),
        "winget" => Some(format!("winget uninstall --id {list}")),
        _ => None,
    }
}

fn install_command(manager: &str, list: &str, sudo: &str) -> Option<String> {
    match manager {
        "dnf" | "yum" => Some(format!("{sudo}{manager} install {list}")),
        "zypper" => Some(format!("{sudo}zypper install {list}")),
        "apt" | "apt-get" => Some(format!("{sudo}apt-get install {list}")),
        "brew" => Some(format!("brew install {list}")),
        "choco" => Some(format!("choco install {list} -y")),
        "winget" => Some(format!("winget install --id {list}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{UpdateJobStatus, UpdateJobTarget};
    use chrono::Utc;

    fn job(
        operation: UpdateOperationType,
        packages: &[&str],
        statuses: &[(&str, UpdateTargetStatus)],
    ) -> UpdateJob {
        UpdateJob {
            id: "job-1".to_string(),
            status: UpdateJobStatus::Completed,
            operation_type: operation,
            package_names: packages.iter().map(|p| p.to_string()).collect(),
            target_group_id: None,
            target_nodes: statuses.iter().map(|(c, _)| c.to_string()).collect(),
            requires_approval: false,
            scheduled_for: None,
            maintenance_window_start: None,
            maintenance_window_end: None,
            requested_by: "admin".to_string(),
            approved_by: None,
            approval_notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            targets: statuses
                .iter()
                .enumerate()
                .map(|(i, (certname, status))| UpdateJobTarget {
                    id: format!("t{i}"),
                    certname: certname.to_string(),
                    status: *status,
                    dispatched_at: None,
                    completed_at: None,
                    last_error: None,
                })
                .collect(),
            results: Vec::new(),
        }
    }

    fn managers(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(c, m)| (c.to_string(), m.to_string()))
            .collect()
    }

    #[test]
    fn untouched_targets_need_no_rollback() {
        let j = job(
            UpdateOperationType::PackageUpdate,
            &["nginx"],
            &[
                ("a", UpdateTargetStatus::Queued),
                ("b", UpdateTargetStatus::Cancelled),
                ("c", UpdateTargetStatus::Rejected),
            ],
        );
        let g = build_guidance(&j, &managers(&[("a", "dnf")]));
        assert!(!g.applicable);
        assert!(g.general_notes.is_empty());
        assert!(g
            .targets
            .iter()
            .all(|t| !t.rollback_needed && t.steps.is_empty()));
    }

    #[test]
    fn dnf_update_offers_history_undo_and_downgrade() {
        let j = job(
            UpdateOperationType::PackageUpdate,
            &["nginx", "openssl"],
            &[("a", UpdateTargetStatus::Succeeded)],
        );
        let g = build_guidance(&j, &managers(&[("a", "dnf")]));
        assert!(g.applicable);
        let cmds: Vec<_> = g.targets[0]
            .steps
            .iter()
            .filter_map(|s| s.command.as_deref())
            .collect();
        assert!(cmds.contains(&"sudo dnf history undo <id>"));
        assert!(cmds.contains(&"sudo dnf downgrade nginx openssl"));
    }

    #[test]
    fn apt_update_pins_previous_version_placeholder() {
        let j = job(
            UpdateOperationType::SecurityPatch,
            &["openssl"],
            &[("a", UpdateTargetStatus::Succeeded)],
        );
        let g = build_guidance(&j, &managers(&[("a", "apt")]));
        assert!(g.targets[0].steps.iter().any(|s| s.command.as_deref()
            == Some("sudo apt-get install --allow-downgrades openssl=<previous-version>")));
        assert!(g.general_notes.iter().any(|n| n.contains("vulnerability")));
    }

    #[test]
    fn install_is_undone_by_remove_and_remove_by_install() {
        let i = job(
            UpdateOperationType::PackageInstall,
            &["htop"],
            &[("a", UpdateTargetStatus::Succeeded)],
        );
        let g = build_guidance(&i, &managers(&[("a", "zypper")]));
        assert_eq!(
            g.targets[0].steps[0].command.as_deref(),
            Some("sudo zypper remove htop")
        );

        let r = job(
            UpdateOperationType::PackageRemove,
            &["htop"],
            &[("a", UpdateTargetStatus::Succeeded)],
        );
        let g = build_guidance(&r, &managers(&[("a", "choco")]));
        assert_eq!(
            g.targets[0].steps[0].command.as_deref(),
            Some("choco install htop -y")
        );
    }

    #[test]
    fn unknown_manager_gives_generic_step_without_command() {
        let j = job(
            UpdateOperationType::SystemPatch,
            &[],
            &[("a", UpdateTargetStatus::Succeeded)],
        );
        let g = build_guidance(&j, &HashMap::new());
        assert_eq!(g.targets[0].steps.len(), 1);
        assert!(g.targets[0].steps[0].command.is_none());
        assert!(g.general_notes.iter().any(|n| n.contains("reboot")));
    }

    #[test]
    fn unsafe_package_names_never_reach_commands() {
        let j = job(
            UpdateOperationType::PackageUpdate,
            &["nginx; rm -rf /", "--force", "curl"],
            &[("a", UpdateTargetStatus::Succeeded)],
        );
        let g = build_guidance(&j, &managers(&[("a", "dnf")]));
        let t = &g.targets[0];
        assert!(t
            .steps
            .iter()
            .filter_map(|s| s.command.as_deref())
            .all(|c| !c.contains("rm -rf") && !c.contains("--force")));
        assert!(t
            .steps
            .iter()
            .any(|s| s.command.as_deref() == Some("sudo dnf downgrade curl")));
        assert!(t
            .warnings
            .iter()
            .any(|w| w.contains("unsupported characters")));
    }

    #[test]
    fn failed_and_dispatched_targets_carry_warnings() {
        let j = job(
            UpdateOperationType::PackageUpdate,
            &["curl"],
            &[
                ("a", UpdateTargetStatus::Failed),
                ("b", UpdateTargetStatus::Dispatched),
            ],
        );
        let g = build_guidance(&j, &managers(&[("a", "yum"), ("b", "yum")]));
        assert!(g.targets[0]
            .warnings
            .iter()
            .any(|w| w.contains("partially")));
        assert!(g.targets[1]
            .warnings
            .iter()
            .any(|w| w.contains("still running")));
    }
}
