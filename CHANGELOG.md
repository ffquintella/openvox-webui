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

### Changed
- Rust clippy lint fixes across config, middleware, models, and services
- API routing split into public vs protected routes; auth middleware applies only to protected routes
- Groups, fact templates, and users are tenant-scoped in repository queries
- Consistent dark theme styles across layout, navigations, cards, inputs, and alerting UI

### Fixed
- `/api/v1/auth/*` endpoints no longer blocked by global auth middleware
- ESLint warning cleanup in Settings page (`no-console`)

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Security
- Nothing yet

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
