---
ptf: 1
project: openvox-webui
---

# Roadmap

OpenVox WebUI is a web interface for managing and monitoring OpenVox infrastructure. It provides:

- Real-time infrastructure monitoring and visualization
- Node classification using dynamic groups (similar to Puppet Enterprise)
- PuppetDB querying with graphical dashboards
- Facter-based node management and classification
- Configuration management through YAML files
- Role-based access control for secure multi-user environments

Detailed documentation for implemented features is in [docs/implemented-features/](docs/implemented-features/). Reference material that used to live in this file (key features, roles and permissions, API reference, installation, development guidelines, version history, references) is now in [docs/PROJECT_OVERVIEW.md](docs/PROJECT_OVERVIEW.md). The format of this file and `CHANGELOG.md` is defined by the Project Tracking Format (PTF) v1.

Status: Phases 1-9 are complete and deployed (M1-M11); Phase 10 (application inventory and update management) is in progress (M12-M17); Phase 11 (plugin platform) is proposed, design only (M18-M31).

## [M1] Foundation
> outcome: The project builds, runs, has its RBAC foundation in place and is covered by an automated test setup.
> phase: P1
> spec: S1, S2, S3, S4

- [x] T1 Set up project structure and tooling (S1)
- [x] T2 Build core backend architecture (S2)
- [x] T3 Add RBAC foundation (S3)
- [x] T4 Add testing infrastructure (S4)

## [M2] Authentication and Authorization
> outcome: Users can register and sign in, and every protected resource is guarded by role-based permissions managed through tools and an API.
> phase: P1
> spec: S5, S6, S7

- [x] T5 Add user management and JWT authentication (S5)
- [x] T6 Implement RBAC with roles and permissions (S6)
- [x] T7 Add RBAC management tools and API (S7)

## [M3] Infrastructure Integration
> outcome: The application reads PuppetDB data through a cache and manages the Puppet CA through its own API.
> phase: P1
> spec: S8, S9, S10, S11, S12

- [x] T8 Integrate PuppetDB (S8)
- [x] T9 Add data caching layer (S9)
- [x] T10 Expose PuppetDB API endpoints (S10)
- [x] T11 Add Puppet CA management backend (S11)
- [x] T12 Add CA API endpoints (S12)

## [M4] Node Classification
> outcome: Nodes are classified into hierarchical groups by fact-based rules and pinning, and Puppet agents can fetch their classification.
> phase: P1
> spec: S13

- [x] T13 Add rule-based node classification (S13)

## [M5] Facter Integration
> outcome: Facts can be generated from custom templates and exported in several formats.
> phase: P1
> spec: S14

- [x] T14 Add Facter integration and external fact generation (S14)

## [M6] Dashboard and Visualization
> outcome: Operators see fleet health and node status in a real-time dashboard with visualizations.
> phase: P1
> spec: S15

- [x] T15 Add dashboard and visualizations (S15)

## [M7] Configuration Management
> outcome: Application configuration is managed through YAML files and a validated settings UI with versioning and import/export.
> phase: P1
> spec: S16

- [x] T16 Add YAML configuration management with settings UI (S16)

## [M8] CA Management UI
> outcome: Certificate requests and certificates can be reviewed and managed from the web UI.
> phase: P1
> spec: S17

- [x] T17 Add CA management UI (S17)

## [M9] Advanced Features
> outcome: Reporting, alerting, multi-tenancy, API keys and audit logging are available.
> phase: P1
> spec: S18

- [x] T18 Add reporting with compliance and drift baselines (S18)
- [x] T19 Add alerting with notification channels (S18, S24)
- [x] T20 Add multi-tenancy and API keys (S18)
- [x] T21 Add audit logging (S18)

## [M10] Production Readiness
> outcome: The application is performant, hardened, installable from native packages and a Puppet module, and fully documented.
> phase: P1
> spec: S19

- [x] T22 Optimize performance (S19)
- [x] T23 Harden security (S19)
- [x] T24 Build RPM and DEB packages (S19)
- [x] T25 Add Puppet module for automated deployment (S19)
- [x] T26 Write production documentation suite (S19)

## [M11] Post-launch Enhancements
> outcome: Operational features added after the original nine phases (agent bootstrap, Windows agents, code deploy, CVE detection, scalability and self-protection) are released.
> phase: P1
> note: Added at migration. These features shipped in releases 0.20.0 to 0.41.6 and were not part of the original phase plan.

