-- Pre-aggregated per-day report counts powering the Dashboard's
-- "Weekly Activity Trend" chart. A background scheduler refreshes the rows
-- hourly so the chart no longer has to fetch every report on each load.
CREATE TABLE IF NOT EXISTS report_daily_summary (
    date       TEXT PRIMARY KEY,           -- UTC day, YYYY-MM-DD
    changed    INTEGER NOT NULL DEFAULT 0,
    unchanged  INTEGER NOT NULL DEFAULT 0,
    failed     INTEGER NOT NULL DEFAULT 0,
    noop       INTEGER NOT NULL DEFAULT 0,
    total      INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
