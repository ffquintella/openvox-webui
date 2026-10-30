# OpenVox WebUI Configuration Reference

Complete reference for all configuration options in OpenVox WebUI.

## Configuration File

The main configuration file is located at `/etc/openvox-webui/config.yaml` and uses YAML format.

## Configuration Sections

### Server Configuration

Controls the web server behavior.

```yaml
server:
  host: "127.0.0.1"        # IP address to bind to (use 0.0.0.0 for all interfaces)
  port: 5051                # Port to listen on
  workers: 4                # Number of worker threads
  serve_frontend: true      # Whether to serve the frontend application
  static_dir: "/usr/share/openvox-webui/frontend"  # Path to frontend files
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `host` | string | `127.0.0.1` | IP address to bind the server to. Use `0.0.0.0` to listen on all interfaces |
| `port` | integer | `5051` | TCP port to listen on |
| `workers` | integer | `4` | Number of worker threads for handling requests |
| `serve_frontend` | boolean | `true` | Whether to serve the React frontend |
| `static_dir` | path | `/usr/share/openvox-webui/frontend` | Path to frontend static files |

### TLS Configuration

Enable HTTPS with TLS certificates.

```yaml
server:
  tls:
    cert_file: "/path/to/cert.pem"
    key_file: "/path/to/key.pem"
    min_version: "TLS1.3"
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `cert_file` | path | - | Path to TLS certificate file (PEM format) |
| `key_file` | path | - | Path to TLS private key file (PEM format) |
| `min_version` | string | `TLS1.3` | Minimum TLS version: `TLS1.2` or `TLS1.3` |

### Database Configuration

SQLite database settings.

```yaml
database:
  url: "sqlite:///var/lib/openvox-webui/openvox.db"
  max_connections: 10
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | `sqlite:///var/lib/openvox-webui/openvox.db` | Database connection string |
| `max_connections` | integer | `10` | Maximum number of concurrent database connections |

### Logging Configuration

Controls application logging behavior.