- [x] T27 Bootstrap Linux agents with a curl command
- [x] T28 Bootstrap Windows agents with PowerShell
- [x] T29 Delete nodes with RBAC permission
- [x] T30 Deploy code with r10k from Code Deploy
- [x] T31 Authenticate classification with mTLS or shared key
- [x] T32 Add self-healing ENC watchdog
- [x] T33 Pre-aggregate report summaries for the dashboard
- [x] T34 Paginate nodes server-side with fleet-wide stats
- [x] T35 Report machine variable assignments
- [x] T36 Track auth sessions and enforce idle timeout
- [x] T37 Add memory monitor with restart on exhaustion
- [x] T38 Support Windows agents in the Puppet module
- [x] T39 Add compliance and drift baseline editors
- [x] T40 Detect CVE vulnerabilities from NVD and CISA KEV
- [x] T41 Alert on update-job failures with configurable max runtime

## [M12] Inventory Schema and Transport
> outcome: Agents submit inventory to an authenticated ingestion API that stores it in a dedicated database.
> version: 0.28.0
> phase: P2
> spec: S20

- [x] T42 Define inventory schema and ingestion API (S20)
- [x] T43 Add inventory submission to the Puppet module (S20)
- [x] T44 Store inventory in a dedicated SQLite database (S20)

## [M13] OS-Specific Collectors
> outcome: Linux, Windows and macOS nodes report packages, applications, websites, runtimes, containers and users.
> version: 0.28.0
> phase: P2
> spec: S20

- [x] T45 Collect Linux RPM and DEB package inventory (S20)
- [x] T46 Collect Windows applications and IIS inventory (S20)
- [x] T47 Collect macOS app bundles and Homebrew inventory (S20)
- [x] T48 Discover Apache, NGINX, Tomcat and JBoss deployments (S20)
- [x] T49 Collect container and local user inventory (S20)

## [M14] Node Inventory UI
> outcome: Operators can browse, search and filter a node's inventory and its history on the node detail page.
> version: 0.28.0
> phase: P2
> spec: S20

- [x] T50 Add node inventory cards and detail sections (S20)
- [x] T51 Add search, filter and history views per node (S20)

## [M15] Version Intelligence and Scheduled Jobs
> outcome: The server knows the latest available versions per platform, flags outdated software and detects stale inventory on a schedule.
> version: 0.28.0
> phase: P2
> spec: S20

- [x] T52 Sync repository metadata for version catalogs (S20)
- [x] T53 Compare installed and latest versions per node (S20)
- [x] T54 Schedule inventory jobs and detect stale inventory (S20)

## [M16] Fleet Statistics and Compliance Reporting
> outcome: A separate inventory and update dashboard shows updated versus outdated nodes, patch aging, coverage and drift.
> version: 0.28.0
> phase: P2
> spec: S20

- [x] T55 Add inventory and update dashboard (S20)
- [x] T56 Report updated versus outdated statistics (S20)
- [x] T57 Report patch aging, coverage and drift metrics (S20)

## [M17] Update Orchestration
> outcome: Operators can safely run single-node and bulk updates with scheduling, approvals, execution tracking, audit trails and RBAC controls.
> phase: P2
> spec: S20
> note: Old roadmap status "In Progress" (Phase 10.6); foundations shipped in 0.28.0 and the feature set grew through 0.41.x.

- [x] T58 Update single nodes (S20)
- [x] T59 Update node groups in bulk (S20)
- [x] T60 Schedule updates with approval workflow (S20)
- [x] T61 Track update execution and results (S20)
- [x] T62 Audit update requests, approvals and outcomes (S20)
- [ ] T63 Enforce maintenance windows and concurrency limits (S20)
- [ ] T64 Provide rollback guidance for update jobs (S20)

## [M18] Plugin PoCs
> outcome: Proofs of concept show that per-plugin storage limits, wasmtime resource limits and package size impact are acceptable.
> phase: P3
> spec: S21, S23

- [ ] T65 Prove per-plugin SQLite storage with enforced limits (S21)
- [ ] T66 Prove wasmtime fuel and memory limits inside Axum (S21)
- [ ] T67 Measure build and binary size impact on RPM and DEB packages (S21)

