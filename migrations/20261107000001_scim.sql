-- M31 (T106): inbound SCIM 2.0 provisioning (RFC 7643, RFC 7644).
--
-- A SCIM-provisioned account is a row of `users` with `directory_id = 'scim'`:
-- like an M30 directory account it has no local password, its identity fields
-- are read-only in the UI and deactivation revokes its sessions and API keys.
-- The tables below hold what `users` has no column for.

-- Bearer tokens of SCIM clients. Only a SHA-256 hash of the secret is kept;
-- the token itself is shown once, when it is created or rotated.
CREATE TABLE IF NOT EXISTS scim_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    token_hint TEXT NOT NULL,         -- last characters of the secret, for recognition
    created_by TEXT,
    created_at TEXT NOT NULL,
    rotated_at TEXT,
    expires_at TEXT,
    last_used_at TEXT,
    revoked_at TEXT,
    revoked_by TEXT
);

-- SCIM attributes of provisioned users. `deleted_at` is set by a SCIM DELETE:
-- the account is deactivated and kept, and the resource is gone for SCIM.
CREATE TABLE IF NOT EXISTS scim_users (
    user_id TEXT PRIMARY KEY,
    external_id TEXT,
    display_name TEXT,
    given_name TEXT,
    family_name TEXT,
    formatted_name TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_scim_users_external_id
    ON scim_users(external_id) WHERE external_id IS NOT NULL;

-- Groups pushed by the SCIM client. They only exist to be mapped to roles
-- (auth_role_mappings with directory_id = 'scim').
CREATE TABLE IF NOT EXISTS scim_groups (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    external_id TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_scim_groups_display_name
    ON scim_groups(display_name COLLATE NOCASE);
CREATE UNIQUE INDEX IF NOT EXISTS idx_scim_groups_external_id
    ON scim_groups(external_id) WHERE external_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS scim_group_members (
    group_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    added_at TEXT NOT NULL,
    PRIMARY KEY (group_id, user_id),
    FOREIGN KEY (group_id) REFERENCES scim_groups(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_scim_group_members_user ON scim_group_members(user_id);
