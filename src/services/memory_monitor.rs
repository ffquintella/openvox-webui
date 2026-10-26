//! Process memory monitor: samples usage against the cgroup `MemoryHigh`
//! limit, warns early, and exits non-zero when usage stays pinned near it.

use std::fs;
use std::sync::RwLock;
use std::time::Duration;

use serde::Serialize;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::config::MemoryMonitorConfig;

const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// Most recent sample, readable from request handlers (health endpoint).
static LATEST: RwLock<Option<MemorySnapshot>> = RwLock::new(None);

/// A single memory sample.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemorySnapshot {
    /// Non-reclaimable memory in use, in bytes (cgroup `anon`, else VmRSS)
    pub used_bytes: u64,
    /// Limit the usage is compared against, in bytes (None when unlimited)
    pub limit_bytes: Option<u64>,
    /// `used_bytes / limit_bytes` as a percentage (None when unlimited)
    pub percent: Option<f64>,
}

impl MemorySnapshot {
    fn new(used_bytes: u64, limit_bytes: Option<u64>) -> Self {
        let percent = limit_bytes
            .filter(|l| *l > 0)
            .map(|l| used_bytes as f64 * 100.0 / l as f64);
        Self {
            used_bytes,
            limit_bytes,
            percent,
        }
    }

    /// Latest sample taken by the background monitor, if it is running.
    pub fn latest() -> Option<MemorySnapshot> {
        LATEST.read().ok().and_then(|g| g.clone())
    }
}

/// Handle to the running monitor task.
pub struct MemoryMonitorHandle {
    _task: JoinHandle<()>,
}

/// Parse a cgroup v2 limit file value; `max` means unlimited.
fn parse_limit(raw: &str) -> Option<u64> {
    let v = raw.trim();
    if v == "max" {
        None
    } else {
        v.parse().ok()
    }
}

/// Extract a `key value` counter from `memory.stat`.
fn stat_value(stat: &str, key: &str) -> Option<u64> {
    stat.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next() == Some(key))
            .then(|| parts.next().and_then(|v| v.parse().ok()))
            .flatten()
    })
}

/// Extract VmRSS (kB) from `/proc/self/status` content, as bytes.
fn parse_vm_rss(status: &str) -> Option<u64> {
    status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

/// Resolve this process's cgroup v2 directory.
fn cgroup_dir() -> Option<String> {
    let content = fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = content.lines().find_map(|l| l.strip_prefix("0::"))?;
    Some(format!("{}{}", CGROUP_ROOT, path.trim_end()))
}

/// Take one sample using the cgroup when available, falling back to VmRSS.
fn sample(cfg: &MemoryMonitorConfig) -> Option<MemorySnapshot> {
    let fallback_limit = (cfg.fallback_limit_mb > 0).then(|| cfg.fallback_limit_mb * 1024 * 1024);

    if let Some(dir) = cgroup_dir() {
        let read = |name: &str| fs::read_to_string(format!("{}/{}", dir, name)).ok();
        let limit = read("memory.high")
            .and_then(|v| parse_limit(&v))
            .or_else(|| read("memory.max").and_then(|v| parse_limit(&v)))
            .or(fallback_limit);
        // `anon` excludes reclaimable page cache, which `memory.current`
        // includes and which the kernel drops long before throttling.
        let used = read("memory.stat").and_then(|s| stat_value(&s, "anon"));
        if let Some(used) = used {
            return Some(MemorySnapshot::new(used, limit));
        }
    }

    let rss = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| parse_vm_rss(&s))?;
    Some(MemorySnapshot::new(rss, fallback_limit))
}

/// Decision taken for one sample.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Ok,
    Warn,
    Restart,
}

/// Track consecutive over-threshold samples and decide what to do.
fn evaluate(
    cfg: &MemoryMonitorConfig,
    snap: &MemorySnapshot,
    consecutive_over: &mut u32,
) -> Action {
    let Some(pct) = snap.percent else {
        *consecutive_over = 0;
        return Action::Ok;
    };

    if cfg.restart_percent > 0 && pct >= f64::from(cfg.restart_percent) {
        *consecutive_over += 1;
        if *consecutive_over >= cfg.restart_after_samples.max(1) {
            return Action::Restart;
        }
        return Action::Warn;
    }

    *consecutive_over = 0;
    if pct >= f64::from(cfg.warn_percent) {
        Action::Warn
    } else {
        Action::Ok
    }
}

fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