```yaml
logging:
  level: "info"
  format: "json"
  target: "file"
  log_dir: "/var/log/openvox-webui"
  log_prefix: "openvox-webui"
  daily_rotation: true
  max_log_files: 30
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `level` | string | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `format` | string | `json` | Log format: `json`, `compact`, `pretty` |
| `target` | string | `file` | Log destination: `stdout`, `stderr`, `file` |
| `log_dir` | path | `/var/log/openvox-webui` | Directory for log files (when target=file) |
| `log_prefix` | string | `openvox-webui` | Prefix for log filenames |
| `daily_rotation` | boolean | `true` | Enable daily log rotation |
| `max_log_files` | integer | `30` | Maximum number of rotated log files to keep |

### PuppetDB Configuration

Connection settings for PuppetDB integration.

```yaml
puppetdb:
  url: "https://puppetdb.example.com:8081"
  timeout: 30
  ssl:
    cert_path: "/etc/puppetlabs/puppet/ssl/certs/webui.pem"
    key_path: "/etc/puppetlabs/puppet/ssl/private_keys/webui.pem"
    ca_path: "/etc/puppetlabs/puppet/ssl/certs/ca.pem"
    verify: true
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | - | PuppetDB API URL (https://host:port) |
| `timeout` | integer | `30` | Request timeout in seconds |
| `ssl.cert_path` | path | - | Path to client SSL certificate |
| `ssl.key_path` | path | - | Path to client SSL private key |
| `ssl.ca_path` | path | - | Path to CA certificate |
| `ssl.verify` | boolean | `true` | Verify SSL certificates |

### Authentication Configuration

JWT and session management settings.

```yaml
auth:
  jwt_secret: "your-secret-key-min-32-characters-long"
  jwt_expiry: "24h"
  token_expiry_hours: 24
  refresh_token_expiry_days: 7
  session_timeout: 3600
  password_min_length: 8
  max_login_attempts: 5
  lockout_duration: 900
  bcrypt_cost: 12
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `jwt_secret` | string | *required* | Secret key for JWT token signing (minimum 32 characters) |
| `jwt_expiry` | string | `24h` | JWT token expiry time (format: `1h`, `24h`, `7d`) |
| `token_expiry_hours` | integer | `24` | Access token expiry in hours |
| `refresh_token_expiry_days` | integer | `7` | Refresh token expiry in days |
| `session_timeout` | integer | `3600` | Session timeout in seconds |
| `password_min_length` | integer | `8` | Minimum password length |
| `max_login_attempts` | integer | `5` | Maximum failed login attempts before lockout |
| `lockout_duration` | integer | `900` | Account lockout duration in seconds |
| `bcrypt_cost` | integer | `12` | Bcrypt hashing cost (4-31, higher = more secure but slower) |

### Initial Admin Account

Create default admin user on first startup.

```yaml
initial_admin:
  username: "admin"
  password: "ChangeMe123!"
  email: "admin@example.com"
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `username` | string | `admin` | Admin username |
| `password` | string | *required* | Admin password (change immediately after first login!) |
| `email` | string | - | Admin email address |

### Cache Configuration

Settings for caching PuppetDB and other data.

```yaml
cache:
  ttl: 300
  max_entries: 1000
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ttl` | integer | `300` | Cache time-to-live in seconds |
| `max_entries` | integer | `1000` | Maximum number of cache entries |

### Dashboard Configuration

Frontend dashboard preferences.

```yaml
dashboard:
  theme: "light"
  default_page_size: 25
  refresh_interval: 30
  time_range: "24h"
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `theme` | string | `light` | Default theme: `light`, `dark` |
| `default_page_size` | integer | `25` | Default pagination size |
| `refresh_interval` | integer | `30` | Auto-refresh interval in seconds |
| `time_range` | string | `24h` | Default time range for charts |

### RBAC Configuration

Role-based access control settings.

```yaml
rbac:
  default_role: "viewer"
  session_timeout: 3600
  max_login_attempts: 5
  lockout_duration: 900
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `default_role` | string | `viewer` | Default role for new users |
| `session_timeout` | integer | `3600` | Session timeout in seconds |
| `max_login_attempts` | integer | `5` | Failed login attempts before lockout |
| `lockout_duration` | integer | `900` | Lockout duration in seconds |

### Classification Configuration

Node classification engine settings.

```yaml
classification:
  cache_ttl: 300
  max_rules_per_group: 100
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `cache_ttl` | integer | `300` | Classification cache TTL in seconds |
| `max_rules_per_group` | integer | `100` | Maximum rules allowed per node group |

### Facter Configuration

External facts generation settings.

```yaml
facter:
  templates_dir: "/etc/openvox-webui/templates"
  output_format: "yaml"
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `templates_dir` | path | `/etc/openvox-webui/templates` | Directory for facter templates |
| `output_format` | string | `yaml` | Output format: `yaml`, `json` |

### Puppet CA Configuration

Puppet Certificate Authority integration.

```yaml
puppet_ca:
  url: "https://puppet.example.com:8140"
  ssl:
    cert_path: "/etc/puppetlabs/puppet/ssl/certs/webui.pem"
    key_path: "/etc/puppetlabs/puppet/ssl/private_keys/webui.pem"
    ca_path: "/etc/puppetlabs/puppet/ssl/certs/ca.pem"
    verify: true
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | - | Puppet Server URL |
| `ssl.cert_path` | path | - | Client certificate path |
| `ssl.key_path` | path | - | Client key path |
| `ssl.ca_path` | path | - | CA certificate path |
| `ssl.verify` | boolean | `true` | Verify SSL certificates |

## Environment Variables

Configuration can be overridden with environment variables:

| Variable | Configuration Path | Example |
|----------|-------------------|---------|
| `OPENVOX_HOST` | `server.host` | `0.0.0.0` |
| `OPENVOX_PORT` | `server.port` | `8080` |
| `OPENVOX_DATABASE_URL` | `database.url` | `sqlite:///tmp/test.db` |
| `OPENVOX_LOG_LEVEL` | `logging.level` | `debug` |
| `OPENVOX_JWT_SECRET` | `auth.jwt_secret` | `my-secret-key` |
| `PUPPETDB_URL` | `puppetdb.url` | `https://pdb:8081` |

## Secret References

Any string value in `config.yaml` (or set by an environment override) can be a
reference to a secret instead of the secret itself:

```yaml
auth:
  jwt_secret: "secret://file//run/secrets/openvox-jwt"
  directories:
    - id: corp
      bind_password: "secret://file//run/secrets/ad-bind-password"
      # ... see "Directories (LDAP and Active Directory)"
code_deploy:
  encryption_key: "secret://env/OPENVOX_DEPLOY_KEY"

secrets:
  file_allowed_dirs: ["/run/secrets", "/etc/openvox-webui/secrets"]
```

Syntax: `secret://<provider>/<path>[#<key>]`. The whole value must be the
reference; it is not expanded inside a longer string.

| Provider | Reference | Notes |
|----------|-----------|-------|
| `env` | `secret://env/NAME` | Value of the environment variable `NAME` |
| `file` | `secret://file//absolute/path` | File content without one trailing newline; absolute path, no `..`, at most 64 KB, symlinks are resolved before the check |

`#key` selects a field of a JSON object value (`secret://env/DB_CREDS#password`).
`secrets.file_allowed_dirs` limits where the `file` provider may read; empty
means any absolute path.

References are resolved once, when the configuration loads, so a changed secret
needs a restart. A reference that cannot be resolved stops the startup with the
field name and the reference (never a value). Resolved values are not written
to logs, the database or API responses; each resolution is recorded in the audit
log (`secrets` / `secret.resolved`: provider, path, purpose, result).

`GET /api/v1/secret-providers` lists the providers and
`POST /api/v1/secret-providers/{id}/test` (`{"path": "...", "key": "..."}`)
checks that a reference resolves, without returning the value. Both need
`plugins:admin`; every test is audited (`secret.tested`).

## Configuration Validation

Validate configuration before starting:

```bash
openvox-webui --check-config
```

Or with custom config file:

```bash
openvox-webui --config /path/to/config.yaml --check-config
```

## Configuration Examples

### Minimal Configuration

```yaml
server:
  host: "0.0.0.0"
  port: 5051

database:
  url: "sqlite:///var/lib/openvox-webui/openvox.db"

auth:
  jwt_secret: "change-this-to-a-random-32-char-string"

logging:
  level: "info"
```

### Production Configuration

```yaml
server:
  host: "0.0.0.0"
  port: 5051
  workers: 8
  tls:
    cert_file: "/etc/ssl/certs/openvox.pem"
    key_file: "/etc/ssl/private/openvox.key"
    min_version: "TLS1.3"

database:
  url: "sqlite:///var/lib/openvox-webui/openvox.db"
  max_connections: 20

logging:
  level: "info"
  format: "json"
  target: "file"
  log_dir: "/var/log/openvox-webui"
  daily_rotation: true
  max_log_files: 90

puppetdb:
  url: "https://puppetdb.example.com:8081"
  timeout: 60
  ssl:
    cert_path: "/etc/puppetlabs/puppet/ssl/certs/webui.pem"
    key_path: "/etc/puppetlabs/puppet/ssl/private_keys/webui.pem"
    ca_path: "/etc/puppetlabs/puppet/ssl/certs/ca.pem"
    verify: true

auth:
  jwt_secret: "use-a-strong-randomly-generated-secret-here"
  token_expiry_hours: 8
  refresh_token_expiry_days: 30
  session_timeout: 28800
  max_login_attempts: 3
  lockout_duration: 1800
  bcrypt_cost: 14

cache:
  ttl: 600
  max_entries: 5000

dashboard:
  theme: "dark"
  default_page_size: 50
  refresh_interval: 60

classification:
  cache_ttl: 600
  max_rules_per_group: 200
```

### Development Configuration

```yaml
server:
  host: "127.0.0.1"
  port: 3000

database:
  url: "sqlite://./dev.db"

logging:
  level: "debug"
  format: "pretty"
  target: "stdout"

auth:
  jwt_secret: "dev-secret-not-for-production"
  token_expiry_hours: 24

cache:
  ttl: 60
```

## Security Best Practices

1. **JWT Secret**: Use a strong, randomly generated secret of at least 32 characters
2. **TLS**: Always enable TLS in production environments
3. **Passwords**: Set strong admin password and change default credentials immediately
4. **File Permissions**: Ensure config files are readable only by the openvox-webui user
5. **Database**: Protect database file with appropriate permissions
6. **Logging**: Avoid logging sensitive data; use `json` format for security monitoring

## See Also

- [Installation Guide](INSTALLATION.md)
- [Upgrade Guide](UPGRADE.md)
- [Backup and Restore](BACKUP.md)

## Authentication Pipeline (MFA and SSO)

```yaml
auth:
  mfa:
    factor_key: "secret://file//run/secrets/openvox-factor-key"  # default: jwt_secret
    max_attempts: 5            # wrong factor responses per challenge
    totp_issuer: "OpenVox WebUI"
    webauthn:                  # omit to disable WebAuthn
      rp_id: "puppet.example.com"
      rp_name: "OpenVox WebUI"
      origin: "https://puppet.example.com"
    oidc:
      - id: corp
        issuer: "https://idp.example.com"
        client_id: "openvox"
        client_secret: "secret://env/OIDC_CLIENT_SECRET"
        redirect_uri: "https://puppet.example.com/api/v1/auth/providers/corp/callback"
        asserts_mfa: false     # trust the IdP's amr/acr as the second factor
        jit: false             # create users on first login
        default_role: viewer   # required with jit; never taken from IdP claims
        trust_email: false
saml:
  asserts_mfa: false
```

Without a policy nothing changes. Administrators define MFA policies with
`PUT /api/v1/auth/policies` (`{"policies": [{"name": "admins", "conditions":
{"roles": ["admin"], "providers": ["local"]}, "required_acr": 2}]}`). Changing
`factor_key` makes enrolled factors unreadable, so users must re-enroll.

## Directories (LDAP and Active Directory)

Users can sign in with a directory account and are provisioned from it, with
roles taken from group-to-role mappings. Directories are tried in the order
listed, after the local account: the local administrator never depends on a
directory. The built-in provider is the `ldap` Cargo feature (on by default).

```yaml
auth:
  directories:
    - id: corp                       # provider name in MFA policies and audit
      label: "Corporate account"
      kind: active_directory         # or: ldap (OpenLDAP, 389-ds)
      enabled: true
      servers:                       # tried in order (failover)
        - "ldaps://dc1.example.com:636"
        - "ldaps://dc2.example.com:636"
      start_tls: false               # true with ldap:// URLs
      ca_file: /etc/openvox-webui/ad-ca.pem   # default: system trust store
      bind_dn: "CN=svc-openvox,OU=Service,DC=example,DC=com"
      bind_password: "secret://file//run/secrets/ad-bind-password"
      base_dn: "DC=example,DC=com"
      # {u} is replaced by the escaped login name
      user_filter: "(&(objectCategory=person)(objectClass=user)(sAMAccountName={u}))"
      domains: ["EXAMPLE", "example.com"]     # accept EXAMPLE\user and user@example.com
      attributes:                    # defaults depend on kind
        username: sAMAccountName     # uid on LDAP
        email: mail
        display_name: displayName    # cn on LDAP
        id: objectGUID               # entryUUID / nsUniqueId on LDAP
      groups:
        base_dn: "OU=Groups,DC=example,DC=com"   # default: base_dn
        filter: "(objectClass=group)"
        nested: true                 # AD: LDAP_MATCHING_RULE_IN_CHAIN
      connect_timeout_secs: 5
      timeout_secs: 10               # per operation
      page_size: 500                 # RFC 2696 paged results
      lookup: true                   # search users and groups from the admin page
      jit: false                     # create the account on the first login
      deny_without_role: true        # refuse users who end up with no role
      sync:
        enabled: false
        interval_secs: 3600          # minimum 300
```

Rules enforced at startup (the server refuses to start otherwise):

- `ldaps://` URLs, or `ldap://` with `start_tls: true`. Plain `ldap://` is
  accepted only for `localhost`, `127.0.0.1` and `::1`. Certificates are always
  verified; with `ca_file` only that CA is trusted.
- `bind_dn` and `bind_password` are required (no anonymous search);
  `bind_password` should be a `secret://` reference. It is never returned by
  the API nor written to logs.
- `user_filter` must contain `{u}`; ids must be unique and must not collide
  with `local`, `saml` or an OIDC provider id.

Behaviour:

- **Login** is search-then-bind with the service account. An empty password is
  refused before contacting the server, user input is escaped (RFC 4515), an
  unknown user costs a dummy bind like a wrong password, and both get the same
  `401`. Active Directory accounts that are disabled, expired or locked are
  refused. A directory error or timeout is a denial (the next directory is
  tried). The reason of every denial is audited (`auth.login_denied`), never
  the password.
- **Accounts** are linked by the directory's stable identifier (`objectGUID`,
  `entryUUID`, `nsUniqueId`), never by name or e-mail. An account whose name or
  e-mail is already used by another local account is not provisioned
  (`username_conflict`). Username, e-mail and password of a directory account
  are read-only in the WebUI.
