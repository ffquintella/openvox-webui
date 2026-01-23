# Changelog

All notable changes to OpenVox WebUI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Global SMTP Configuration:** New settings table with key-value structure for flexible application settings
- **SMTP Settings UI:** New "Email/SMTP" tab in Admin Settings for configuring SMTP server (host, port, username, password, sender, TLS)
- **Simplified Email Channels:** Email notification channels now only require recipient email address
- SMTP settings API endpoints (GET/PUT /api/v1/settings/smtp)
- Settings repository for managing key-value application settings
- Automatic SMTP configuration injection when creating email channels
- **Warning alerts** in Alerting page when no notification channels are configured
- Disabled "New Rule" button when no channels are configured (with warning tooltip)

### Changed
- **Email Channel Architecture:** Separated email recipient (channel-specific) from SMTP configuration (system-wide in Admin Settings)
- Email channels config now only contains recipient list, SMTP settings loaded from global configuration
- CreateChannelRequest now accepts partial configs to support simpler email channel creation

### Removed
- **Default notification channels** via new migration - system removes pre-configured system-email and system-webhook channels that were non-functional

### Fixed
- Fixed Rust compilation errors in evaluate_condition function - properly handle optional AlertCondition fields (field and value)
- **Improved Alert Rules UX:** Added ability to add/remove multiple conditions easily with dedicated Add/Remove buttons
- **Fixed Notification Channel Selection:** Channels can now be selected independently without affecting other channels (added unique IDs and stopPropagation)
- **Standardized Condition Inputs:** Both simple and advanced modes now use select boxes for type/field and operator selections for better consistency and usability
- **Added Condition Validation:** Required fields validation with detailed error messages showing which fields are missing in each condition
- **Fixed condition_operator Type Mismatch:** Changed from 'AND'/'OR' to 'all'/'any' to match backend expectations (was causing 422 errors)
- **Improved Dropdown Labels:** Alert rule type dropdown now shows descriptive text explaining each type
- **Better Value Placeholder:** Value field now shows examples (e.g., failed, 24, 2024-01-22)
- **Enhanced Combo Box Labels:** All dropdowns now show descriptive labels with explanations for better user understanding
  - Field dropdown: Shows field purpose (e.g., "Node Status - Node connection state")
  - Operator dropdown: Shows symbols and descriptions (e.g., "= (equals) - Exact match")
  - Type dropdown: Explains each condition type (e.g., "Last Report Time - Detect stale nodes")
- **Added Advanced Mode Help:** Comprehensive help section in advanced mode explaining how to fill config JSON with examples
- **Improved Notification Channel Creation:** Dynamic labels and hints for channel types
  - Label changes based on channel type (e.g., "SMTP URL" for Email, "Slack Webhook URL" for Slack)
  - Specific placeholder examples for each channel type
  - Error handling with user-friendly messages
  - Type dropdown now shows channel descriptions
  - **Email Channel:** Now accepts recipient email address instead of SMTP URL (SMTP server configured in Admin Settings)

### Added
- **Enhanced Alert Rules Conditions System:** Backend now supports both simple and advanced condition formats
  - Advanced format with condition types: NodeStatus, NodeFact, ReportMetric, EnvironmentFilter, GroupFilter, NodeCountThreshold, TimeWindowFilter, LastReportTime, ConsecutiveFailures, ConsecutiveChanges, ClassChangeFrequency
  - Frontend toggle to switch between simple (field/operator/value) and advanced format
  - Backend automatically handles both formats during deserialization

### Changed
- **Alert Rules Conditions System:** Comprehensive condition evaluation engine for alert rules
  - Condition types: NodeStatus, NodeFact, ReportMetric, EnvironmentFilter, GroupFilter, NodeCountThreshold, TimeWindowFilter, LastReportTime, ConsecutiveFailures, ConsecutiveChanges, ClassChangeFrequency
  - Condition operators: `=`, `!=`, `~`, `!~`, `>`, `>=`, `<`, `<=`, `in`, `not_in`, `exists`, `not_exists`, `contains`, `not_contains`
  - Logical operators: AND, OR for combining multiple conditions
  - Data types: String, Integer, Float, Boolean with appropriate operators
  - RuleEvaluator implementation for background rule evaluation (5 minute intervals)
  - Comprehensive documentation in [docs/ALERT_RULES_CONDITIONS.md](docs/ALERT_RULES_CONDITIONS.md)
  - API endpoints: `/api/v1/alerting/rules/:id/test` for rule testing
  - Database schema for alert rules, conditions, and triggers
  - **Frontend Condition Builder UI:** Interactive condition editor with full support for all 11 condition types
    - Visual condition builder component with add/remove/duplicate functionality
    - Context-sensitive input fields based on condition type
    - Support for AND/OR logical operators between conditions
    - Real-time validation and summary of conditions
    - Responsive design with dark mode support
    - Individual enable/disable toggle for each condition
    - TypeScript types updated to match backend condition structure
    - Integration with alert rule creation and editing modals
  
- **Advanced Alerting Conditions:** Extended conditions for infrastructure health monitoring
  - **LastReportTime:** Detect stale nodes (haven't reported in N hours)
  - **ConsecutiveFailures:** Identify unstable nodes (N failures in X hours)
  - **ConsecutiveChanges:** Alert on excessive resource changes (N changes in X hours)
  - **ClassChangeFrequency:** Monitor class churn (class changed N+ times in X hours)
  - Evaluation functions for computing consecutive metrics from report history
  - Performance optimized with report caching and query batching
  - Common alert scenarios documented with real-world examples
  - **Integration tests** for alert condition evaluation with 4 test cases covering matches and non-matches
  - **Test factory functions** for creating test data (nodes, reports, consecutive metrics)

### Changed
- Build scripts: use `docker buildx --load` for RPM/DEB builders to avoid container-driver EOF issues on Docker Desktop
- **Alerting Documentation:** Enhanced with complete condition structure and evaluation logic
  - Added 11 condition types covering infrastructure monitoring scenarios
  - Added 7 examples from simple to complex multi-condition rules
  - Updated API endpoints to include rule testing and manual evaluation
  - Added database schema documentation
  - Added comprehensive best practices and common scenarios guide

### Details
  - Created feature-specific documents for all 9 phases (19 feature files)
  - Added [docs/implemented-features/README.md](docs/implemented-features/README.md) with complete index and navigation
  - Compacted ROADMAP.md to provide high-level overview with links to detailed docs
  - Features are now organized by phase with quick reference summary

### Details
The ROADMAP.md file has been reorganized for better maintainability:
- **Old structure:** Single 1100+ line document with extensive implementation details
- **New structure:** Quick reference guide (300+ lines) with links to 19 detailed feature documents
- Each feature document includes: completed tasks, detailed implementation, API endpoints, key files
- Detailed documentation is maintained separately in `docs/implemented-features/` directory
- This allows for easier navigation, faster documentation loading, and better organization of content
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
