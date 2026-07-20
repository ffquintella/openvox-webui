-- no-transaction
-- Relax the alert_rules.rule_type CHECK constraint to allow the 'update_job'
-- rule type (alerting on update jobs that fail or exceed their max runtime).
-- SQLite cannot ALTER a CHECK constraint in place, so the table is rebuilt.
-- 'vulnerability' is also added to match the AlertRuleType enum, which already
-- supported it but was rejected by the original constraint.
--
-- This migration runs outside a transaction so foreign keys can be disabled
-- during the rebuild; otherwise DROP TABLE would cascade-delete alerts,
-- alert_rule_channels, and alert_silences that reference alert_rules.

PRAGMA foreign_keys=OFF;

CREATE TABLE alert_rules_new (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    rule_type TEXT NOT NULL CHECK (rule_type IN ('node_status', 'compliance', 'drift', 'report_failure', 'vulnerability', 'update_job', 'custom')),
    conditions TEXT NOT NULL,
    condition_operator TEXT NOT NULL DEFAULT 'all' CHECK (condition_operator IN ('all', 'any')),
    severity TEXT NOT NULL DEFAULT 'warning' CHECK (severity IN ('info', 'warning', 'critical')),
    cooldown_minutes INTEGER NOT NULL DEFAULT 60,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

INSERT INTO alert_rules_new (
    id, name, description, rule_type, conditions, condition_operator,
    severity, cooldown_minutes, is_enabled, created_by, created_at, updated_at
)
SELECT
    id, name, description, rule_type, conditions, condition_operator,
    severity, cooldown_minutes, is_enabled, created_by, created_at, updated_at
FROM alert_rules;

DROP TABLE alert_rules;
ALTER TABLE alert_rules_new RENAME TO alert_rules;

CREATE INDEX IF NOT EXISTS idx_alert_rules_type ON alert_rules(rule_type);
CREATE INDEX IF NOT EXISTS idx_alert_rules_enabled ON alert_rules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_alert_rules_severity ON alert_rules(severity);

PRAGMA foreign_keys=ON;
