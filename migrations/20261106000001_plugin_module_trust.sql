-- Trusted module pages (M31, T105): one approval per plugin, bound to a
-- version and its package hash. Acted on only while
-- plugins.allow_trusted_modules is on; see PLUGIN_PLATFORM.md section 17.13.

CREATE TABLE IF NOT EXISTS plugin_module_trust (
    plugin_id TEXT PRIMARY KEY REFERENCES plugins(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    package_sha256 TEXT NOT NULL,
    -- JSON array of {id, slot, src, integrity} as approved
    modules_json TEXT NOT NULL,
    trusted_by TEXT NOT NULL,
    trusted_at TEXT NOT NULL
);