- **Roles** come from mappings (`/api/v1/directories/{id}/role-mappings`, by
  group DN, SID or objectGUID), applied at every login and synchronization. A
  role that is not mapped is never granted, `super_admin` cannot be mapped, and
  only roles a directory granted are removed; roles assigned by hand stay.
- **Synchronization** (`sync.enabled`, or *Sync now* on the Directories page)
  refreshes names, e-mails and roles, creates users who match a mapping, and
  deactivates (never deletes) accounts that disappeared from the directory or
  are disabled there, revoking their sessions and API keys. An account that
  comes back is reactivated (its API keys stay revoked). A run that cannot
  list the directory, or gets no users at all, changes nothing. Every run is
  recorded in `directory_sync_runs`, audited and published as
  `directory.sync.finished`.
- MFA policies apply to directory logins like to any provider (condition
  `providers: ["corp"]`); the break-glass path stays reserved to local
  super administrators.

## SCIM Provisioning (inbound SCIM 2.0)

An identity provider (Entra ID, Okta, OneLogin, Keycloak with a SCIM plugin,
...) can create, update, deactivate and group users through a SCIM 2.0 service
(RFC 7643, RFC 7644) at `/api/v1/scim/v2`. It is off by default. Tokens and
group-to-role mappings are managed on the **SCIM Provisioning** page; the rest
is configuration.

