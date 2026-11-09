# mfa-fido2: host hand-off note

This note tells the authors of the mfa-fido2 module (`openvox-webui-modules/plugins/mfa-fido2`,
SPEC.md and the draft `plugin.yaml`) what the host now provides for each host dependency (H-1 to H-10)
and open question (OQ-1 to OQ-23) of SPEC.md, with the exact contract names and JSON shapes. The host
design in [PLUGIN_PLATFORM.md](PLUGIN_PLATFORM.md) sections 17.15 to 17.20 prevails over SPEC.md where
they differ; the shapes below were checked against the code on branch `feature/plugin-auth-factor-runtime`
(tasks T116 to T121) and, where marked, exercised end to end in
[e2e-mfa-fido2-transcript.md](e2e-mfa-fido2-transcript.md).

Status words: **done** (available as SPEC.md asked), **answered differently** (available in another
form), **partly done**, **deferred** (not available, with the reason), **no host work** (a module or
operator decision).

## 1. Summary

| ID | SPEC subject | Status | Where |
|---|---|---|---|
| H-1 / OQ-1 | Plugin factor runtime on `wasm` | answered differently (details) | PLATFORM 17.15, 17.19 |
| H-2 / OQ-2 | Random bytes for guests | answered differently: host op `crypto.random` | PLATFORM 17.15 |
| H-3 / OQ-3 | `now_unix` in the enrolment context | done | PLATFORM 17.19 |
| H-4 / OQ-4 | Ceremony purpose | done | PLATFORM 17.19 |
| H-5 / OQ-7 | Host WebAuthn browser client | done | CHANGELOG (H-5), transcript B |
| H-6 / OQ-8 | `profile.security`, `admin.auth`, declarative specs | done | PLATFORM 17.18 |
| H-7 / OQ-9 | WASM API routes with user identity and step-up flag | done | PLATFORM 17.16 |
| H-8 / OQ-10 | Stable factor error codes, plugin i18n | partly done: codes yes, plugin i18n deferred | PLATFORM 17.19 |
| H-9 / OQ-11 | Factor id namespacing | done | PLATFORM 17.15 |
| H-10 / OQ-20 | `auth.factor.removed` | done | PLATFORM 17.19, transcript A section 9 |

Every row was exercised end to end with the test-only stand-in of the module
(`tests/fixtures/plugins/mfa-fido2-standin`), a software authenticator and Chrome's virtual
authenticator. No physical security key was used; see the transcript for what that does not prove.

## 2. Host dependencies

### H-1 Plugin factors on the `wasm` runtime (OQ-1)

Status: answered differently in details (done). Code: `src/services/plugins/auth_factors.rs`,
`src/services/plugins/manifest.rs`, `src/services/auth_pipeline/`.

- Manifest: `contributes.auth_factors: [{id, kind, label, amr, enrollment, fields}]` with `api_version: 2`.
  Needs `auth:factor` declared and not optional, runtime `wasm` or `builtin` (never `remote`), at most 4
  factors. `id` `[a-z][a-z0-9-]`, 1 to 32 characters, no `:`; `kind` `otp | webauthn | push`; `amr` one
  value or a list (OQ-5); `enrollment` `host-managed | external`; `fields` up to 8 names
  `[a-z][a-z0-9_]{0,31}`; `label` 1 to 64 characters. Unknown keys are refused. Authentication plugins
  cannot contribute `module` UI.
- Handler kind `factor`, handlers `begin_enroll`, `complete_enroll`, `begin_challenge`, `verify`. Guest
  request (ABI): `{"kind":"factor","handler":"<name>","input":{...}}`; the answer goes inside the ABI
  envelope `{"ok": ...}`.
- Enrolment input: `{"factor":"security-key","user_id":"<uuid>","username":"alice","existing":[{"id":"..","data":{..}}],"pending":null|{"id":"..","data":{..}},"now_unix":1794220800,"response":{..}}`
  (`response` only in `complete_enroll`; it is the client body `{"credential": {...}, "label": "..."}`).
