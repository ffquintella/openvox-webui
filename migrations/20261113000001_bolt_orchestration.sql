-- OpenBolt orchestration (M32, S27): allowlist, jobs and per-target results.

CREATE TABLE IF NOT EXISTS bolt_allowlist (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('task', 'plan')),
    name TEXT NOT NULL,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    created_by TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (organization_id, kind, name)
);

CREATE TABLE IF NOT EXISTS bolt_jobs (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('task', 'plan', 'command')),
    name TEXT NOT NULL,
    -- Real parameters until the job ends, then the redacted copy (see service).
    parameters TEXT NOT NULL DEFAULT '{}',
    sensitive_names TEXT NOT NULL DEFAULT '[]',
    targets TEXT NOT NULL DEFAULT '[]',
    target_group_id TEXT,
    noop INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    requested_by TEXT NOT NULL,
    requested_by_id TEXT NOT NULL,
    approved_by TEXT,
    approval_notes TEXT,
    maintenance_window_start TEXT,
    maintenance_window_end TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_bolt_jobs_org_created ON bolt_jobs (organization_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_bolt_jobs_status ON bolt_jobs (status);

CREATE TABLE IF NOT EXISTS bolt_job_targets (
    job_id TEXT NOT NULL REFERENCES bolt_jobs(id) ON DELETE CASCADE,
    certname TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    exit_code INTEGER,
    result TEXT,
    output TEXT,
    started_at TEXT,
    finished_at TEXT,
    PRIMARY KEY (job_id, certname)
);

CREATE INDEX IF NOT EXISTS idx_bolt_job_targets_certname ON bolt_job_targets (certname);

-- super_admin bypasses permission checks in the API, but the UI reads the stored rows.
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0006-0001-000000000021', '00000000-0000-0000-0000-000000000006', 'orchestration', 'admin', 'all');
