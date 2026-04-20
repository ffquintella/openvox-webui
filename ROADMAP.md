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

- Application inventory and update management
- WebSocket real-time updates
- GraphQL API support
- Plugin/extension system
- Mobile app support
- Internationalization (i18n)
- LDAP/SAML/OIDC SSO integration
- Attribute-based access control (ABAC)
- OpenBolt orchestration integration

---

## Phase 10: Application Inventory & Update Management

Status: In progress. Phases 10.1 through 10.5 are implemented, and Phase 10.6 is in progress with update-job orchestration foundations in place.

### Goal
- Turn OpenVox WebUI into a central inventory and update management console for Windows, Linux, and macOS nodes managed through the OpenVox Puppet module.
- Collect application, website, runtime, package, and OS version data from agents, store historical inventory on the server, identify outdated software from default repositories, and provide controlled update operations for single nodes or groups of nodes.

### Scope Summary
- Inventory collection from Windows, Linux, and macOS nodes
- Inventory transport through the OpenVox Puppet module back to the server
- Database schema and APIs for inventory snapshots, application metadata, and update state
- Node-level UI cards showing OS version, OS patch state, and installed applications
- Fleet statistics UI for updated and outdated nodes, stale inventory, and update compliance
- Background jobs to resolve latest versions from vendor-default repositories and package sources
- On-demand and bulk update workflows with auditability and safety controls

### Inventory Coverage

#### Operating System Data
- OS family, distribution, edition, architecture, kernel version, OS version, and patch level
- Last inventory collection time and last successful update time
- Host package manager and update channel metadata

#### Linux Inventory
- Installed RPM packages for Oracle Linux, Red Hat, and SUSE families
- Installed DEB packages for Ubuntu and Debian families
- Package epoch, version, release, architecture, repository source, and install time when available
- Service-backed application discovery for Apache HTTPD, NGINX, Tomcat, and JBoss
- Website and application metadata:
  - Apache virtual hosts, site roots, enabled modules, bound ports, TLS certificate references
  - NGINX server blocks, upstreams, document roots, bound ports, TLS certificate references
  - Tomcat deployed WARs, context paths, app base, connector ports, runtime version
  - JBoss / WildFly deployed applications, server groups, runtime version, management bindings

#### Windows Inventory
- Installed applications from registered Windows uninstall sources
- Product name, publisher, version, install date, uninstall identity, architecture, and install scope
- IIS website and application inventory:
  - Sites, applications, virtual directories, bindings, app pools, physical paths, and certificate bindings
- OS version and patch metadata from Windows management facts
- Update source metadata aligned to Chocolatey for version intelligence

#### macOS Inventory
- Installed `.app` bundles from standard application locations
- Bundle identifier, display name, version, short version, install path, signing metadata when available
- Homebrew formula and cask inventory with installed versions, tap/source, and install paths
- macOS version, build number, and available software update metadata

### Delivery Architecture

#### Agent Collection via Puppet Module
- Extend the `openvox_webui` Puppet module to collect and normalize inventory locally
- Provide OS-specific collection helpers:
  - Linux package collectors for RPM and DEB systems
  - Windows collectors for installed applications and IIS inventory
  - macOS collectors for app bundles, Homebrew formulas, and casks
  - Runtime collectors for Apache, NGINX, Tomcat, and JBoss deployments
- Normalize data into a shared inventory payload schema
- Send payloads back to the server through authenticated module-driven reporting endpoints
- Support incremental submissions and full snapshots
- Track collector version so server-side parsers can handle schema evolution

#### Server-Side Processing
- Validate, deduplicate, and persist host inventory snapshots
- Maintain both current-state tables and historical snapshots for trend reporting
- Correlate package inventory with service/application inventory on the same host
- Mark stale records when a host has not reported within the expected collection interval

### Database Plan

#### Core Tables
- `host_inventory_snapshots`
- `host_os_inventory`
- `host_package_inventory`
- `host_application_inventory`
- `host_web_inventory`
- `host_runtime_inventory`
- `host_update_status`
- `repository_version_catalog`
- `update_jobs`
- `update_job_targets`
- `update_job_results`

#### Data Model Principles
- Separate canonical software identity from host-specific installed instances
- Store raw collector payloads for troubleshooting and parser migrations
- Support multiple inventory records per host for the same product when installed in different locations
- Preserve historical snapshots for compliance and trend views
- Record source confidence and discovery method for each inventory item

### API Plan
- Inventory ingestion endpoint for Puppet module submissions
- Node inventory endpoints for current state and history
- Fleet statistics endpoints for update posture and inventory coverage
- Version intelligence endpoints for latest-known versions by platform and package source
- Update orchestration endpoints for single-node and bulk update requests
- Audit endpoints for update job actions, approvals, and outcomes

### UI Plan