- Challenge input: `{"factor":"security-key","user_id":"<uuid>","username":"alice","records":[{"id":"..","data":{..}}],"flow_state":null|"..","now_unix":1794220800,"purpose":"login"|"step_up","response":{..}}`
  (`response` only in `verify`; it is the client's `response` member, `{"credential": {...}}`).
- Answers: `begin_enroll` `{"client":{..},"mode":{"pending":{..}}}` or `{"client":{..},"mode":{"immediate":{..}}}`;
  `complete_enroll` `{"data":{..},"label":"..."}` (`label` optional; over 64 characters or with control
  characters is `invalid_label`); `begin_challenge` `{"client":{..},"flow_state":".."}` (`flow_state`
  optional, at most 64 KiB); `verify` `{"outcome":"verified","amr":"hwk"|["hwk","pin"],"updates":[["<record id>",{..}]]}`,
  `{"outcome":"failed","retryable":true|false}` or `{"outcome":"pending"}`; any handler may answer
  `{"error":{"kind":"unavailable"|"invalid"|"internal","code":"<code>"}}`.
- Fail closed: unknown fields, a malformed answer, a trap, a timeout, a late answer, a quota or breaker
  refusal, a plugin that is no longer enabled when the call runs or a withdrawn `auth:factor` grant
  are `factor_unavailable` (HTTP 503 at `begin`; transcript A section 8a). A plugin disabled by an
  administrator is different: its factor is no longer offered at all (OQ-23). `verified` counts only with declared `amr` values and updates of records the plugin was
  given, each at most once; updates are written with compare-and-set.
- The client sees the plugin's `client` unchanged: `POST /api/v1/users/me/factors/{factor_id}/enroll/begin`
  answers `{"factor_id": "...", "enrollment": <client>}` and `POST /api/v1/auth/challenge/{id}/begin`
  `{"factor_id": "...", "challenge": <client>}`.
- Since T121, loading and compiling the module is not charged to `timeout_ms` (section 7, bug 1).

### H-2 Random bytes (OQ-2)

Status: answered differently: a host operation instead of a `random` field in the contexts.
`crypto.random` `{"len": 1..=64}` answers `{"bytes": "<base64url, no padding>"}` from the operating
system CSPRNG. Capability `crypto:random` (low risk). Errors: `invalid` (length), `forbidden`
(capability not declared or not approved), `unavailable` (entropy source). Each call counts against the
1000 host calls of an invocation. Use one call of 32 bytes per challenge and per new user handle
(SPEC 9.2). The capability is not checked at install: a package without it installs and then fails
every `begin_*` call at run time (section 5).

### H-3 `now_unix` in the enrolment context (OQ-3)

Status: done. `EnrollContext.now_unix: u64`, set once per call; plugin input `now_unix` in
`begin_enroll` and `complete_enroll`. Drop the `SELECT strftime('%s','now')` fallback (SPEC A-5 is no
longer needed).

### H-4 Ceremony purpose (OQ-4)

Status: done. `ChallengeContext.purpose: ChallengePurpose` (`Login` or `StepUp`); plugin input
`"purpose": "login" | "step_up"` in `begin_challenge` and `verify`. An unknown stored value makes the
challenge invalid. `step_up_user_verification` (SPEC phase 2) can key on it.

### H-5 Host WebAuthn browser client (OQ-7)

Status: done (commits `5e2b68f`, `1cd0bab`). The web UI runs the ceremony for every factor of kind
`webauthn`, built-in or plugin, on the sign-in page, in the step-up dialog and in the "Security keys and
passkeys" section of the profile (and in the enrollment-only screen of OQ-12).
- Options: the plugin's `client`, with or without a `{"publicKey": ...}` wrapper, in the WebAuthn Level 3
  JSON encoding (binary members base64url without padding). Read with
  `PublicKeyCredential.parseCreationOptionsFromJSON` / `parseRequestOptionsFromJSON` when the browser has
  them, else a base64url decoder.
- Results: the credential as Level 3 JSON (`toJSON()` or a manual serializer: `id`, `rawId`, `type`,
  `response.clientDataJSON`, `response.attestationObject` or `authenticatorData`/`signature`/`userHandle`,
  `response.transports`, `clientExtensionResults`, `authenticatorAttachment`). Enrolment posts
  `{"credential": {...}, "label": "..."}` (label left out when empty); verification posts
  `{"factor_id": "mfa-fido2:security-key", "response": {"credential": {...}}}`.
- The factor id is percent-encoded in paths (`mfa-fido2%3Asecurity-key`) and accepted by the server.
- Verified in transcript B with Chrome's virtual authenticator (registration with attestation `none`,
  assertions, the step-up dialog, `InvalidStateError` for an excluded credential).