```yaml
auth:
  scim:
    enabled: false                   # default; while false every SCIM request answers 404
    rate_limit_per_minute: 600       # per token; the burst is the same number
    max_auth_failures_per_minute: 10 # per client address, then 429
    max_body_bytes: 1048576          # 4096 to 16777216
    max_page_size: 200               # cap of `count` in list responses (1 to 1000)
    max_token_lifetime_days: 365     # optional: every token must expire within this
    base_url: "https://webui.example.com"   # optional: absolute meta.location
    sso_provider: saml               # or the id of an OIDC provider under auth.mfa.oidc
    sso_link_attribute: user_name    # or external_id
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | boolean | `false` | Serve the SCIM endpoint |
| `rate_limit_per_minute` | integer | `600` | Requests per minute and per token (1 to 100000) |
| `max_auth_failures_per_minute` | integer | `10` | Failed authentications per client address before `429` |
| `max_body_bytes` | integer | `1048576` | Largest request body; larger bodies get `413` |
| `max_page_size` | integer | `200` | Largest page of a list response |
| `max_token_lifetime_days` | integer | none | When set, tokens must have an expiry of at most this many days |
| `base_url` | string | none | Public URL used in `meta.location`; without it the location is a path |
| `sso_provider` | string | none | `saml` or an OIDC provider id: how SCIM users sign in |
| `sso_link_attribute` | string | `user_name` | `user_name` or `external_id`: the SCIM value that identifies the user at the SSO provider |

The values are validated at startup (the server refuses to start otherwise):
`sso_provider` must be `saml` or a configured OIDC provider id. The directory id
`scim` is reserved.

Configure the identity provider with the tenant URL
`https://<host>/api/v1/scim/v2` and a token issued on the SCIM Provisioning page.

