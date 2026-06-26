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

## [0.37.13] - 2026-06-26

### Fixed
- Repaired the backend test suite: alert rule integration tests now seed an active auth session so they pass the session-validation middleware, and the `require_permission_middleware` doctest was updated to match the current `AppState`/`AppConfig` shape.
- Pinned `samael` back to 0.0.19; 0.0.21 fails to compile without the `xmlsec` feature (its `quick_xml` imports are feature-gated while the code uses them unconditionally), breaking the default build.

## [0.37.12] - 2026-06-26

### Fixed
- Dashboard status overview cards and the Node Status Distribution chart were capped at 100 nodes because they were computed from the (paginated) node list. They now use fleet-wide server-side aggregates so the totals match the real node count on large deployments. (#150)

### Added
- `GET /api/v1/nodes/stats` now also returns a `by_health` breakdown (healthy/warning/critical/unknown) computed fleet-wide by PuppetDB, mirroring the dashboard's per-node health logic.

## [0.37.11] - 2026-06-18

### Added
- `pagination_default_limit` and `pagination_max_limit` class parameters that render the `pagination` section of `config.yaml`, allowing the default and maximum page size for the `/nodes` and `/facts` endpoints to be managed via Puppet/Hiera.

## [0.37.10] - 2026-06-18

### Added
- Server-side pagination for the Nodes page with Previous/Next controls. The list now shows the true fleet-wide node count (via a new `X-Total-Count` response header) instead of being silently capped at 100, and search/status filtering is performed by PuppetDB rather than over a truncated client-side list. (#142)
- `GET /api/v1/nodes/stats` endpoint returning aggregate node counts (total, by status, by environment) computed by PuppetDB. The Analytics overview now uses it so the "Total Nodes" and "Environments" cards stay accurate and cheap regardless of fleet size, instead of deriving counts from a 100-node sample.
- `pagination` configuration section (`default_limit`, `max_limit`) to override the default and maximum page size for the `/nodes` and `/facts` endpoints.

### Changed
- The `/nodes` and `/facts` list endpoints now use the configurable `pagination.default_limit` (default 100) and clamp client-requested `limit` values to `pagination.max_limit` (default 5000), keeping responses bounded while allowing larger fleets to be queried.

## [0.37.9] - 2026-06-16

### Fixed
- Code Deploy "Failed to fetch from remote" over HTTPS (completes the 0.37.8 fix). `git2` 0.21 ships `default = []`, so the `https` and `ssh` transports were never compiled into libgit2 after the 0.20→0.21 bump, producing `there is no TLS stream available; class=Ssl (16)` on every HTTPS fetch. The `git2` dependency now explicitly enables the `https` and `ssh` features (alongside `vendored-libgit2`/`vendored-openssl`), so libgit2 is built with the OpenSSL TLS and SSH transports. The RPM/DEB builder images also install `perl-core`, required to compile the vendored OpenSSL from source.

## [0.37.8] - 2026-06-16

### Fixed
- Code Deploy "Failed to fetch from remote" over HTTPS. The release binary's bundled libgit2 was compiled without a TLS backend (`there is no TLS stream available; class=Ssl (16)`), so every HTTPS fetch failed before authentication regardless of a valid PAT or certificate. `git2` now builds with `vendored-libgit2` and `vendored-openssl`, guaranteeing an HTTPS-capable libgit2 on every build target. (Incomplete — the `https`/`ssh` transports still needed to be enabled; see 0.37.9.)

## [0.37.7] - 2026-06-15

### Fixed
- Alert rules with simple-format conditions (e.g. Node Status connection state, Compliance status, Report Failure status) never matched and never triggered. The field lookup split keys like `node.status` on the dot and looked for a nested object, but the evaluation context stores them as flat keys, so the field value always resolved to nothing. Field lookups now match the literal key first before falling back to nested traversal.

## [0.37.6] - 2026-06-15

### Added
- Patch Age chart on the dashboard is now interactive: clicking a bucket opens a drill-down listing the nodes in that bucket, with each node's age and last-patched time.
- Node Groups tree remembers which groups are expanded or collapsed across visits (stored in a cookie), and the "All Groups" header has collapse-all / expand-all controls.

### Fixed
- "Evaluate Rules" on the Alerting page now shows feedback (a result banner reporting how many alerts were triggered, a hint when there are no enabled rules, and an error message on failure) instead of silently doing nothing.
- Restored a compiling backend after the `sqlx` 0.8.6 → 0.9.0 and `git2` 0.20 → 0.21 dependency bumps: dynamic SQL strings are now wrapped with `AssertSqlSafe`, and git2 accessors that changed from `Option` to `Result` (`Remote::url`, `Signature::name`/`email`, `Commit::message`, `Reference::shorthand`) are handled with `.ok()`.

## [0.37.5] - 2026-06-15

### Changed
- When creating a new node group while a group is selected, the "Parent Group" field is now pre-populated with the selected group.

## [0.37.4] - 2026-05-29

### Security
- Removed the deprecated `X-XSS-Protection` response header. Modern browsers ignore it (Chrome/Edge removed the XSS Auditor and Firefox never implemented it), and the legacy filter could itself be abused; XSS protection is provided by the Content-Security-Policy and React's default output escaping.

## [0.37.3] - 2026-05-21

### Fixed
- Activity Heatmap rendered all white because Tailwind's JIT compiler purged the `bg-success-*` / `bg-warning-*` / `bg-danger-*` classes that were only referenced via dynamic string interpolation inside `getColorIntensity`. Switched the cells and the legend to inline `backgroundColor` styles with an explicit color ramp, and gave cells a `min-w-[12px]` floor so a sparse grid is still legible.

## [0.37.2] - 2026-05-21

### Added
- **Hourly report summary table** (`report_hourly_summary`) with two new endpoints — `GET /api/v1/reports/hourly-summary?hours=N` and `GET /api/v1/reports/activity-heatmap?days=N`. Analytics "Report Metrics Over Time" (24h/7d/30d) and the Activity Heatmap now read from these pre-aggregated tables instead of fetching up to 10,000 reports per page load.

### Changed
- Report summary scheduler now refreshes both daily and hourly tables in a single PQL fetch per cycle (one `reports[end_time, status]` projection over the rolling 31-day window) instead of issuing one count query per (day, status). Cuts the per-cycle PuppetDB roundtrip count by ~56× and unblocks the heatmap pipeline.

## [0.37.1] - 2026-05-21

### Fixed
- Replace deprecated `quick_xml::Attribute::unescape_value` with `normalized_value(XmlVersion::Implicit1_0)` in the repository checker to silence quick-xml 0.40 deprecation warnings.

## [0.37.0] - 2026-05-21

### Added
- **Pre-aggregated daily report summary** (`report_daily_summary` table) populated hourly from PuppetDB. The Dashboard's "Weekly Activity Trend" chart now reads from a new `GET /api/v1/reports/daily-summary` endpoint instead of fetching up to 5000 reports per page load, so the chart populates instantly and no longer drops days when PuppetDB scans time out.
- **Inventory prune for inactive nodes.** The inventory maintenance scheduler now deletes inventory rows (snapshots, packages, applications, web/runtime/container/users, update-status, repo configs) for certnames that are no longer active in PuppetDB. Fixes the dashboard mismatch where "Inventory Coverage / reporting nodes" exceeded "Total Nodes" because deactivated/expired hosts lingered in the local inventory DB.
- **Self-healing ENC watchdog.** `openvox_webui::enc` now deploys `/opt/openvox/enc-watchdog.sh` plus a systemd timer (`openvox-enc-watchdog.timer`) that runs every 5 minutes. It probes the ENC end-to-end and recovers from two failure modes seen in production: (a) corrupted/missing script — restored from a sibling `${enc_script_path}.template` managed by the same class, and (b) puppetserver JVM wedge ("Cannot run program ... error=2") — recovered via `systemctl restart puppetserver`. New parameters: `enable_watchdog`, `puppetserver_service_name`, `puppet_user`, `watchdog_allow_restart`, `watchdog_journal_lookback_min`. Disable with `enable_watchdog => false` if you have external monitoring covering the same failure modes.

### Changed
- **Deployment note:** The packaged OpenVox WebUI service now uses a dedicated SQLite database for inventory data (`/var/lib/openvox-webui/inventory.db`). On the first start of v0.33.0 the service migrates existing inventory rows out of the main DB automatically; inventory endpoints return 503 until that completes. A one-time background `VACUUM` reclaims space on the main DB after the migration.
- **systemd resource limits** raised: `MemoryHigh=3G`, `MemoryMax=6G` (was 1G/2G). `StartLimitIntervalSec` / `StartLimitBurst` moved under `[Unit]`.

### Added
- Add cancel button to Update Jobs UI for jobs in `pending_approval`, `approved`, or `in_progress` states
- Add `POST /api/v1/inventory/updates/{job_id}/cancel` endpoint to cancel update jobs
- Add inline compliance rule editor to create and edit compliance baselines, including add/remove rule controls and fact/operator/value configuration
- Add server-backed auth session tracking so idle timeout enforcement survives page refreshes and token reuse
- Add optional node group selection to drift detection baselines in create and edit modals
- Add `classification.disable_authentication` config flag (default `false`) to disable authentication checks only for public node environment/classification endpoints

### Fixed
- Fix node deletion authorization flow: users with `nodes:delete` permissions assigned through RBAC roles can now delete nodes, the Delete button is disabled when permission is missing, and 403 errors now show a clear authorization message
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