### H-6 Slots and declarative panels (OQ-8)

Status: done (T119). `profile.security` (context `user`) on the Profile page and `admin.auth` (no
context, Settings, Authentication tab) are mounted, declarative only; `admin.auth` contributions must
declare `requires`. Specs are validated at preflight, install and every load (at most 64 KiB, unknown
keys refused, routes must be the plugin's own `api_routes` with the right method). Failures answer
`422 invalid_manifest` with one finding per problem. `GET /api/v1/plugins/ui/contributions/{plugin}/{id}`
lists `routes: [{method, path, allowed, step_up}]`. The specs in `tests/fixtures/plugins/mfa-fido2-ui/`
are the module's `ui/profile-security.json` and `ui/admin-auth.json` and validate against the draft's
routes plus `unsuspend` (section 6). Response shapes the specs read: `GET /me/credentials`
`{below_recommended_minimum, credentials: [...]}`, credential `status`
`active | suspended | clone_suspected | blocked`, `GET /admin/credentials` `{items, next_cursor}`,
`GET /admin/authenticators` `{items: [{aaguid, vendor, model, fips, decision}]}` (not in SPEC yet),
policy preview errors `[{path, code, message}]`. Plugin messages are shown as text for 400, 409, 413
and 422 when at most 300 characters.

### H-7 API routes (OQ-9)

Status: done (T117). `contributes.api_routes: [{method, path, handler?, requires?, rate_limit?, step_up?}]`
needs `api:routes` (medium risk, declared, not optional, approved); served at
`/api/v1/plugins/mfa-fido2<path>` to signed-in users only. Checks in order: 404 (plugin cannot run or no
route) -> 403 `forbidden` (`requires`) -> 429 `rate_limited` with `Retry-After` -> 401
`step_up_required` (`step_up: true`) -> 400 `invalid_request` (query) -> 413 `request_too_large` (256 KiB)
/ 415 / 400 `invalid_json`.
- Guest request: `{"kind":"route","handler":"<handler>","input":{"method","route","params","query","body","user":{"id","username","organization_id","roles"},"request_id","now_unix"}}`
  (`route` is the declared path; no header, cookie, token, session id, address or e-mail).
- Answer: `{"status"?: 200|201|202|204|400|403|404|409|413|422|429, "body"?: json}`; anything else,
  including a plugin 503, becomes `503 {"error":"service_unavailable","code":"plugin_unavailable"}`.
- Plugin-owned permissions (`plugin.mfa-fido2.admin`) are held by plugin administrators until plugin
  permissions can be assigned to roles (SPEC A-6).

### H-8 Stable factor error codes and plugin i18n (OQ-10)

Status: partly done. Codes: done. A plugin `{"error":{"kind","code"}}` becomes
`FactorError::Coded {kind, code, message}` (an unsafe code, not `[a-z0-9_.-]{1,64}`, becomes
`plugin_error`). API bodies keep `code` as the category and add the plugin code:
`503 {"error":"service_unavailable","code":"factor_unavailable","message":"no_usable_credential","details":{"factor_error":{"kind":"unavailable","code":"no_usable_credential"}}}`
(transcript A section 7b) and `400 {"error":"bad_request","code":"factor_invalid","details":{"factor_error":{"kind":"invalid","code":"<code>"}}}`;
`internal` stays a generic 500. In `verify` an `invalid` error is a retryable failure answered
`401 factor_failed` with `details.attempts_left` (no enumeration at authentication). The web UI shows
`auth.factor_error.<code>` from the host catalog for host codes only. Deferred: plugin message catalogs
(`mfa_fido2.error.<code>` from the package `i18n/*.json`), which need a served, validated catalog per
plugin version and locale.

### H-9 Factor id namespacing (OQ-11)

Status: done. The qualified id `<plugin-id>:<factor-id>` (`mfa-fido2:security-key`) is the descriptor
id, the id in `factors[]`, the `factor_id` of the factor and challenge routes and the `user_factors.kind`
of the records. A plugin receives only records of its own factor (SPEC A-3 cannot happen; the defensive
rule of SPEC section 6 is harmless). SPEC A-4 holds: the pending record id at `complete_enroll` becomes
the id of the verified record (transcript A: `host_record_id` equals the host record id).

### H-10 `auth.factor.removed` (OQ-20)

Status: done. `DELETE /api/v1/users/me/factors/{id}` and `POST /api/v1/users/{id}/factors/reset` publish
one core event per removed verified record, in the same transaction as the removal: type
`auth.factor.removed`, payload `{"user_id": "<uuid>", "factor": "mfa-fido2:security-key", "record_id": "<record id>"}`,
actor `{kind: "user", id: <username of the acting user>}`. Pending enrolments publish nothing. Subscribe
with `events.subscribe: [{type: auth.factor.removed, handler: on_factor_removed}]` and the optional
capability `events:subscribe`; the handler receives the event envelope (`type`, `payload`, ...). The
stand-in's handler marked the key removed in its index about 0.5 s after the `DELETE` (transcript A
section 9). Delivery is at least once: deduplicate by event id.

