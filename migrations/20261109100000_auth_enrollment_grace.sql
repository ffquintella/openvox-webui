-- OQ-12: enrollment grace windows and enrollment-only sessions.
-- Additive: a new table, two indexes and a nullable column (rollback steps
-- are in docs/architecture/PLUGIN_PLATFORM.md section 17.16).

-- A window in which a user with nothing enrolled may sign in to an
-- enrollment-only session. Rows are kept as history.
CREATE TABLE IF NOT EXISTS auth_enrollment_grace (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    reason TEXT NOT NULL,             -- 'factor_reset' | 'policy'
    granted_by TEXT,                  -- administrator user id; NULL = system
    expires_at TEXT NOT NULL,         -- fixed at grant time, never extended
    logins INTEGER NOT NULL DEFAULT 0,
    closed_at TEXT,
    closed_reason TEXT,               -- 'enrolled' | 'superseded' | 'expired' | 'exhausted'
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_auth_enrollment_grace_user
    ON auth_enrollment_grace(user_id, reason);

-- At most one open window per user.
CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_enrollment_grace_open
    ON auth_enrollment_grace(user_id) WHERE closed_at IS NULL;

-- NULL = a full session; 'enrollment' = may only list and enroll factors.
ALTER TABLE auth_sessions ADD COLUMN scope TEXT;
