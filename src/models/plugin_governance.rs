//! Plugin governance models (M22): usage rollups, circuit breaker state and
//! the administrator dashboard views.

use chrono::{DateTime, Utc};
use serde::Serialize;

/// Upper bounds (milliseconds) of the latency histogram; one extra bucket
/// holds everything above the last bound.
pub const LATENCY_BUCKETS_MS: [u64; 12] = [
    5, 10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 60_000,
];
pub const LATENCY_BUCKET_COUNT: usize = LATENCY_BUCKETS_MS.len() + 1;

/// One (plugin, version, kind, hour) rollup row.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageRow {
    pub plugin_id: String,
    pub version: String,
    pub kind: String,
    pub hour: String,
    pub calls: u64,
    pub errors: u64,
    pub traps: u64,
    pub timeouts: u64,
    pub quota_rejections: u64,
    pub circuit_rejections: u64,
    pub total_ms: u64,
    pub max_ms: u64,
    pub latency_buckets: Vec<u64>,
}

impl UsageRow {
    pub fn merge(&mut self, o: &UsageRow) {
        self.calls += o.calls;
        self.errors += o.errors;
        self.traps += o.traps;
        self.timeouts += o.timeouts;
        self.quota_rejections += o.quota_rejections;
        self.circuit_rejections += o.circuit_rejections;
        self.total_ms += o.total_ms;
        self.max_ms = self.max_ms.max(o.max_ms);
        if self.latency_buckets.len() < o.latency_buckets.len() {
            self.latency_buckets.resize(o.latency_buckets.len(), 0);
        }
        for (a, b) in self.latency_buckets.iter_mut().zip(&o.latency_buckets) {
            *a += b;
        }
    }

    pub fn failures(&self) -> u64 {
        self.errors + self.traps + self.timeouts
    }
}

/// Estimated latency percentile (upper bound of the bucket that holds it).
pub fn percentile_ms(buckets: &[u64], q: f64, max_ms: u64) -> Option<u64> {
    let total: u64 = buckets.iter().sum();
    if total == 0 {
        return None;
    }
    let rank = ((total as f64) * q).ceil().max(1.0) as u64;
    let mut seen = 0;
    for (i, n) in buckets.iter().enumerate() {
        seen += n;
        if seen >= rank {
            return Some(match LATENCY_BUCKETS_MS.get(i) {
                Some(bound) => (*bound).min(max_ms.max(1)),
                None => max_ms,
            });
        }
    }
    Some(max_ms)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    Closed,
    HalfOpen,
    Open,
}

impl CircuitState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CircuitState::Closed => "closed",
            CircuitState::HalfOpen => "half_open",
            CircuitState::Open => "open",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CircuitView {
    pub state: CircuitState,
    /// Trips in a row without the breaker closing in between
    pub trips: u32,
    pub retry_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl CircuitView {
    pub fn closed() -> Self {
        Self {
            state: CircuitState::Closed,
            trips: 0,
            retry_at: None,
            last_error: None,
        }
    }
}

/// Persisted breaker state of a plugin that is not closed.
#[derive(Debug, Clone)]
pub struct CircuitRecord {
    pub plugin_id: String,
    pub state: CircuitState,
    pub trips: u32,
    pub opened_at: DateTime<Utc>,
    pub retry_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageTotals {
    pub calls: u64,
    pub errors: u64,
    pub traps: u64,
    pub timeouts: u64,
    pub quota_rejections: u64,
    pub circuit_rejections: u64,
    pub error_rate: f64,
    pub avg_ms: Option<f64>,
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
    pub p99_ms: Option<u64>,
    pub max_ms: u64,
}

impl UsageTotals {
    pub fn from_rows<'a>(rows: impl Iterator<Item = &'a UsageRow>) -> Self {
        let mut acc = UsageRow::default();
        for r in rows {
            acc.merge(r);
        }
        let failures = acc.failures();
        Self {
            calls: acc.calls,
            errors: acc.errors,
            traps: acc.traps,
            timeouts: acc.timeouts,
            quota_rejections: acc.quota_rejections,
            circuit_rejections: acc.circuit_rejections,
            error_rate: if acc.calls == 0 {
                0.0
            } else {
                failures as f64 / acc.calls as f64
            },
            avg_ms: (acc.calls > 0).then(|| acc.total_ms as f64 / acc.calls as f64),
            p50_ms: percentile_ms(&acc.latency_buckets, 0.50, acc.max_ms),
            p95_ms: percentile_ms(&acc.latency_buckets, 0.95, acc.max_ms),
            p99_ms: percentile_ms(&acc.latency_buckets, 0.99, acc.max_ms),
            max_ms: acc.max_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginHealth {
    Healthy,
    Degraded,
    CircuitOpen,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginGovernanceEntry {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub state: String,
    pub state_reason: Option<String>,
    pub health: PluginHealth,
    pub circuit: CircuitView,
    pub invocations_per_minute_limit: Option<u32>,
    pub last_hour: UsageTotals,
    pub last_24h: UsageTotals,
    pub revoked_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceOverview {
    pub plugins: Vec<PluginGovernanceEntry>,
    /// Plugin ids with the most handler time in the last 24 hours
    pub top_consumers: Vec<String>,
    pub open_circuits: u32,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsagePoint {
    pub hour: String,
    pub calls: u64,
    pub errors: u64,
    pub traps: u64,
    pub timeouts: u64,
    pub quota_rejections: u64,
    pub circuit_rejections: u64,
    pub p95_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionUsage {
    pub version: String,
    pub totals: UsageTotals,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginMetricsView {
    pub plugin_id: String,
    pub hours: u32,
    pub circuit: CircuitView,
    pub totals: UsageTotals,
    pub by_version: Vec<VersionUsage>,
    pub series: Vec<UsagePoint>,
}
