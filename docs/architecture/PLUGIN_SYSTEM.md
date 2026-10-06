# OpenVox WebUI Plugin System — Design

Status: **Proposal** (no implementation yet)

## 1. Goals and non-goals

A plugin can extend OpenVox WebUI in six ways (*extension points*):

| # | Extension point | Example |
|---|-----------------|---------|
| 1 | **Authentication provider** | LDAP/AD, OIDC, RADIUS, hardware token |
| 2 | **Menu items** | add a sidebar entry, hide/move/rename a built-in one |
| 3 | **Pages** | a new screen at `/p/<plugin>/<page>` |
| 4 | **User provider** | sync/lookup users and groups from an external directory (SCIM, LDAP, HR system) |
| 5 | **Secret provider** | read secrets from Vault, CyberArk, AWS/Azure secret stores |
| 6 | **APIs** | plugin-defined REST endpoints under `/api/v1/plugins/<id>/…`, plus a host API that plugins call |

Non-goals (v1): replacing core RBAC, modifying core DB tables, running arbitrary plugin code inside the server process.

The existing SAML login (`src/services/saml.rs`) and local login (`src/services/auth.rs`) become the first two *built-in* auth providers behind the same trait. That keeps one code path and proves the abstraction.

## 2. Key decision: how plugins run

| Option | Isolation | Cost | Verdict |
|---|---|---|---|
| Dynamic `.so` loading | None (shares memory with JWT keys, DB pool) | Rust has no stable ABI; unsafe | Rejected |
| WASM (wasmtime) | Strong | Large host-API surface, hard for devs | Later, optional |
| **Out-of-process (HTTP/JSON)** | Process/network boundary | Low; any language | **Chosen for v1** |
| Compiled-in (Cargo feature) | None, but trusted | Trivial | Supported for built-ins only |

So there are two plugin *runtimes* behind one internal trait set:

- `builtin` — Rust code compiled into the binary (local, SAML, and any first-party provider).
- `remote` — a separate service the WebUI talks to over HTTPS, described by a manifest. The WebUI is the client for auth/user/secret hooks and a reverse proxy for plugin APIs and UI assets.

A plugin never receives the server's DB pool, JWT signing key or config. It only gets what the host API (§8) grants, scoped by its declared capabilities.

## 3. Plugin manifest

Each plugin ships a `plugin.yaml` (also served by remote plugins at `GET /.well-known/openvox-plugin`).

```yaml
api_version: 1                 # plugin API version (semver major)
id: acme-vault                 # [a-z0-9-], globally unique, immutable
name: ACME Vault Integration
version: 1.2.0
publisher: ACME Corp
min_webui_version: 0.30.0

runtime:
  kind: remote                 # builtin | remote
  base_url: https://vault-plugin.internal:8443
  auth:                        # how WebUI authenticates itself to the plugin
    type: mtls                 # mtls | bearer
    client_cert_ref: secret://builtin/plugin-acme-vault-cert
  timeout_ms: 5000
  health_path: /health

# Everything a plugin may do must be declared. Undeclared => denied (403).
capabilities:
  - auth.provider
  - user.provider
  - secret.provider
  - menu.contribute
  - page.contribute
  - api.expose
  - host.audit.write           # host APIs the plugin wants to call (§8)
  - host.nodes.read

contributes:
  auth_providers: [ ... ]      # §5
  user_providers: [ ... ]      # §6
  secret_providers: [ ... ]    # §7
  menus: [ ... ]               # §4
  pages: [ ... ]               # §4
  api_routes: [ ... ]          # §9

config_schema:                 # JSON Schema; renders the plugin's settings form
  type: object
  properties:
    vault_addr: { type: string, format: uri }
    token:      { type: string, x-secret: true }   # stored encrypted, never returned by API
  required: [vault_addr]
```

Rules enforced at install/enable time: unique `id`, supported `api_version`, every `contributes` entry has a matching `capabilities` entry, every permission string is namespaced `plugin.<id>.*`, and `config_schema` fields marked `x-secret` are stored via the encryption service already used for backups (`services/backup_encryption.rs`).

