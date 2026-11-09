-- OQ-6 (T120): an MFA policy may require a phishing-resistant factor (a
-- WebAuthn security key or passkey), which TOTP and recovery codes cannot
-- satisfy. Additive; 0 keeps the previous behaviour.
ALTER TABLE auth_policies ADD COLUMN require_phishing_resistant INTEGER NOT NULL DEFAULT 0;