## [M19] Plugin Manager and Signatures
> outcome: Administrators can install signed plugins and manage trusted keys, and unsigned or untrusted plugins can never be installed or loaded.
> phase: P3
> spec: S21, S23

- [ ] T68 Define plugin manifest and .ovxp package (S21)
- [ ] T69 Verify plugin signatures with ed25519 (S21)
- [ ] T70 Manage trusted keys through API and UI (S21)
- [ ] T71 Add plugin admin API and Plugins page (S21)
- [ ] T72 Add built-in runtime and slot registry for menus and pages (S21)

## [M20] WASM Runtime and Host API
> outcome: WASM plugins run sandboxed with approved capabilities, own storage and enforced resource limits.
> phase: P3
> spec: S21, S23

- [ ] T73 Add WASM runtime and host API (S21)
- [ ] T74 Add capability gate and approval flow (S21)
- [ ] T75 Provide kv and db:own storage with migrations (S21)
- [ ] T76 Add lifecycle hooks and resource limits (S21)

## [M21] Events, Jobs and Webhooks
> outcome: Plugins react to events, run scheduled jobs and exchange signed webhooks.
> phase: P3
> spec: S21, S23

- [ ] T77 Add event bus with outbox and dispatcher (S21)
- [ ] T78 Add scheduled plugin jobs (S21)
- [ ] T79 Add inbound and outbound webhooks (S21)

## [M22] Telemetry and Governance
> outcome: Administrators can monitor plugins, and a failing or abusive plugin is isolated by quotas and a circuit breaker.
> phase: P3
> spec: S21, S23

- [ ] T80 Add plugin metrics and quotas (S21)
- [ ] T81 Add circuit breaker and administrator dashboard (S21)
- [ ] T82 Expose Prometheus metrics endpoint (S21)

## [M23] Plugin UI and SDK
> outcome: Plugins contribute sandboxed UI through the full slot catalog and a TypeScript SDK.
> phase: P3
> spec: S21, S23

- [ ] T83 Add sandboxed iframe pages (S21)
- [ ] T84 Add TypeScript plugin SDK (S21)
- [ ] T85 Add theme tokens and complete slot catalog (S21)

## [M24] Updates and Rollback
> outcome: Plugins can be upgraded in batch or on schedule, rolled back and locally revoked.
> phase: P3
> spec: S21, S23

- [ ] T86 Add batch and scheduled plugin updates (S21)
- [ ] T87 Store plugin versions and support rollback (S21)
- [ ] T88 Add local plugin revocation (S21)

## [M25] Facts and Reports
> outcome: Plugins provide facts and report types, exports, annotations and datasets.
> phase: P3
> spec: S21, S23

- [ ] T89 Add plugin facts with merged view and opt-in agent export (S21)
- [ ] T90 Add plugin report types, export formats, annotations and datasets (S21)

## [M26] Advanced Classification (core)
> outcome: Classification supports expressions, group behavior policies and explainable results while existing BDD classification scenarios pass unchanged.
> phase: P3
> spec: S21, S23
> note: Independent of the plugin runtime and can start earlier.

- [ ] T91 Add expression engine for classification rules (S21)
- [ ] T92 Add group behavior policies (S21)
- [ ] T93 Add explain, provenance and impact preview (S21)

## [M27] Classification Extensions for Plugins
> outcome: Plugins extend classification with operators, membership providers and generated groups without destabilizing the hot path.
> phase: P3
> spec: S21, S23

- [ ] T94 Add pure operators, functions and membership providers (S21)
- [ ] T95 Add dynamic and generated groups with churn guard (S21)
- [ ] T96 Add merge strategies and safe degradation (S21)

## [M28] Secret Providers
> outcome: Configuration can reference secrets through a resolver with env and file providers.
> phase: P3
> spec: S21, S22, S23

- [ ] T97 Add SecretResolver with env and file providers (S22)
- [ ] T98 Resolve secret:// references in configuration (S22)

## [M29] Authentication Pipeline
> outcome: MFA and SSO can be enforced by policy while the local break-glass administrator remains available.
> phase: P3
> spec: S21, S22, S23

- [ ] T99 Add challenge and step-up authentication pipeline (S22)
- [ ] T100 Add built-in TOTP, recovery codes and WebAuthn (S22)
- [ ] T101 Add OIDC and refactor local and SAML as providers (S22)
- [ ] T102 Add MFA policies (S22)

