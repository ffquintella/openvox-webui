-- Plugin updates and rollback (M24): automatic update policy and window per
-- plugin, a version the scheduler must skip (after a failed automatic update
-- or a rollback), and who revoked a stored version and why.
-- See docs/architecture/PLUGIN_PLATFORM.md sections 7.4, 15 and 17.5.

-- off | patch | minor | any
ALTER TABLE plugins ADD COLUMN auto_update TEXT NOT NULL DEFAULT 'off';
-- Five-field cron expression (UTC); an update is applied in a matching minute.
-- NULL = at any time.
ALTER TABLE plugins ADD COLUMN update_window TEXT;
-- Version the automatic updater skips until an administrator acts.
ALTER TABLE plugins ADD COLUMN update_hold_version TEXT;
ALTER TABLE plugins ADD COLUMN update_hold_reason TEXT;

ALTER TABLE plugin_versions ADD COLUMN revoked_reason TEXT;
ALTER TABLE plugin_versions ADD COLUMN revoked_by TEXT;
ALTER TABLE plugin_versions ADD COLUMN revoked_at TEXT;
