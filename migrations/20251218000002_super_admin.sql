-- Cross-tenant super admin role (system)
-- Provides a distinct role from per-tenant admin, intended for multi-tenant operations.

INSERT OR IGNORE INTO roles (id, name, display_name, description, is_system)
VALUES (
    '00000000-0000-0000-0000-000000000006',
    'super_admin',
    'Super Administrator',
    'Cross-tenant administrator with full system access',
    TRUE
);

-- Super admin permissions (full admin on all resources)
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0006-0001-000000000001', '00000000-0000-0000-0000-000000000006', 'nodes', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000002', '00000000-0000-0000-0000-000000000006', 'groups', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000003', '00000000-0000-0000-0000-000000000006', 'reports', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000004', '00000000-0000-0000-0000-000000000006', 'facts', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000005', '00000000-0000-0000-0000-000000000006', 'users', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000006', '00000000-0000-0000-0000-000000000006', 'roles', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000007', '00000000-0000-0000-0000-000000000006', 'settings', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000008', '00000000-0000-0000-0000-000000000006', 'audit_logs', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000009', '00000000-0000-0000-0000-000000000006', 'facter_templates', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000010', '00000000-0000-0000-0000-000000000006', 'api_keys', 'admin', 'all');

-- Assign super_admin role to default admin user for bootstrapping
INSERT OR IGNORE INTO user_roles (id, user_id, role_id)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000006'
);

