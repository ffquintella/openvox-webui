-- M29: authentication pipeline (challenges, factors, MFA policies, SSO state).

-- Methods (amr) and level (acr) reached by each session. Existing sessions are
-- password sessions at level 1.
ALTER TABLE auth_sessions ADD COLUMN amr TEXT NOT NULL DEFAULT '["pwd"]';
ALTER TABLE auth_sessions ADD COLUMN acr INTEGER NOT NULL DEFAULT 1;

-- Enrolled factors. `data_encrypted` holds the seed or credential, sealed by
-- the host; it is only decrypted while the owner's challenge is verified.
CREATE TABLE IF NOT EXISTS user_factors (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    data_encrypted TEXT NOT NULL,
    verified INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_user_factors_user ON user_factors(user_id);

-- Recovery codes are single use and stored only as argon2 hashes.
CREATE TABLE IF NOT EXISTS user_recovery_codes (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    code_hash TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_user_recovery_codes_user ON user_recovery_codes(user_id);

-- A challenge sits between a validated primary identity and a session. It is
-- opaque, single use, short lived and bound to the client.
CREATE TABLE IF NOT EXISTS auth_challenges (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    purpose TEXT NOT NULL,            -- 'login' | 'step_up'
    provider TEXT NOT NULL,
    amr TEXT NOT NULL,                -- methods already proven (JSON array)
    required_acr INTEGER NOT NULL,
    session_id TEXT,                  -- step-up: the session being raised
    client_hash TEXT NOT NULL,        -- hash of IP + User-Agent
    active_factor TEXT,
    flow_state TEXT,                  -- sealed per-factor state (WebAuthn)
    attempts INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_auth_challenges_expires ON auth_challenges(expires_at);

-- MFA policies: when to require which level.
CREATE TABLE IF NOT EXISTS auth_policies (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 100,
    conditions TEXT NOT NULL DEFAULT '{}',
    required_acr INTEGER NOT NULL DEFAULT 2,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Browser-bound, single-use state: SSO (state, nonce, PKCE) and WebAuthn
-- enrollment ceremonies.
CREATE TABLE IF NOT EXISTS auth_flow_state (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,               -- 'oidc' | 'webauthn_enroll'
    provider TEXT NOT NULL,
    user_id TEXT,
    browser_hash TEXT,
    payload_encrypted TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_auth_flow_state_expires ON auth_flow_state(expires_at);

-- External identities, linked by (provider, subject) and never by e-mail.
CREATE TABLE IF NOT EXISTS user_identities (
    provider_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_login_at TEXT,
    PRIMARY KEY (provider_id, external_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_user_identities_user ON user_identities(user_id);
