//! Runs the `bolt` binary (T113).
//!
//! One run is one process: it gets a private temporary directory (inventory,
//! parameters and target list, mode 0600) that is deleted when the run ends,
//! a hard timeout, and a way to cancel. Parameters never appear on the
//! command line, and no shell is involved.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::watch;

use super::config::BoltConfig;
use super::inventory::{self, BoltTarget};
use super::model::{redact_text, redact_value, truncate_text, JobKind};

/// Largest output read from the process; the rest is dropped.
const MAX_PROCESS_OUTPUT: usize = 16 * 1024 * 1024;

/// What the process did to one target.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetOutcome {
    pub certname: String,
    /// `success`, `failure` or `skipped`.
    pub status: String,
    pub exit_code: Option<i32>,
    pub result: Value,
    pub output: String,
}

#[derive(Debug, Clone, Default)]
pub struct RunOutcome {
    pub targets: Vec<TargetOutcome>,
    /// Failure of the run itself (binary missing, bad output, plan error).
    pub error: Option<String>,
    pub timed_out: bool,
    pub cancelled: bool,
}

pub struct RunRequest<'a> {
    pub kind: JobKind,
    pub name: &'a str,
    pub params: &'a Value,
    pub targets: &'a [BoltTarget],
    pub noop: bool,
}

/// A private directory, removed on drop.
pub struct PrivateDir {
    path: PathBuf,
}

impl PrivateDir {
    pub fn create() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("openvox-bolt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes a new file only the owner can read.
    pub fn write(&self, name: &str, content: &str) -> std::io::Result<PathBuf> {
        use std::io::Write;
        let p = self.path.join(name);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(&p)?.write_all(content.as_bytes())?;
        Ok(p)
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn common_args(cfg: &BoltConfig) -> Vec<String> {
    let mut a = vec![
        "--format".to_string(),
        "json".to_string(),
        "--no-color".to_string(),
        "--project".to_string(),
        cfg.project_dir.to_string_lossy().into_owned(),
    ];
    if !cfg.modulepath.is_empty() {
        let mp = cfg
            .modulepath
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");
        a.push("--modulepath".into());
        a.push(mp);
    }
    a
}

/// The arguments of a run. The files are written in the private directory.
pub fn build_args(
    cfg: &BoltConfig,
    kind: JobKind,
    name: &str,
    noop: bool,
    inventory_file: &Path,
    targets_file: &Path,
    params_file: Option<&Path>,
) -> Vec<String> {
    let mut a: Vec<String> = match kind {
        JobKind::Task => vec!["task".into(), "run".into(), name.into()],
        JobKind::Plan => vec!["plan".into(), "run".into(), name.into()],
        JobKind::Command => vec!["command".into(), "run".into(), name.into()],
    };
    a.extend(common_args(cfg));
    a.push("--inventoryfile".into());
    a.push(inventory_file.to_string_lossy().into_owned());
    a.push("--targets".into());
    a.push(format!("@{}", targets_file.to_string_lossy()));
    a.push("--concurrency".into());
    a.push(cfg.target_concurrency.to_string());
    a.push("--no-save-rerun".into());
    if noop && kind != JobKind::Command {
        a.push("--noop".into());
    }
    if let Some(p) = params_file {
        if kind != JobKind::Command {
            a.push("--params".into());
            a.push(format!("@{}", p.to_string_lossy()));
        }
    }
    a
}

fn base_command(cfg: &BoltConfig, args: &[String]) -> Command {
    let mut c = Command::new(&cfg.binary_path);
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("BOLT_DISABLE_ANALYTICS", "true");
    c
}

#[derive(Debug)]
enum Stop {
    Spawn(String),
    TimedOut,
    Cancelled,
}

/// Resolves when the cancel flag is set; never when there is no receiver.
async fn cancelled(cancel: Option<&mut watch::Receiver<bool>>) {
    match cancel {
        Some(rx) => {
            if *rx.borrow() {
                return;
            }
            while rx.changed().await.is_ok() {
                if *rx.borrow() {
                    return;
                }
            }
            std::future::pending::<()>().await
        }
        None => std::future::pending::<()>().await,
    }
}

/// Runs a command to the end, or until the timeout or the cancel signal.
/// Dropping the child future kills the process.
async fn execute(
    mut cmd: Command,
    timeout: Duration,
    cancel: Option<&mut watch::Receiver<bool>>,
) -> Result<std::process::Output, Stop> {
    let child = cmd.spawn().map_err(|e| Stop::Spawn(e.to_string()))?;
    tokio::select! {
        out = child.wait_with_output() => out.map_err(|e| Stop::Spawn(e.to_string())),
        _ = tokio::time::sleep(timeout) => Err(Stop::TimedOut),
        _ = cancelled(cancel) => Err(Stop::Cancelled),
    }
}

/// Runs a task, plan or command on the targets.
pub async fn run(
    cfg: &BoltConfig,
    req: &RunRequest<'_>,
    mut cancel: watch::Receiver<bool>,
) -> RunOutcome {
    let mut outcome = RunOutcome::default();
    let setup = (|| -> Result<(PrivateDir, Vec<String>), String> {
        let inv = inventory::render(req.targets, cfg)?;
        let dir = PrivateDir::create().map_err(|e| format!("cannot create work dir: {e}"))?;
        let inv_file = dir
            .write("inventory.yaml", &inv.to_string())
            .map_err(|e| e.to_string())?;
        let names: Vec<&str> = req.targets.iter().map(|t| t.certname.as_str()).collect();
        let targets_file = dir
            .write("targets.txt", &names.join("\n"))
            .map_err(|e| e.to_string())?;
        let params_file = if req.kind == JobKind::Command {
            None
        } else {
            Some(
                dir.write("params.json", &req.params.to_string())
                    .map_err(|e| e.to_string())?,
            )
        };
        let args = build_args(
            cfg,
            req.kind,
            req.name,
            req.noop,
            &inv_file,
            &targets_file,
            params_file.as_deref(),
        );
        Ok((dir, args))
    })();
    let (_dir, args) = match setup {
        Ok(v) => v,
        Err(e) => {
            outcome.error = Some(e);
            return outcome;
        }
    };

    let result = execute(
        base_command(cfg, &args),
        Duration::from_secs(cfg.timeout_seconds),
        Some(&mut cancel),
    )
    .await;
    match result {
        Err(Stop::Spawn(e)) => outcome.error = Some(format!("cannot run bolt: {e}")),
        Err(Stop::TimedOut) => {
            outcome.timed_out = true;
            outcome.error = Some(format!("timed out after {}s", cfg.timeout_seconds));
        }
        Err(Stop::Cancelled) => outcome.cancelled = true,
        Ok(out) => {
            let stdout =
                String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(MAX_PROCESS_OUTPUT)])
                    .into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr[..out.stderr.len().min(64 * 1024)])
                .into_owned();
            let (targets, error) = parse_run(
                req,
                &stdout,
                &stderr,
                out.status.code(),
                cfg.max_output_bytes,
            );
            outcome.targets = targets;
            outcome.error = error;
        }
    }
    outcome
}

