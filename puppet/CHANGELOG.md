# Changelog

All notable changes to this Puppet module will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Maintenance Instructions

**Important:** This changelog should be updated before every commit that introduces user-facing changes. When making changes:

1. Add your changes under the `[Unreleased]` section
2. Use the appropriate category: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`
3. When releasing a new version, move unreleased changes to a new version section with the release date

## [Unreleased]

### Changed
- **Deployment note:** The packaged OpenVox WebUI service now uses a dedicated SQLite database for inventory data (`/var/lib/openvox-webui/inventory.db`). On the first start of v0.33.0 the service migrates existing inventory rows out of the main DB automatically; inventory endpoints return 503 until that completes. A one-time background `VACUUM` reclaims space on the main DB after the migration.
- **systemd resource limits** raised: `MemoryHigh=3G`, `MemoryMax=6G` (was 1G/2G). `StartLimitIntervalSec` / `StartLimitBurst` moved under `[Unit]`.

### Added
- Add cancel button to Update Jobs UI for jobs in `pending_approval`, `approved`, or `in_progress` states
- Add `POST /api/v1/inventory/updates/{job_id}/cancel` endpoint to cancel update jobs
- Add inline compliance rule editor to create and edit compliance baselines, including add/remove rule controls and fact/operator/value configuration
- Add server-backed auth session tracking so idle timeout enforcement survives page refreshes and token reuse
- Add optional node group selection to drift detection baselines in create and edit modals

### Fixed
- **Package post-install:** When PuppetDB is on the same host, the configure script now uses the agent `certname` (or FQDN) for the PuppetDB URL instead of `localhost`, so TLS matches the Jetty certificate. A failed post-install connection check no longer aborts `dpkg`/`apt` (warning only).
- Fix login session persistence after browser refresh by waiting for auth store hydration before protected-route redirects
- Fix access token expiration handling by adding automatic refresh-token retry for authenticated API requests
- Fix intermittent WebUI freezes by hardening browser storage access, showing hydration loader state, and adding API request timeout defaults
- Fix SQLite contention spikes during inventory ingestion by serializing version-catalog refreshes and debouncing post-ingest refresh triggers
- Fix frontend lint and hook-order issues across Node Detail, Alerting, Login, Notifications, and auth/session store modules
- Fix report metrics over time chart by fetching a 30-day report data window (`since`) with a higher 10000-report limit and using linear interpolation to avoid misleading smoothing
- Fix hung/stale update jobs: dispatched targets that receive no agent response within 2 hours are automatically marked as failed and the parent job status is rolled up accordingly
- Fix compliance baseline modal action buttons to use the shared design-system button styles
- Fix missing idle session expiration by automatically logging users out after 30 minutes of inactivity in both the GUI and protected API session validation
- Fix drift baseline cards to show the selected group name instead of the raw group ID
- Fix drift baseline cards to show an explicit "All nodes" scope when no node group is selected

### Added
- Add edit functionality for compliance baselines (PUT endpoint + edit modal)
- Add help guide to the new compliance baseline modal with tips on naming, severity levels, and rules
- Add placeholder text in compliance baseline form fields for better UX

### Changed
- Restyle "New Baseline" button on Compliance tab to an outlined style for a more polished look
- Update baseline card action buttons to show edit (pencil) and delete icons side by side

### Added
- Add repository-based package version checking: nodes report their configured repository metadata (YUM/APT/Zypper/Winget), server periodically fetches latest package versions directly from repositories
- Add `fleet_repository_configs` and `node_repository_configs` database tables for storing repository metadata
- Add `RepoCheckerService` with YUM (repodata/primary.xml) and APT (Packages.gz) metadata parsing
- Add background repo checker scheduler with configurable interval (default: 24h)
- Add API endpoints: `GET /inventory/repositories` to list fleet repos, `POST /inventory/repositories/check` to trigger manual check
- Add facter collector methods for YUM, APT, Zypper, and Winget repository discovery
- Add `source_kind` field to `OutdatedInventoryItem` to distinguish "repo-checked" vs "fleet-observed" outdated determinations
- Add `winget` as default Windows package repository source
- Add "Update Schedules" tab to Node Groups for scheduling one-time and recurring (cron-based) update tasks with optional approval workflows
- Add `group_update_schedules` database table and full CRUD API (`GET/POST /groups/:id/update-schedules`, `PUT/DELETE /groups/:id/update-schedules/:scheduleId`, `POST .../run`)
- Add background update schedule scheduler that auto-creates UpdateJobs when schedules are due

### Fixed
- Fix Oracle Linux / RedHat inventory collector missing `Last Successful Update` when `dnf history` reports abbreviated transaction actions like `E, I, U`
- Detect last successful system update time from dnf/yum history, apt logs, and zypper history instead of always reporting "Never recorded"
- Fix false-positive outdated package detection when fleet has nodes on different OS major versions (e.g., el8 vs el9) by adding `os_version_pattern` dimension to the version catalog
- Fix Update Compliance donut chart legend showing "value" instead of proper category labels (Compliant/Outdated/Stale); always include all 3 compliance categories even when count is zero
- Fix RPM/DEB package build failure caused by missing `react-is` peer dependency required by `recharts` v3
- Fix inventory submission 422 error caused by container runtime entries missing the `image` field — make `image` optional in `HostContainerInventoryItem` so runtime-only entries (e.g. Docker Engine) are accepted
- Install base64 gem for r10k compatibility with Ruby 3.2 (puppet_forge requires >= 0.2.0, but Ruby 3.2 only ships 0.1.1)
- Fix dashboard stats grid layout: 5 status cards now fit on a single row instead of wrapping
- Fix Activity Heatmap showing "No activity data available" — now correctly parses PuppetDB raw metrics format (`{data: [...], href: "..."}`) in addition to pre-parsed format
- Fix outdated software "Affected Nodes" count inflated by duplicate package entries (e.g., gpg-pubkey) — now counts unique nodes per package
- Fix dashboard stats cards not summing to Total Nodes — added Warning category for stale nodes and aligned all cards to health-based classification
- Add openvox-webui user to puppet group in ENC manifest for r10k cache directory access
- Fix r10k deployments incorrectly marked as failed when killed by SIGSYS/SIGPIPE signal after successful completion

### Added
- Add Updates analytics dashboard tab under Analytics with fleet update metrics, compliance charts, patch age distribution, top outdated software, and update job history
- Add drill-down capabilities: click outdated software to see affected nodes with version details, click compliance categories to see node lists, expand update jobs to see per-target results
- Add backend endpoints for outdated software and compliance category drill-down queries
- Add Windows bootstrap support: PowerShell script served from `/api/v1/bootstrap/windows-script` that auto-discovers and installs the latest OpenVox Agent MSI from downloads.voxpupuli.org
- Add Windows bootstrap command section to Add Node page with copy-to-clipboard, non-interactive, and dry-run modes
- Add container inventory collection: detects Docker CE, Docker Enterprise, and Podman installations; collects all containers with status, image, ports, mounts, and runtime type
- Add user inventory collection: collects local system users with UID, groups, shell, home directory, lock status, and last login (Linux, macOS, and Windows)
- Add containers and users tabs to node detail inventory view with filtering and search
- Add drill-down modals to Dashboard Inventory Compliance cards and pie chart — click any stat to see affected nodes
- Add drill-down to Dashboard Top Outdated Software — click any package to see affected nodes with version details
- Inventory client now polls for and executes pending update jobs (system patch, security patch, package operations)
- Version catalog and update statuses now refresh automatically after inventory submission

### Fixed
- Inventory now collects all packages instead of capping at 500 items per category (default raised to 10,000)
- Removed frontend display cap of 250 items for packages and applications; now uses progressive loading
- Update jobs no longer stay stuck in "queued" status — client executes them on next Puppet run
- Version catalog now populates without requiring `inventory.enabled: true` in server config

### Changed
- `inventory_max_items` parameter default raised from 500 to 10,000 (upper bound from 5,000 to 50,000)

### Fixed
- r10k deployments now default to `pool_size: 1` to work around Ruby 3.2 segfault in `File.chown` under multithreaded execution
  - New `r10k_pool_size` config option and `code_deploy_r10k_pool_size` Puppet parameter
  - Includes `pool_size` in generated `r10k.yaml`
- Removed invalid `--pool-size` CLI argument from r10k invocations (not a valid r10k CLI option)
- Fixed intermittent r10k deployment corruption leaving partial modules (e.g., only CHANGELOG.md)
  - Timeout now sends SIGTERM (graceful) before escalating to SIGKILL after 30s, preventing mid-file-write kills
  - Signal-terminated r10k processes are no longer falsely marked as "successful"
  - Set explicit working directory (`/tmp`) for spawned r10k process to prevent Ruby `getcwd` errors
  - Added `purge_allowlist` to generated `r10k.yaml` to protect deployment marker files

### Added
- Updates management page with 4 tabs: Update Status, Update Jobs, Version Catalog, and Vulnerabilities
- Update dispatcher with 3 scope options: All Updates, Selected Packages, and Security Updates Only
- CVE vulnerability detection with NVD 2.0 and CISA KEV feed integration
- Configurable CVE feed sources in Settings with add/edit/delete/sync controls
- Vulnerability dashboard on Dashboard page showing severity distribution and affected nodes
- Node-level vulnerability warnings in Inventory tab with expandable CVE details table
- Background CVE feed sync and vulnerability match scheduler with configurable intervals
- `SecurityPatch` update operation type that auto-resolves vulnerable packages from CVE matches
- Dry-run preview endpoint for update jobs showing per-node package changes
- `Vulnerability` alert rule type with notifications for critical/KEV matches
- Phase 10.2 Puppet-side inventory collectors for Linux, Windows, and macOS with package/application, website, and runtime discovery
- `openvox_inventory_status` fact for inventory collection status and submission summary
- `openvox_webui::client` options to enable inventory collection, control submission, and cap collected item counts
- Phase 10.1 inventory backend foundation with authenticated node inventory submission endpoint and node inventory/history APIs for the WebUI
- Environment group feature for node groups that assign environments instead of filtering by them
- Match All Nodes option for node groups to control behavior when no rules are defined
- Shared key authentication option for classification endpoint (debug mode)
  - Configure via `classification.shared_key` in config.yaml or `CLASSIFICATION_SHARED_KEY` env var
  - Facter client supports `classification_key` config option
  - Facter client supports `auto_generate_classification_key` to auto-generate and persist key
- New unauthenticated `/api/v1/nodes/:certname/environment` endpoint for early environment detection
  - Returns only the environment assignment (no sensitive data)
  - Allows Puppet agents to determine their environment before certificates are available
- ENC script now supports `classification_key` parameter for shared key authentication
- ENC script now falls back to unauthenticated `/environment` endpoint when `/classify` auth fails
- Main `openvox_webui` class now accepts `classification_key` parameter
- `openvox_webui::client` class now supports `classification_key` parameter for shared key authentication

### Changed

- `openvox_environment` fact now uses dedicated `/environment` endpoint (no authentication required)
- Group permission checks now use async database queries for better performance and consistency
- Frontend lint toolchain now uses ESLint 10 with updated TypeScript and React plugin dependencies

### Fixed
- Inventory collector normalization now works on older Ruby runtimes without `filter_map`

### Security
- Classification endpoint (`/api/v1/nodes/:certname/classify`) now requires client certificate authentication (mTLS)
- Added optional shared key authentication as alternative to mTLS for debugging purposes

- Pinned nodes now correctly match their group even when parent groups don't match via rules
- Child groups now only match nodes that also match their parent group (for non-pinned nodes)
- Bootstrap script now correctly uses Vox Pupuli release packages from apt.voxpupuli.org with manual configuration fallback
- Bootstrap script now detects and works around broken Vox Pupuli release packages that create invalid APT sources
- Bootstrap script now cleans up existing broken openvox repository configurations before attempting fresh install
- Bootstrap script now disables automatic service restarts during package installation (needrestart)
- Classification rule inline editing now keeps the current rule values visible when opening the edit form

## [0.22.0] - 2025-01-15

### Added
- Node deletion feature with RBAC permission (`nodes:delete`)
- Delete button on node detail page with confirmation dialog
- Node deletion removes pinned associations, revokes certificate, and deactivates in PuppetDB

### Fixed
- Parent group dropdown in create dialog now correctly shows all available groups
- Classification inheritance for child groups without rules now works correctly

## [0.21.1] - 2025-01-14

### Added
- Node bootstrap feature for adding new Puppet agents via curl command
- Bootstrap script with OS detection for RHEL/Debian-based systems

### Changed
- Renamed `puppet_server_url` to `openvox_server_url` in bootstrap configuration

### Fixed
- Include scripts directory in Docker package builds

## [0.21.0] - 2025-01-13

### Fixed
- Include documentation, puppet, and tests directories in Docker package builds
- Make run-scheduled-reports binary required in packages
- Align Facts Explorer search input style with Nodes page

## [0.20.0] - 2025-01-12

### Added
- Deployment cancel feature with process kill support
- Incremental Docker builds for faster package creation

## [0.1.0] - 2024-12-16

### Added
- Initial release
- Package installation for RPM and DEB systems
- Systemd service management
- Configuration file templating
- PuppetDB connection support with SSL
- Hiera integration for all parameters
- Initial admin account setup
- Support for RHEL 8/9, CentOS 8/9, Rocky 8/9, AlmaLinux 8/9
- Support for Fedora 38/39/40
- Support for Debian 11/12
- Support for Ubuntu 22.04/24.04
