-- Multi-tenancy & advanced security foundations
-- Adds organizations, tenant-scoped data columns, API key role scoping, and improved audit metadata.

-- Organizations / tenants
CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Default organization (used to backfill existing rows)
INSERT OR IGNORE INTO organizations (id, name, slug)
VALUES ('00000000-0000-0000-0000-000000000010', 'Default', 'default');

-- Tenant columns (SQLite does not support adding FK constraints via ALTER TABLE)
ALTER TABLE users ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE node_groups ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE fact_templates ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE api_keys ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE audit_log ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';

-- Reporting/analytics tenant columns
ALTER TABLE saved_reports ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE report_schedules ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE report_executions ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE compliance_baselines ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';
ALTER TABLE drift_baselines ADD COLUMN organization_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000010';

-- API key scoped roles
CREATE TABLE IF NOT EXISTS api_key_roles (
    id TEXT PRIMARY KEY,
    api_key_id TEXT NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(api_key_id, role_id)
);

CREATE INDEX IF NOT EXISTS idx_api_key_roles_key ON api_key_roles(api_key_id);
CREATE INDEX IF NOT EXISTS idx_api_key_roles_role ON api_key_roles(role_id);

-- Helpful tenant indexes
CREATE INDEX IF NOT EXISTS idx_users_org ON users(organization_id);
CREATE INDEX IF NOT EXISTS idx_node_groups_org ON node_groups(organization_id);
CREATE INDEX IF NOT EXISTS idx_fact_templates_org ON fact_templates(organization_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_org ON api_keys(organization_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_org ON audit_log(organization_id);
CREATE INDEX IF NOT EXISTS idx_saved_reports_org ON saved_reports(organization_id);
CREATE INDEX IF NOT EXISTS idx_report_schedules_org ON report_schedules(organization_id);
CREATE INDEX IF NOT EXISTS idx_report_executions_org ON report_executions(organization_id);
CREATE INDEX IF NOT EXISTS idx_compliance_baselines_org ON compliance_baselines(organization_id);
CREATE INDEX IF NOT EXISTS idx_drift_baselines_org ON drift_baselines(organization_id);
