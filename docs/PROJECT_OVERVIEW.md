# OpenVox WebUI Project Overview

> Reference material moved verbatim from the pre-PTF `ROADMAP.md` during the migration to the Project Tracking Format. The current version below was updated from v0.41.1 to v0.41.6.

## Key Features Summary

### Authentication & Authorization
- User registration and management
- JWT token-based authentication
- Role-Based Access Control (RBAC)
- 5 built-in roles with customizable permissions
- Permission caching for performance
- Multi-tenancy support with tenant isolation
- API key management for programmatic access

### Infrastructure Management
- PuppetDB integration with full query support
- Puppet CA certificate management
- Node discovery and classification
- Group hierarchy with rule-based matching
- 10 comparison operators for classification rules
- Fact generation with custom templates
- Multi-format export (JSON, YAML, Shell)

### Monitoring & Analytics
- Real-time dashboard with visualizations
- Node status monitoring and health indicators
- Report generation (compliance, drift detection, custom)
- Machine variable assignment reporting with machine, variable-name, and value searches
- Scheduled report execution
- Alert rules with multiple notification channels
- Alert acknowledgment and silencing
- Comprehensive audit logging

### Configuration
- YAML-based application configuration
- Settings management UI with validation
- Configuration versioning and history
- Import/export functionality
- Per-user dashboard preferences
- Support for PuppetDB, PostgreSQL, and SQLite

### Deployment & Operations
- Native RPM and DEB packages
- Puppet module for infrastructure-as-code
- Docker containerization
- Systemd service integration
- Security hardening (TLS 1.3, rate limiting, security headers)
- Performance optimization (caching, lazy loading, query optimization)
- Complete documentation and upgrade guides

---

## Versioning Strategy

This project follows [Semantic Versioning](https://semver.org/): `MAJOR.MINOR.PATCH`

### Current Version
Current release: **v0.41.6**

Check version with: `make version`

### Bump Commands
```bash
make bump-patch       # v1.0.1 → v1.0.2 (bug fixes)
make bump-minor       # v1.0.0 → v1.1.0 (new features)
make bump-major       # v1.0.0 → v2.0.0 (breaking changes)
```

---

## Default Roles & Permissions

### Built-in Roles

| Role | Description |
|------|-------------|
| **Admin** | Full system access |
| **Operator** | Day-to-day operations (read/write) |
| **Viewer** | Read-only access |
| **GroupAdmin** | Manage specific groups |
| **Auditor** | Compliance and audit logs |

### Permission Scopes
- **all** - System-wide access
- **environment** - Environment-specific
- **group** - Group-specific
- **owned** - User-owned resources only
- **specific** - Specific resource IDs

---

## API Reference

### Core Endpoints
- **Auth:** `POST /api/v1/auth/login`, `POST /api/v1/auth/refresh`
- **Roles:** `GET/POST /api/v1/roles`, `GET/PUT/DELETE /api/v1/roles/:id`
- **Nodes:** `GET /api/v1/nodes`, `GET /api/v1/nodes/:certname`
- **Groups:** `GET/POST /api/v1/groups`, `GET/PUT/DELETE /api/v1/groups/:id`
- **Reports:** `GET /api/v1/reports`, `POST /api/v1/analytics/saved-reports`, `GET /api/v1/analytics/variable-assignments`
- **Alerts:** `GET/POST /api/v1/alerting/*`, `GET /api/v1/alerting/alerts`
- **CA:** `GET /api/v1/ca/*`, `POST /api/v1/ca/sign/:certname`
- **Settings:** `GET /api/v1/settings`, `POST /api/v1/settings/import`

For complete API documentation, see the feature-specific documents in [docs/implemented-features/](implemented-features/).

---

## Installation & Deployment

### Supported Systems
- RHEL 8+, CentOS 8+, Rocky Linux 8+
- Debian 11+, Ubuntu 20.04+
- Docker containers
- Standalone binary installations

### Installation Methods
1. **Native Packages:** RPM (RHEL/CentOS/Fedora) or DEB (Debian/Ubuntu)
2. **Puppet Module:** Use `openvox-webui` module from Puppet Forge
3. **Docker:** Pre-built Docker images available
4. **Manual:** Standalone binary with systemd service

See [docs/INSTALLATION.md](INSTALLATION.md) for detailed setup instructions.

---

## Documentation

| Document | Purpose |
|----------|---------|
| [INSTALLATION.md](INSTALLATION.md) | Setup and deployment guide |
| [CONFIGURATION.md](CONFIGURATION.md) | All configuration options |
| [UPGRADE.md](UPGRADE.md) | Version upgrade procedures |
| [BACKUP.md](BACKUP.md) | Data backup and recovery |
| [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | Common issues and solutions |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Development setup and guidelines |
| [architecture/](architecture/) | System design and architecture |

---

## Development Guidelines

### File Organization
- **Backend (Rust):** `src/` with services, handlers, models, repositories
- **Frontend (TypeScript):** `frontend/src/` with pages, components, hooks, services
- **Tests:** `tests/` with unit, integration, and BDD tests
- **Database:** `migrations/` with SQL migration files
- **Configuration:** `config/` with schema and example files

### File Size Limits
- Maximum 1000 lines per source file
- Split larger files proactively at 800 lines
- Keep concerns separated (services, models, handlers)

### Testing Requirements
- Unit tests for all new services
- Integration tests for API endpoints
- BDD features for user-facing functionality
- Aim for 80%+ code coverage
---

## Version History

| Version | Status | Key Features |
|---------|--------|--------------|
| **0.41.x** | ✅ Current | Searchable machine variable assignment reporting |
| 1.0.x | Planned | Stable production milestone |
| 0.9.x | ✅ Released | Performance optimization, security, packages |
| 0.8.x | ✅ Released | Reporting, alerting, multi-tenancy |
| 0.7.5.x | ✅ Released | CA management UI |
| 0.7.x | ✅ Released | Configuration management |
| 0.6.x | ✅ Released | Dashboard and visualizations |
| 0.5.x | ✅ Released | Facter integration |
| 0.4.x | ✅ Released | Node classification |
| 0.3.x | ✅ Released | PuppetDB integration |
| 0.2.x | ✅ Released | Authentication and RBAC |
| 0.1.x | ✅ Released | Foundation and infrastructure |

---

## References

- [OpenVox Project](https://voxpupuli.org/openvox/)
- [OpenVox GitHub](https://github.com/openvoxproject)
- [PuppetDB API Documentation](https://puppet.com/docs/puppetdb/latest/api/)
- [Puppet Node Classification](https://www.puppet.com/docs/pe/2023.8/grouping_and_classifying_nodes.html)
- [NIST RBAC Model](https://csrc.nist.gov/projects/role-based-access-control)