#### Node Detail Enhancements
- Add an inventory summary card with:
  - OS version
  - OS patch level
  - Last inventory time
  - Last update time
  - Counts of installed applications, packages, and detected websites
- Add inventory tabs or sections for:
  - Packages
  - Installed applications
  - Websites and virtual hosts
  - Application runtimes and deployments
  - Update status and available updates

#### Fleet Update Dashboard
- Separate UI section for update and inventory analytics
- Statistics and charts for:
  - Updated vs outdated nodes
  - Nodes missing inventory
  - Nodes missing update checks
  - Time since last patch
  - Top outdated packages and applications
  - Platform and OS distribution
- Filters by OS family, environment, group, application, runtime, and update status

#### Update Operations UI
- Single-node update action
- Bulk update action for selected nodes or groups
- Dry-run / preview before execution
- Scheduling, maintenance window, and approval options
- Job progress, success/failure breakdown, and rollback guidance where applicable

### Latest Version Intelligence Plan

#### Linux Sources
- Oracle Linux: DNF/YUM repository metadata from configured default repositories
- Red Hat / compatible RPM systems: DNF/YUM repository metadata
- SUSE: Zypper repository metadata
- Ubuntu and Debian: APT repository metadata
- Compare installed versions against latest versions visible in default repositories for each node platform

#### Windows Source
- Use Chocolatey package metadata as the default version source for Windows update intelligence
- Map installed applications to Chocolatey packages where possible
- Flag unmatched software separately rather than reporting false update recommendations

#### macOS Source
- Homebrew formula and cask metadata for managed Homebrew software
- macOS software update metadata for OS patch visibility
- App bundle version checking only where a trustworthy managed source exists

### Scheduled Background Jobs
- Inventory freshness job to detect stale or missing host submissions
- Repository sync job to refresh latest-version catalogs by platform
- Version comparison job to compute per-node outdated status
- Statistics aggregation job for dashboard performance
- Update execution watcher for long-running update jobs and result rollups

### Update Execution Plan

#### Execution Model
- Use the Puppet/OpenVox control plane to trigger updates safely on managed nodes
- Support package-manager-native actions:
  - `dnf`/`yum` for RPM systems
  - `apt` for DEB systems
  - `zypper` for SUSE
  - `choco` for Windows
  - `brew` for Homebrew-managed macOS software
- Distinguish:
  - OS patch updates
  - Package updates
  - Application/runtime updates

#### Safety Controls
- RBAC permissions for viewing, scheduling, approving, and executing updates
- Group-scoped update permissions
- Maintenance window enforcement
- Maximum concurrency controls
- Dry-run previews and explicit confirmation for bulk actions
- Full audit logging of requested, approved, started, completed, and failed updates

### Delivery Phases

#### Phase 10.1: Inventory Schema & Transport (Completed)
- Define canonical inventory schema and ingestion API
- Extend Puppet module with inventory submission support
- Add database tables for OS, packages, applications, websites, runtimes, and snapshots

#### Phase 10.2: OS-Specific Collectors (Completed)
- Linux RPM/DEB collectors
- Windows installed applications and IIS collectors
- macOS app bundle and Homebrew collectors
- Apache, NGINX, Tomcat, and JBoss discovery collectors

#### Phase 10.3: Node Inventory UI (Completed)
- Node inventory cards and detailed inventory sections
- Search, filter, and history views per node

#### Phase 10.4: Version Intelligence & Scheduled Jobs (Completed)
- Repository sync workers for Linux, Windows, and macOS sources
- Outdated-version comparison engine
- Scheduled jobs and stale inventory detection

#### Phase 10.5: Fleet Statistics & Compliance Reporting (Completed)
- Separate inventory/update dashboard
- Updated vs outdated statistics
- Patch aging, coverage, and drift metrics

#### Phase 10.6: Update Orchestration (In Progress)
- Single-node updates
- Bulk group updates
- Scheduling, approvals, execution tracking, and audit logging

### Open Questions
- Whether unmanaged Windows applications without Chocolatey mapping should be shown as inventory-only or support custom catalog matching
- Whether Tomcat and JBoss application collection should parse deployment manifests deeply or start with runtime and deployment names only
- Whether website inventory should also track TLS expiration, certificates, and reverse-proxy upstream dependencies in the first release
- Whether update execution should use immediate remote actions, queued Puppet runs, or both
- Whether Homebrew-managed updates should be limited to formulae/casks explicitly marked as managed by policy

### Success Criteria
- Every managed node reports OS version and inventory on a predictable schedule
- Operators can see current installed applications and websites directly from the node view
- Fleet dashboard shows accurate updated/outdated status by platform and group
- Latest-version checks use trusted default package sources for each supported OS family
- Operators can safely run single-node and bulk updates with audit trails and RBAC controls

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