## 3. Open questions

| ID | Status | Answer (contract) |
|---|---|---|
| OQ-1 | answered differently (details) | H-1 |
| OQ-2 | answered differently | `crypto.random` host op (H-2) |
| OQ-3 | done | H-3 |
| OQ-4 | done | H-4 |
| OQ-5 | done | `swk` reaches level 2 like `otp` and `hwk`. Manifest `amr` is one value or a list of at most 4 distinct values: methods `otp`, `hwk`, `swk`, qualifiers `pin`, `user`, `fpt`, `face`, `iris`, `retina`, `vbm`, at least one method. The `verify` answer `amr` is a string or a list of declared values with at least one method. Every value is added to the session `amr`. Declare `[hwk, swk, pin]` to report synced keys and user verification. |
| OQ-6 | answered differently | A policy flag, not `required_factors` or `acr 3` in the API: `PUT /api/v1/auth/policies` with `"require_phishing_resistant": true` (needs `required_acr: 2`). A factor qualifies when `kind: webauthn` and its `amr` holds `hwk` or `swk` (descriptor `phishing_resistant: true`; `mfa-fido2:security-key` qualifies). Sign-in and step-up offer only qualifying factors; others are refused with 400 before they run; a qualifying verification records `acr: 3`; no qualifying factor: grace window or `403 phishing_resistant_factor_required`; step-up `401` carries `required_acr: 3`. Nothing to change in the module. |
| OQ-7 | done | H-5 |
| OQ-8 | done | H-6 |
| OQ-9 | done | H-7, including `step_up: true` per route |
| OQ-10 | partly done | H-8 (codes yes, plugin i18n deferred) |
| OQ-11 | done | H-9 |
| OQ-12 | done (SSO excluded) | `auth.mfa.enrollment_grace_secs` (0 off, 60 to 604800) and `auth.mfa.enrollment_grace_on_policy`. A reset (`POST /api/v1/users/{id}/factors/reset` answers `{"removed": n, "enrollment_grace": {"expires_at", "reason"} \| null}`) opens a window; during it a password login answers `202 {"status":"enrollment_required","session_scope":"enrollment","access_token","token_type","expires_in","user"}`. The session may only call `GET /api/v1/users/me/factors` and `POST .../factors/{factor_id}/enroll/begin\|complete`; elsewhere `403 enrollment_only_session`. The first enrolled factor ends it (`"reauthenticate": true`). SSO sign-ins keep `403 mfa_enrollment_required`. A plugin factor enrolled there ends the window like a built-in one. |
| OQ-13 | done | `auth.step_up.max_age_secs` (0 off, 60 to 2592000). A stale level answers `401 {"code":"step_up_required","details":{"required_acr","step_up","reason":"max_age","max_age_secs"}}`. Uses the session's creation time (every verification issues a new session). |
| OQ-14 | done (notification deferred) | Adding a credential of a factor the user already holds needs step-up at `enroll/begin` and again at `enroll/complete`. Every enrolled credential publishes `auth.factor.enrolled` `{user_id, factor, record_id}` and audits `auth.factor_enrolled` `{factor, record, additional}`. No user notification: `notify.send` is still refused. |
| OQ-15 | done | `PATCH /api/v1/users/me/factors/{id}` `{"label": "..."}` -> `200 {"id","label"}`; trimmed, 1 to 64 characters, no control characters (`400 factor_invalid` with `details.factor_error.code: invalid_label`); owner and verified records only (404 otherwise); audit `auth.factor_renamed`; no step-up; the plugin is not called. |
| OQ-16 | no host work | The host has no FIPS-validated crypto module for plugins. If deployments require one, verification has to move into a validated host module; out of scope for a WASM plugin (SPEC OQ-16 stands). |
| OQ-17 | deferred (module phase 3) | No host change in this branch. The existing `http:egress` capability (allow-listed hosts) and plugin configuration or routes are the building blocks for MDS3; trust-anchor custody stays a module and operator decision. |
| OQ-18 | deferred | `/.well-known/webauthn` (Related Origin Requests) is not served. Proposed: `{"origins": [...]}` built from configuration, with the relying-party settings of `auth.mfa.webauthn` and plugins extended to several origins. |
| OQ-19 | deferred | Keep the plugin or fold it into the built-in factor: decide after module phase 2. |
| OQ-20 | done | H-10 |
| OQ-21 | done | `audit.write` during a route call stores the calling user (`user_id`, organization) from the host context; request fields cannot change it; event, job and factor calls still store no user. Non-GET route calls are also audited by the host as `plugin.route.invoked`. `actor_user_id` in details (SPEC 15.4) is redundant for route calls. |
| OQ-22 | deferred | No `metrics.inc`. Proposed: a declared allow-list of counter names and label keys, a low-risk capability, bounded labels, exported as `openvox_plugin_custom_total{plugin, name, ...}`. |
| OQ-23 | done | Factors of a disabled or uninstalled plugin are left out of `usable_kinds` (records kept for a re-enable); a plugin in any other state (error after the breaker or an integrity failure, an enabled plugin with a withdrawn grant) keeps them counted, so an outage fails closed. `POST /api/v1/plugins/{id}/disable` adds `warnings: [{"code":"users_without_usable_factor","message","count","users"}]`; `DELETE /api/v1/plugins/{id}` answers `200 {"uninstalled","warnings"}` instead of 204 when users are stranded; audit `plugin.users_without_usable_factor`. |

