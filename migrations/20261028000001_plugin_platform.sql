-- Plugin platform (M19): installed plugins, stored versions and trusted signing keys.
-- Packages are verified (ed25519) before a row is written here; see
-- docs/architecture/PLUGIN_PLATFORM.md sections 4.3, 7 and 15.

CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY,
    active_version TEXT,
    -- installed | configured | enabled | disabled | error | update_pending
    state TEXT NOT NULL DEFAULT 'installed',
    state_reason TEXT,
    schema_version INTEGER NOT NULL DEFAULT 0,
    trusted INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 100,
    installed_by TEXT,
    installed_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS plugin_versions (
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    api_version INTEGER NOT NULL,
    manifest_json TEXT NOT NULL,
    package_sha256 TEXT NOT NULL,
    signature TEXT NOT NULL,
    signed_by_key_id TEXT NOT NULL,
    signed_at TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, version)
);

CREATE INDEX IF NOT EXISTS idx_plugin_versions_signer
    ON plugin_versions(signed_by_key_id);

CREATE TABLE IF NOT EXISTS plugin_trusted_keys (
    key_id TEXT PRIMARY KEY,
    public_key BLOB NOT NULL,
    label TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'admin',        -- builtin | admin
    allowed_plugin_ids TEXT,                     -- JSON array; NULL = any plugin
    expires_at TEXT,
    state TEXT NOT NULL DEFAULT 'active',        -- active | revoked
    added_by TEXT,
    added_at TEXT NOT NULL,
    approved_by TEXT,
    revoked_at TEXT,
    revoked_reason TEXT
);
