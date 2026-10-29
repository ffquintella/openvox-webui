-- Plugin reports (M25, T90): annotations written by plugins on Puppet run
-- reports, plugin datasets and the cursor of the report.received watcher.
-- Plugin report types are stored in the existing saved_reports table as
-- 'plugin:<plugin-id>:<type-id>'. See docs/architecture/PLUGIN_PLATFORM.md
-- sections 12.3, 15 and 17.7.

-- PuppetDB reports are immutable: plugins annotate them here instead.
CREATE TABLE IF NOT EXISTS report_annotations (
    report_hash TEXT NOT NULL,
    plugin_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    severity TEXT,                               -- info | low | medium | high | critical
    created_at TEXT NOT NULL,
    PRIMARY KEY (report_hash, plugin_id, key)
);

CREATE INDEX IF NOT EXISTS idx_report_annotations_plugin ON report_annotations(plugin_id, key);

-- Datasets a plugin provides; the schema comes from its manifest.
CREATE TABLE IF NOT EXISTS plugin_datasets (
    plugin_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    schema_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, dataset_id)
);

CREATE TABLE IF NOT EXISTS plugin_dataset_rows (
    plugin_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    row_key TEXT NOT NULL,
    row_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, dataset_id, row_key)
);

-- receive_time of the newest PuppetDB report published as report.received.
CREATE TABLE IF NOT EXISTS plugin_report_watch (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_receive_time TEXT NOT NULL
);
