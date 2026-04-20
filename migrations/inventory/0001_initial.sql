-- Initial schema for the dedicated inventory SQLite database.
--
-- Consolidated from the original phase-10 inventory migrations that used to
-- live under migrations/*.sql. In older deployments those tables were created
-- in the main application database; starting with this release inventory lives
-- in its own SQLite file (default: /var/lib/openvox-webui/inventory.db) so
-- high-write ingestion from Puppet agents does not block the main DB's UI
-- readers under SQLite's single-writer model.
--
-- Tables in this database:
--   * schema_meta                    — internal key/value markers (migrator state)
--   * host_inventory_snapshots       — one row per agent inventory POST
--   * host_os_inventory              — current OS details per node
--   * host_package_inventory         — installed system packages
--   * host_application_inventory     — installed applications
--   * host_web_inventory             — web server / site configurations
--   * host_runtime_inventory         — runtimes (JVM, Node, Ruby, Python, .NET, …)
--   * host_container_inventory       — Docker / Podman containers
--   * host_user_inventory            — system user accounts
--   * host_update_status             — computed patch-compliance status
--   * repository_version_catalog     — observed / repo-checked software versions
--   * node_repository_configs        — per-node package-manager repo configs
--   * fleet_repository_configs       — deduplicated fleet-wide repo configs
--   * update_jobs / targets / results — patch orchestration state
--
-- NOTE: `group_update_schedules` is intentionally NOT in this DB — it has a
-- foreign key to `node_groups` which lives in the main application database.

-- -----------------------------------------------------------------------------
-- schema_meta: used by the startup migrator to record that it has moved
-- pre-existing inventory data out of the main DB. Queried by
-- `src/db/inventory_migration.rs`.
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS schema_meta (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- -----------------------------------------------------------------------------
-- host_inventory_snapshots
-- -----------------------------------------------------------------------------
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
    container_count INTEGER NOT NULL DEFAULT 0,
    user_count INTEGER NOT NULL DEFAULT 0,
    raw_payload TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_host_inventory_snapshots_certname_collected_at
    ON host_inventory_snapshots(certname, collected_at DESC);

-- -----------------------------------------------------------------------------
-- host_os_inventory
-- -----------------------------------------------------------------------------
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

-- -----------------------------------------------------------------------------
-- host_package_inventory
-- -----------------------------------------------------------------------------
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

-- -----------------------------------------------------------------------------
-- host_application_inventory
-- -----------------------------------------------------------------------------
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

-- -----------------------------------------------------------------------------
-- host_web_inventory
-- -----------------------------------------------------------------------------
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

-- -----------------------------------------------------------------------------
-- host_runtime_inventory
-- -----------------------------------------------------------------------------
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

-- -----------------------------------------------------------------------------
-- host_container_inventory
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS host_container_inventory (
    id TEXT PRIMARY KEY,
    certname TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    container_id TEXT NOT NULL,
    name TEXT NOT NULL,
    image TEXT NOT NULL,
    status TEXT NOT NULL,
    status_detail TEXT,
    created_at TEXT,
    ports_json TEXT NOT NULL DEFAULT '[]',
    mounts_json TEXT NOT NULL DEFAULT '[]',
    runtime_type TEXT NOT NULL,
    metadata_json TEXT,
    row_created_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_host_container_inventory_certname
    ON host_container_inventory(certname, runtime_type, name);

-- -----------------------------------------------------------------------------
-- host_user_inventory
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS host_user_inventory (
    id TEXT PRIMARY KEY,
    certname TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    username TEXT NOT NULL,
    uid INTEGER,
    sid TEXT,
    gid INTEGER,
    home_directory TEXT,
    shell TEXT,
    user_type TEXT,
    groups_json TEXT NOT NULL DEFAULT '[]',
    last_login TEXT,
    locked INTEGER,
    gecos TEXT,
    metadata_json TEXT,
    row_created_at TEXT NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES host_inventory_snapshots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_host_user_inventory_certname
    ON host_user_inventory(certname, username);

-- -----------------------------------------------------------------------------
-- host_update_status
-- -----------------------------------------------------------------------------
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

-- -----------------------------------------------------------------------------
-- repository_version_catalog
-- Includes os_version_pattern added in the 20260407000001_catalog_os_version
-- legacy migration; index includes source_kind from 20260323100000_repo_config_tables.
-- -----------------------------------------------------------------------------
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
    updated_at TEXT NOT NULL,
    os_version_pattern TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_repository_version_catalog_identity
    ON repository_version_catalog(
        platform_family,
        distribution,
        package_manager,
        software_type,
        software_name,
        COALESCE(repository_source, ''),
        source_kind,
        COALESCE(os_version_pattern, '')
    );

-- -----------------------------------------------------------------------------
-- node_repository_configs
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS node_repository_configs (
    id                TEXT PRIMARY KEY,
    certname          TEXT NOT NULL,
    snapshot_id       TEXT REFERENCES host_inventory_snapshots(id) ON DELETE SET NULL,
    os_family         TEXT NOT NULL,
    distribution      TEXT NOT NULL,
    os_version        TEXT NOT NULL,
    package_manager   TEXT NOT NULL,
    repo_id           TEXT NOT NULL,
    repo_name         TEXT,
    repo_type         TEXT NOT NULL,
    base_url          TEXT,
    mirror_list_url   TEXT,
    distribution_path TEXT,
    components        TEXT,
    architectures     TEXT,
    enabled           INTEGER NOT NULL DEFAULT 1,
    gpg_check         INTEGER,
    created_at        TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_node_repository_configs_identity
    ON node_repository_configs(certname, repo_id, package_manager);

CREATE INDEX IF NOT EXISTS idx_node_repository_configs_certname
    ON node_repository_configs(certname);

-- -----------------------------------------------------------------------------
-- fleet_repository_configs
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS fleet_repository_configs (
    id                  TEXT PRIMARY KEY,
    os_family           TEXT NOT NULL,
    distribution        TEXT NOT NULL,
    os_version_pattern  TEXT NOT NULL,
    package_manager     TEXT NOT NULL,
    repo_id             TEXT NOT NULL,
    repo_name           TEXT,
    repo_type           TEXT NOT NULL,
    base_url            TEXT,
    mirror_list_url     TEXT,
    distribution_path   TEXT,
    components          TEXT,
    architectures       TEXT,
    enabled             INTEGER NOT NULL DEFAULT 1,
    last_checked_at     TEXT,
    last_check_status   TEXT,
    last_check_error    TEXT,
    reporting_nodes     INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_fleet_repository_configs_identity
    ON fleet_repository_configs(os_family, distribution, os_version_pattern, package_manager, repo_id);

-- -----------------------------------------------------------------------------
-- update_jobs / targets / results
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS update_jobs (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    operation_type TEXT NOT NULL,
    package_names_json TEXT NOT NULL,
    target_group_id TEXT,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    scheduled_for TEXT,
    maintenance_window_start TEXT,
    maintenance_window_end TEXT,
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    approval_notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_update_jobs_status_created
    ON update_jobs(status, created_at DESC);

CREATE TABLE IF NOT EXISTS update_job_targets (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    certname TEXT NOT NULL,
    status TEXT NOT NULL,
    dispatched_at TEXT,
    completed_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(job_id) REFERENCES update_jobs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_update_job_targets_job
    ON update_job_targets(job_id);

CREATE INDEX IF NOT EXISTS idx_update_job_targets_certname_status
    ON update_job_targets(certname, status);

CREATE TABLE IF NOT EXISTS update_job_results (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    certname TEXT NOT NULL,
    status TEXT NOT NULL,
    summary TEXT,
    output TEXT,
    started_at TEXT,
    finished_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(job_id) REFERENCES update_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY(target_id) REFERENCES update_job_targets(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_update_job_results_job
    ON update_job_results(job_id, created_at DESC);

-- -----------------------------------------------------------------------------
-- group_update_schedules
--
-- Moved into the inventory DB so that InventoryRepository (which owns the
-- CRUD for it) can remain single-pool. The FK to node_groups(id) that existed
-- in the legacy schema is dropped here because `node_groups` lives in the
-- main application DB and SQLite cannot enforce cross-database foreign keys.
-- Orphans (schedules whose group was deleted) are tolerated by the scheduler
-- and can be pruned by operators by dropping the stale schedule row.
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS group_update_schedules (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    schedule_type TEXT NOT NULL,
    cron_expression TEXT,
    scheduled_for TEXT,
    operation_type TEXT NOT NULL,
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
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_group_update_schedules_group
    ON group_update_schedules(group_id);

CREATE INDEX IF NOT EXISTS idx_group_update_schedules_due
    ON group_update_schedules(enabled, next_run_at);