## 4. Menus and pages

### 4.1 Menu contributions

```yaml
menus:
  - id: vault-secrets
    section: main              # main | admin | user-menu
    label: Vault Secrets
    icon: KeyRound             # lucide-react icon name (allow-listed)
    href: /p/acme-vault/secrets
    order: 450                 # core items use 100,200,… so plugins can interleave
    requires: { resource: plugin.acme-vault.secrets, action: read }
  # modify/remove built-ins:
  - target: core:updates       # stable id of a built-in menu item
    op: hide                   # hide | rename | move
  - target: core:facter-templates
    op: rename
    label: Fact Templates
```

- Built-in items receive stable ids (`core:nodes`, `core:users`, …). Today they are an array literal in `frontend/src/components/Layout.tsx`; it becomes the *base* registry.
- **Removing is cosmetic, not a security control.** A hidden item's route and API still exist and still enforce RBAC. Only users with `plugins:admin` can install a plugin that uses `hide`, and it is audit-logged. Plugins may not hide `core:settings` or `core:plugins` (lock-out protection).
- Conflict resolution: operations apply in plugin priority order (admin-configurable); last writer wins for `rename`/`move`; `hide` wins over everything.
- Items are filtered server-side per user: the menu endpoint only returns entries whose `requires` the caller holds, so plugin metadata isn't leaked.

### 4.2 Page contributions

```yaml
pages:
  - id: secrets
    path: /p/acme-vault/secrets          # always under /p/<plugin-id>/
    title: Vault Secrets
    requires: { resource: plugin.acme-vault.secrets, action: read }
    render:
      type: ...                          # one of the three below
```

Three render types, in increasing power and risk:

1. **`declarative`** (recommended): the plugin supplies a JSON page description (table, form, detail view, chart, markdown) bound to the plugin's API routes. The host renders it with its own React components, so styling, i18n, dark mode and a11y come for free and there is no third-party JS.
   ```yaml
   render:
     type: declarative
     layout: table
     data_source: { route: GET /secrets }
     columns: [ { key: path, label: Path }, { key: updated_at, label: Updated, format: datetime } ]
     row_actions: [ { label: Rotate, route: POST /secrets/{path}/rotate, confirm: true } ]
   ```
2. **`iframe`**: plugin serves its own UI from `assets_url`; the host embeds it in `<iframe sandbox="allow-scripts allow-forms">` (no `allow-same-origin`) with a CSP `frame-src` for that origin. Communicates via a postMessage SDK (`@openvox/plugin-sdk`): `getContext()`, `requestToken()` (short-lived, plugin-scoped), `navigate()`, `toast()`, theme sync.
3. **`module`** (trusted only): an ES module loaded into the host app, exporting a React component. Requires the plugin to be marked `trusted: true` by an admin and a Subresource Integrity hash in the manifest. Off by default (`plugins.allow_trusted_modules: false`).

Frontend wiring: `App.tsx` adds one catch-all `<Route path="/p/:pluginId/*" element={<PluginPage/>}/>` and `Layout.tsx` reads menus from `GET /api/v1/plugins/ui/menu` (React Query hook `usePluginMenu`). No rebuild of the frontend is needed to add a plugin.

## 5. Authentication providers

```rust
// src/plugins/traits.rs
#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn descriptor(&self) -> AuthProviderDescriptor;   // id, label, kind, icon, order
    /// Credential flows (LDAP, RADIUS): username+password(+otp) in, identity out.
    async fn authenticate(&self, req: CredentialAuthRequest) -> Result<AuthOutcome, AuthError>;
    /// Redirect flows (OIDC, SAML): step 1 builds the IdP URL, step 2 consumes the callback.
    async fn begin(&self, req: BeginRequest) -> Result<RedirectStart, AuthError>;
    async fn complete(&self, req: CallbackRequest) -> Result<AuthOutcome, AuthError>;
}

pub enum AuthOutcome {
    Success(ExternalIdentity),                 // see below
    MfaRequired { challenge_id: String, methods: Vec<MfaMethod> },
    Denied { reason_code: String },            // generic message shown to user
}

pub struct ExternalIdentity {
    pub provider_id: String,
    pub external_id: String,                   // stable subject id
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub groups: Vec<String>,                   // external groups -> role mapping
}
```

