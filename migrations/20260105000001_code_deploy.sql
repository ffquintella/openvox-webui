-- Code Deploy Feature: Git-based environment management
-- Similar to Puppet Code Manager with r10k integration

-- SSH keys for Git authentication
CREATE TABLE IF NOT EXISTS code_ssh_keys (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    private_key_encrypted TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Git repositories configuration
CREATE TABLE IF NOT EXISTS code_repositories (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    branch_pattern TEXT NOT NULL DEFAULT '*',
    ssh_key_id TEXT REFERENCES code_ssh_keys(id) ON DELETE SET NULL,
    webhook_secret TEXT,
    poll_interval_seconds INTEGER NOT NULL DEFAULT 300,
    is_control_repo INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_error_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Discovered environments (from branches)
CREATE TABLE IF NOT EXISTS code_environments (
    id TEXT PRIMARY KEY NOT NULL,
    repository_id TEXT NOT NULL REFERENCES code_repositories(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    branch TEXT NOT NULL,
    current_commit TEXT,
    current_commit_message TEXT,
    current_commit_author TEXT,
    current_commit_date TEXT,
    last_synced_at TEXT,
    auto_deploy INTEGER NOT NULL DEFAULT 0,
    requires_approval INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repository_id, name)
);

-- Deployment history
CREATE TABLE IF NOT EXISTS code_deployments (
    id TEXT PRIMARY KEY NOT NULL,
    environment_id TEXT NOT NULL REFERENCES code_environments(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    commit_message TEXT,
    commit_author TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    requested_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    approved_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    approved_at TEXT,
    rejected_at TEXT,
    rejection_reason TEXT,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT,
    r10k_output TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_code_repositories_name ON code_repositories(name);
CREATE INDEX IF NOT EXISTS idx_code_environments_repository ON code_environments(repository_id);
CREATE INDEX IF NOT EXISTS idx_code_environments_name ON code_environments(name);
CREATE INDEX IF NOT EXISTS idx_code_deployments_environment ON code_deployments(environment_id);
CREATE INDEX IF NOT EXISTS idx_code_deployments_status ON code_deployments(status);
CREATE INDEX IF NOT EXISTS idx_code_deployments_created ON code_deployments(created_at DESC);
