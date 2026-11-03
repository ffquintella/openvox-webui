-- The super_admin role bypasses permission checks in the API, but the web UI
-- reads the stored permission rows to decide which pages to show. Grant the
-- plugin resources explicitly so the Plugins page is reachable.
INSERT OR IGNORE INTO permissions (id, role_id, resource, action, scope_type) VALUES
    ('00000000-0000-0006-0001-000000000011', '00000000-0000-0000-0000-000000000006', 'plugins', 'admin', 'all'),
    ('00000000-0000-0006-0001-000000000012', '00000000-0000-0000-0000-000000000006', 'plugin_trust', 'admin', 'all');
