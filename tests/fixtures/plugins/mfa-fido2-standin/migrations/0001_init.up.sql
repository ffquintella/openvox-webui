-- Stand-in subset of the mfa-fido2 SPEC section 10.3 schema: the credential
-- index (listing, rename, suspension, collision check) and the replay ledger.
-- credential_id is base64url text here (the SPEC uses a BLOB).
CREATE TABLE credentials_index (
  credential_ref      TEXT PRIMARY KEY,
  credential_id       TEXT NOT NULL UNIQUE,
  user_id             TEXT NOT NULL,
  username            TEXT NOT NULL,
  host_record_id      TEXT,
  rp_id               TEXT NOT NULL,
  aaguid              TEXT NOT NULL,
  alg                 INTEGER NOT NULL,
  attestation_format  TEXT NOT NULL,
  attestation_trust   TEXT NOT NULL,
  transports          TEXT NOT NULL DEFAULT '[]',
  backup_eligible     INTEGER NOT NULL,
  backup_state        INTEGER NOT NULL,
  sign_count_seen     INTEGER NOT NULL DEFAULT 0,
  nickname            TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'active'
                      CHECK (status IN ('active','suspended','clone_suspected','removed')),
  status_reason       TEXT,
  created_at          INTEGER NOT NULL,
  last_used_at        INTEGER,
  last_used_purpose   TEXT
);
CREATE INDEX credentials_index_user ON credentials_index(user_id, status);

CREATE TABLE used_challenges (
  challenge_sha256 TEXT PRIMARY KEY,
  kind             TEXT NOT NULL CHECK (kind IN ('create','get')),
  user_id          TEXT NOT NULL,
  used_at          INTEGER NOT NULL,
  expires_at       INTEGER NOT NULL
);
CREATE INDEX used_challenges_expiry ON used_challenges(expires_at);