- **Authentication**: only `Authorization: Bearer <token>`; no session, cookie,
  JWT or API key opens SCIM, and a SCIM token opens nothing else. Tokens are
  shown once, stored as SHA-256 hashes, compared in constant time, can expire,
  be rotated (the old secret stops working at once) and be revoked. Issuing,
  rotating and revoking need `users:admin` and step-up; they are audited
  (`scim.token_created|rotated|revoked`), as are failed authentications
  (`scim.auth_failed`, reason only). The discovery endpoints
  (`ServiceProviderConfig`, `ResourceTypes`, `Schemas`) also need a token.
- **Accounts**: SCIM users have no local password and cannot sign in with one;
  they sign in through `sso_provider`. With `saml`, the account gets
  `auth_provider: saml` and its NameID is the linked value; with an OIDC
  provider the value is linked as the OIDC `sub`. Without `sso_provider` a SCIM
  user cannot sign in. Username and e-mail are read-only in the WebUI, as for
  directory accounts.
- **Isolation**: SCIM only sees accounts it provisioned. Local accounts (the
  break-glass administrator among them) and directory accounts are invisible
  to it; a `userName` or e-mail they use, in any letter case, is a `409
  uniqueness` conflict; they cannot be group members; `super_admin` can never
  be mapped.
- **Deactivation**: `active: false` (PATCH or PUT) deactivates the account and
  revokes its sessions and API keys at once; `active: true` reactivates it
  (revoked keys stay revoked). `DELETE` also deactivates and keeps the account,
  removes its SCIM groups, roles and SSO link, and the resource answers 404
  from then on. A later POST with the same `externalId` (or the same `userName`
  when neither has an `externalId`) brings the account back.
