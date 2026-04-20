-- Reporting and Analytics Schema
-- Custom report definitions, scheduled reports, and execution history

-- Report definitions (saved reports with query configurations)
CREATE TABLE IF NOT EXISTS saved_reports (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    report_type TEXT NOT NULL, -- 'compliance', 'node_health', 'change_tracking', 'drift_detection', 'custom'
    query_config TEXT NOT NULL, -- JSON configuration for the report query
    created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_public BOOLEAN NOT NULL DEFAULT FALSE, -- Whether other users can view this report
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Scheduled reports configuration
CREATE TABLE IF NOT EXISTS report_schedules (
    id TEXT PRIMARY KEY,
    report_id TEXT NOT NULL REFERENCES saved_reports(id) ON DELETE CASCADE,
    schedule_cron TEXT NOT NULL, -- Cron expression (e.g., "0 8 * * *" for daily at 8am)
    timezone TEXT NOT NULL DEFAULT 'UTC',
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    output_format TEXT NOT NULL DEFAULT 'json', -- 'json', 'csv', 'pdf'
    email_recipients TEXT, -- JSON array of email addresses
    last_run_at TIMESTAMP,
    next_run_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Report execution history
CREATE TABLE IF NOT EXISTS report_executions (
    id TEXT PRIMARY KEY,
    report_id TEXT NOT NULL REFERENCES saved_reports(id) ON DELETE CASCADE,
    schedule_id TEXT REFERENCES report_schedules(id) ON DELETE SET NULL,
    executed_by TEXT REFERENCES users(id) ON DELETE SET NULL, -- NULL if scheduled execution
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending', 'running', 'completed', 'failed'
    started_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP,
    row_count INTEGER,
    output_format TEXT NOT NULL,
    output_data TEXT, -- JSON result data (for smaller reports)
    output_file_path TEXT, -- File path for larger exports
    error_message TEXT,
    execution_time_ms INTEGER
);

-- Compliance baselines (for compliance reports)
CREATE TABLE IF NOT EXISTS compliance_baselines (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    rules TEXT NOT NULL, -- JSON array of compliance rules
    severity_level TEXT NOT NULL DEFAULT 'medium', -- 'low', 'medium', 'high', 'critical'
    created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Drift detection baselines (for drift reports)
CREATE TABLE IF NOT EXISTS drift_baselines (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    node_group_id TEXT REFERENCES node_groups(id) ON DELETE SET NULL,
    baseline_facts TEXT NOT NULL, -- JSON object of expected fact values
    tolerance_config TEXT, -- JSON config for acceptable drift tolerances
    created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Report templates (predefined report configurations)
CREATE TABLE IF NOT EXISTS report_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    report_type TEXT NOT NULL,
    query_config TEXT NOT NULL, -- JSON configuration template
    is_system BOOLEAN NOT NULL DEFAULT FALSE, -- System templates cannot be deleted
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_saved_reports_type ON saved_reports(report_type);
CREATE INDEX IF NOT EXISTS idx_saved_reports_created_by ON saved_reports(created_by);
CREATE INDEX IF NOT EXISTS idx_report_schedules_enabled ON report_schedules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_report_schedules_next_run ON report_schedules(next_run_at);
CREATE INDEX IF NOT EXISTS idx_report_executions_report ON report_executions(report_id);
CREATE INDEX IF NOT EXISTS idx_report_executions_status ON report_executions(status);
CREATE INDEX IF NOT EXISTS idx_report_executions_started ON report_executions(started_at);
CREATE INDEX IF NOT EXISTS idx_compliance_baselines_severity ON compliance_baselines(severity_level);
CREATE INDEX IF NOT EXISTS idx_drift_baselines_group ON drift_baselines(node_group_id);

-- Insert default system report templates
INSERT OR IGNORE INTO report_templates (id, name, description, report_type, query_config, is_system) VALUES
(
    '00000000-0000-0000-0000-000000000001',
    'Node Health Summary',
    'Overview of node health status across the infrastructure',
    'node_health',
    '{"metrics":["node_count","failed_count","changed_count","unchanged_count"],"group_by":"environment","time_range":"24h"}',
    TRUE
),
(
    '00000000-0000-0000-0000-000000000002',
    'Compliance Summary',
    'Compliance status summary for all nodes',
    'compliance',
    '{"include_details":true,"severity_filter":["medium","high","critical"],"group_by":"node_group"}',
    TRUE
),
(
    '00000000-0000-0000-0000-000000000003',
    'Recent Changes',
    'Report of all changes in the last 24 hours',
    'change_tracking',
    '{"time_range":"24h","include_resources":true,"status_filter":["changed"]}',
    TRUE
),
(
    '00000000-0000-0000-0000-000000000004',
    'Configuration Drift',
    'Detect configuration drift from defined baselines',
    'drift_detection',
    '{"compare_mode":"baseline","include_all_facts":false,"ignore_volatile_facts":true}',
    TRUE
),
(
    '00000000-0000-0000-0000-000000000005',
    'Failed Runs Report',
    'Report of all failed Puppet runs',
    'node_health',
    '{"status_filter":["failed"],"time_range":"7d","include_error_details":true}',
    TRUE
);
