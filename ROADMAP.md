# OpenVox WebUI Roadmap

This document outlines the development roadmap for OpenVox WebUI, a web interface for managing and monitoring OpenVox infrastructure.

## Vision

Provide a modern, intuitive web interface for OpenVox that enables:
- Real-time infrastructure monitoring and visualization
- Node classification using dynamic groups (similar to Puppet Enterprise)
- PuppetDB querying with graphical dashboards
- Facter-based node management and classification
- Configuration management through YAML files
- Role-based access control for secure multi-user environments

> **Note:** Detailed documentation for all implemented features is available in [docs/implemented-features/](docs/implemented-features/). This roadmap provides a high-level overview with phase summaries.

---

## Project Status: ✅ ALL PHASES COMPLETE

All phases (1-9) have been completed and deployed to production. This is a summary view for quick reference. For detailed information on any phase, feature, or component, see the [implemented-features/](docs/implemented-features/) directory.

---

## Phase Overview & Quick Links

### Phase 1: Foundation ✅
- [1.1 Project Setup](docs/implemented-features/01-project-setup.md)
- [1.2 Core Backend Architecture](docs/implemented-features/02-core-backend.md)
- [1.3 RBAC Foundation](docs/implemented-features/03-rbac-foundation.md)
- [1.4 Testing Infrastructure](docs/implemented-features/04-testing-infrastructure.md)

### Phase 2: Authentication & Authorization ✅
- [2.1 User Management](docs/implemented-features/05-user-management.md)
- [2.2 RBAC Implementation](docs/implemented-features/06-rbac-implementation.md)
- [2.3-2.5 RBAC Management Tools & API](docs/implemented-features/07-rbac-management.md)

### Phase 3: Infrastructure Integration ✅
- [3.1 PuppetDB Integration](docs/implemented-features/08-puppetdb-integration.md)
- [3.2 Data Caching Layer](docs/implemented-features/09-caching-layer.md)
- [3.3 PuppetDB API Endpoints](docs/implemented-features/10-puppetdb-api.md)
- [3.4 Puppet CA Management (Backend)](docs/implemented-features/11-puppet-ca-backend.md)
- [3.5 CA API Endpoints](docs/implemented-features/12-ca-api-endpoints.md)

### Phase 4: Node Classification ✅
- [Node Classification System](docs/implemented-features/13-node-classification.md)

### Phase 5: Facter Integration ✅
- [Facter Integration](docs/implemented-features/14-facter-integration.md)

### Phase 6: Dashboard & Visualization ✅
- [Dashboard & Visualization](docs/implemented-features/15-dashboard-visualization.md)

### Phase 7: Configuration Management ✅
- [Configuration Management](docs/implemented-features/16-configuration-management.md)

### Phase 7.5: CA Management UI ✅
- [CA Management UI](docs/implemented-features/17-ca-management-ui.md)

### Phase 8: Advanced Features ✅
- [Reporting, Alerting & Multi-Tenancy](docs/implemented-features/18-advanced-features.md)

### Phase 9: Production Readiness ✅
- [Production Readiness](docs/implemented-features/19-production-readiness.md)

---

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
Check version with: `make version`

### Bump Commands
```bash
make version-patch    # v1.0.1 → v1.0.2 (bug fixes)
make version-minor    # v1.0.0 → v1.1.0 (new features)
make version-major    # v1.0.0 → v2.0.0 (breaking changes)
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
- **Reports:** `GET /api/v1/reports`, `POST /api/v1/analytics/saved-reports`
- **Alerts:** `GET/POST /api/v1/alerting/*`, `GET /api/v1/alerting/alerts`
- **CA:** `GET /api/v1/ca/*`, `POST /api/v1/ca/sign/:certname`
- **Settings:** `GET /api/v1/settings`, `POST /api/v1/settings/import`

For complete API documentation, see the feature-specific documents in [docs/implemented-features/](docs/implemented-features/).

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

See [docs/INSTALLATION.md](docs/INSTALLATION.md) for detailed setup instructions.

---

## Documentation

| Document | Purpose |
|----------|---------|
| [INSTALLATION.md](docs/INSTALLATION.md) | Setup and deployment guide |
| [CONFIGURATION.md](docs/CONFIGURATION.md) | All configuration options |
| [UPGRADE.md](docs/UPGRADE.md) | Version upgrade procedures |
| [BACKUP.md](docs/BACKUP.md) | Data backup and recovery |
| [TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | Common issues and solutions |
| [DEVELOPMENT.md](docs/DEVELOPMENT.md) | Development setup and guidelines |
| [architecture/](docs/architecture/) | System design and architecture |

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

## Future Enhancements

- WebSocket real-time updates
- GraphQL API support
- Plugin/extension system
- Mobile app support
- Internationalization (i18n)
- LDAP/SAML/OIDC SSO integration
- Attribute-based access control (ABAC)
- OpenBolt orchestration integration

---

## Version History

| Version | Status | Key Features |
|---------|--------|--------------|
| **1.0.x** | ✅ Current | Production-ready with full feature set |
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
