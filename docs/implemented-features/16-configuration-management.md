# Phase 7: Configuration Management

## Completed Tasks

### 7.1 YAML Configuration System - COMPLETE
- [x] Application configuration schema (JSON Schema in config/schema/)
- [x] PuppetDB connection settings (PuppetDbConfig with SSL support)
- [x] Authentication configuration (AuthConfig with JWT, bcrypt settings)
- [x] Node group definitions (GroupsConfig, NodeGroupDefinition)
- [x] Classification rules definitions (ClassificationRuleDefinition)
- [x] Dashboard layout preferences (DashboardConfig with widgets, theme, pagination)
- [x] **RBAC configuration in YAML** (RbacConfig with roles, permissions, lockout)

### 7.2 Configuration UI - COMPLETE
- [x] Settings management interface (tabbed: General, Dashboard, RBAC, Import/Export, Server Info)
- [x] YAML editor with validation (textarea with monospace font, syntax checking)
- [x] Configuration import/export (export to YAML, upload file, download config)
- [x] Configuration versioning (history display with timestamp, user, action)
- [x] Server information display (version, uptime, features, git commit)
- [x] Dashboard preferences editor (time range, refresh interval, pagination, theme)
- [x] RBAC configuration viewer (default role, session timeout, lockout settings)
- [x] Configuration validation with warnings (dry run, semantic validation)

## Details

Comprehensive configuration management system for application settings and preferences:

### Configuration Files

**Main Configuration (config.yaml):**
```yaml
server:
  host: "0.0.0.0"
  port: 8000
  workers: 4

puppetdb:
  host: "localhost"
  port: 8081
  ssl_cert: "/etc/puppetlabs/puppet/ssl/certs/..."
  ssl_key: "/etc/puppetlabs/puppet/ssl/private_keys/..."
  ssl_ca: "/etc/puppetlabs/puppet/ssl/certs/ca.pem"
  verify_ssl: true

auth:
  jwt_secret: "your-secret-key"
  token_expiry: 3600
  refresh_expiry: 604800

dashboard:
  time_range: "24h"
  refresh_interval: 30
  items_per_page: 50
  widgets:
    - type: "status_distribution"
      position: 0
    - type: "report_trends"
      position: 1

rbac:
  default_role: "viewer"
  session_timeout: 1800
  failed_login_lockout: 5
```

**Groups Configuration (groups.yaml):**
```yaml
groups:
  - name: "All Nodes"
    description: "Default group"
    parent_group: null
    rules:
      - fact: "os.family"
        operator: "~"
        value: ".*"
    classes:
      - "base"
```

### JSON Schema Files

**config.schema.json:**
- Server configuration constraints
- PuppetDB connection validation
- Authentication settings validation
- Dashboard configuration validation
- RBAC settings validation

**groups.schema.json:**
- Group name validation
- Rule operator validation
- Fact path validation
- Class name validation

### Configuration Models

```rust
pub struct Config {
    pub server: ServerConfig,
    pub puppetdb: Option<PuppetDbConfig>,
    pub auth: AuthConfig,
    pub dashboard: DashboardConfig,
    pub rbac: RbacConfig,
    pub cache: CacheConfig,
}

pub struct DashboardConfig {
    pub time_range: String,
    pub refresh_interval: u32,
    pub items_per_page: u32,
    pub theme: String,
    pub widgets: Vec<WidgetConfig>,
}

pub struct RbacConfig {
    pub default_role: String,
    pub session_timeout: u32,
    pub failed_login_lockout: u32,
    pub roles: Vec<RoleConfig>,
}
```

### Settings API Endpoints

**General Settings:**
```
GET    /api/v1/settings            # Get all settings
GET    /api/v1/settings/export     # Export config as YAML
POST   /api/v1/settings/import     # Import config
POST   /api/v1/settings/validate   # Validate YAML
GET    /api/v1/settings/history    # Config change history
```

**Dashboard Settings:**
```
GET    /api/v1/settings/dashboard  # Get dashboard config
PUT    /api/v1/settings/dashboard  # Update dashboard config
```

**RBAC Settings:**
```
GET    /api/v1/settings/rbac       # Get RBAC config
```

**Server Info:**
```
GET    /api/v1/settings/server     # Get server information
```

### Frontend Settings Page

**Components:**

- **GeneralSettingsTab:**
  - Display all configuration sections
  - Read-only view or editable
  - Validation indicators

- **DashboardSettingsTab:**
  - Time range selector
  - Refresh interval input
  - Pagination settings
  - Theme selector
  - Widget configuration
  - Unsaved changes indicator

- **RbacSettingsTab:**
  - Display default role
  - Show session timeout
  - Show lockout settings
  - List configured roles
  - Permission summary

- **ImportExportTab:**
  - YAML editor with syntax highlighting
  - Validation feedback
  - Export to file button
  - Upload configuration file
  - Configuration history
  - Diff viewer for changes

- **ServerInfoTab:**
  - Application version
  - Uptime
  - Git commit hash
  - Enabled features
  - Database status
  - Cache status

### Configuration Versioning

Track all configuration changes:

```json
{
  "timestamp": "2026-01-22T16:00:00Z",
  "user": "admin",
  "action": "updated",
  "changes": {
    "dashboard.time_range": "24h -> 48h",
    "cache.ttl.nodes": "300 -> 600"
  },
  "status": "success"
}
```

### Validation

**Schema Validation:**
- Type checking
- Required field validation
- Format validation
- Constraint checking

**Semantic Validation:**
- PuppetDB connection testable
- JWT secret complexity
- Port availability
- Directory permissions

### Configuration Precedence

1. Default built-in configuration
2. YAML configuration file
3. Environment variables
4. Runtime updates via API

### Key Features

- **Dry Run:** Validate without applying changes
- **Rollback:** Revert to previous configuration
- **Versioning:** Full history with timestamps
- **Diff View:** See what changed between versions
- **Export:** Download configuration as YAML
- **Import:** Upload configuration file
- **Validation:** Real-time validation feedback

## Key Files

- `config/config.yaml` - Main configuration
- `config/schema/config.schema.json` - Config schema
- `config/groups.example.yaml` - Groups example
- `src/config/mod.rs` - Config loading
- `frontend/src/pages/Settings.tsx` - Settings UI
- `frontend/src/components/SettingsTabs/` - Tab components