/// Turns the process output into per-target outcomes.
pub fn parse_run(
    req: &RunRequest<'_>,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    max_output: usize,
) -> (Vec<TargetOutcome>, Option<String>) {
    let parsed: Option<Value> = serde_json::from_str(stdout.trim()).ok();
    if req.kind == JobKind::Plan {
        let ok = exit_code == Some(0);
        let result = parsed.unwrap_or_else(|| json!(stdout.trim()));
        let error = if ok {
            None
        } else {
            Some(plan_error(&result, stderr))
        };
        let shown = redact_value(&result);
        let text = truncate_text(&redact_text(&shown.to_string()), max_output);
        let targets = req
            .targets
            .iter()
            .map(|t| TargetOutcome {
                certname: t.certname.clone(),
                status: if ok { "success" } else { "failure" }.into(),
                exit_code,
                result: shown.clone(),
                output: text.clone(),
            })
            .collect();
        return (targets, error);
    }
    match parsed {
        Some(v) if v.get("items").is_some() => (parse_items(req, &v, max_output), None),
        parsed => {
            let msg = parsed
                .as_ref()
                .and_then(|v| v.get("msg"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| first_line(stderr, stdout));
            (Vec::new(), Some(truncate_text(&redact_text(&msg), 1000)))
        }
    }
}

fn first_line(stderr: &str, stdout: &str) -> String {
    let s = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("bolt failed without output")
        .to_string()
}

fn plan_error(result: &Value, stderr: &str) -> String {
    let msg = result
        .get("msg")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| first_line(stderr, ""));
    truncate_text(&redact_text(&msg), 1000)
}

