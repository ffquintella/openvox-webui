-- M30: directory providers (LDAP and Active Directory).

-- A user provisioned from a directory has `directory_id` set: the directory
-- owns the identity fields (username, e-mail) and there is no local password.
-- Deactivation keeps the row (audit history, ownership) and blocks every way
-- in: login, token refresh, sessions and API keys.
ALTER TABLE users ADD COLUMN directory_id TEXT;
ALTER TABLE users ADD COLUMN deactivated_at TEXT;
ALTER TABLE users ADD COLUMN deactivated_reason TEXT;
CREATE INDEX IF NOT EXISTS idx_users_directory ON users(directory_id);

-- Revoked API keys stay listed but no longer authenticate.
ALTER TABLE api_keys ADD COLUMN revoked_at TEXT;

-- Group-to-role mapping per directory. `group_ref` is the group's DN, SID or
-- objectGUID; a role that is not mapped is never granted by a directory.
CREATE TABLE IF NOT EXISTS auth_role_mappings (
    id TEXT PRIMARY KEY,
    directory_id TEXT NOT NULL,
    group_ref TEXT NOT NULL,
    group_ref_kind TEXT NOT NULL,     -- 'dn' | 'sid' | 'guid'
    group_label TEXT,
    role_id TEXT NOT NULL,
    created_by TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,
    UNIQUE (directory_id, group_ref, role_id)
);
CREATE INDEX IF NOT EXISTS idx_auth_role_mappings_directory ON auth_role_mappings(directory_id);

-- Roles a directory granted through a mapping. Only these are removed when the
-- mapping no longer applies; roles assigned by an administrator are untouched.
CREATE TABLE IF NOT EXISTS directory_role_grants (
    user_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    directory_id TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    PRIMARY KEY (user_id, role_id, directory_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE
);

-- One row per synchronization run (scheduled or manual).
CREATE TABLE IF NOT EXISTS directory_sync_runs (
    id TEXT PRIMARY KEY,
    directory_id TEXT NOT NULL,
    trigger_kind TEXT NOT NULL,       -- 'schedule' | 'manual'
    status TEXT NOT NULL,             -- 'running' | 'ok' | 'partial' | 'failed'
    started_at TEXT NOT NULL,
    finished_at TEXT,
    created INTEGER NOT NULL DEFAULT 0,
    updated INTEGER NOT NULL DEFAULT 0,
    deactivated INTEGER NOT NULL DEFAULT 0,
    reactivated INTEGER NOT NULL DEFAULT 0,
    errors INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    started_by TEXT
);
CREATE INDEX IF NOT EXISTS idx_directory_sync_runs_dir ON directory_sync_runs(directory_id, started_at);
