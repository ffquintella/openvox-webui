# Changelog

All notable changes to OpenVox WebUI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial project structure
- ROADMAP.md with development phases
- CONTRIBUTING.md with contribution guidelines
- BDD test infrastructure with Cucumber
- Rust backend skeleton with Axum
- React frontend skeleton with TypeScript
- YAML configuration system design
- Documentation framework
- Architecture overview and user guide (docs/architecture, docs/user-guide)
- Development guide (docs/DEVELOPMENT.md) covering build/test/package steps
- Puppet CA client and management API (status, CSR list, sign, reject, revoke, renew)
- Puppet CA configuration block (`puppet_ca`) and RBAC permissions for certificates
- Classification engine (Phase 4.1): rule evaluation, operators coverage, hierarchical inheritance
- Enabled BDD `node_classification.feature` scenarios
- Cucumber scenarios for Puppet CA operations (status, requests, sign/reject/revoke, renew)
- Multi-tenancy foundations: `organizations` table and per-tenant `organization_id` columns across core tables
- `super_admin` system role for cross-tenant administration
- API key management (`/api/v1/api-keys`) with role-scoped keys and API key authentication
- Audit logging improvements with `/api/v1/audit-logs` endpoint
- User-selectable theme: light/dark toggle with persistence (localStorage) and early application to avoid FOUC
- Tailwind dark mode enabled (`darkMode: 'class'`) and ThemeToggle component added to sidebar
- Database batch loading methods to eliminate N+1 query patterns in GroupRepository and RBAC services
- Selective permission cache invalidation using role-to-users reverse lookup mapping
- Frontend lazy loading with React.lazy() and Suspense for all page components
- Native package building infrastructure (Phase 9.3):
  - Enhanced `scripts/build-packages.sh` with version auto-detection, Docker builds, and binary tarball support
  - RPM spec file for RHEL/CentOS/Fedora/Rocky with proper dependencies and security hardening
  - DEB packaging for Debian/Ubuntu with libssl3/libssl1.1 alternatives
  - Systemd service unit with comprehensive security hardening (SystemCallFilter, MemoryDenyWriteExecute, etc.)
  - Environment configuration files (`/etc/default/openvox-webui`, `/etc/sysconfig/openvox-webui`)
  - GPG package signing support
  - Comprehensive packaging documentation (`packaging/README.md`)
- Puppet module for automated deployment (Phase 9.4):
  - Complete `openvox-webui` Puppet module with 50+ configurable parameters
  - Template-based configuration generation (`config.yaml.epp`)
  - Custom facter for PuppetDB auto-discovery (`puppetdb_connection`)
  - Automatic SSL certificate detection for PuppetDB integration
  - Support for 8 OS distributions (RHEL/CentOS/Rocky/Alma/Fedora/Debian/Ubuntu)
  - Hiera integration with data defaults
  - Example manifests for common deployment scenarios
  - Module README with complete usage documentation
- Production documentation suite (Phase 9.5):
  - Installation guide (`docs/INSTALLATION.md`) covering RPM, DEB, and Puppet methods
  - Configuration reference (`docs/CONFIGURATION.md`) with all parameters and examples
  - Upgrade guide (`docs/UPGRADE.md`) with version-specific migration notes and rollback procedures
  - Backup and restore guide (`docs/BACKUP.md`) with automated scripts and disaster recovery
  - Troubleshooting sections for common deployment issues
  - Security best practices integrated throughout documentation

### Changed
- Rust clippy lint fixes across config, middleware, models, and services
- API routing split into public vs protected routes; auth middleware applies only to protected routes
- Groups, fact templates, and users are tenant-scoped in repository queries
- Consistent dark theme styles across layout, navigations, cards, inputs, and alerting UI
- GroupRepository.get_all() optimized from 1+2N queries to 3 queries using batch loading
- RBAC get_all_roles() and get_user_roles() optimized from N+1 to 2 queries each
- Frontend bundle split into vendor chunks (react, query, charts, ui) for better caching

### Fixed
- `/api/v1/auth/*` endpoints no longer blocked by global auth middleware
- ESLint warning cleanup in Settings page (`no-console`)

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Security
- Rate limiting middleware (IP-based) with configurable limits for auth endpoints (stricter) and API endpoints (standard)
- Security headers middleware adding HSTS, CSP, X-Frame-Options, X-Content-Type-Options, X-XSS-Protection, Referrer-Policy, and Permissions-Policy
- API responses include Cache-Control headers to prevent caching of sensitive data
- TLS 1.3 as default minimum version (configurable via `server.tls.min_version`)
- ALPN support for HTTP/2 and HTTP/1.1 over TLS

---

## Version History

### [0.1.0] - TBD

#### Added
- Core Axum server implementation
- Authentication system (JWT)
- YAML configuration management
- Basic API routing
- Logging and tracing infrastructure
- Unit and integration test framework
- Docker development environment

---

## Release Notes Template

When releasing a new version, copy this template:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- New features

### Changed
- Changes in existing functionality

### Deprecated
- Soon-to-be removed features

### Removed
- Removed features

### Fixed
- Bug fixes

### Security
- Vulnerability fixes
```

---

## Links

[Unreleased]: https://github.com/openvoxproject/openvox-webui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.1.0
