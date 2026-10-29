-- Plugin governance (M22): hourly usage rollups per (plugin, version, kind)
-- and the persisted state of each plugin's circuit breaker.
-- See docs/architecture/PLUGIN_PLATFORM.md sections 10 and 15.

CREATE TABLE IF NOT EXISTS plugin_usage_hourly (
    plugin_id TEXT NOT NULL,
    version TEXT NOT NULL,
    kind TEXT NOT NULL,                          -- event | job | webhook | lifecycle
    hour TEXT NOT NULL,                          -- RFC 3339, truncated to the hour (UTC)
    calls INTEGER NOT NULL DEFAULT 0,
    errors INTEGER NOT NULL DEFAULT 0,
    traps INTEGER NOT NULL DEFAULT 0,
    timeouts INTEGER NOT NULL DEFAULT 0,
    quota_rejections INTEGER NOT NULL DEFAULT 0,
    circuit_rejections INTEGER NOT NULL DEFAULT 0,
    total_ms INTEGER NOT NULL DEFAULT 0,
    max_ms INTEGER NOT NULL DEFAULT 0,
    latency_buckets TEXT NOT NULL DEFAULT '[]',  -- JSON array of counts per bucket
    PRIMARY KEY (plugin_id, version, kind, hour)
);

CREATE INDEX IF NOT EXISTS idx_plugin_usage_hour ON plugin_usage_hourly(hour);

CREATE TABLE IF NOT EXISTS plugin_circuit (
    plugin_id TEXT PRIMARY KEY,
    -- open | half_open (closed plugins have no row)
    state TEXT NOT NULL,
    trips INTEGER NOT NULL DEFAULT 1,
    opened_at TEXT NOT NULL,
    retry_at TEXT NOT NULL,
    last_error TEXT
);
