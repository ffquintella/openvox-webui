-- Group Update Schedules: recurring and one-time update policies for node groups

CREATE TABLE IF NOT EXISTS group_update_schedules (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    schedule_type TEXT NOT NULL,                    -- 'one_time' | 'recurring'
    cron_expression TEXT,                           -- 6-field cron for recurring (sec min hour dom month dow)
    scheduled_for TEXT,                             -- ISO8601 timestamp for one-time
    operation_type TEXT NOT NULL,                   -- system_patch | security_patch | package_update
    package_names_json TEXT NOT NULL DEFAULT '[]',
    requires_approval INTEGER NOT NULL DEFAULT 0,
    maintenance_window_start TEXT,
    maintenance_window_end TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    next_run_at TEXT,
    last_job_id TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(group_id) REFERENCES node_groups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_group_update_schedules_group
    ON group_update_schedules(group_id);

CREATE INDEX IF NOT EXISTS idx_group_update_schedules_due
    ON group_update_schedules(enabled, next_run_at);