- **Roles**: groups pushed by the provider map to roles by display name
  (without case), external id or SCIM group id. Roles change as soon as a
  membership or mapping changes; roles assigned by hand are kept.
- **Events**: `user.created`, `user.updated` and `user.deactivated` are
  published to plugins.

See [PLUGIN_PLATFORM.md section 17.14](architecture/PLUGIN_PLATFORM.md) for the
supported SCIM features and their limits.

## Plugins: Trusted Module Pages

Plugin pages normally run in a sandboxed iframe or are rendered by the host
from JSON. A `module` contribution is different: its JavaScript runs inside the
WebUI page, in the same origin and with the signed-in user's access, without a
sandbox. It is therefore off unless you turn it on, and even then only runs for
a plugin version an administrator explicitly trusted.

```yaml
plugins:
  allow_trusted_modules: false   # default; true lets plugins:trust approve module pages
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `allow_trusted_modules` | boolean | `false` | Allow `render: module` contributions of trusted plugin versions to be served |

- The setting is read at startup; a warning is logged when it is on.
- While it is off, module contributions are never listed or served and trusting
  a plugin answers `409 trusted_modules_disabled`. Plugins that contain modules
  still install; their other contributions work.
- Trusting needs the `plugins:trust` permission and the administrator's password
  (`POST /api/v1/plugins/{id}/module-trust`); withdrawing needs `plugins:admin`
  or `plugins:trust` (`DELETE` on the same path). Both are audited.
- An approval covers one version: updating, rolling back or revoking that
  version removes it, so new code always needs a new approval.
- Authentication and user-provider plugins can never run as trusted modules.
- The browser checks each module against the SRI hash the plugin declares and
  the host verifies against the signed package. See
  [PLUGIN_PLATFORM.md section 17.13](architecture/PLUGIN_PLATFORM.md) for details.