## [M30] Directory Providers
> outcome: Users authenticate and are provisioned from LDAP or Active Directory with group-to-role mapping.
> phase: P3
> spec: S21, S22, S23

- [ ] T103 Add LDAP and Active Directory provider (S22)

## [M31] Advanced Hooks
> outcome: Restricted interceptors, trusted module pages (if approved) and inbound SCIM are available.
> phase: P3
> spec: S21, S23

- [ ] T104 Add restricted interceptors (S21)
- [ ] T105 Add trusted module pages if approved (S21)
- [ ] T106 Add inbound SCIM (S21)

## [M32] Stable Production Release
> outcome: Version 1.0.0 is released as the stable production milestone.
> version: 1.0.0

- [ ] T107 Release stable 1.0.0

## Backlog
- [ ] T108 Add WebSocket real-time updates
- [ ] T109 Add GraphQL API support
- [ ] T110 Add mobile app support
- [ ] T111 Add internationalization
- [ ] T112 Add attribute-based access control
- [ ] T113 Integrate OpenBolt orchestration
- [>] T114 Add multi-tenant plugin scoping (S21)
  - from: M18
  - why: deferred out of the first plugin platform release; the design keeps it reachable
- [>] T115 Add plugin marketplace (S21)
  - from: M18
  - why: deferred out of the first plugin platform release; the design keeps it reachable

## Specs
| ID | Title | Path | Status |
|----|-------|------|--------|
| S1 | Project setup | docs/implemented-features/01-project-setup.md | approved |
| S2 | Core backend architecture | docs/implemented-features/02-core-backend.md | approved |
| S3 | RBAC foundation | docs/implemented-features/03-rbac-foundation.md | approved |
| S4 | Testing infrastructure | docs/implemented-features/04-testing-infrastructure.md | approved |
| S5 | User management | docs/implemented-features/05-user-management.md | approved |
| S6 | RBAC implementation | docs/implemented-features/06-rbac-implementation.md | approved |
| S7 | RBAC management tools and API | docs/implemented-features/07-rbac-management.md | approved |
| S8 | PuppetDB integration | docs/implemented-features/08-puppetdb-integration.md | approved |
| S9 | Data caching layer | docs/implemented-features/09-caching-layer.md | approved |
| S10 | PuppetDB API endpoints | docs/implemented-features/10-puppetdb-api.md | approved |
| S11 | Puppet CA management (backend) | docs/implemented-features/11-puppet-ca-backend.md | approved |
| S12 | CA API endpoints | docs/implemented-features/12-ca-api-endpoints.md | approved |
| S13 | Node classification | docs/implemented-features/13-node-classification.md | approved |
| S14 | Facter integration | docs/implemented-features/14-facter-integration.md | approved |
| S15 | Dashboard and visualization | docs/implemented-features/15-dashboard-visualization.md | approved |
| S16 | Configuration management | docs/implemented-features/16-configuration-management.md | approved |
| S17 | CA management UI | docs/implemented-features/17-ca-management-ui.md | approved |
| S18 | Reporting, alerting and multi-tenancy | docs/implemented-features/18-advanced-features.md | approved |
| S19 | Production readiness | docs/implemented-features/19-production-readiness.md | approved |
| S20 | Application inventory and update management plan | docs/architecture/INVENTORY_UPDATES.md | approved |
| S21 | Plugin platform design | docs/architecture/PLUGIN_PLATFORM.md | draft |
| S22 | Plugin system extension points | docs/architecture/PLUGIN_SYSTEM.md | draft |
| S23 | Plugin platform roadmap notes | docs/architecture/PLUGIN_PLATFORM_ROADMAP.md | draft |
| S24 | Alert rules conditions | docs/ALERT_RULES_CONDITIONS.md | approved |
| S25 | CA API reference | docs/api/ca.md | approved |

## Phases
| ID | Title | Outcome | Target |
|----|-------|---------|--------|
| P1 | Core platform | Foundation, authentication, integration, classification, facter, dashboard, configuration, CA, advanced features and production readiness are shipped (old Phases 1-9). |  |
| P2 | Inventory and update management | Operators inventory and update Windows, Linux and macOS nodes from one console (old Phase 10). |  |
| P3 | Plugin platform | Administrators extend the application with signed, sandboxed plugins (old Phase 11). |  |