## 4. Known divergences from SPEC.md (host prevails)

1. SPEC 8.3 input fields `random` and `purpose`: `random` is not sent (use `crypto.random`); `purpose` is
   sent. The input also has `factor` (the local factor id) and `now_unix` in both contexts.
2. Error placement (SPEC 8.3, 12.3): business errors go inside the ABI result,
   `{"ok":{"error":{"kind","code"}}}`. An ABI-level `{"error": "..."}` is a handler failure: it is
   `factor_unavailable` and counts towards the circuit breaker (5 failures in 60 s isolate the plugin).
3. SPEC 12.3 "`Invalid` / `Failed{retryable: false}`": in `verify` the host turns any `invalid` error into
   a retryable failure that keeps the challenge. To burn it (`ceremony_replayed`, `ceremony_invalid`,
   `credential_suspended`), answer `{"outcome":"failed","retryable":false}`.
4. SPEC 12.3 "the client only sees a generic failure at authentication": the host also returns the
   plugin code in `message` and `details.factor_error` of 400 and 503 answers (for example
   `no_usable_credential` at `begin`); `verify` failures stay generic (`factor_failed`). Host audit
   entries do not carry plugin codes: write reason codes with `audit.write`.
5. SPEC 4.1 "a user with no usable factor passes step-up": still true, and with OQ-23 a user whose
   only factors belong to a disabled plugin is such a user (section 9). A user whose plugin factor is in
   an outage keeps step-up required and gets `503 factor_unavailable` at `POST /auth/step-up`.
