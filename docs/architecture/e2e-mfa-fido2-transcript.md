# End-to-end check: the host runs the mfa-fido2 plugin (T121)

This file records one run of the end-to-end check of the host contracts that the mfa-fido2 module
(`openvox-webui-modules/plugins/mfa-fido2`, SPEC.md) depends on. The hand-off note that interprets it
is [MFA_FIDO2_HOST_HANDOFF.md](MFA_FIDO2_HOST_HANDOFF.md). Host design: PLUGIN_PLATFORM.md sections
17.15 to 17.20.

## What was used, stated plainly

- **No physical security key was used.** No YubiKey or other hardware authenticator took part in any
  step. The authenticators are software only: a P-256 software authenticator written for the Rust test
  (keys made at run time, `packed` self-attestation) and Chrome's virtual authenticator driven over the
  DevTools protocol (`WebAuthn.addVirtualAuthenticator`, CTAP2 over USB, attestation `none`). Nothing
  here is a claim about hardware: transports (USB, NFC, BLE), PIN entry, attestation consent prompts,
  CTAP1/U2F keys and vendor attestation chains were not exercised.
- **The plugin is a test-only stand-in, not the mfa-fido2 module.** It lives in
  `tests/fixtures/plugins/mfa-fido2-standin` (Rust built for `wasm32-unknown-unknown`, about 220 KB) and
  runs in the host WASM runtime under the manifest `tests/fixtures/plugins/mfa-fido2-standin/plugin.yaml`,
  which is derived from the module's draft `plugin.yaml`. The WebAuthn checks and the ES256 signature
  verification inside it are real (RustCrypto `p256`, `sha2`), so a wrong signature is refused by
  cryptography, not by a flag.
- **Two drivers.** Transcript A is `cargo test` over the HTTP router of the test app (the same API
  routes and authentication middleware as the server; no rate limiter, no static files). Transcript B is
  the real server binary (`target/debug/openvox-webui`, debug build, fresh SQLite database), the built
  web UI (`frontend/dist`) and headless Google Chrome, so the browser path of the host WebAuthn client
  (H-5) runs for real: `navigator.credentials.create/get`, the Level 3 JSON encoding, the step-up dialog
  and the error messages.

## What the stand-in does not prove

- The mfa-fido2 module itself: its CBOR and COSE parsers, attestation formats (`packed` with `x5c`,
  `fido-u2f`, `tpm`, `android-key`, `apple`), trust roots and chains, the AAGUID catalog, allow and deny
  lists, FIPS mode, EdDSA and RS256, configuration rules CV-2 to CV-10, index reconciliation from host
  records, the purge job, i18n, fuzzing (AC-25) and the fuel and latency budget (AC-27). The stand-in
  implements ES256 with `none` and `packed` self-attestation, CV-1 only, definite-length CBOR with a
  depth limit and no duplicate keys, a replay ledger and a credential index in `db:own`.
- Real authenticators and other browsers: only Chrome (headless) with a virtual authenticator ran.
  Safari, Firefox, mobile browsers, platform authenticators and synced passkeys were not tried.
- Production conditions: the server ran as a debug build over plain HTTP on `localhost` (RP ID
  `localhost`), with no reverse proxy, TLS, several origins or several server instances. Module
  compilation time was measured only in the debug build (about 2 s for 220 KB).
- Clone detection with real counters: the Chrome virtual authenticator advances its counter by two per
  sign-in (signCount 1, 3, 7 in transcript B), which the stand-in accepts as increasing; a counter
  regression was not exercised end to end.
- Anything about `auth.step_up.max_age_secs` over HTTP (not configured in either run), SSO sign-ins,
  enrollment grace windows and phishing-resistant policies (not enabled in either run).

## How to run it again

```bash
# A: HTTP API of the test app (part of `cargo test`; needs the wasm32-unknown-unknown target)
OVX_E2E_TRANSCRIPT=/tmp/a.md cargo test --test integration_tests mfa_fido2 -- --nocapture
# B: real server, built web UI and Chrome (see scripts/README.md)
cargo build && (cd frontend && npm run build)
OVX_E2E_OUT=/tmp/b.md node scripts/e2e-mfa-fido2-browser.mjs
```

Both print a redacted transcript: access and refresh tokens, passwords, the trust re-authentication
header and recovery codes are replaced by `<redacted>`, and long opaque values (challenges, credential
ids, signatures, client data) are shortened. Private keys are generated per run, kept in memory and
never printed; the package signing key is random per run.

## Findings of the check

Bugs, each fixed test-first in its own commit:

1. **Module compilation was charged to the handler timeout** (`e20f4e7`). The first call of a plugin
   version loaded and compiled its WASM module inside `timeout_ms`; the 220 KB stand-in takes about
   2 s to compile in a debug build against its 2000 ms limit, so enabling it failed with
   `lifecycle_failed` (install hook timed out) and the first factor, route or interceptor call could
   fail closed. `HandlerInvoker::prepare` now loads and compiles before the clock starts.
2. **`db.query` turned NULL into `0`, `0.0` or `""`** (`170dd1e`) for columns with a declared type,
   so a never-used key showed `last_used_at` 1970-01-01 in the plugin panel.
3. **The sign-in page showed "Request failed with status code 403"** (`e7fadd8`) for every fail-closed
   refusal at sign-in (`mfa_enrollment_required`, `factor_unavailable`,
   `phishing_resistant_factor_required`); it now shows host messages and announces them.

Observations, not changed:

- A disabled plugin is not an outage (OQ-23): its factors stop counting, so a user whose only factor it
  was gets `403 mfa_enrollment_required` (transcripts A 8c and B B8b), not `403 factor_unavailable`.
  `403 factor_unavailable` at sign-in is the answer while the plugin is installed but cannot run (A 8b:
  the stored package fails its integrity check). With the `auth:factor` grant withdrawn but the plugin
  enabled, the factor is still offered and `begin` answers `503 factor_unavailable` (A 8a, B B8a).
- The authentication rate limit (1 request per second, burst 5, per peer address) also covers
  `challenge/{id}/begin` and `verify`: a security-key sign-in spends 3 of the 5, and the browser run
  has to wait between sign-ins. Behind a reverse proxy all users share the proxy's address.
- With `user_verification: discouraged` the Chrome virtual authenticator does not set UV, so the
  session `amr` is `["pwd", "hwk"]`; the software authenticator of transcript A sets UV and gets
  `["pwd", "hwk", "pin"]`.
