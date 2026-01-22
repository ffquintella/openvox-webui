-- Global Settings
-- Key-value store for application settings

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Insert default SMTP configuration
INSERT OR IGNORE INTO settings (key, value, description) VALUES
    ('smtp.host', '', 'SMTP server hostname'),
    ('smtp.port', '587', 'SMTP server port'),
    ('smtp.username', '', 'SMTP authentication username'),
    ('smtp.password', '', 'SMTP authentication password'),
    ('smtp.from_address', '', 'Default sender email address'),
    ('smtp.use_tls', 'true', 'Use TLS encryption'),
    ('smtp.configured', 'false', 'Whether SMTP is configured');