/// Start the background memory monitor.
pub fn start_memory_monitor(cfg: MemoryMonitorConfig) -> Option<MemoryMonitorHandle> {
    if !cfg.enabled {
        info!("Memory monitor disabled");
        return None;
    }

    info!(
        "Starting memory monitor (interval: {}s, warn: {}%, restart: {}% x{} samples)",
        cfg.interval_secs, cfg.warn_percent, cfg.restart_percent, cfg.restart_after_samples
    );

    let task = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(cfg.interval_secs.max(1)));
        let mut consecutive_over = 0u32;
        let mut last_warn_bucket: Option<u64> = None;

        loop {
            ticker.tick().await;

            let Some(snap) = sample(&cfg) else {
                continue;
            };
            if let Ok(mut guard) = LATEST.write() {
                *guard = Some(snap.clone());
            }

            match evaluate(&cfg, &snap, &mut consecutive_over) {
                Action::Ok => last_warn_bucket = None,
                Action::Warn => {
                    // Log on first crossing, then every further 5 points, so a
                    // pinned process does not flood the journal.
                    let bucket = snap.percent.map(|p| (p / 5.0) as u64);
                    if bucket != last_warn_bucket {
                        warn!(
                            "Memory usage high: {} MiB of {} MiB ({:.1}%)",
                            mib(snap.used_bytes),
                            snap.limit_bytes.map(mib).unwrap_or(0),
                            snap.percent.unwrap_or(0.0)
                        );
                        last_warn_bucket = bucket;
                    }
                }
                Action::Restart => {
                    error!(
                        "Memory usage pinned at {:.1}% of limit ({} MiB) for {} samples; \
                         exiting so the service manager can restart a healthy instance",
                        snap.percent.unwrap_or(0.0),
                        mib(snap.used_bytes),
                        consecutive_over
                    );
                    std::process::exit(1);
                }
            }
        }
    });

    Some(MemoryMonitorHandle { _task: task })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MemoryMonitorConfig {
        MemoryMonitorConfig {
            warn_percent: 80,
            restart_percent: 95,
            restart_after_samples: 3,
            ..MemoryMonitorConfig::default()
        }
    }

    fn snap(used: u64, limit: Option<u64>) -> MemorySnapshot {
        MemorySnapshot::new(used, limit)
    }

    #[test]
    fn parses_cgroup_limit() {
        assert_eq!(parse_limit("max\n"), None);
        assert_eq!(parse_limit("3221225472\n"), Some(3_221_225_472));
        assert_eq!(parse_limit("garbage"), None);
    }

    #[test]
    fn parses_memory_stat_and_vm_rss() {
        let stat = "anon 1048576\nfile 4096\nanon_thp 0\n";
        assert_eq!(stat_value(stat, "anon"), Some(1_048_576));
        assert_eq!(stat_value(stat, "missing"), None);
        assert_eq!(
            parse_vm_rss("Name:\tx\nVmRSS:\t   2048 kB\n"),
            Some(2_097_152)
        );
    }

    #[test]
    fn unlimited_never_acts() {
        let mut n = 0;
        assert_eq!(
            evaluate(&cfg(), &snap(u64::MAX / 2, None), &mut n),
            Action::Ok
        );
    }

    #[test]
    fn warns_between_thresholds() {
        let mut n = 0;
        assert_eq!(evaluate(&cfg(), &snap(50, Some(100)), &mut n), Action::Ok);
        assert_eq!(evaluate(&cfg(), &snap(85, Some(100)), &mut n), Action::Warn);
    }

    #[test]
    fn restarts_only_after_sustained_breach() {
        let mut n = 0;
        let hot = snap(97, Some(100));
        assert_eq!(evaluate(&cfg(), &hot, &mut n), Action::Warn);
        assert_eq!(evaluate(&cfg(), &hot, &mut n), Action::Warn);
        assert_eq!(evaluate(&cfg(), &hot, &mut n), Action::Restart);
    }

    #[test]
    fn dip_resets_the_breach_counter() {
        let mut n = 0;
        let hot = snap(97, Some(100));
        evaluate(&cfg(), &hot, &mut n);
        evaluate(&cfg(), &hot, &mut n);
        assert_eq!(evaluate(&cfg(), &snap(10, Some(100)), &mut n), Action::Ok);
        assert_eq!(evaluate(&cfg(), &hot, &mut n), Action::Warn);
    }

    #[test]
    fn restart_percent_zero_disables_restart() {
        let mut c = cfg();
        c.restart_percent = 0;
        let mut n = 0;
        for _ in 0..10 {
            assert_ne!(evaluate(&c, &snap(99, Some(100)), &mut n), Action::Restart);
        }
    }
}