fn parse_items(req: &RunRequest<'_>, v: &Value, max_output: usize) -> Vec<TargetOutcome> {
    let mut by_name = std::collections::BTreeMap::new();
    for item in v["items"].as_array().into_iter().flatten() {
        let Some(name) = item.get("target").and_then(Value::as_str) else {
            continue;
        };
        let value = item.get("value").cloned().unwrap_or(Value::Null);
        let ok = item.get("status").and_then(Value::as_str) == Some("success");
        let exit_code = value
            .get("exit_code")
            .and_then(Value::as_i64)
            .or_else(|| value["_error"]["details"]["exit_code"].as_i64())
            .map(|c| c as i32)
            .or(if ok { Some(0) } else { None });
        let shown = redact_value(&value);
        let output = shown
            .get("_output")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| shown["_error"]["msg"].as_str().map(str::to_string))
            .or_else(|| {
                let (o, e) = (shown["stdout"].as_str(), shown["stderr"].as_str());
                (o.is_some() || e.is_some())
                    .then(|| format!("{}{}", o.unwrap_or(""), e.unwrap_or("")))
            })
            .unwrap_or_default();
        by_name.insert(
            name.to_string(),
            TargetOutcome {
                certname: name.to_string(),
                status: if ok { "success" } else { "failure" }.into(),
                exit_code,
                result: shown,
                output: truncate_text(&redact_text(&output), max_output),
            },
        );
    }
    // Every requested target gets an entry, so a node Bolt skipped is visible.
    req.targets
        .iter()
        .map(|t| {
            by_name.remove(&t.certname).unwrap_or(TargetOutcome {
                certname: t.certname.clone(),
                status: "skipped".into(),
                exit_code: None,
                result: Value::Null,
                output: String::new(),
            })
        })
        .collect()
}

// ----------------------------------------------------------------------------
// Discovery
// ----------------------------------------------------------------------------

/// Runs a read-only `bolt ... show` command and returns its JSON.
pub async fn show(cfg: &BoltConfig, args: &[&str]) -> Result<Value, String> {
    let mut a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    a.extend(common_args(cfg));
    let out = execute(base_command(cfg, &a), Duration::from_secs(60), None)
        .await
        .map_err(|s| match s {
            Stop::Spawn(e) => format!("cannot run bolt: {e}"),
            Stop::TimedOut => "bolt did not answer in time".to_string(),
            Stop::Cancelled => "cancelled".to_string(),
        })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(first_line(&err, ""));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("bolt returned invalid JSON: {e}"))
}