- The administrator in these runs has no second factor, so routes declared `step_up: true` (the
  plugin's `unsuspend`) let them through (A 7b): step-up applies only to users with a usable factor.

## Transcript A: software authenticator, HTTP API of the test app (`cargo test`, 2026-11-09)
Guest module: 222804 bytes; compiling it took 2007 ms in this build of the host.

### 1. Administrator signs in and installs the plugin
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"admin"}
< 200
< {"access_token":"<redacted>","expires_in":86400,"refresh_token":"<redacted>","token_type":"Bearer","user":{"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:19Z","deactivated_at":null,"directory_id":null,"email":"admin@localhost","force_password_change":true,"id":"00000000-0000-0000-0000-000000000001","organization_id":"00000000-0000-0000-0000-000000000010","role":"admin","updated_at":"2026-11-09T20:43:19Z","username":"admin"}}
```
Package: `mfa-fido2` 0.1.0, 86945 bytes, signed with a key generated for this run.
```text
> POST /api/v1/plugins/preflight (Bearer <redacted>)
> <.ovxp, 86945 bytes>
< 200
< {"added_capabilities":"[0 items]","can_install":true,"capabilities":"[9 items]","declared_pii_fields":"[2 items]","installed_version":null,"issues":[],"manifest":"{8 keys}","package_sha256":"086a31c22846...(64 chars)","rollback_warning":null,"signature":"{6 keys}"}
```
```text
> POST /api/v1/plugins (Bearer <redacted>)
> x-trust-key-confirm: 481d52f6
> x-trust-key-label: mfa-fido2 e2e
> x-reauth-password: <redacted>
> <.ovxp, 86945 bytes>
< 201
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"installed","state_reason":null,"update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:22.276952Z","versions":"[1 items]"}
```
```text
> PUT /api/v1/plugins/mfa-fido2/capabilities (Bearer <redacted>)
> Content-Type: application/json
> {"capabilities":["auth:factor","crypto:random","api:routes","db:own","audit:write","events:publish:mfa-fido2.*","events:subscribe","ui:slot:profile.security","ui:slot:admin.auth"]}
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"installed","state_reason":null,"update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:22.276952Z","versions":"[1 items]"}
```
```text
> PUT /api/v1/plugins/mfa-fido2/config (Bearer <redacted>)
> Content-Type: application/json
> {"rp":{"id":"localhost","name":"OpenVox WebUI","origins":["http://localhost:3000"]}}
< 200
< {"rp":{"id":"localhost","name":"OpenVox WebUI","origins":["http://localhost:3000"]}}
```
```text
> POST /api/v1/plugins/mfa-fido2/enable (Bearer <redacted>)
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"enabled","state_reason":null,"update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:26.195421Z","versions":"[1 items]"}
```
```text
> GET /api/v1/auth/providers
< 200
< {"factors":[{"amr":"otp","enrollment":"host-managed","fields":["code"],"id":"totp","kind":"otp","label":"Authenticator app","phishing_resistant":false},{"amr":"rcv","enrollment":"host-managed","fields":["code"],"id":"recovery","kind":"recovery","label":"Recovery code","phishing_resistant":false},{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"providers":[{"fields":[{"name":"username","type":"text"},{"name":"password","type":"password"}],"id":"local","kind":"credentials","label":"Local account"}]}
```

### 2. Administrator creates alice; alice signs in with her password (no policy yet)
```text
> POST /api/v1/users (Bearer <redacted>)
> Content-Type: application/json
> {"email":"alice@example.com","password":"<redacted>","role":"viewer","username":"alice"}
< 201
< {"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:26.409938Z","deactivated_at":null,"directory_id":null,"email":"alice@example.com","force_password_change":false,"id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","organization_id":"00000000-0000-0000-0000-000000000010","role":"viewer","updated_at":"2026-11-09T20:43:26.409938Z","username":"alice"}
```
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 200
< {"access_token":"<redacted>","expires_in":86400,"refresh_token":"<redacted>","token_type":"Bearer","user":{"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:26.409938Z","deactivated_at":null,"directory_id":null,"email":"alice@example.com","force_password_change":false,"id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","organization_id":"00000000-0000-0000-0000-000000000010","role":"viewer","updated_at":"2026-11-09T20:43:26.409938Z","username":"alice"}}
```

### 3. alice enrols a security key (enroll/begin, authenticator, enroll/complete)
```text
> POST /api/v1/users/me/factors/mfa-fido2:security-key/enroll/begin (Bearer <redacted>)
> Content-Type: application/json
> {}
< 200
< {"enrollment":{"publicKey":{"attestation":"none","authenticatorSelection":{"authenticatorAttachment":"cross-platform","requireResidentKey":false,"residentKey":"discouraged","userVerification":"discouraged"},"challenge":"r9mltNx1Que2LjsQ4nFbtpNsnTlYAr9dF15_lsBkXD0","excludeCredentials":[],"extensions":{"credProps":true},"pubKeyCredParams":[{"alg":-7,"type":"public-key"}],"rp":{"id":"localhost","name":"OpenVox WebUI"},"timeout":120000,"user":{"displayName":"alice","id":"12PS4Gjdwa2RkMHfIAUPll7THka1Ny6e2veRPIRuMIg","name":"alice"}}},"factor_id":"mfa-fido2:security-key"}
```
```text
> POST /api/v1/users/me/factors/mfa-fido2:security-key/enroll/complete (Bearer <redacted>)
> Content-Type: application/json
> {"credential":{"authenticatorAttachment":"cross-platform","clientExtensionResults":{"credProps":{"rk":false}},"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","rawId":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","response":{"attestationObject":"o2NmbXRmcGFj...(371 chars)","clientDataJSON":"eyJjaGFsbGVu...(183 chars)","transports":["usb"]},"type":"public-key"},"label":"Desk key"}
< 200
< {"enrolled":"mfa-fido2:security-key","recovery":{"codes":"<redacted>","shown_once":true}}
```
```text
> GET /api/v1/plugins/mfa-fido2/me/credentials (Bearer <redacted>)
< 200
< {"below_recommended_minimum":true,"credentials":[{"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:26Z","credential_ref":"MKIsfMS76-fzSRiza5_5wQ","discoverable":null,"host_record_id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","last_used_at":null,"nickname":"Desk key","status":"active","transports":["usb"],"usable":true}],"policy":{"attachment":"cross-platform","max_credentials":10,"recommended_min_credentials":2,"user_verification":"discouraged"},"rp_id":"localhost"}
```
Audit `plugin.mfa-fido2.credential.registered`: {"aaguid":"00000000-0000-0000-0000-000000000000","alg":-7,"backup_eligible":false,"credential_ref":"MKIsfMS76-fzSRiza5_5wQ","format":"packed","trust":"self","user_id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","uv":true}

### 3b. The profile.security panel: its routes for alice, and a rename
```text
> GET /api/v1/plugins/ui/contributions/mfa-fido2/security-keys (Bearer <redacted>)
< 200
< {"href":null,"icon":null,"id":"security-keys","label":null,"order":100,"path":null,"plugin_id":"mfa-fido2","plugin_name":"FIDO2 Security Keys (WebAuthn MFA)","render":"declarative","routes":[{"allowed":true,"method":"GET","path":"/me/credentials","step_up":false},{"allowed":true,"method":"PATCH","path":"/me/credentials/{credential_ref}","step_up":false}],"slot":"profile.security","spec":"{2 keys}","title":"Security keys"}
```
```text
> PATCH /api/v1/plugins/mfa-fido2/me/credentials/MKIsfMS76-fzSRiza5_5wQ (Bearer <redacted>)
> Content-Type: application/json
> {"nickname":"Desk key (USB-C)"}
< 200
< {"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:26Z","credential_ref":"MKIsfMS76-fzSRiza5_5wQ","discoverable":null,"host_record_id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","last_used_at":null,"nickname":"Desk key (USB-C)","status":"active","transports":["usb"],"usable":true}
```
```text
> GET /api/v1/plugins/mfa-fido2/admin/summary (Bearer <redacted>)
< 403
< {"code":"forbidden","error":"forbidden","message":"You do not have the permission this route requires"}
```

### 4. Step-up: adding a second key is guarded (401), step-up challenge, retry
```text
> POST /api/v1/users/me/factors/mfa-fido2:security-key/enroll/begin (Bearer <redacted>)
> Content-Type: application/json
> {}
< 401
< {"code":"step_up_required","details":{"required_acr":2,"step_up":"/api/v1/auth/step-up"},"error":"unauthorized","message":"This action needs a stronger authentication level"}
```
```text
> POST /api/v1/auth/step-up (Bearer <redacted>)
< 202
< {"challenge_id":"75affa57-46f7-44c4-b39f-f487ce30c264","expires_in":300,"factors":[{"amr":"rcv","enrollment":"host-managed","fields":["code"],"id":"recovery","kind":"recovery","label":"Recovery code","phishing_resistant":false},{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"required_acr":2,"status":"mfa_required"}
```
```text
> POST /api/v1/auth/challenge/75affa57-46f7-44c4-b39f-f487ce30c264/begin
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key"}
< 200
< {"challenge":{"publicKey":{"allowCredentials":[{"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","transports":["usb"],"type":"public-key"}],"challenge":"BX65x86jDD5z6nV2_p6Al1wDEVx8MPHHTgXEY4RgOYc","hints":["security-key"],"rpId":"localhost","timeout":120000,"userVerification":"discouraged"}},"factor_id":"mfa-fido2:security-key"}
```
```text
> POST /api/v1/auth/challenge/75affa57-46f7-44c4-b39f-f487ce30c264/verify
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key","response":{"credential":{"authenticatorAttachment":"cross-platform","clientExtensionResults":{},"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","rawId":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","response":{"authenticatorData":"SZYN5YgOjGh0...(50 chars)","clientDataJSON":"eyJjaGFsbGVu...(179 chars)","signature":"MEYCIQDo-s-j...(96 chars)","userHandle":null},"type":"public-key"}}}
< 200
< {"access_token":"<redacted>","expires_in":86400,"refresh_token":"<redacted>","token_type":"Bearer","user":{"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:26.409938Z","deactivated_at":null,"directory_id":null,"email":"alice@example.com","force_password_change":false,"id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","organization_id":"00000000-0000-0000-0000-000000000010","role":"viewer","updated_at":"2026-11-09T20:43:26.409938Z","username":"alice"}}
```
Stepped-up session claims: amr ["pwd","hwk","pin"] acr 2
```text
> GET /api/v1/users/me/factors (Bearer <redacted>)
< 401
< {"error":"unauthorized","message":"Session expired due to inactivity"}
```
```text
> POST /api/v1/users/me/factors/mfa-fido2:security-key/enroll/begin (Bearer <redacted>)
> Content-Type: application/json
> {}
< 200
< {"enrollment":{"publicKey":{"attestation":"none","authenticatorSelection":{"authenticatorAttachment":"cross-platform","requireResidentKey":false,"residentKey":"discouraged","userVerification":"discouraged"},"challenge":"oNVU_AVEvGladDz3oMUo3Gxs5SmjIgS8uoG1dijUA6A","excludeCredentials":[{"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","transports":["usb"],"type":"public-key"}],"extensions":{"credProps":true},"pubKeyCredParams":[{"alg":-7,"type":"public-key"}],"rp":{"id":"localhost","name":"OpenVox WebUI"},"timeout":120000,"user":{"displayName":"alice","id":"12PS4Gjdwa2RkMHfIAUPll7THka1Ny6e2veRPIRuMIg","name":"alice"}}},"factor_id":"mfa-fido2:security-key"}
```
```text
> POST /api/v1/users/me/factors/mfa-fido2:security-key/enroll/complete (Bearer <redacted>)
> Content-Type: application/json
> {"credential":{"authenticatorAttachment":"cross-platform","clientExtensionResults":{"credProps":{"rk":false}},"id":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","rawId":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","response":{"attestationObject":"o2NmbXRmcGFj...(372 chars)","clientDataJSON":"eyJjaGFsbGVu...(183 chars)","transports":["usb"]},"type":"public-key"},"label":"Backup key"}
< 200
< {"enrolled":"mfa-fido2:security-key"}
```

### 5. alice signs out; the token no longer works
```text
> POST /api/v1/auth/logout (Bearer <redacted>)
< 200
< {"message":"Successfully logged out"}
```
```text
> GET /api/v1/users/me/factors (Bearer <redacted>)
< 401
< {"error":"unauthorized","message":"Session expired due to inactivity"}
```

### 6. Administrator requires a second factor; alice signs in with the challenge
```text
> PUT /api/v1/auth/policies (Bearer <redacted>)
> Content-Type: application/json
> {"policies":[{"conditions":{},"name":"everyone","required_acr":2}]}
< 200
< {"policies":[{"conditions":{"providers":[],"roles":[]},"enabled":true,"id":"5124c419-4f06-49b9-89dd-22a138dbe180","name":"everyone","priority":100,"require_phishing_resistant":false,"required_acr":2}]}
```
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 202
< {"challenge_id":"e011e377-2083-4d7f-8751-f339750c246b","expires_in":300,"factors":[{"amr":"rcv","enrollment":"host-managed","fields":["code"],"id":"recovery","kind":"recovery","label":"Recovery code","phishing_resistant":false},{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"required_acr":2,"status":"mfa_required"}
```
```text
> POST /api/v1/auth/challenge/e011e377-2083-4d7f-8751-f339750c246b/begin
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key"}
< 200
< {"challenge":{"publicKey":{"allowCredentials":[{"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","transports":["usb"],"type":"public-key"},{"id":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","transports":["usb"],"type":"public-key"}],"challenge":"LsjwPhNi4qq-nchNqOQzcFuI8zBRLuo5RNca3AAZ92U","hints":["security-key"],"rpId":"localhost","timeout":120000,"userVerification":"discouraged"}},"factor_id":"mfa-fido2:security-key"}
```
```text
> POST /api/v1/auth/challenge/e011e377-2083-4d7f-8751-f339750c246b/verify
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key","response":{"credential":{"authenticatorAttachment":"cross-platform","clientExtensionResults":{},"id":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","rawId":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","response":{"authenticatorData":"SZYN5YgOjGh0...(50 chars)","clientDataJSON":"eyJjaGFsbGVu...(179 chars)","signature":"MEYCIQDq5yD9...(96 chars)","userHandle":null},"type":"public-key"}}}
< 200
< {"access_token":"<redacted>","expires_in":86400,"refresh_token":"<redacted>","token_type":"Bearer","user":{"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:26.409938Z","deactivated_at":null,"directory_id":null,"email":"alice@example.com","force_password_change":false,"id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","organization_id":"00000000-0000-0000-0000-000000000010","role":"viewer","updated_at":"2026-11-09T20:43:26.409938Z","username":"alice"}}
```
```text
> GET /api/v1/plugins/mfa-fido2/me/credentials (Bearer <redacted>)
< 200
< {"below_recommended_minimum":false,"credentials":[{"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:26Z","credential_ref":"MKIsfMS76-fzSRiza5_5wQ","discoverable":null,"host_record_id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","last_used_at":"2026-11-09T20:43:28Z","nickname":"Desk key (USB-C)","status":"active","transports":["usb"],"usable":true},{"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:28Z","credential_ref":"m1hE-08bjdLPqfOsNf-bvA","discoverable":null,"host_record_id":"7827bc40-f6cf-4714-ab55-9b9075cf42cd","last_used_at":"2026-11-09T20:43:28Z","nickname":"Backup key","status":"active","transports":["usb"],"usable":true}],"policy":{"attachment":"cross-platform","max_credentials":10,"recommended_min_credentials":2,"user_verification":"discouraged"},"rp_id":"localhost"}
```

### 7. Negative: a well-formed assertion with a wrong signature
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 202
< {"challenge_id":"2b2c2d32-c19a-4395-90a1-311a7cbf5887","expires_in":300,"factors":[{"amr":"rcv","enrollment":"host-managed","fields":["code"],"id":"recovery","kind":"recovery","label":"Recovery code","phishing_resistant":false},{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"required_acr":2,"status":"mfa_required"}
```
```text
> POST /api/v1/auth/challenge/2b2c2d32-c19a-4395-90a1-311a7cbf5887/begin
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key"}
< 200
< {"challenge":{"publicKey":{"allowCredentials":[{"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","transports":["usb"],"type":"public-key"},{"id":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","transports":["usb"],"type":"public-key"}],"challenge":"tT8glH8nFW0QJUcLSlnBsSHt2eCQ6viJdQzbfYa5gQE","hints":["security-key"],"rpId":"localhost","timeout":120000,"userVerification":"discouraged"}},"factor_id":"mfa-fido2:security-key"}
```
```text
> POST /api/v1/auth/challenge/2b2c2d32-c19a-4395-90a1-311a7cbf5887/verify
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key","response":{"credential":{"authenticatorAttachment":"cross-platform","clientExtensionResults":{},"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","rawId":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","response":{"authenticatorData":"SZYN5YgOjGh0...(50 chars)","clientDataJSON":"eyJjaGFsbGVu...(179 chars)","signature":"MEQCICgAPKNQ...(94 chars)","userHandle":null},"type":"public-key"}}}
< 401
< {"code":"factor_failed","details":{"attempts_left":4},"error":"unauthorized","message":"The response was not accepted"}
```
`auth_challenges.attempts` = 1; plugin audit `credential.verification_failed`: {"credential_ref":"MKIsfMS76-fzSRiza5_5wQ","purpose":"login","reason":"signature_invalid","retryable":true,"user_id":"52226fa4-70b2-40f9-b90b-ea9316207fe9"}; host audit `auth.challenge_failed` entries: 1
```text
> POST /api/v1/auth/challenge/2b2c2d32-c19a-4395-90a1-311a7cbf5887/begin
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key"}
< 200
< {"challenge":{"publicKey":{"allowCredentials":[{"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","transports":["usb"],"type":"public-key"},{"id":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","transports":["usb"],"type":"public-key"}],"challenge":"hqP_SOl0FHAfhbjsBSr224WTtP4LrshimAT5jdvhQ6I","hints":["security-key"],"rpId":"localhost","timeout":120000,"userVerification":"discouraged"}},"factor_id":"mfa-fido2:security-key"}
```
```text
> POST /api/v1/auth/challenge/2b2c2d32-c19a-4395-90a1-311a7cbf5887/verify
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key","response":{"credential":{"authenticatorAttachment":"cross-platform","clientExtensionResults":{},"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","rawId":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","response":{"authenticatorData":"SZYN5YgOjGh0...(50 chars)","clientDataJSON":"eyJjaGFsbGVu...(179 chars)","signature":"MEUCIQDo_wK9...(95 chars)","userHandle":null},"type":"public-key"}}}
< 200
< {"access_token":"<redacted>","expires_in":86400,"refresh_token":"<redacted>","token_type":"Bearer","user":{"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:26.409938Z","deactivated_at":null,"directory_id":null,"email":"alice@example.com","force_password_change":false,"id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","organization_id":"00000000-0000-0000-0000-000000000010","role":"viewer","updated_at":"2026-11-09T20:43:26.409938Z","username":"alice"}}
```

### 7b. Suspended keys: the plugin's own error code reaches the API (H-8)
```text
> GET /api/v1/plugins/mfa-fido2/admin/credentials (Bearer <redacted>)
< 200
< {"items":[{"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:26Z","credential_ref":"MKIsfMS76-fzSRiza5_5wQ","discoverable":null,"host_record_id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","last_used_at":"2026-11-09T20:43:29Z","nickname":"Desk key (USB-C)","status":"active","transports":["usb"],"usable":true,"user_id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","username":"alice"},{"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:28Z","credential_ref":"m1hE-08bjdLPqfOsNf-bvA","discoverable":null,"host_record_id":"7827bc40-f6cf-4714-ab55-9b9075cf42cd","last_used_at":"2026-11-09T20:43:28Z","nickname":"Backup key","status":"active","transports":["usb"],"usable":true,"user_id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","username":"alice"}],"next_cursor":null}
```
```text
> POST /api/v1/plugins/mfa-fido2/admin/credentials/MKIsfMS76-fzSRiza5_5wQ/suspend (Bearer <redacted>)
> Content-Type: application/json
> {"reason":"Reported lost"}
< 200
< {"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":"Reported lost","created_at":"2026-11-09T20:43:26Z","credential_ref":"MKIsfMS76-fzSRiza5_5wQ","discoverable":null,"host_record_id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","last_used_at":"2026-11-09T20:43:29Z","nickname":"Desk key (USB-C)","status":"suspended","transports":["usb"],"usable":false}
```
```text
> POST /api/v1/plugins/mfa-fido2/admin/credentials/m1hE-08bjdLPqfOsNf-bvA/suspend (Bearer <redacted>)
> Content-Type: application/json
> {"reason":"Reported lost"}
< 200
< {"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":"Reported lost","created_at":"2026-11-09T20:43:28Z","credential_ref":"m1hE-08bjdLPqfOsNf-bvA","discoverable":null,"host_record_id":"7827bc40-f6cf-4714-ab55-9b9075cf42cd","last_used_at":"2026-11-09T20:43:28Z","nickname":"Backup key","status":"suspended","transports":["usb"],"usable":false}
```
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 202
< {"challenge_id":"1146436d-eba9-4b20-808d-d5c13e865409","expires_in":300,"factors":[{"amr":"rcv","enrollment":"host-managed","fields":["code"],"id":"recovery","kind":"recovery","label":"Recovery code","phishing_resistant":false},{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"required_acr":2,"status":"mfa_required"}
```
```text
> POST /api/v1/auth/challenge/1146436d-eba9-4b20-808d-d5c13e865409/begin
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key"}
< 503
< {"code":"factor_unavailable","details":{"factor_error":{"code":"no_usable_credential","kind":"unavailable"}},"error":"service_unavailable","message":"no_usable_credential"}
```
`unsuspend` is declared with `step_up: true`; the administrator has no second factor, so step-up does not apply to them (a user without a usable factor passes).
```text
> POST /api/v1/plugins/mfa-fido2/admin/credentials/MKIsfMS76-fzSRiza5_5wQ/unsuspend (Bearer <redacted>)
> Content-Type: application/json
> {}
< 200
< {"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:26Z","credential_ref":"MKIsfMS76-fzSRiza5_5wQ","discoverable":null,"host_record_id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","last_used_at":"2026-11-09T20:43:29Z","nickname":"Desk key (USB-C)","status":"active","transports":["usb"],"usable":true}
```
```text
> POST /api/v1/plugins/mfa-fido2/admin/credentials/m1hE-08bjdLPqfOsNf-bvA/unsuspend (Bearer <redacted>)
> Content-Type: application/json
> {}
< 200
< {"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:28Z","credential_ref":"m1hE-08bjdLPqfOsNf-bvA","discoverable":null,"host_record_id":"7827bc40-f6cf-4714-ab55-9b9075cf42cd","last_used_at":"2026-11-09T20:43:28Z","nickname":"Backup key","status":"active","transports":["usb"],"usable":true}
```

### 8. Fail closed: alice keeps only her security keys (removes her recovery codes)
```text
> GET /api/v1/users/me/factors (Bearer <redacted>)
< 200
< {"available":[{"amr":"otp","enrollment":"host-managed","fields":["code"],"id":"totp","kind":"otp","label":"Authenticator app","phishing_resistant":false},{"amr":"rcv","enrollment":"host-managed","fields":["code"],"id":"recovery","kind":"recovery","label":"Recovery code","phishing_resistant":false},{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"factors":[{"created_at":"2026-11-09T20:43:26.619793+00:00","id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","kind":"mfa-fido2:security-key","label":"Desk key","last_used_at":"2026-11-09T20:43:29.205771+00:00","verified":true},{"created_at":"2026-11-09T20:43:28.724572+00:00","id":"115ec9f2-a958-45e2-b0af-0e886ec4bc6f","kind":"recovery","label":"Recovery codes","last_used_at":null,"remaining":10,"verified":true},{"created_at":"2026-11-09T20:43:28.748203+00:00","id":"7827bc40-f6cf-4714-ab55-9b9075cf42cd","kind":"mfa-fido2:security-key","label":"Backup key","last_used_at":"2026-11-09T20:43:28.974370+00:00","verified":true}],"session":{"acr":2,"amr":["pwd","hwk","pin"]}}
```
```text
> DELETE /api/v1/users/me/factors/115ec9f2-a958-45e2-b0af-0e886ec4bc6f (Bearer <redacted>)
< 200
< {"removed":"115ec9f2-a958-45e2-b0af-0e886ec4bc6f"}
```

### 8a. Outage: the auth:factor grant is withdrawn while the plugin stays enabled
```text
> PUT /api/v1/plugins/mfa-fido2/capabilities (Bearer <redacted>)
> Content-Type: application/json
> {"capabilities":["crypto:random","api:routes","db:own","audit:write","events:publish:mfa-fido2.*","events:subscribe","ui:slot:profile.security","ui:slot:admin.auth"]}
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"enabled","state_reason":null,"update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:26.195421Z","versions":"[1 items]"}
```
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 202
< {"challenge_id":"7148d76c-1ab9-4d2b-8c73-8537ea880c4d","expires_in":300,"factors":[{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"required_acr":2,"status":"mfa_required"}
```
```text
> POST /api/v1/auth/challenge/7148d76c-1ab9-4d2b-8c73-8537ea880c4d/begin
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key"}
< 503
< {"code":"factor_unavailable","error":"service_unavailable","message":"factor_unavailable"}
```
```text
> PUT /api/v1/plugins/mfa-fido2/capabilities (Bearer <redacted>)
> Content-Type: application/json
> {"capabilities":["auth:factor","crypto:random","api:routes","db:own","audit:write","events:publish:mfa-fido2.*","events:subscribe","ui:slot:profile.security","ui:slot:admin.auth"]}
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"enabled","state_reason":null,"update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:26.195421Z","versions":"[1 items]"}
```

### 8b. Outage: the stored package fails its integrity check (plugin in error)
The stored `.ovxp` is altered on disk (one byte), then verified.
```text
> POST /api/v1/plugins/mfa-fido2/verify (Bearer <redacted>)
< 422
< {"code":"integrity_failed","error":"validation_error","message":"the stored package no longer matches its pinned hash"}
```
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 403
< {"code":"factor_unavailable","error":"forbidden","message":"None of the second factors enrolled for this account is available; use a recovery code or ask an administrator"}
```
The original package is put back, verified and the plugin enabled again.
```text
> POST /api/v1/plugins/mfa-fido2/verify (Bearer <redacted>)
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"error","state_reason":"integrity_failed","update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:29.671776Z","versions":"[1 items]"}
```
```text
> POST /api/v1/plugins/mfa-fido2/enable (Bearer <redacted>)
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"enabled","state_reason":null,"update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:31.886213Z","versions":"[1 items]"}
```

### 8c. The administrator disables the plugin
```text
> POST /api/v1/plugins/mfa-fido2/disable (Bearer <redacted>)
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"disabled","state_reason":"disabled_by_admin","update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:31.897569Z","versions":"[1 items]","warnings":[{"code":"users_without_usable_factor","count":1,"message":"1 user has no usable second factor left: alice","users":["alice"]}]}
```
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 403
< {"code":"mfa_enrollment_required","error":"forbidden","message":"Policy requires a second factor and none is enrolled; ask an administrator"}
```

### 8d. Break-glass: the local super administrator still signs in
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"admin"}
< 200
< {"access_token":"<redacted>","expires_in":86400,"refresh_token":"<redacted>","token_type":"Bearer","user":{"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:19Z","deactivated_at":null,"directory_id":null,"email":"admin@localhost","force_password_change":true,"id":"00000000-0000-0000-0000-000000000001","organization_id":"00000000-0000-0000-0000-000000000010","role":"admin","updated_at":"2026-11-09T20:43:19Z","username":"admin"}}
```
Audit `auth.break_glass`: {"alert":true,"reason":"policy requires a second factor, none enrolled"}

### 8e. Re-enabling restores the factor (records were kept)
```text
> POST /api/v1/plugins/mfa-fido2/enable (Bearer <redacted>)
< 200
< {"active_version":"0.1.0","approvals":"[9 items]","auto_update":"off","available_version":null,"capabilities":"[9 items]","id":"mfa-fido2","installed_at":"2026-11-09T20:43:22.276952Z","installed_by":"admin","manifest":"{15 keys}","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"481d52f6142a...(64 chars)","signer_label":"mfa-fido2 e2e","state":"enabled","state_reason":null,"update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:43:36.230245Z","versions":"[1 items]"}
```
```text
> POST /api/v1/auth/login
> Content-Type: application/json
> {"password":"<redacted>","username":"alice"}
< 202
< {"challenge_id":"8f543b5f-de0c-4a80-a378-4260d30079a8","expires_in":300,"factors":[{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"required_acr":2,"status":"mfa_required"}
```
```text
> POST /api/v1/auth/challenge/8f543b5f-de0c-4a80-a378-4260d30079a8/begin
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key"}
< 200
< {"challenge":{"publicKey":{"allowCredentials":[{"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","transports":["usb"],"type":"public-key"},{"id":"g5ID7P3joUaK5LOOaO0X0uI6dJkoyPGOu26WiR3qnOw","transports":["usb"],"type":"public-key"}],"challenge":"8R8FXnLAv9keFSKOHxvnl9Ot-6DPSoa70LBcmbhf0Ug","hints":["security-key"],"rpId":"localhost","timeout":120000,"userVerification":"discouraged"}},"factor_id":"mfa-fido2:security-key"}
```
```text
> POST /api/v1/auth/challenge/8f543b5f-de0c-4a80-a378-4260d30079a8/verify
> Content-Type: application/json
> {"factor_id":"mfa-fido2:security-key","response":{"credential":{"authenticatorAttachment":"cross-platform","clientExtensionResults":{},"id":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","rawId":"Zo5EpHX4UXeSO--JFhRl5SJGw5cTUGiprQBDhwuHN2s","response":{"authenticatorData":"SZYN5YgOjGh0...(50 chars)","clientDataJSON":"eyJjaGFsbGVu...(179 chars)","signature":"MEYCIQCCuokx...(96 chars)","userHandle":null},"type":"public-key"}}}
< 200
< {"access_token":"<redacted>","expires_in":86400,"refresh_token":"<redacted>","token_type":"Bearer","user":{"active":true,"auth_provider":"local","created_at":"2026-11-09T20:43:26.409938Z","deactivated_at":null,"directory_id":null,"email":"alice@example.com","force_password_change":false,"id":"52226fa4-70b2-40f9-b90b-ea9316207fe9","organization_id":"00000000-0000-0000-0000-000000000010","role":"viewer","updated_at":"2026-11-09T20:43:26.409938Z","username":"alice"}}
```

### 9. Removing a key reaches the plugin as auth.factor.removed (H-10)
```text
> GET /api/v1/users/me/factors (Bearer <redacted>)
< 200
< {"available":[{"amr":"otp","enrollment":"host-managed","fields":["code"],"id":"totp","kind":"otp","label":"Authenticator app","phishing_resistant":false},{"amr":"rcv","enrollment":"host-managed","fields":["code"],"id":"recovery","kind":"recovery","label":"Recovery code","phishing_resistant":false},{"amr":["hwk","swk","pin"],"enrollment":"host-managed","fields":["credential"],"id":"mfa-fido2:security-key","kind":"webauthn","label":"Security key","phishing_resistant":true}],"factors":[{"created_at":"2026-11-09T20:43:26.619793+00:00","id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","kind":"mfa-fido2:security-key","label":"Desk key","last_used_at":"2026-11-09T20:43:36.460288+00:00","verified":true},{"created_at":"2026-11-09T20:43:28.748203+00:00","id":"7827bc40-f6cf-4714-ab55-9b9075cf42cd","kind":"mfa-fido2:security-key","label":"Backup key","last_used_at":"2026-11-09T20:43:28.974370+00:00","verified":true}],"session":{"acr":2,"amr":["pwd","hwk","pin"]}}
```
```text
> DELETE /api/v1/users/me/factors/7827bc40-f6cf-4714-ab55-9b9075cf42cd (Bearer <redacted>)
< 200
< {"removed":"7827bc40-f6cf-4714-ab55-9b9075cf42cd"}
```
The plugin's `on_factor_removed` handler marked the key removed 521 ms after the DELETE (event bus dispatch, default tick 1 s).
```text
> GET /api/v1/plugins/mfa-fido2/me/credentials (Bearer <redacted>)
< 200
< {"below_recommended_minimum":true,"credentials":[{"attestation_trust":"self","authenticator":{"aaguid":"00000000-0000-0000-0000-000000000000","fips":false,"model":null,"vendor":null},"backup_eligible":false,"backup_state":false,"blocked_reason":null,"created_at":"2026-11-09T20:43:26Z","credential_ref":"MKIsfMS76-fzSRiza5_5wQ","discoverable":null,"host_record_id":"3aba882d-4d7a-40a5-b625-7c0443fb9cc3","last_used_at":"2026-11-09T20:43:36Z","nickname":"Desk key (USB-C)","status":"active","transports":["usb"],"usable":true}],"policy":{"attachment":"cross-platform","max_credentials":10,"recommended_min_credentials":2,"user_verification":"discouraged"},"rp_id":"localhost"}
```

## Transcript B: real server, built web UI, Chrome with a CDP virtual authenticator (2026-11-09)

Server: target/debug/openvox-webui on http://localhost:5099, web UI from frontend/dist, fresh database.
Package: mfa-fido2 0.1.0 stand-in, 86944 bytes, signed with a key made for this run.
The administrator password is set in the fresh database (seeded hash is a placeholder).

### B1. Administrator installs, approves, configures and enables the plugin (HTTP API)
```text
> POST /api/v1/auth/login
> {"username":"admin","password":"<redacted>"}
< 200
< {"access_token":"<redacted>","refresh_token":"<redacted>","token_type":"Bearer","expires_in":86400,"user":{"id":"00000000-0000-0000-0000-000000000001","organization_id":"00000000-0000-0000-0000-000000000010","username":"admin","email":"admin@localhost","role":"admin","force_password_change":false,"auth_provider":"local","directory_id":null,"active":true,"deactivated_at":null,"created_at":"2026-11-09T20:40:55Z","updated_at":"2026-11-09T20:40:55Z"}}
```
```text
> POST /api/v1/plugins/preflight (Bearer <redacted>)
> <.ovxp, 86944 bytes>
< 200
< {"package_sha256":"f32c1de6b206...(64 chars)","installed_version":null,"issues":[],"can_install":true,"rollback_warning":null}
```
```text
> POST /api/v1/plugins (Bearer <redacted>)
> x-trust-key-confirm: 7c43d1dc
> x-trust-key-label: mfa-fido2 browser e2e
> x-reauth-password: <redacted>
> <.ovxp, 86944 bytes>
< 201
< {"id":"mfa-fido2","name":"FIDO2 Security Keys (WebAuthn MFA)","publisher":"OpenVox WebUI Modules","active_version":"0.1.0","state":"installed","state_reason":null,"pending_version":null,"runtime_kind":"wasm","signer_key_id":"7c43d1dc5e28...(64 chars)","signer_label":"mfa-fido2 browser e2e","installed_by":"admin","installed_at":"2026-11-09T20:40:56.255817Z","updated_at":"2026-11-09T20:40:56.255817Z","available_version":null,"auto_update":"off","update_window":null,"update_hold_version":null,"update_hold_reason":null,"module_trust":null}
```
```text
> PUT /api/v1/plugins/mfa-fido2/capabilities (Bearer <redacted>)
> {"capabilities":["auth:factor","crypto:random","api:routes","db:own","audit:write","events:publish:mfa-fido2.*","events:subscribe","ui:slot:profile.security","ui:slot:admin.auth"]}
< 200
< {"id":"mfa-fido2","name":"FIDO2 Security Keys (WebAuthn MFA)","publisher":"OpenVox WebUI Modules","active_version":"0.1.0","state":"installed","state_reason":null,"pending_version":null,"runtime_kind":"wasm","signer_key_id":"7c43d1dc5e28...(64 chars)","signer_label":"mfa-fido2 browser e2e","installed_by":"admin","installed_at":"2026-11-09T20:40:56.255817Z","updated_at":"2026-11-09T20:40:56.255817Z","available_version":null,"auto_update":"off","update_window":null,"update_hold_version":null,"update_hold_reason":null,"module_trust":null}
```
```text
> PUT /api/v1/plugins/mfa-fido2/config (Bearer <redacted>)
> {"rp":{"id":"localhost","name":"OpenVox WebUI","origins":["http://localhost:5099"]}}
< 200
< {"rp":{"id":"localhost","name":"OpenVox WebUI","origins":["http://localhost:5099"]}}
```
```text
> POST /api/v1/plugins/mfa-fido2/enable (Bearer <redacted>)
< 200
< {"id":"mfa-fido2","name":"FIDO2 Security Keys (WebAuthn MFA)","publisher":"OpenVox WebUI Modules","active_version":"0.1.0","state":"enabled","state_reason":null,"pending_version":null,"runtime_kind":"wasm","signer_key_id":"7c43d1dc5e28...(64 chars)","signer_label":"mfa-fido2 browser e2e","installed_by":"admin","installed_at":"2026-11-09T20:40:56.255817Z","updated_at":"2026-11-09T20:41:00.202133Z","available_version":null,"auto_update":"off","update_window":null,"update_hold_version":null,"update_hold_reason":null,"module_trust":null}
```
```text
> POST /api/v1/users (Bearer <redacted>)
> {"username":"alice","email":"alice@example.com","password":"<redacted>","role":"viewer"}
< 201
< {"id":"f5e190a5-73ae-40d9-86b1-8eb12c7bd614","organization_id":"00000000-0000-0000-0000-000000000010","username":"alice","email":"alice@example.com","role":"viewer","force_password_change":false,"auth_provider":"local","directory_id":null,"active":true,"deactivated_at":null,"created_at":"2026-11-09T20:41:00.422758Z","updated_at":"2026-11-09T20:41:00.422758Z"}
```

Virtual authenticator A: {"protocol":"ctap2","transport":"usb","hasResidentKey":false,"hasUserVerification":true,"isUserVerified":true,"automaticPresenceSimulation":true} (CDP WebAuthn.addVirtualAuthenticator).

### B2. alice signs in with her password in the web UI (no policy yet)
UI: signed in, now at /.
Browser API calls (password sign-in):
```text
GET /api/v1/settings/server -> 200
POST /api/v1/auth/login -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/notifications/stream -> 200
GET /api/v1/notifications/stats -> 200
GET /api/v1/plugins/ui/slots -> 200
GET /api/v1/notifications -> 200
```

### B3. alice enrols a security key from her profile (navigator.credentials.create)
UI: "Save your recovery codes" shown with 10 codes (values not recorded).
UI: "Browser key A" is listed by the host section and by the plugin panel (profile.security).
Authenticator A: id dWapsRG6De... rpId localhost signCount 1.
Browser API calls (enrolment):
```text
GET /api/v1/notifications/stream -> 200
GET /api/v1/notifications -> 200
GET /api/v1/notifications/stats -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/plugins/ui/slots -> 200
GET /api/v1/users/me/factors -> 200
GET /api/v1/plugins/ui/contributions/mfa-fido2/security-keys -> 200
GET /api/v1/plugins/mfa-fido2/me/credentials -> 200
POST /api/v1/users/me/factors/mfa-fido2%3Asecurity-key/enroll/begin -> 200
POST /api/v1/users/me/factors/mfa-fido2%3Asecurity-key/enroll/complete -> 200
GET /api/v1/users/me/factors -> 200
```

### B4. Step-up: adding a second key opens the step-up dialog
UI: step-up dialog "Confirm it's you" opened after 401 step_up_required.
UI after step-up, retried call with the same authenticator: ["This security key is already registered to your account."]
Session after step-up: {"amr":["pwd","hwk"],"acr":2}.
Browser API calls (step-up):
```text
POST /api/v1/users/me/factors/mfa-fido2%3Asecurity-key/enroll/begin -> 401
POST /api/v1/auth/step-up -> 202
POST /api/v1/auth/challenge/508ce8c4-267b-407a-85dd-720f82adaaa2/begin -> 200
POST /api/v1/auth/challenge/508ce8c4-267b-407a-85dd-720f82adaaa2/verify -> 200
POST /api/v1/users/me/factors/mfa-fido2%3Asecurity-key/enroll/begin -> 200
```
Authenticator A is unplugged (its credential kept in memory) and an empty authenticator B is plugged in.
UI: "Browser key B" added without a new step-up (the session holds level 2).
Authenticator B: id GubVqM8ygf... rpId localhost signCount 1.
Browser API calls (second key):
```text
POST /api/v1/users/me/factors/mfa-fido2%3Asecurity-key/enroll/begin -> 200
POST /api/v1/users/me/factors/mfa-fido2%3Asecurity-key/enroll/complete -> 200
GET /api/v1/users/me/factors -> 200
```

### B5. alice signs out from the user menu
UI: back on /login; stored token: none.
```text
> GET /api/v1/users/me/factors (Bearer <redacted>)
< 401
< {"error":"unauthorized","message":"Session expired due to inactivity"}
```
Browser API calls (logout):
```text
POST /api/v1/auth/logout -> 200
GET /api/v1/settings/server -> 200
```

### B6. Administrator requires a second factor; alice signs in with her key
```text
> PUT /api/v1/auth/policies (Bearer <redacted>)
> {"policies":[{"name":"everyone","conditions":{},"required_acr":2}]}
< 200
< {"policies":[{"conditions":{"providers":[],"roles":[]},"enabled":true,"id":"481b5948-5290-4781-956c-3d306152f2ac","name":"everyone","priority":100,"require_phishing_resistant":false,"required_acr":2}]}
```
UI: signed in with the security key; session {"amr":["pwd","hwk"],"acr":2}.
Authenticator B: id GubVqM8ygf... rpId localhost signCount 3.
Browser API calls (login challenge):
```text
GET /api/v1/settings/server -> 200
POST /api/v1/auth/login -> 202
POST /api/v1/auth/challenge/78f96cb1-4507-4282-9586-bdbd7365714f/begin -> 200
POST /api/v1/auth/challenge/78f96cb1-4507-4282-9586-bdbd7365714f/verify -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/notifications/stream -> 200
GET /api/v1/notifications -> 200
GET /api/v1/notifications/stats -> 200
GET /api/v1/plugins/ui/slots -> 200
```

### B7. Negative: the authenticator signs with the wrong private key
Credential B is replaced in the authenticator by one with the same id and another P-256 key (CDP WebAuthn.addCredential).
UI: ["The security key response was not accepted. Try again. Attempts left: 4."]
auth_challenges.attempts = 1; plugin audit reason = signature_invalid.
Browser API calls (wrong signature):
```text
POST /api/v1/auth/logout -> 200
GET /api/v1/settings/server -> 200
POST /api/v1/auth/login -> 202
POST /api/v1/auth/challenge/46b5ec75-d60d-4071-98b9-f6674376fa71/begin -> 200
POST /api/v1/auth/challenge/46b5ec75-d60d-4071-98b9-f6674376fa71/verify -> 401
```
The genuine credential B is put back; alice tries again on the same challenge.
UI: signed in after the retry.
Browser API calls (retry):
```text
POST /api/v1/auth/challenge/46b5ec75-d60d-4071-98b9-f6674376fa71/begin -> 200
POST /api/v1/auth/challenge/46b5ec75-d60d-4071-98b9-f6674376fa71/verify -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/notifications/stream -> 200
GET /api/v1/notifications/stats -> 200
GET /api/v1/plugins/ui/slots -> 200
GET /api/v1/notifications -> 200
```

### B8. Fail closed
```text
> GET /api/v1/users/me/factors (Bearer <redacted>)
< 200
< {"factors":[{"created_at":"2026-11-09T20:41:08.228839+00:00","id":"b75a0909-3edf-40cb-89f7-f074414238ad","kind":"mfa-fido2:security-key","label":"Browser key A","last_used_at":"2026-11-09T20:41:10.690159+00:00","verified":true},{"created_at":"2026-11-09T20:41:10.406315+00:00","id":"ffd89d25-6e93-424c-851c-326f5d4df406","kind":"recovery","label":"Recovery codes","last_used_at":null,"remaining":10,"verified":true},{"created_at":"2026-11-09T20:41:10.881934+00:00","id":"95b301ca-b88e-4229-b06f-dbf6e0e59ee1","kind":"mfa-fido2:security-key","label":"Browser key B","last_used_at":"2026-11-09T20:41:31.850654+00:00","verified":true}]}
```
```text
> DELETE /api/v1/users/me/factors/ffd89d25-6e93-424c-851c-326f5d4df406 (Bearer <redacted>)
< 200
< {"removed":"ffd89d25-6e93-424c-851c-326f5d4df406"}
```
alice removed her recovery codes (stepped-up session), so her security keys are her only factor.

#### B8a. Outage: auth:factor withdrawn while the plugin stays enabled
```text
> PUT /api/v1/plugins/mfa-fido2/capabilities (Bearer <redacted>)
> {"capabilities":["crypto:random","api:routes","db:own","audit:write","events:publish:mfa-fido2.*","events:subscribe","ui:slot:profile.security","ui:slot:admin.auth"]}
< 200
< {"id":"mfa-fido2","name":"FIDO2 Security Keys (WebAuthn MFA)","publisher":"OpenVox WebUI Modules","active_version":"0.1.0","state":"enabled","state_reason":null,"pending_version":null,"runtime_kind":"wasm","signer_key_id":"7c43d1dc5e28...(64 chars)","signer_label":"mfa-fido2 browser e2e","installed_by":"admin","installed_at":"2026-11-09T20:40:56.255817Z","updated_at":"2026-11-09T20:41:00.202133Z","available_version":null,"auto_update":"off","update_window":null,"update_hold_version":null,"update_hold_reason":null,"module_trust":null}
```
UI: ["This verification method is unavailable right now. Use another method or try again later."]; still on /login.
Browser API calls (outage):
```text
POST /api/v1/auth/logout -> 200
GET /api/v1/settings/server -> 200
POST /api/v1/auth/login -> 202
POST /api/v1/auth/challenge/3f2cc3a7-61c3-4c80-a31b-faf69a9bdae6/begin -> 503
```
```text
> PUT /api/v1/plugins/mfa-fido2/capabilities (Bearer <redacted>)
> {"capabilities":["auth:factor","crypto:random","api:routes","db:own","audit:write","events:publish:mfa-fido2.*","events:subscribe","ui:slot:profile.security","ui:slot:admin.auth"]}
< 200
< {"id":"mfa-fido2","name":"FIDO2 Security Keys (WebAuthn MFA)","publisher":"OpenVox WebUI Modules","active_version":"0.1.0","state":"enabled","state_reason":null,"pending_version":null,"runtime_kind":"wasm","signer_key_id":"7c43d1dc5e28...(64 chars)","signer_label":"mfa-fido2 browser e2e","installed_by":"admin","installed_at":"2026-11-09T20:40:56.255817Z","updated_at":"2026-11-09T20:41:00.202133Z","available_version":null,"auto_update":"off","update_window":null,"update_hold_version":null,"update_hold_reason":null,"module_trust":null}
```

#### B8b. The administrator disables the plugin
```text
> POST /api/v1/plugins/mfa-fido2/disable (Bearer <redacted>)
< 200
< {"active_version":"0.1.0","auto_update":"off","available_version":null,"id":"mfa-fido2","installed_at":"2026-11-09T20:40:56.255817Z","installed_by":"admin","module_trust":null,"name":"FIDO2 Security Keys (WebAuthn MFA)","pending_version":null,"publisher":"OpenVox WebUI Modules","runtime_kind":"wasm","signer_key_id":"7c43d1dc5e28...(64 chars)","signer_label":"mfa-fido2 browser e2e","state":"disabled","state_reason":"disabled_by_admin","update_hold_reason":null,"update_hold_version":null,"update_window":null,"updated_at":"2026-11-09T20:41:39.408692Z","warnings":[{"code":"users_without_usable_factor","count":1,"message":"1 user has no usable second factor left: alice","users":["alice"]}]}
```
UI: ["Your organization requires a second factor and this account has none. Ask an administrator to set one up for you."]; still on /login.
Browser API calls (disabled plugin):
```text
GET /api/v1/settings/server -> 200
POST /api/v1/auth/login -> 403
```

#### B8c. Break-glass: the local super administrator still signs in
```text
> POST /api/v1/auth/login
> {"username":"admin","password":"<redacted>"}
< 200
< {"access_token":"<redacted>","refresh_token":"<redacted>","token_type":"Bearer","expires_in":86400,"user":{"id":"00000000-0000-0000-0000-000000000001","organization_id":"00000000-0000-0000-0000-000000000010","username":"admin","email":"admin@localhost","role":"admin","force_password_change":false,"auth_provider":"local","directory_id":null,"active":true,"deactivated_at":null,"created_at":"2026-11-09T20:40:55Z","updated_at":"2026-11-09T20:40:55Z"}}
```
auth.break_glass audit entries: 1.

#### B8d. Re-enabled, the keys work again (records were kept)
```text
> POST /api/v1/plugins/mfa-fido2/enable (Bearer <redacted>)
< 200
< {"id":"mfa-fido2","name":"FIDO2 Security Keys (WebAuthn MFA)","publisher":"OpenVox WebUI Modules","active_version":"0.1.0","state":"enabled","state_reason":null,"pending_version":null,"runtime_kind":"wasm","signer_key_id":"7c43d1dc5e28...(64 chars)","signer_label":"mfa-fido2 browser e2e","installed_by":"admin","installed_at":"2026-11-09T20:40:56.255817Z","updated_at":"2026-11-09T20:41:56.518857Z","available_version":null,"auto_update":"off","update_window":null,"update_hold_version":null,"update_hold_reason":null,"module_trust":null}
```
UI: signed in with the security key.
Authenticator B: id GubVqM8ygf... rpId localhost signCount 7.
Browser API calls (re-enabled):
```text
GET /api/v1/settings/server -> 200
POST /api/v1/auth/login -> 202
POST /api/v1/auth/challenge/2a981fd0-df71-4cce-8c6a-898b8897e26d/begin -> 200
POST /api/v1/auth/challenge/2a981fd0-df71-4cce-8c6a-898b8897e26d/verify -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/users/f5e190a5-73ae-40d9-86b1-8eb12c7bd614/permissions -> 200
GET /api/v1/notifications/stream -> 200
GET /api/v1/notifications -> 200
GET /api/v1/plugins/ui/slots -> 200
GET /api/v1/notifications/stats -> 200
```

Credential A (unplugged) was kept only in this process's memory: yes.

Result: every step passed.
