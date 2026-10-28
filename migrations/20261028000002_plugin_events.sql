-- Plugin platform (M21): durable event outbox with per-target deliveries,
-- scheduled plugin jobs, inbound webhook secrets and outbound webhook
-- endpoints. See docs/architecture/PLUGIN_PLATFORM.md sections 9.1 and 9.4.

CREATE TABLE IF NOT EXISTS event_outbox (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    payload TEXT NOT NULL,
    actor_kind TEXT NOT NULL DEFAULT 'system',
    actor_id TEXT,
    correlation_id TEXT,
    occurred_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- pending (not yet fanned out to targets) | fanned_out
    state TEXT NOT NULL DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_event_outbox_state ON event_outbox(state, created_at);

CREATE TABLE IF NOT EXISTS event_deliveries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL REFERENCES event_outbox(id) ON DELETE CASCADE,
    target_kind TEXT NOT NULL,                   -- plugin | webhook
    target_id TEXT NOT NULL,                     -- plugin id or webhook endpoint id
    handler TEXT,
    priority INTEGER NOT NULL DEFAULT 100,
    -- pending | in_flight | delivered | dead_letter | dropped
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    delivered_at TEXT,
    UNIQUE (event_id, target_kind, target_id)
);

CREATE INDEX IF NOT EXISTS idx_event_deliveries_due
    ON event_deliveries(status, next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_event_deliveries_target
    ON event_deliveries(target_kind, target_id);

CREATE TABLE IF NOT EXISTS plugin_jobs (
    plugin_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    next_run_at TEXT,
    last_run_at TEXT,
    last_status TEXT,
    last_error TEXT,
    run_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (plugin_id, job_id)
);

CREATE TABLE IF NOT EXISTS plugin_job_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    trigger TEXT NOT NULL,                       -- schedule | manual
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL,                        -- running | ok | error | timeout
    error TEXT,
    duration_ms INTEGER
);

CREATE INDEX IF NOT EXISTS idx_plugin_job_runs_job
    ON plugin_job_runs(plugin_id, job_id, started_at);

CREATE TABLE IF NOT EXISTS plugin_webhook_secrets (
    plugin_id TEXT NOT NULL,
    hook_id TEXT NOT NULL,
    secret TEXT NOT NULL,
    created_at TEXT NOT NULL,
    rotated_at TEXT NOT NULL,
    last_used_at TEXT,
    PRIMARY KEY (plugin_id, hook_id)
);

CREATE TABLE IF NOT EXISTS plugin_webhook_endpoints (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    secret TEXT NOT NULL,
    event_types TEXT NOT NULL,                   -- JSON array of type patterns
    filter TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
