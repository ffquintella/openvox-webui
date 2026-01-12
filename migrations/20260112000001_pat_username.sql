-- Add username field to PAT tokens for .netrc authentication
-- The username is used along with the PAT token for HTTPS authentication

ALTER TABLE code_pat_tokens ADD COLUMN username TEXT;

-- Create index for username lookups
CREATE INDEX IF NOT EXISTS idx_code_pat_tokens_username ON code_pat_tokens(username);
