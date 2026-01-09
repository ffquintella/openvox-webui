-- Centralized GitHub Personal Access Token (PAT) management for Code Deploy
-- Similar to code_ssh_keys, but for PAT tokens with expiration tracking

-- Create PAT tokens table
CREATE TABLE IF NOT EXISTS code_pat_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    token_encrypted TEXT NOT NULL,
    expires_at TEXT,  -- ISO 8601 datetime when token expires (optional)
    last_validated_at TEXT,  -- Last time token was successfully used
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Add pat_token_id to repositories (replaces github_pat_encrypted)
ALTER TABLE code_repositories ADD COLUMN pat_token_id TEXT REFERENCES code_pat_tokens(id) ON DELETE SET NULL;

-- Index for efficient lookups
CREATE INDEX IF NOT EXISTS idx_code_pat_tokens_name ON code_pat_tokens(name);
CREATE INDEX IF NOT EXISTS idx_code_pat_tokens_expires ON code_pat_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_code_repositories_pat_token ON code_repositories(pat_token_id);
