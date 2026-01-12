-- Node removal tracking for nodes with revoked or missing certificates
-- Nodes are marked as "pending removal" and automatically removed after a configurable period (default 10 days)

CREATE TABLE IF NOT EXISTS pending_node_removals (
    id TEXT PRIMARY KEY NOT NULL,
    certname TEXT NOT NULL UNIQUE,
    -- Reason for pending removal
    removal_reason TEXT NOT NULL CHECK (removal_reason IN ('revoked_certificate', 'no_certificate', 'manual')),
    -- When the node was first marked for removal
    marked_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- When the node will be automatically removed (marked_at + retention period)
    scheduled_removal_at TEXT NOT NULL,
    -- Whether the removal has been executed
    removed_at TEXT,
    -- Additional notes or context
    notes TEXT,
    -- Who initiated the marking (null for automatic)
    marked_by TEXT,
    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for efficient queries on pending removals
CREATE INDEX IF NOT EXISTS idx_pending_node_removals_certname ON pending_node_removals(certname);
CREATE INDEX IF NOT EXISTS idx_pending_node_removals_scheduled ON pending_node_removals(scheduled_removal_at) WHERE removed_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_pending_node_removals_reason ON pending_node_removals(removal_reason);

-- Audit log for node removal events
CREATE TABLE IF NOT EXISTS node_removal_audit (
    id TEXT PRIMARY KEY NOT NULL,
    certname TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('marked', 'unmarked', 'removed', 'extended')),
    reason TEXT,
    performed_by TEXT,
    details TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_node_removal_audit_certname ON node_removal_audit(certname);
CREATE INDEX IF NOT EXISTS idx_node_removal_audit_action ON node_removal_audit(action);
CREATE INDEX IF NOT EXISTS idx_node_removal_audit_created ON node_removal_audit(created_at);
