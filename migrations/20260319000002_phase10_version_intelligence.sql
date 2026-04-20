CREATE TABLE IF NOT EXISTS repository_version_catalog (
    id TEXT PRIMARY KEY,
    platform_family TEXT NOT NULL,
    distribution TEXT NOT NULL,
    package_manager TEXT,
    software_type TEXT NOT NULL,
    software_name TEXT NOT NULL,
    repository_source TEXT,
    latest_version TEXT NOT NULL,
    latest_release TEXT,
    source_kind TEXT NOT NULL,
    observed_nodes INTEGER NOT NULL DEFAULT 0,
    last_seen_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_repository_version_catalog_identity
    ON repository_version_catalog(
        platform_family,
        distribution,
        package_manager,
        software_type,
        software_name,
        COALESCE(repository_source, '')
    );

CREATE TABLE IF NOT EXISTS host_update_status (
    certname TEXT PRIMARY KEY,
    snapshot_id TEXT,
    is_stale INTEGER NOT NULL DEFAULT 0,
    stale_reason TEXT,
    outdated_packages INTEGER NOT NULL DEFAULT 0,
    outdated_applications INTEGER NOT NULL DEFAULT 0,
    total_packages INTEGER NOT NULL DEFAULT 0,
    total_applications INTEGER NOT NULL DEFAULT 0,
    outdated_items_json TEXT,
    checked_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_host_update_status_checked_at
    ON host_update_status(checked_at DESC);
