-- Hour-precision pre-aggregated report counts. Drives the Analytics
-- "Report Metrics Over Time" chart (24h/7d/30d) and the "Activity Heatmap"
-- without each page load having to scan the last 30 days of reports out of
-- PuppetDB.
CREATE TABLE IF NOT EXISTS report_hourly_summary (
    hour       TEXT PRIMARY KEY,           -- UTC bucket start, e.g. "2026-05-21T13:00:00Z"
    changed    INTEGER NOT NULL DEFAULT 0,
    unchanged  INTEGER NOT NULL DEFAULT 0,
    failed     INTEGER NOT NULL DEFAULT 0,
    noop       INTEGER NOT NULL DEFAULT 0,
    total      INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
