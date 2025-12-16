-- Initial database schema for OpenVox WebUI

-- Users table
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Node groups table
CREATE TABLE IF NOT EXISTS node_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    parent_id TEXT REFERENCES node_groups(id) ON DELETE SET NULL,
    environment TEXT,
    rule_match_type TEXT NOT NULL DEFAULT 'all',
    classes TEXT NOT NULL DEFAULT '[]',
    parameters TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Classification rules table
CREATE TABLE IF NOT EXISTS classification_rules (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES node_groups(id) ON DELETE CASCADE,
    fact_path TEXT NOT NULL,
    operator TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Pinned nodes table
CREATE TABLE IF NOT EXISTS pinned_nodes (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES node_groups(id) ON DELETE CASCADE,
    certname TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(group_id, certname)
);

-- Fact templates table
CREATE TABLE IF NOT EXISTS fact_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    facts TEXT NOT NULL DEFAULT '[]',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- API keys table
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    last_used_at TIMESTAMP,
    expires_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Audit log table
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT,
    ip_address TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_node_groups_parent ON node_groups(parent_id);
CREATE INDEX IF NOT EXISTS idx_classification_rules_group ON classification_rules(group_id);
CREATE INDEX IF NOT EXISTS idx_pinned_nodes_group ON pinned_nodes(group_id);
CREATE INDEX IF NOT EXISTS idx_pinned_nodes_certname ON pinned_nodes(certname);
CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_user ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at);

-- Insert default admin user (password: admin - CHANGE IN PRODUCTION!)
INSERT OR IGNORE INTO users (id, username, email, password_hash, role)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'admin',
    'admin@localhost',
    '$argon2id$v=19$m=19456,t=2,p=1$YWRtaW4$admin_hash_placeholder',
    'admin'
);

-- Insert default "All Nodes" group
INSERT OR IGNORE INTO node_groups (id, name, description, rule_match_type)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'All Nodes',
    'Root group for all nodes',
    'all'
);