6. Sign-in with only unavailable factors: `403 factor_unavailable` (never break-glass); with only
   factors of a disabled plugin: the enrollment path (`403 mfa_enrollment_required`, a grace window or,
   for a local super administrator, break-glass). Transcript A sections 8b and 8c show both.
7. SPEC 8.8 "always `hwk`": a list such as `["hwk", "pin"]` is accepted (OQ-5).
8. SPEC 10.5 "renames are written into the host record at the next verification": the host label now
   has its own rename (`PATCH /api/v1/users/me/factors/{id}`, OQ-15) that the plugin never sees, while
   the plugin's nickname changes through its own route. The two names can differ; the plugin may read
   the host `label` from nowhere (it is not in `records`), so keep the plugin nickname authoritative in
   the plugin panel or drop the plugin rename.
9. SPEC 12.3 plugin route `503 unavailable` / `config_invalid`: a plugin cannot answer 503; use 409 or
   422. Host-raised errors use the host shape `{error, message, code}`.
10. SPEC 5 governor note: every factor call is one invocation of kind `factor` against the quota (600
    per minute per plugin by default); a security-key sign-in costs two (`begin_challenge`, `verify`).
11. The host authentication rate limit (1 request per second, burst 5, per peer address) applies to
    `POST /auth/login`, `challenge/{id}/begin` and `verify`, so one key sign-in uses 3 of the 5.
12. Records of the built-in `webauthn` factor are not offered while `auth.mfa.webauthn` is not
    configured.

## 5. Required capabilities

| Capability | Risk | Needed for | Without it |
|---|---|---|---|
| `auth:factor` | critical | the factor (declared, not optional) | manifest refused |
| `crypto:random` | low | challenges and user handles | installs, then every `begin_*` fails (`forbidden` from the host; answer `unavailable` / `random_unavailable`) |
| `api:routes` | medium | `contributes.api_routes` (declared, not optional) | manifest refused (`api_routes need the capability api:routes`) |
| `db:own` | low | index and replay ledger | storage calls refused |
| `audit:write` | low | plugin audit entries | calls refused |
| `events:publish:mfa-fido2.*` | low | the events of SPEC 15.5 | publishing refused |
| `events:subscribe` (optional) | medium | `auth.factor.removed` | lazy reconciliation only |
| `ui:slot:profile.security`, `ui:slot:admin.auth` | low | the panels | slot contributions not shown |

## 6. Manifest adjustments made to the draft `plugin.yaml`

The stand-in manifest `tests/fixtures/plugins/mfa-fido2-standin/plugin.yaml` is the draft with these
changes; the rest (runtime limits, events, slots, jobs, `config_schema` byte for byte, migrations,
lifecycle, telemetry) is unchanged and accepted by the host. Each change was checked by preflight with
the draft itself:

1. `capabilities` gains `api:routes`. The verbatim draft is refused:
   `422 invalid_manifest`: `api_routes need the capability api:routes`.
2. `capabilities` gains `crypto:random`. Not checked at install, but required at run time (section 5).
3. `api_routes` gains `POST /admin/credentials/{credential_ref}/unsuspend` with
   `requires: {resource: plugin.mfa-fido2.admin, action: write}`, `rate_limit: 20/min` and
   `step_up: true`, because `ui/admin-auth.json` uses it. Without it the package is refused:
   `spec 'ui/admin-auth.json' of contribution 'security-key-admin': spec.blocks[3].row_actions[1].route: POST /admin/credentials/{credential_ref}/unsuspend is not one of the plugin's api_routes`
   (or remove that row action from the spec).
4. `min_webui_version: 0.52.0` becomes `0.51.1`, the version of this branch. With `0.52.0` preflight
   reports `requires OpenVox WebUI 0.52.0 or newer` and `can_install: false`. The module should name the
   first host release that contains T116 to T121.
5. `amr: hwk` becomes `amr: [hwk, swk, pin]` (optional, OQ-5): the stand-in answers `hwk`, or `swk` when
   the credential is backup eligible, plus `pin` when the authenticator set UV.