Key points:

- **Plugins authenticate; the host authorizes and issues the session.** A provider returns an `ExternalIdentity`. The host then resolves/creates the local user (reusing `AuthService::create_user_with_auth_provider` and `get_user_by_external_id`), maps `groups` → roles via an admin-configured mapping table, and issues the normal JWT/session. A plugin can never mint tokens or assign a role the admin did not map.
- Account linking is by `(provider_id, external_id)`, never by username or email alone (prevents takeover through a provider that lets users pick their own email).
- Login page: `GET /api/v1/auth/providers` (public) returns enabled providers (`id`, `label`, `kind: credentials|redirect`). The current "detect SAML" call generalizes to this.
- Existing safeguards apply unchanged: rate limiting (`middleware/rate_limit.rs`), lockout, failed/successful login audit entries, force-password-change only for `local`.
- Break-glass: the `local` provider and its admin account cannot be disabled by a plugin; a failing plugin provider degrades to "provider unavailable", never to "allow".
- Fail closed: timeouts, malformed responses, or a bad signature on the response are treated as `Denied`.

Remote wire protocol (HTTPS, JSON, all under the plugin `base_url`):

```
POST /auth/authenticate   {username, password, otp?, client_ip, request_id} -> AuthOutcome
POST /auth/begin          {redirect_uri, state}                           -> {redirect_url}
POST /auth/complete       {query, body, state}                            -> AuthOutcome
```

Passwords transit only over mTLS and are never logged by the host.

## 6. User providers

Separate from authentication: it answers "who exists" and "what groups do they belong to".

```rust
#[async_trait]
pub trait UserProvider: Send + Sync {
    fn descriptor(&self) -> UserProviderDescriptor;
    async fn search_users(&self, q: UserQuery) -> Result<Page<ExternalUser>, ProviderError>;
    async fn get_user(&self, external_id: &str) -> Result<Option<ExternalUser>, ProviderError>;
    async fn list_groups(&self, q: GroupQuery) -> Result<Page<ExternalGroup>, ProviderError>;
    async fn group_members(&self, group_id: &str) -> Result<Page<ExternalUser>, ProviderError>;
    /// Optional: provider can push changes (SCIM-style) -- default is pull only.
    fn supports_push(&self) -> bool { false }
}
```

Modes (per provider, admin-configured):

- **Lookup only**: the Users/Roles UI can search the directory to pre-provision or map groups to roles.
- **JIT provisioning**: user created on first successful login (the default for auth providers).
- **Scheduled sync**: a scheduler job (same pattern as `services/*_scheduler.rs`) reconciles users/groups; disabled upstream users are **deactivated, not deleted**, and their sessions/API keys are revoked.
- **Push (SCIM 2.0)**: optional inbound `POST /api/v1/plugins/<id>/scim/v2/Users` etc., authenticated with a plugin-scoped bearer token.

Users carry `auth_provider` and `external_id` (columns already used by SAML). Users managed by a provider are read-only for identity fields in the UI.

## 7. Secret providers

```rust
#[async_trait]
pub trait SecretProvider: Send + Sync {
    fn descriptor(&self) -> SecretProviderDescriptor;   // scheme, e.g. "vault", capabilities
    async fn get(&self, r: &SecretRef, ctx: &SecretContext) -> Result<SecretValue, SecretError>;
    async fn list(&self, prefix: &SecretRef, ctx: &SecretContext) -> Result<Vec<SecretMeta>, SecretError>; // names only
    async fn put(&self, r: &SecretRef, v: SecretValue, ctx: &SecretContext) -> Result<SecretMeta, SecretError>; // optional
    async fn health(&self) -> ProviderHealth;
}
```

