-- Repository configuration tables for repo-based version checking
-- Nodes report their configured repos; server deduplicates and periodically checks them.

-- Per-node repository configurations reported by the facter collector
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
    repo_type         TEXT NOT NULL,      -- 'yum', 'apt', 'zypper'
    base_url          TEXT,
    mirror_list_url   TEXT,
    distribution_path TEXT,               -- APT distribution (e.g. 'jammy')
    components        TEXT,               -- APT components (e.g. 'main restricted')
    architectures     TEXT,               -- target architectures
    enabled           INTEGER NOT NULL DEFAULT 1,
    gpg_check         INTEGER,
    created_at        TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_node_repository_configs_identity
    ON node_repository_configs(certname, repo_id, package_manager);

CREATE INDEX IF NOT EXISTS idx_node_repository_configs_certname
    ON node_repository_configs(certname);

-- Deduplicated fleet-wide repository configurations
CREATE TABLE IF NOT EXISTS fleet_repository_configs (
    id                  TEXT PRIMARY KEY,
    os_family           TEXT NOT NULL,
    distribution        TEXT NOT NULL,
    os_version_pattern  TEXT NOT NULL,    -- major version (e.g. '9' for RHEL 9.x)
    package_manager     TEXT NOT NULL,
    repo_id             TEXT NOT NULL,
    repo_name           TEXT,
    repo_type           TEXT NOT NULL,    -- 'yum', 'apt', 'zypper'
    base_url            TEXT,
    mirror_list_url     TEXT,
    distribution_path   TEXT,
    components          TEXT,
    architectures       TEXT,
    enabled             INTEGER NOT NULL DEFAULT 1,
    last_checked_at     TEXT,
    last_check_status   TEXT,            -- 'success', 'error', 'pending'
    last_check_error    TEXT,
    reporting_nodes     INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_fleet_repository_configs_identity
    ON fleet_repository_configs(os_family, distribution, os_version_pattern, package_manager, repo_id);

-- Update the repository_version_catalog unique index to include source_kind,
-- allowing both 'fleet-observed' and 'repo-checked' entries for the same package.
DROP INDEX IF EXISTS idx_repository_version_catalog_identity;

CREATE UNIQUE INDEX idx_repository_version_catalog_identity
    ON repository_version_catalog(
        platform_family, distribution, package_manager,
        software_type, software_name,
        COALESCE(repository_source, ''),
        source_kind
    );
