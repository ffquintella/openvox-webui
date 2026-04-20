CREATE TABLE IF NOT EXISTS update_jobs (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    operation_type TEXT NOT NULL,
    package_names_json TEXT NOT NULL,
    target_group_id TEXT,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    scheduled_for TEXT,
    maintenance_window_start TEXT,
    maintenance_window_end TEXT,
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    approval_notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_update_jobs_status_created
    ON update_jobs(status, created_at DESC);

CREATE TABLE IF NOT EXISTS update_job_targets (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    certname TEXT NOT NULL,
    status TEXT NOT NULL,
    dispatched_at TEXT,
    completed_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(job_id) REFERENCES update_jobs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_update_job_targets_job
    ON update_job_targets(job_id);

CREATE INDEX IF NOT EXISTS idx_update_job_targets_certname_status
    ON update_job_targets(certname, status);

CREATE TABLE IF NOT EXISTS update_job_results (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    certname TEXT NOT NULL,
    status TEXT NOT NULL,
    summary TEXT,
    output TEXT,
    started_at TEXT,
    finished_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(job_id) REFERENCES update_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY(target_id) REFERENCES update_job_targets(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_update_job_results_job
    ON update_job_results(job_id, created_at DESC);
