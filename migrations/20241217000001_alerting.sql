-- Alerting and Notifications Schema
-- Phase 8.2: Alert rule configuration, notifications, and alert history

-- Notification channels (webhook, email, slack, teams)
CREATE TABLE IF NOT EXISTS notification_channels (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL CHECK (channel_type IN ('webhook', 'email', 'slack', 'teams')),
    config TEXT NOT NULL,  -- JSON configuration specific to channel type
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_by TEXT,  -- User ID who created this channel
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

-- Alert rules define conditions that trigger alerts
CREATE TABLE IF NOT EXISTS alert_rules (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    -- Rule configuration
    rule_type TEXT NOT NULL CHECK (rule_type IN ('node_status', 'compliance', 'drift', 'report_failure', 'custom')),
    conditions TEXT NOT NULL,  -- JSON array of conditions to evaluate
    condition_operator TEXT NOT NULL DEFAULT 'all' CHECK (condition_operator IN ('all', 'any')),

    -- Severity and behavior
    severity TEXT NOT NULL DEFAULT 'warning' CHECK (severity IN ('info', 'warning', 'critical')),
    cooldown_minutes INTEGER NOT NULL DEFAULT 60,  -- Minimum time between re-alerts

    -- Status
    is_enabled INTEGER NOT NULL DEFAULT 1,

    -- Ownership
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

-- Link alert rules to notification channels
CREATE TABLE IF NOT EXISTS alert_rule_channels (
    id TEXT PRIMARY KEY NOT NULL,
    rule_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (rule_id) REFERENCES alert_rules(id) ON DELETE CASCADE,
    FOREIGN KEY (channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE,
    UNIQUE(rule_id, channel_id)
);

-- Alert instances (triggered alerts)
CREATE TABLE IF NOT EXISTS alerts (
    id TEXT PRIMARY KEY NOT NULL,
    rule_id TEXT NOT NULL,

    -- Alert details
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'critical')),

    -- Context
    context TEXT,  -- JSON with additional context (node, report, etc.)

    -- Status tracking
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'acknowledged', 'resolved', 'silenced')),
    acknowledged_by TEXT,
    acknowledged_at TEXT,
    resolved_at TEXT,

    -- Timestamps
    triggered_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_notified_at TEXT,

    FOREIGN KEY (rule_id) REFERENCES alert_rules(id) ON DELETE CASCADE,
    FOREIGN KEY (acknowledged_by) REFERENCES users(id) ON DELETE SET NULL
);

-- Notification history (delivery tracking)
CREATE TABLE IF NOT EXISTS notification_history (
    id TEXT PRIMARY KEY NOT NULL,
    alert_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,

    -- Delivery status
    status TEXT NOT NULL CHECK (status IN ('pending', 'sent', 'failed', 'retrying')),
    attempt_count INTEGER NOT NULL DEFAULT 1,

    -- Response details
    response_code INTEGER,
    response_body TEXT,
    error_message TEXT,

    -- Timestamps
    sent_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (alert_id) REFERENCES alerts(id) ON DELETE CASCADE,
    FOREIGN KEY (channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE
);

-- Alert silences (temporarily suppress alerts)
CREATE TABLE IF NOT EXISTS alert_silences (
    id TEXT PRIMARY KEY NOT NULL,

    -- Silence scope
    rule_id TEXT,  -- NULL means all rules
    matchers TEXT,  -- JSON for label matchers

    -- Duration
    starts_at TEXT NOT NULL,
    ends_at TEXT NOT NULL,

    -- Metadata
    reason TEXT NOT NULL,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (rule_id) REFERENCES alert_rules(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_notification_channels_type ON notification_channels(channel_type);
CREATE INDEX IF NOT EXISTS idx_notification_channels_enabled ON notification_channels(is_enabled);

CREATE INDEX IF NOT EXISTS idx_alert_rules_type ON alert_rules(rule_type);
CREATE INDEX IF NOT EXISTS idx_alert_rules_enabled ON alert_rules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_alert_rules_severity ON alert_rules(severity);

CREATE INDEX IF NOT EXISTS idx_alerts_rule_id ON alerts(rule_id);
CREATE INDEX IF NOT EXISTS idx_alerts_status ON alerts(status);
CREATE INDEX IF NOT EXISTS idx_alerts_severity ON alerts(severity);
CREATE INDEX IF NOT EXISTS idx_alerts_triggered_at ON alerts(triggered_at);

CREATE INDEX IF NOT EXISTS idx_notification_history_alert_id ON notification_history(alert_id);
CREATE INDEX IF NOT EXISTS idx_notification_history_channel_id ON notification_history(channel_id);
CREATE INDEX IF NOT EXISTS idx_notification_history_status ON notification_history(status);

CREATE INDEX IF NOT EXISTS idx_alert_silences_rule_id ON alert_silences(rule_id);
CREATE INDEX IF NOT EXISTS idx_alert_silences_ends_at ON alert_silences(ends_at);