6. The draft's comments ("not installable yet") are dropped.

The package layout the host accepted: `plugin.yaml`, `backend/plugin.wasm` (core module importing only
`openvox.host_call`), `migrations/0001_init.up.sql` and `.down.sql`, `ui/profile-security.json`,
`ui/admin-auth.json`.

## 7. Bugs found by the end-to-end check (fixed)

1. `e20f4e7` Module compilation was charged to the handler timeout: the first call of a plugin version
   compiled the module inside `timeout_ms` (about 2 s for 220 KB in a debug build against 2000 ms), so
   enabling failed with `lifecycle_failed` and the first factor call could fail closed. Fixed with
   `HandlerInvoker::prepare`, outside the timeout, for factors, routes, interceptors and every other
   handler. A larger module still pays its compilation once per version and process, untimed.
2. `170dd1e` `db.query` returned `0`, `0.0` or `""` for NULL values of typed columns; it now returns
   `null`. Module code written against the old behaviour (for example `last_used_at == 0`) must test
   for `null`.
3. `e7fadd8` The sign-in page showed "Request failed with status code 403" for fail-closed refusals; it
   now shows host messages for `mfa_enrollment_required`, `factor_unavailable` and
   `phishing_resistant_factor_required`.

## 8. Deferred items

- Plugin i18n catalogs (H-8, OQ-10): `<plugin>.*` keys from the package `i18n/*.json` are not served;
  declarative specs show their text as written and factor errors show host or category messages.
- User notification of a new credential (OQ-14): needs a `notify.send` host operation; the hook is the
  `auth.factor.enrolled` event.
- OQ-18 `/.well-known/webauthn`, OQ-22 `metrics.inc`, OQ-17 MDS3 (module phase 3), OQ-19.
- In-memory rate limits: route `rate_limit` counters (and the host authentication limiter) live in the
  process, are reset by a restart or a reload and are not shared between server instances.
- SSO excluded from enrollment grace windows (OQ-12): OIDC and SAML sign-ins keep
  `403 mfa_enrollment_required`; the sign-in provider is not recorded on sessions, so step-up applies
  phishing-resistance policies by role whatever their provider condition.
- Also not done: host validation of `config_schema` (settings are stored as given; the module must
  validate), a settings form generated from `config_schema`, per-route body limits, request and
  response schemas, OpenAPI merge, `remote` runtime factors and routes, admin listing of grace windows.

## 9. Residual risks

- **Break-glass under a phishing-resistant policy.** A local super administrator with nothing enrolled
  still signs in with a password only (audited `auth.break_glass` with `alert: true`), whatever the
  policy, and passes every step-up check (including plugin routes declared `step_up: true`, transcript A
  section 7b) because they have no usable factor. Enrol a key for that account and watch the alert.
- **Grace-window enrolment with TOTP.** During an enrollment grace window the password alone enrols
  the first factor (trust on first use). The web UI only offers WebAuthn there, but TOTP can be enrolled
  through the API in the same window unless a phishing-resistant policy matches the user. Keep windows
  short and reset only on a verified request.
- **OQ-23 step-up bypass when the plugin is disabled.** Disabling mfa-fido2 makes users whose only
  factor is a security key "users without a usable factor": sign-in is refused under a policy, but
  their existing sessions pass step-up-guarded actions until they sign out. The disable answer lists
  them (`users_without_usable_factor`) and the audit entry has `alert: true`; it is never blocked.
- **`auth.factor.enrolled` is fire-and-forget.** It is published after the record is stored, outside
  that transaction, and a publishing failure is only logged, so a future notification can miss a new
  credential (unlike `auth.factor.removed`, which is transactional).
- **The HTTP mapping of `max_age` is not covered by an HTTP test.** The decision is covered in the
  pipeline and the BDD feature; the `401` body (`details.reason: "max_age"`, `max_age_secs`) built in
  `src/api/auth_pipeline.rs` is not, and neither run of the end-to-end check enabled it.
- `crypto:random` is not enforced at install (section 5), and the end-to-end check ran only with
  software authenticators, Chrome and a debug build (see the transcript).
