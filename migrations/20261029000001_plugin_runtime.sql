-- Plugin runtime (M20): approved capabilities, key/value storage, settings,
-- migration bookkeeping for plugin-owned databases and pending upgrades.
-- See docs/architecture/PLUGIN_PLATFORM.md sections 6, 7 and 15.

CREATE TABLE IF NOT EXISTS plugin_grants (
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    capability TEXT NOT NULL,
    scope_json TEXT,
    granted_by TEXT,
    granted_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, capability)
);

CREATE TABLE IF NOT EXISTS plugin_kv (
    plugin_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value BLOB NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

CREATE TABLE IF NOT EXISTS plugin_config (
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_by TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

-- Migrations applied to a plugin's own database (db:own).
CREATE TABLE IF NOT EXISTS plugin_migrations (
    plugin_id TEXT NOT NULL,
    number INTEGER NOT NULL,
    name TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, number)
);

-- Version waiting for an administrator to approve the capabilities it adds;
-- the previous version stays active meanwhile.
ALTER TABLE plugins ADD COLUMN pending_version TEXT;