**Reference syntax.** Anywhere the app accepts a credential in configuration (PuppetDB token, Git deploy key, SMTP password, SAML key, webhook secret, other plugins' `x-secret` fields) it may instead accept a reference:

```
secret://<provider-id>/<path>[#<key>][?version=<n>]
secret://vault-prod/kv/openvox/puppetdb#token
```

A single `SecretResolver` in the host resolves references; callers never talk to providers directly.

```rust
pub struct SecretResolver { /* providers, cache, audit */ }
impl SecretResolver {
    pub async fn resolve(&self, r: &SecretRef, ctx: &SecretContext) -> Result<Secret, SecretError>;
}
// Secret wraps the value: Debug/Display print "[REDACTED]", zeroize on drop, no Serialize.
```

Rules:

- **Late binding + TTL cache**: values are fetched at use time and cached in memory only, with the provider-suggested TTL (default 60 s, max configurable). Never written to the DB, logs, error messages, backups or API responses.
- **Fail closed** with a typed error (`NotFound`, `Denied`, `Unavailable`, `Timeout`); callers decide whether to retry. Optional stale-while-error window is off by default.
- **Authorization**: each reference is resolved on behalf of a `SecretContext { requester: core|plugin(id)|user(id), purpose }`. A plugin can resolve only references whose provider allow-list (admin-set) includes it and only under path prefixes it was granted.
- **Every resolve/put/list is audited** (who, provider, path, purpose, result — never the value).
- Provider authentication to the backing vault (AppRole, Kubernetes auth, IAM role, client cert) is configured in the provider's `config_schema`; its own bootstrap credential is the only secret stored locally, encrypted at rest.
- Remote wire protocol:
  ```
  POST /secrets/get   {path, key?, version?, request_id, requester} -> {value, ttl_s, version, content_type}
  POST /secrets/list  {prefix}                                      -> {items:[{path, updated_at}]}
  POST /secrets/put   {path, value, ...}                            -> {version}
  ```
  Responses are `Cache-Control: no-store`.
- A built-in `env` and `file` provider (`secret://env/NAME`, `secret://file//run/secrets/x`) ship in core so the mechanism works without any plugin.

## 8. Host API (plugins calling the WebUI)

Plugins that declare `host.*` capabilities get a **plugin token**: a short-lived JWT (aud `openvox-plugin:<id>`, ≤15 min, scope list) issued per request or via the SDK. They call the normal API under `/api/v1/…` and the extra namespace below. The token is checked by a `plugin_auth_middleware` that intersects (declared capabilities) ∩ (admin-granted capabilities) ∩ (endpoint's required capability). Acting *as a user* uses token exchange: the user's request is forwarded with an `X-OpenVox-Delegated-User` assertion signed by the host, so the plugin's call is limited to that user's RBAC permissions.

| Capability | Grants |
|---|---|
| `host.nodes.read`, `host.facts.read`, `host.reports.read` | read-only existing endpoints |
| `host.audit.write` | `POST /api/v1/plugins/host/audit` (event type auto-prefixed `plugin.<id>.`) |
| `host.notifications.send` | `POST /api/v1/plugins/host/notifications` |
| `host.kv` | per-plugin key/value store `GET/PUT/DELETE /api/v1/plugins/host/kv/{key}` (quota-limited) |
| `host.secrets.resolve` | `POST /api/v1/plugins/host/secrets/resolve` (subject to §7 authorization) |
| `host.events.subscribe` | webhook subscription to events (see §10) |

## 9. Plugin-exposed APIs

Plugins register routes; the host reverse-proxies them so clients see one origin, one auth model.

```yaml
api_routes:
  - method: GET
    path: /secrets                    # exposed as GET /api/v1/plugins/acme-vault/secrets
    requires: { resource: plugin.acme-vault.secrets, action: read }
    rate_limit: 60/min
    request_schema:  { ... }          # optional JSON Schema, validated by host before proxying
    response_schema: { ... }
    openapi: ./openapi.yaml           # optional, merged into /api/v1/openapi.json
```

Proxy behavior (`src/api/plugins_proxy.rs`):

1. Normal `auth_middleware` first (JWT / API key) → then RBAC check for `requires` → then rate limit.
2. Strip `Authorization`/`Cookie`; add `X-OpenVox-User`, `X-OpenVox-Roles`, `X-OpenVox-Request-Id`, and a signed delegated-user assertion.
3. Enforce body-size and timeout limits; stream response; drop hop-by-hop headers.
4. Unauthenticated plugin routes are not allowed (webhook-style routes must declare `auth: signature` and a verification secret).
5. Audit log entry for every non-GET call.

## 10. Lifecycle, events and hooks

Plugin states: `installed → configured → enabled ⇄ disabled → uninstalled`, plus `error` (health check failing; extension points are skipped, menus hidden).

Event hooks (optional capability `host.events.subscribe`), delivered as signed webhooks with retry and dead-letter:
`auth.login.success|failure`, `user.created|updated|deactivated`, `node.classified`, `report.received`, `deploy.finished`, `alert.fired`.

Hooks are **notify-only** in v1 (no veto/mutation), which keeps the core path deterministic and plugin failures harmless.

## 11. Backend implementation layout

```
src/plugins/
  mod.rs              PluginManager: load, validate, enable/disable, hot-reload registry
  manifest.rs         serde types + JSON-Schema validation of plugin.yaml
  registry.rs         Arc<RwLock<Registry>> of providers/menus/pages/routes (swap atomically)
  traits.rs           AuthProvider, UserProvider, SecretProvider
  remote/             HTTP client (reqwest, mTLS, timeouts, circuit breaker)
  builtin/            local_auth.rs, saml_auth.rs, env_secret.rs, file_secret.rs
  secrets.rs          SecretResolver, Secret (zeroizing), SecretRef parser
  capabilities.rs     capability enum + checks
src/api/plugins.rs         admin CRUD + UI metadata endpoints
src/api/plugins_proxy.rs   reverse proxy for plugin routes
src/middleware/plugin_auth.rs
migrations/2026XXXX_plugins.sql
```

`AppState` gains `plugins: Arc<PluginManager>` and `secrets: Arc<SecretResolver>`.

Database tables:

```sql
plugins(id TEXT PK, version TEXT, manifest_json TEXT, manifest_sha256 TEXT, state TEXT,
        trusted INTEGER DEFAULT 0, priority INTEGER DEFAULT 100,
        installed_by TEXT, installed_at TEXT, updated_at TEXT)
plugin_config(plugin_id TEXT, key TEXT, value_encrypted BLOB, is_secret INTEGER,
              PRIMARY KEY(plugin_id, key))
plugin_capability_grants(plugin_id TEXT, capability TEXT, granted_by TEXT, granted_at TEXT)
plugin_kv(plugin_id TEXT, key TEXT, value BLOB, PRIMARY KEY(plugin_id, key))
auth_role_mappings(provider_id TEXT, external_group TEXT, role_id TEXT)
secret_grants(plugin_id TEXT, provider_id TEXT, path_prefix TEXT)
```

Plugin-defined permissions (`plugin.<id>.<resource>`) are registered into the existing RBAC tables on enable and removed on uninstall; they are never auto-assigned to roles.

## 12. Admin API (all require `plugins:admin` unless noted)

```
GET    /api/v1/plugins                              list installed
POST   /api/v1/plugins                              install (manifest URL or body) -> shows requested capabilities
GET    /api/v1/plugins/:id                          details, state, health
PUT    /api/v1/plugins/:id/config                   save settings (secret fields write-only)
POST   /api/v1/plugins/:id/enable | /disable
POST   /api/v1/plugins/:id/grants                   approve capabilities
DELETE /api/v1/plugins/:id                          uninstall
POST   /api/v1/plugins/:id/test                     connectivity/health check

GET    /api/v1/plugins/ui/menu                      (any authed user) merged, permission-filtered menu
GET    /api/v1/plugins/ui/pages/:pluginId/:pageId   (any authed user) page descriptor
GET    /api/v1/auth/providers                       (public) enabled login methods
GET/PUT /api/v1/auth/providers/:id/role-mappings    external group -> role
GET    /api/v1/secret-providers                     list providers + health
POST   /api/v1/secret-providers/:id/test            resolve a test ref (value never returned)
ANY    /api/v1/plugins/:id/*                        proxied plugin routes (§9)
```

Plus `/api/v1/plugins/host/*` for the host API in §8.

## 13. Security requirements (aligned with the FGV secure-development norm / G-002)

- **Least privilege**: deny by default; capabilities declared, admin-approved, and re-approval required if an update adds any.
- **Install provenance**: manifest SHA-256 pinned at install; optional detached signature (`plugin.yaml.sig`, ed25519) with an admin-managed trust list; `plugins.require_signed: true` option.
- **Transport**: remote plugins require HTTPS; mTLS or bearer; no plaintext in production (`allow_insecure_http` only honored for `localhost`). SSRF guard: `base_url` host allow-list, block link-local/metadata addresses unless explicitly allowed.
- **Secrets**: `Secret` type with redaction + zeroize; never in logs, errors, backups, or API output; `x-secret` config fields write-only.
- **Input/Output**: all manifest-supplied strings sanitized; icons from an allow-list; declarative pages rendered by host components only (no `dangerouslySetInnerHTML`); iframe sandbox + CSP.
- **Audit**: install/enable/disable/grant/config change, every login attempt via a plugin, every secret access, every proxied write. Reuses `audit_logs`.
- **Resilience**: per-plugin timeout, circuit breaker, bulkheads; a dead plugin never blocks startup or login via `local`.
- **Supply chain**: plugin dependencies are the plugin's responsibility, but the host exposes version/health so the inventory is tracked (register each plugin in the system record per the norm).
- **Data protection (LGPD)**: provider descriptors declare which personal-data fields they receive/return; shown to the admin on install.

## 14. Versioning and compatibility

- `api_version` is the plugin API major. The host supports N and N-1; unsupported plugins are installed but kept `disabled` with a clear reason.
- The wire protocol only gets additive changes within a major; unknown JSON fields must be ignored by both sides.
- Manifest and host API are published as OpenAPI + JSON Schema in `docs/api/plugins/`, and an SDK is provided for Rust (`openvox-plugin-sdk` crate) and TypeScript (`@openvox/plugin-sdk`).

## 15. Phased delivery

| Phase | Scope |
|---|---|
| 1 | `plugins` tables, manifest, `PluginManager`, admin API + Plugins page; menu registry (core items get stable ids); `/p/:pluginId/*` route; declarative pages |
| 2 | `SecretResolver` + `env`/`file` providers + `secret://` support in config; remote secret provider; audit |
| 3 | `AuthProvider` trait; refactor local + SAML into built-ins; `/auth/providers`; remote credential + redirect providers; role mappings |
| 4 | `UserProvider` (lookup, JIT, scheduled sync, SCIM) |
| 5 | Plugin API proxy, host API + plugin tokens, event webhooks, OpenAPI merge, iframe pages, SDKs |
| 6 | Signed plugins, trusted `module` pages, optional WASM runtime |

## 16. Open questions

1. Should remote plugins be allowed to register *unauthenticated* routes (e.g., SAML-like callbacks), or only through the host's `/auth/*` flows? (Proposal: only through host flows.)
2. Multi-organization: are plugins global or per-organization? (Proposal: installed globally, enabled/configured per organization.)
3. Is `module`-type frontend code acceptable at all for the FGV deployment, or should it be compiled out?
4. Does the secret provider need write (`put`) in v1, or read-only?
