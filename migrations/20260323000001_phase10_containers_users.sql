-- Container inventory (Docker CE, Docker Enterprise, Podman)
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

-- System user inventory
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

-- Add count columns to snapshots table
ALTER TABLE host_inventory_snapshots ADD COLUMN container_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE host_inventory_snapshots ADD COLUMN user_count INTEGER NOT NULL DEFAULT 0;
