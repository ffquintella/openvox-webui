-- Server Backup Feature: Database backup and restore management
-- Migration: 20260113000001_server_backup.sql

-- Backup metadata and history
CREATE TABLE IF NOT EXISTS server_backups (
    id TEXT PRIMARY KEY NOT NULL,
    filename TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    checksum TEXT NOT NULL,              -- SHA-256 hash of encrypted file
    uncompressed_size INTEGER,           -- Original size before compression
    is_encrypted INTEGER NOT NULL DEFAULT 1,
    encryption_salt TEXT,                -- Salt for key derivation (base64)
    encryption_nonce TEXT,               -- Nonce for ChaCha20-Poly1305 (base64)
    trigger_type TEXT NOT NULL DEFAULT 'manual',  -- manual, scheduled
    status TEXT NOT NULL DEFAULT 'pending',        -- pending, in_progress, completed, failed, deleted
    error_message TEXT,
    started_at TEXT,
    completed_at TEXT,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    includes_database INTEGER NOT NULL DEFAULT 1,
    includes_config INTEGER NOT NULL DEFAULT 1,
    database_version TEXT,               -- App version at backup time
    notes TEXT,                          -- User-provided description
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Backup schedule configuration (stored in DB to allow UI changes)
CREATE TABLE IF NOT EXISTS backup_schedules (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL DEFAULT 'default',
    is_active INTEGER NOT NULL DEFAULT 1,
    frequency TEXT NOT NULL DEFAULT 'daily',  -- hourly, daily, weekly, custom, disabled
    cron_expression TEXT,                     -- Custom cron when frequency=custom
    time_of_day TEXT DEFAULT '02:00',         -- HH:MM for daily/weekly
    day_of_week INTEGER DEFAULT 0,            -- 0-6 for weekly (0=Sunday)
    retention_count INTEGER NOT NULL DEFAULT 30,
    last_run_at TEXT,
    next_run_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Restore history
CREATE TABLE IF NOT EXISTS backup_restores (
    id TEXT PRIMARY KEY NOT NULL,
    backup_id TEXT NOT NULL REFERENCES server_backups(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, in_progress, completed, failed
    error_message TEXT,
    started_at TEXT,
    completed_at TEXT,
    restored_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_server_backups_status ON server_backups(status);
CREATE INDEX IF NOT EXISTS idx_server_backups_created ON server_backups(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_server_backups_trigger ON server_backups(trigger_type);
CREATE INDEX IF NOT EXISTS idx_backup_restores_backup ON backup_restores(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_schedules_active ON backup_schedules(is_active);

-- Insert default schedule
INSERT OR IGNORE INTO backup_schedules (id, name, frequency, time_of_day, retention_count)
VALUES ('00000000-0000-0000-0000-000000000001', 'default', 'daily', '02:00', 30);