/// `bolt --version`, for the health check.
pub async fn version(cfg: &BoltConfig) -> Result<String, String> {
    let cmd = base_command(cfg, &["--version".to_string()]);
    let out = execute(cmd, Duration::from_secs(15), None)
        .await
        .map_err(|s| match s {
            Stop::Spawn(e) => format!("cannot run bolt: {e}"),
            other => format!("{other:?}"),
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err("bolt --version failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req<'a>(kind: JobKind, targets: &'a [BoltTarget], params: &'a Value) -> RunRequest<'a> {
        RunRequest {
            kind,
            name: "package",
            params,
            targets,
            noop: false,
        }
    }

    fn tg(n: &str) -> BoltTarget {
        BoltTarget {
            certname: n.into(),
            address: None,
            kernel: None,
        }
    }

    #[test]
    fn args_keep_parameters_and_targets_off_the_command_line() {
        let cfg = BoltConfig::default();
        let a = build_args(
            &cfg,
            JobKind::Task,
            "package",
            true,
            Path::new("/t/inventory.yaml"),
            Path::new("/t/targets.txt"),
            Some(Path::new("/t/params.json")),
        );
        assert_eq!(&a[..3], ["task", "run", "package"]);
        assert!(a.contains(&"--noop".to_string()));
        assert!(a.contains(&"@/t/targets.txt".to_string()));
        assert!(a.contains(&"@/t/params.json".to_string()));
        assert!(a.contains(&"--no-save-rerun".to_string()));
        assert!(a.windows(2).any(|w| w == ["--format", "json"]));
    }

    #[test]
    fn commands_take_no_params_or_noop() {
        let a = build_args(
            &BoltConfig::default(),
            JobKind::Command,
            "uptime",
            true,
            Path::new("/i"),
            Path::new("/t"),
            Some(Path::new("/p")),
        );
        assert!(!a.contains(&"--noop".to_string()) && !a.contains(&"--params".to_string()));
    }

    #[test]
    fn parses_task_items_and_marks_missing_targets_skipped() {
        let out = r#"{"items":[
            {"target":"a","status":"success","value":{"_output":"ok password=zzz"}},
            {"target":"b","status":"failure","value":{"_error":{"msg":"boom","details":{"exit_code":3}}}}
        ]}"#;
        let targets = [tg("a"), tg("b"), tg("c")];
        let p = json!({});
        let (r, err) = parse_run(&req(JobKind::Task, &targets, &p), out, "", Some(2), 1000);
        assert!(err.is_none());
        assert_eq!(r[0].status, "success");
        assert_eq!(r[0].exit_code, Some(0));
        assert!(!r[0].output.contains("zzz"));
        assert_eq!(r[1].status, "failure");
        assert_eq!(r[1].exit_code, Some(3));
        assert_eq!(r[1].output, "boom");
        assert_eq!(r[2].status, "skipped");
    }

    #[test]
    fn malformed_output_is_a_run_error() {
        let targets = [tg("a")];
        let p = json!({});
        let (r, err) = parse_run(
            &req(JobKind::Task, &targets, &p),
            "not json",
            "Error: no such task\nmore",
            Some(1),
            100,
        );
        assert!(r.is_empty());
        assert_eq!(err.unwrap(), "Error: no such task");
        let (_, err) = parse_run(
            &req(JobKind::Task, &targets, &p),
            r#"{"kind":"bolt/unknown-task","msg":"Task not found"}"#,
            "",
            Some(1),
            100,
        );
        assert_eq!(err.unwrap(), "Task not found");
    }

    #[test]
    fn plan_results_apply_to_every_target() {
        let targets = [tg("a"), tg("b")];
        let p = json!({});
        let (ok, err) = parse_run(
            &req(JobKind::Plan, &targets, &p),
            r#"{"done":true}"#,
            "",
            Some(0),
            100,
        );
        assert!(err.is_none() && ok.iter().all(|t| t.status == "success"));
        let (bad, err) = parse_run(
            &req(JobKind::Plan, &targets, &p),
            r#"{"kind":"bolt/run-failure","msg":"plan failed"}"#,
            "",
            Some(2),
            100,
        );
        assert_eq!(err.unwrap(), "plan failed");
        assert!(bad.iter().all(|t| t.status == "failure"));
    }

    #[test]
    fn private_dir_is_owner_only_and_removed() {
        let dir = PrivateDir::create().unwrap();
        let p = dir.path().to_path_buf();
        let _f = dir.write("x", "secret").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&_f).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        assert!(dir.write("x", "again").is_err());
        drop(dir);
        assert!(!p.exists());
    }

    #[cfg(unix)]
    fn fake_bolt(body: &str) -> (PrivateDir, BoltConfig) {
        use std::os::unix::fs::PermissionsExt;
        let dir = PrivateDir::create().unwrap();
        let path = dir.write("bolt", &format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cfg = BoltConfig {
            enabled: true,
            binary_path: path,
            timeout_seconds: 5,
            ..Default::default()
        };
        (dir, cfg)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_fake_bolt_end_to_end() {
        let (_d, cfg) = fake_bolt(
            r#"echo '{"items":[{"target":"a","status":"success","value":{"_output":"hi"}}]}'"#,
        );
        let targets = [tg("a")];
        let p = json!({});
        let (_tx, rx) = watch::channel(false);
        let out = run(&cfg, &req(JobKind::Task, &targets, &p), rx).await;
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.targets[0].output, "hi");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn times_out_and_cancels() {
        let (_d, mut cfg) = fake_bolt("sleep 30");
        cfg.timeout_seconds = 1;
        let targets = [tg("a")];
        let p = json!({});
        let (_tx, rx) = watch::channel(false);
        let out = run(&cfg, &req(JobKind::Task, &targets, &p), rx).await;
        assert!(out.timed_out);

        cfg.timeout_seconds = 30;
        let (tx, rx) = watch::channel(false);
        let handle = {
            let cfg = cfg.clone();
            let targets = targets.to_vec();
            tokio::spawn(async move {
                let p = json!({});
                run(&cfg, &req(JobKind::Task, &targets, &p), rx).await
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        tx.send(true).unwrap();
        let out = handle.await.unwrap();
        assert!(out.cancelled && !out.timed_out);
    }

    #[tokio::test]
    async fn a_missing_binary_is_a_run_error() {
        let cfg = BoltConfig {
            binary_path: PathBuf::from("/nonexistent/bolt"),
            ..Default::default()
        };
        let targets = [tg("a")];
        let p = json!({});
        let (_tx, rx) = watch::channel(false);
        let out = run(&cfg, &req(JobKind::Task, &targets, &p), rx).await;
        assert!(out.error.unwrap().contains("cannot run bolt"));
    }
}
