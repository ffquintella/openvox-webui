-- Classification extensions for plugins (M27): materialized members of
-- dynamic groups, provider state, membership updates held by the churn
-- guard and group template instances.
-- See docs/architecture/PLUGIN_PLATFORM.md sections 13 and 17.8.
-- Group kind, owner and provider live in node_groups.behavior_json.

CREATE TABLE IF NOT EXISTS plugin_group_members (
    group_id TEXT NOT NULL,
    certname TEXT NOT NULL,
    computed_at TEXT NOT NULL,                   -- RFC 3339 (UTC), last time the provider confirmed the member
    PRIMARY KEY (group_id, certname)
);

CREATE INDEX IF NOT EXISTS idx_plugin_group_members_certname ON plugin_group_members(certname);

-- Last refresh outcome per dynamic group.
CREATE TABLE IF NOT EXISTS plugin_membership_state (
    group_id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    last_attempt_at TEXT,
    last_refresh_at TEXT,                        -- last successful, applied refresh
    last_status TEXT NOT NULL DEFAULT 'never',   -- never | ok | failed | held
    last_error TEXT,
    stale_alerted INTEGER NOT NULL DEFAULT 0
);

-- An update the churn guard did not apply, waiting for an administrator.
CREATE TABLE IF NOT EXISTS plugin_membership_held (
    group_id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    added_json TEXT NOT NULL,
    removed_json TEXT NOT NULL,
    current_count INTEGER NOT NULL,
    proposed_count INTEGER NOT NULL,
    churn_pct REAL NOT NULL,
    limit_pct INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

-- A group template put to use by an administrator; generated groups are
-- node_groups rows whose behavior points back at the instance.
CREATE TABLE IF NOT EXISTS plugin_group_template_instances (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    template_id TEXT NOT NULL,
    name TEXT NOT NULL UNIQUE,
    parent_id TEXT,
    classes_json TEXT NOT NULL DEFAULT '{}',
    variables_json TEXT NOT NULL DEFAULT '{}',
    approved_by TEXT,
    approved_at TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    last_status TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_plugin_group_template_instances_plugin
    ON plugin_group_template_instances(plugin_id);
