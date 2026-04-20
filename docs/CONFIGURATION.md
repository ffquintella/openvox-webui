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
