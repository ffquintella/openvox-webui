CREATE TABLE IF NOT EXISTS host_inventory_snapshots (
    id TEXT PRIMARY KEY,
    certname TEXT NOT NULL,
    collector_version TEXT NOT NULL,
    collected_at TEXT NOT NULL,
    is_full_snapshot INTEGER NOT NULL DEFAULT 1,
    os_family TEXT NOT NULL,
    distribution TEXT NOT NULL,
    os_version TEXT NOT NULL,
    package_count INTEGER NOT NULL DEFAULT 0,
    application_count INTEGER NOT NULL DEFAULT 0,
    website_count INTEGER NOT NULL DEFAULT 0,
    runtime_count INTEGER NOT NULL DEFAULT 0,
    raw_payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_host_inventory_snapshots_certname_collected_at
    ON host_inventory_snapshots(certname, collected_at DESC);

CREATE TABLE IF NOT EXISTS host_os_inventory (
    certname TEXT PRIMARY KEY,
    snapshot_id TEXT NOT NULL,
    os_family TEXT NOT NULL,
    distribution TEXT NOT NULL,
    edition TEXT,
    architecture TEXT,
    kernel_version TEXT,
    os_version TEXT NOT NULL,
    patch_level TEXT,
    package_manager TEXT,
    update_channel TEXT,
    last_inventory_at TEXT,
    last_successful_update_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS host_package_inventory (
    id TEXT PRIMARY KEY,
    certname TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    name TEXT NOT NULL,
    epoch TEXT,
    version TEXT NOT NULL,
    release TEXT,
    architecture TEXT,
    repository_source TEXT,
    install_path TEXT,
    install_time TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_host_package_inventory_certname
    ON host_package_inventory(certname, name);

CREATE TABLE IF NOT EXISTS host_application_inventory (
    id TEXT PRIMARY KEY,
    certname TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    name TEXT NOT NULL,
    publisher TEXT,
    version TEXT NOT NULL,
    architecture TEXT,
    install_scope TEXT,
    install_path TEXT,
    application_type TEXT,
    bundle_identifier TEXT,
    uninstall_identity TEXT,
    install_date TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_host_application_inventory_certname
    ON host_application_inventory(certname, name);

CREATE TABLE IF NOT EXISTS host_web_inventory (
    id TEXT PRIMARY KEY,
    certname TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    server_type TEXT NOT NULL,
    site_name TEXT NOT NULL,
    bindings_json TEXT NOT NULL,
    document_root TEXT,
    application_pool TEXT,
    tls_certificate_reference TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_host_web_inventory_certname
    ON host_web_inventory(certname, server_type, site_name);

CREATE TABLE IF NOT EXISTS host_runtime_inventory (
    id TEXT PRIMARY KEY,
    certname TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    runtime_type TEXT NOT NULL,
    runtime_name TEXT NOT NULL,
    runtime_version TEXT,
    install_path TEXT,
    management_endpoint TEXT,
    deployed_units_json TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_host_runtime_inventory_certname
    ON host_runtime_inventory(certname, runtime_type, runtime_name);
