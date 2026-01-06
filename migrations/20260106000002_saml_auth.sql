-- SAML 2.0 Authentication Support
-- Adds fields to track authentication provider and SAML-specific user data

-- Add authentication provider tracking to users
-- Supports: 'local' (password), 'saml', 'both'
ALTER TABLE users ADD COLUMN auth_provider TEXT NOT NULL DEFAULT 'local';

-- Add external identifier for SAML users (stores SAML NameID or similar)
ALTER TABLE users ADD COLUMN external_id TEXT;

-- Add IdP entity ID for SAML users (identifies which IdP authenticated them)
ALTER TABLE users ADD COLUMN idp_entity_id TEXT;

-- Track last SAML authentication timestamp
ALTER TABLE users ADD COLUMN last_saml_auth_at TIMESTAMP;

-- Create index for external ID lookups (common SAML flow)
CREATE INDEX IF NOT EXISTS idx_users_external_id ON users(external_id);

-- Create index for IdP entity lookups
CREATE INDEX IF NOT EXISTS idx_users_idp_entity ON users(idp_entity_id);

-- Table for storing SAML authentication request state
-- Used to verify SAML responses and prevent replay attacks
CREATE TABLE IF NOT EXISTS saml_auth_requests (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,  -- SAML AuthnRequest ID
    relay_state TEXT,                  -- Original URL user was trying to access
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP NOT NULL
);

-- Index for cleanup of expired requests
CREATE INDEX IF NOT EXISTS idx_saml_auth_requests_expires ON saml_auth_requests(expires_at);
