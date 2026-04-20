-- RBAC (Role-Based Access Control) schema

-- Roles table
CREATE TABLE IF NOT EXISTS roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    description TEXT,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    parent_id TEXT REFERENCES roles(id) ON DELETE SET NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Permissions table
CREATE TABLE IF NOT EXISTS permissions (
    id TEXT PRIMARY KEY,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    resource TEXT NOT NULL,
    action TEXT NOT NULL,
    scope_type TEXT NOT NULL DEFAULT 'all',
    scope_value TEXT,
    constraint_type TEXT,
    constraint_value TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(role_id, resource, action, scope_type, scope_value)
);

-- User-Role assignments table
CREATE TABLE IF NOT EXISTS user_roles (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, role_id)
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_permissions_role ON permissions(role_id);
CREATE INDEX IF NOT EXISTS idx_permissions_resource ON permissions(resource);
CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id);
CREATE INDEX IF NOT EXISTS idx_user_roles_role ON user_roles(role_id);
CREATE INDEX IF NOT EXISTS idx_roles_parent ON roles(parent_id);

-- Insert system roles
INSERT OR IGNORE INTO roles (id, name, display_name, description, is_system)
VALUES
    ('00000000-0000-0000-0000-000000000001', 'admin', 'Administrator', 'Full system access with all permissions', TRUE),
    ('00000000-0000-0000-0000-000000000002', 'operator', 'Operator', 'Day-to-day operations access', TRUE),
    ('00000000-0000-0000-0000-000000000003', 'viewer', 'Viewer', 'Read-only access to all resources', TRUE),
    ('00000000-0000-0000-0000-000000000004', 'group_admin', 'Group Administrator', 'Full access to assigned node groups', TRUE),
    ('00000000-0000-0000-0000-000000000005', 'auditor', 'Auditor', 'Read access with audit log visibility', TRUE);

-- Insert Admin permissions (full admin on all resources)
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0001-0001-000000000001', '00000000-0000-0000-0000-000000000001', 'nodes', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000002', '00000000-0000-0000-0000-000000000001', 'groups', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000003', '00000000-0000-0000-0000-000000000001', 'reports', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000004', '00000000-0000-0000-0000-000000000001', 'facts', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000005', '00000000-0000-0000-0000-000000000001', 'users', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000006', '00000000-0000-0000-0000-000000000001', 'roles', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000007', '00000000-0000-0000-0000-000000000001', 'settings', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000008', '00000000-0000-0000-0000-000000000001', 'audit_logs', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000009', '00000000-0000-0000-0000-000000000001', 'facter_templates', 'admin', 'all'),
    ('00000000-0000-0001-0001-000000000010', '00000000-0000-0000-0000-000000000001', 'api_keys', 'admin', 'all');

-- Insert Operator permissions
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0002-0001-000000000001', '00000000-0000-0000-0000-000000000002', 'nodes', 'read', 'all'),
    ('00000000-0000-0002-0001-000000000002', '00000000-0000-0000-0000-000000000002', 'nodes', 'classify', 'all'),
    ('00000000-0000-0002-0001-000000000003', '00000000-0000-0000-0000-000000000002', 'groups', 'read', 'all'),
    ('00000000-0000-0002-0001-000000000004', '00000000-0000-0000-0000-000000000002', 'groups', 'create', 'all'),
    ('00000000-0000-0002-0001-000000000005', '00000000-0000-0000-0000-000000000002', 'groups', 'update', 'all'),
    ('00000000-0000-0002-0001-000000000006', '00000000-0000-0000-0000-000000000002', 'reports', 'read', 'all'),
    ('00000000-0000-0002-0001-000000000007', '00000000-0000-0000-0000-000000000002', 'facts', 'read', 'all'),
    ('00000000-0000-0002-0001-000000000008', '00000000-0000-0000-0000-000000000002', 'settings', 'read', 'all');

-- Insert Viewer permissions (read-only)
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0003-0001-000000000001', '00000000-0000-0000-0000-000000000003', 'nodes', 'read', 'all'),
    ('00000000-0000-0003-0001-000000000002', '00000000-0000-0000-0000-000000000003', 'groups', 'read', 'all'),
    ('00000000-0000-0003-0001-000000000003', '00000000-0000-0000-0000-000000000003', 'reports', 'read', 'all'),
    ('00000000-0000-0003-0001-000000000004', '00000000-0000-0000-0000-000000000003', 'facts', 'read', 'all');

-- Insert Group Admin permissions
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0004-0001-000000000001', '00000000-0000-0000-0000-000000000004', 'groups', 'admin', 'specific'),
    ('00000000-0000-0004-0001-000000000002', '00000000-0000-0000-0000-000000000004', 'nodes', 'read', 'all'),
    ('00000000-0000-0004-0001-000000000003', '00000000-0000-0000-0000-000000000004', 'nodes', 'classify', 'all');

-- Insert Auditor permissions
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0005-0001-000000000001', '00000000-0000-0000-0000-000000000005', 'nodes', 'read', 'all'),
    ('00000000-0000-0005-0001-000000000002', '00000000-0000-0000-0000-000000000005', 'groups', 'read', 'all'),
    ('00000000-0000-0005-0001-000000000003', '00000000-0000-0000-0000-000000000005', 'reports', 'read', 'all'),
    ('00000000-0000-0005-0001-000000000004', '00000000-0000-0000-0000-000000000005', 'reports', 'export', 'all'),
    ('00000000-0000-0005-0001-000000000005', '00000000-0000-0000-0000-000000000005', 'facts', 'read', 'all'),
    ('00000000-0000-0005-0001-000000000006', '00000000-0000-0000-0000-000000000005', 'audit_logs', 'read', 'all');

-- Assign admin role to default admin user
INSERT OR IGNORE INTO user_roles (id, user_id, role_id)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001'
);
