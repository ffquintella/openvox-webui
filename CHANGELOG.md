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
- Classification service hierarchy inheritance tests
 - Classification engine (Phase 4.1): rule evaluation, operators coverage, hierarchical inheritance
 - Enabled BDD `node_classification.feature` scenarios
 - Cucumber scenarios for Puppet CA operations (status, requests, sign/reject/revoke, renew)

### Changed
- Rust clippy lint fixes across config, middleware, models, and services
- Auth middleware now applied globally via ServiceBuilder in main router to ensure all protected routes have access to authentication context

### Fixed
- 401 Unauthorized on protected auth endpoints (change-password, me) - auth middleware now properly applied to all routes via ServiceBuilder

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Fixed
- ESLint warning cleanup in Settings page (`no-console`)

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
