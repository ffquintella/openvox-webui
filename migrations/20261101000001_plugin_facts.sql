-- Plugin facts (M25, T89): values provided by plugins, the administrator's
-- opt-in per fact, provider status, the last classification computed with
-- plugin facts (freeze and audit) and the last export digest per node.
-- See docs/architecture/PLUGIN_PLATFORM.md sections 12.2, 15 and 17.6.

CREATE TABLE IF NOT EXISTS plugin_facts (
    plugin_id TEXT NOT NULL,
    certname TEXT NOT NULL,
    name TEXT NOT NULL,                          -- full name: <namespace>.<name>
    value_json TEXT NOT NULL,
    computed_at TEXT NOT NULL,                   -- RFC 3339 (UTC)
    expires_at TEXT,                             -- computed_at + ttl_s
    stale_notified INTEGER NOT NULL DEFAULT 0,   -- plugin.fact.stale already published
    PRIMARY KEY (plugin_id, certname, name)
);

CREATE INDEX IF NOT EXISTS idx_plugin_facts_certname ON plugin_facts(certname);
CREATE INDEX IF NOT EXISTS idx_plugin_facts_computed ON plugin_facts(computed_at);

-- Administrator opt-in per fact; both uses are off by default.
CREATE TABLE IF NOT EXISTS plugin_fact_settings (
    plugin_id TEXT NOT NULL,
    name TEXT NOT NULL,
    usable_in_rules INTEGER NOT NULL DEFAULT 0,
    exportable INTEGER NOT NULL DEFAULT 0,
    approved_by TEXT,
    approved_at TEXT,
    PRIMARY KEY (plugin_id, name)
);

-- Last write or resolution outcome per provider (shown on the plugin screen).
CREATE TABLE IF NOT EXISTS plugin_fact_providers (
    plugin_id TEXT PRIMARY KEY,
    last_success_at TEXT,
    last_failure_at TEXT,
    last_error TEXT,
    failures INTEGER NOT NULL DEFAULT 0,
    rejected INTEGER NOT NULL DEFAULT 0
);

-- Last classification computed with plugin facts, per node and scope
-- ('all' for the agent endpoints, an organization id otherwise). Used by
-- classification.freeze_on_provider_failure and to audit changes.
CREATE TABLE IF NOT EXISTS plugin_fact_classifications (
    certname TEXT NOT NULL,
    scope TEXT NOT NULL,
    result_json TEXT NOT NULL,
    signature TEXT NOT NULL,
    influenced INTEGER NOT NULL DEFAULT 0,       -- plugin facts changed the result
    facts_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (certname, scope)
);

-- Digest of the plugin facts last exported to each node; a change is audited.
CREATE TABLE IF NOT EXISTS plugin_fact_exports (
    certname TEXT PRIMARY KEY,
    digest TEXT NOT NULL,
    names_json TEXT NOT NULL,
    exported_at TEXT NOT NULL
);
