-- Add GitHub Personal Access Token (PAT) support to Code Deploy
-- Allows repositories to use PAT authentication as an alternative to SSH keys

-- Add authentication type to repositories
ALTER TABLE code_repositories ADD COLUMN auth_type TEXT NOT NULL DEFAULT 'ssh';
-- auth_type can be: 'ssh', 'pat', or 'none' (for public repos)

-- Add encrypted PAT field
ALTER TABLE code_repositories ADD COLUMN github_pat_encrypted TEXT;

-- Update ssh_key_id to be nullable since PAT repos won't need it
-- (Already nullable in original schema)

-- Add check constraint to ensure auth method is valid
-- SQLite doesn't support CHECK in ALTER TABLE, so we'll validate in application code
