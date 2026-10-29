---
ptf: 1
project: openvox-webui
---

# Changelog

All notable changes to OpenVox WebUI are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) with the Project Tracking Format (PTF) v1 extensions (`Postponed` and `Abandoned` categories, task and milestone references), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintenance: add every user-facing change under `## [Unreleased]` before committing, and update `ROADMAP.md` in the same commit; when releasing, move the unreleased entries to a new version section with the release date.

## [Unreleased]

### Added
- Add plugin UI and SDK (M23): plugin pages and panels declared with `render: iframe` load from the verified package in a sandboxed iframe (`allow-scripts allow-forms`, no same-origin, no cookies or token) from `/api/v1/plugins/ui-assets/<token>/...`, an unguessable expiring path token created by `POST /api/v1/plugins/ui/sessions`; every response carries a CSP with a `sandbox` directive and `connect-src` limited to the approved `http:egress` hosts, only web files of the enabled version are served, and a disabled, upgraded or revoked plugin stops being served at once (T83). The `@openvox/plugin-sdk` TypeScript package (`sdk/typescript`) gives the page `getContext`, `showNotification`, `navigate`, `resize`, `on('theme.changed')` and a read-only `api.fetch` proxy gated by the plugin's approved `nodes:read`, `facts:read`, `reports:read`, `groups:read` and `users:read` capabilities and the user's own RBAC, over a MessageChannel handshake that only accepts the frame's own opaque-origin window (T84). The host exposes `--ov-*` design tokens (colors, fonts, radius, light and dark) as CSS custom properties and posts allowlisted values to frames on load and on every theme change (T85). The slot catalog is versioned and served at `GET /api/v1/plugins/ui/catalog` with each slot's context contract, `max_items` (enforced when serving the registry) and whether the host mounts it; the host now mounts `toolbar.global`, `footer.global`, `dashboard.widgets`, `node.detail.tabs`, `node.detail.actions`, `group.detail.panels`, `report.detail.panels`, `settings.sections` and `page` for declarative and iframe contributions (T85)

### Changed
- The security headers middleware keeps a Content-Security-Policy and Referrer-Policy that a handler already set instead of overwriting them, so sandboxed plugin pages keep their stricter policy
- Add plugin telemetry and governance (M22): every handler call (events, jobs, webhooks, lifecycle hooks) is measured per plugin, version and kind (calls, errors, traps, timeouts, latency histogram) and rolled up hourly into `plugin_usage_hourly` (T80); a per-plugin call quota (`plugins.governance.max_invocations_per_minute`, lowered by `runtime.limits.invocations_per_minute` in the manifest) refuses calls over the limit; a circuit breaker isolates a failing plugin (`plugins.governance.breaker_*`): it goes to `error` with its extension points out of play, an audit entry and a `plugin.circuit.opened` event are written, and it is tried again half open after an exponential backoff and closed after successful calls, or reset by an administrator (`POST /api/v1/plugins/{id}/circuit/reset`) (T81); `GET /api/v1/plugins/governance` and `GET /api/v1/plugins/{id}/metrics` feed a new Governance tab on the Plugins page with health, usage, breaker state and top consumers (T81); `GET /api/v1/plugins/metrics` exposes Prometheus metrics (`openvox_plugin_*`) for a scraper using an API key with `plugins:read` (T82)

## [0.43.0] - 2026-10-29

### Added
- Add the WASM runtime and host API for plugins (M20): `wasm` plugins run in wasmtime with a fresh store per call, fuel, a memory ceiling, a wall-clock deadline (epoch interruption), a per-plugin concurrency bulkhead and no WASI, files, network or clock; a trap, timeout or exhausted limit is a handled error that never affects the core or other plugins (T73, T76). Guests use a small JSON-over-memory ABI (`alloc`, `handle`, one `openvox.host_call` import) instead of the Component Model; this also makes `wasm` handlers of M21 events, jobs and webhooks run.
- Add the capability gate and approval flow (M20): effective capabilities are `declared ∩ approved ∩ plugins.policy`; administrators approve per capability (`GET/PUT /api/v1/plugins/{id}/capabilities`, optional capabilities via `optional: true`), `enable` is refused with `capabilities_not_approved` until every required one is approved, `plugins.policy.denied_capabilities` is a ceiling no approval can exceed, and an upgrade that adds capabilities stays pending (`pending_version`) while the previous version keeps running until it is approved (T74).
- Add plugin storage (M20): `kv` (quotas of 10,000 keys and 10 MB by default) and `db:own`, a private SQLite file per plugin with no `ATTACH`/`load_extension`/other schemas, a size cap and a per-query deadline, plus declarative `migrations/NNNN_name.up.sql|.down.sql` applied at enable and upgrade in one transaction each with checksums; `DELETE /api/v1/plugins/{id}?purge=true` deletes the data, `GET /api/v1/plugins/{id}/storage` shows usage and `GET/PUT /api/v1/plugins/{id}/config` stores settings (T75).
- Add plugin lifecycle hooks (`install`, `upgrade`, `enable`, `disable`, `uninstall`) called under the plugin timeout and audited; a failing `install`, `enable` or `upgrade` hook aborts the change (T76).
- Add the host API for plugins: `kv.*`, `db.query/exec`, `nodes.list/get`, `facts.get`, `reports.query`, `events.publish`, `audit.write`, `log.write`, `config.get` and `http.fetch` (https only, host allowlist, public addresses only after DNS resolution and on every redirect, 1 MiB responses). `email.send`, `notify.send` and `secrets.resolve` answer `unavailable` for now.

## [0.42.0] - 2026-10-28

### Added
- Add events, jobs and webhooks for plugins (M21): a durable event bus (`event_outbox`) whose dispatcher fans events out to enabled plugin subscriptions and outbound webhook endpoints with at-least-once delivery, priority ordering, host-evaluated payload filters, exponential-backoff retries, dead-lettering after `plugins.events.max_attempts`, a per-plugin concurrency bulkhead and handler timeouts, with plugin lifecycle and trust events (`plugin.installed|upgraded|enabled|disabled`, `plugin.trust.key_added|key_revoked`) published on it (T77); cron-scheduled plugin jobs from `contributes.jobs` that never overlap, are not replayed after downtime, are recorded per run and can be run on demand (T78); HMAC-SHA256 signed inbound webhooks at `/api/v1/plugins/{id}/webhooks/{hook}` (host-generated rotatable secret shown once, timestamp window, replay cache, rate and body limits) and administrator-managed signed outbound webhooks, plus admin endpoints under `/api/v1/plugins/events`, `/jobs` and `/webhooks/outbound` (T79). Handlers of `wasm` and `remote` plugins run through the runtime provided by M20; until then only built-in plugins can be invoked
- Add the plugin manager (M19): signed `.ovxp` packages with a validated `plugin.yaml` manifest, mandatory ed25519 signature verification at install and at every load (a package that is unsigned, tampered with, or signed by an unknown, revoked, expired or out-of-scope key is never installed or loaded), trusted key management through `/api/v1/plugins/trust/keys` and the Plugins page (separate `plugin_trust` permission, password re-authentication, fingerprint confirmation when trusting a key during install), plugin admin API and Plugins page (preflight, install, enable, disable, verify, uninstall, audited), periodic integrity re-verification, and a built-in runtime plus a permission-filtered slot registry for menus, pages and dashboard widgets with declarative rendering (T68, T69, T70, T71, T72)
- Add the M18 plugin platform proofs of concept in `poc/plugin-poc` and record the decisions in `docs/architecture/PLUGIN_POC_RESULTS.md`: per-plugin SQLite storage with enforced size, query-time and statement limits (T65), wasmtime fuel and memory limits inside Axum (T66), and the build and binary size impact of wasmtime (+6.4 MB stripped, about +1.7 to +2.7 MB compressed in the RPM) (T67)
- Add the plugin platform design documents and the Phase 11 roadmap (M18, S21, S22, S23)
- Audit update jobs in the shared audit log (`resource_type` `update_jobs`, queryable through `/api/v1/audit-logs`): requested (manual, schedule manual run and scheduled run), approved, rejected, cancelled, dispatched to a node, per-target result, finished and timed out after the maximum runtime (T62)

### Changed
- Adopt the Project Tracking Format for `ROADMAP.md` and `CHANGELOG.md`, moving roadmap reference material to `docs/PROJECT_OVERVIEW.md`, `docs/architecture/INVENTORY_UPDATES.md` and `docs/architecture/PLUGIN_PLATFORM_ROADMAP.md` (S20, S23)

### Postponed
- Postpone multi-tenant plugin scoping to the backlog: it is deferred out of the first plugin platform release and the design keeps it reachable (T114)
- Postpone the plugin marketplace to the backlog: it is deferred out of the first plugin platform release and the design keeps it reachable (T115)

## [0.41.6] - 2026-10-26

### Added
- Add a memory monitor that samples cgroup `anon` memory against `MemoryHigh` (falling back to RSS) every 30 s, warns from 80% of the limit and exits non-zero after 4 consecutive samples at or above 95% so systemd restarts a healthy instance, configurable under `memory_monitor` (`enabled`, `interval_secs`, `warn_percent`, `restart_percent` (0 disables), `restart_after_samples`, `fallback_limit_mb`) and reported as a `memory` component by `/api/v1/health/detailed` (T37)

### Changed
- Double the packaged systemd memory limits to `MemoryHigh=6G` / `MemoryMax=12G` (was 3G/6G) after a production hang at the 3G ceiling (T37)

## [0.41.5] - 2026-10-16

### Fixed
- Fix RPM and DEB package builds failing with "patch location `vendor/samael` does not contain packages matching `samael`" by copying `vendor/` in `scripts/build-packages.sh`; this also affected 0.41.2 to 0.41.4, so 0.41.5 is the first release since 0.41.1 to ship RPM and DEB packages (T24)

## [0.41.4] - 2026-10-16

### Changed
- Update dependencies: `dirs` 7.0, `rstest` 0.27, the Rust minor-version group (5 crates) and the frontend npm minor-version group (11 packages)

## [0.41.3] - 2026-10-07

### Fixed
- Dispatch update jobs to, and accept results from, nodes whose agents report an upper-case certname (e.g. `SEGDC1VPR0009.FGV.BR`), which were previously compared case-sensitively so the job never arrived and failed after the 240-minute maximum runtime (T58)

## [0.41.2] - 2026-10-06

### Fixed
- Add the `puppet` user to the `openvox-webui` group in the RPM and DEB install scripts so puppetserver can still read `/etc/puppetlabs/code/environments` (owned `openvox-webui:openvox-webui`, mode `750`); restart puppetserver after installing so the new group membership takes effect (T24)

## [0.41.1] - 2026-08-28

### Added
- Add a variable value filter to the machine variable assignments report, matching case-insensitively across strings, numbers, booleans, arrays and objects before pagination so totals and page boundaries stay accurate (T35)

## [0.41.0] - 2026-08-28

### Added
- Add a machine variable assignments report to Analytics showing every effective external variable each machine receives after group inheritance and overrides, with independent machine and variable searches, environment and matched-group context, and pagination (T35)

### Fixed
- Fix repository validation after the August 2026 dependency rollup by aligning the declared Rust 1.88 and Node.js 22.22.2 toolchains with the updated dependencies, using the TypeScript 6 API supported by `typescript-eslint`, clearing formatter and lint warnings, retaining `samael` 0.0.19 until its no-`xmlsec` build is fixed upstream, and running the previously skipped alert-rule BDD scenarios

## [0.40.7] - 2026-07-29

### Fixed
- Fix node status alert rules so they detect stale nodes: the evaluator now exposes `node.last_report` (hours since the last Puppet run, recomputed on every evaluation), `node.last_report_minutes`, `node.last_report_at`, `node.never_reported`, `node.name`, `node.deactivated` and `node.expired`, and triggered alerts record each node's last report time and age (T19)
- Fix alert conditions on a field that does not exist for the evaluated subject matching with the `!=` operator and firing for every node, treating a JSON `null` value as absent (T19)
- Implement the `not_in`, `exists` and `not_exists` condition operators offered in the alert rule form, which previously evaluated to false and were logged as unknown operators (T19)

## [0.40.6] - 2026-07-28

### Added
- Add Windows package inventory via Chocolatey (`choco list`) and winget (`winget export`), replacing the hardcoded empty package list (T46)
- Add Chocolatey sources to the Windows repository inventory and read `winget source export` JSON instead of parsing the localised fixed-width table (T46)
- Add Windows runtime inventory for IIS application pools (runtime version, pipeline mode, identity, assigned sites and applications), .NET and .NET Core shared runtimes, .NET Framework, and registered JDK/JRE installations (T46)
- Add Windows OS inventory fields: product name as edition, Update Build Revision as patch level, `DisplayVersion` (e.g. `23H2`) as update channel, and the last successful update from the Windows Update agent history with a `Win32_QuickFixEngineering` fallback (T46)
- Execute Windows update jobs instead of rejecting them with "Unsupported OS family", driving the Windows Update agent over COM for system and security patching (no PSWindowsUpdate dependency) and Chocolatey or winget for package install, update and remove (T58)
- Collect per-user installed applications from every loaded `HKEY_USERS` profile hive instead of only the `LocalSystem` `HKCU`, and filter out system components and update or hotfix registry entries (T46)

### Changed
- Move the platform path resolution used by the custom facts into a single pluginsynced helper, `lib/puppet_x/openvox_webui/paths.rb`, shared by `openvox_inventory` and `openvox_classification` (T38)
- Route the four `openvox_classification.rb` facts through one `OpenVoxClassification` module covered by unit tests in `test/openvox_classification_test.rb` (T38)

### Fixed
- Fix node classification on Windows by reading `%PROGRAMDATA%\PuppetLabs\facter\openvox-client.yaml` (with a literal `C:\ProgramData` fallback), so `openvox_classification`, `openvox_groups`, `openvox_classes`, `openvox_variables`, `openvox_environment` and the per-variable facts are no longer empty there (T38)
- Fix inventory collection on Windows by reading the same `openvox-client.yaml` location in the `openvox_inventory_status` fact, whose `confine` block never matched so the collector never ran even with `inventory_enabled => true` (T38)
- Resolve the Windows Puppet confdir (`%PROGRAMDATA%\PuppetLabs\puppet\etc`) for certname discovery and the mTLS certificate and CA fallbacks in both classification and inventory, so Windows nodes authenticate without explicit `ssl_cert` and `ssl_key` (T38)
- Fix `auto_generate_classification_key` on Windows by storing the key at `%PROGRAMDATA%\PuppetLabs\facter\openvox-classification-key` (Unix keeps its path, so existing keys are reused) and by requiring `fileutils`, whose absence made generation fail with `uninitialized constant FileUtils` (T38)
- Offer the client certificate on every HTTPS classification request instead of only when `ssl_verify` is enabled, so nodes combining `ssl_verify => false` with certificate authentication no longer get `401` (T31)
- Invoke PowerShell with `-EncodedCommand` instead of replacing newlines with `; `, which produced invalid PowerShell for multi-line constructs and broke the Windows application and IIS site collectors (T46)
- Fix `command_available?` on Windows by using `where` instead of the POSIX `command -v`, so Docker/Podman discovery and package-manager detection work there (T49)
- Fix Windows local user records to carry group membership, profile paths and an ISO 8601 last-logon timestamp, and derive the account type from the well-known RID instead of treating every disabled account as a system account (T49)

## [0.40.5] - 2026-07-28

### Fixed
- Make the Windows bootstrap register and start the OpenVox agent as a service reliably: write `puppet.conf` to `%PROGRAMDATA%\PuppetLabs\puppet\etc` with `runinterval = 30m` and a backup of any existing config, re-assert the Automatic startup type with `sc.exe config`, wait for the service to reach `Running` and report its state, startup mode and exit code otherwise, run a repair install (`REINSTALL=ALL PUPPET_AGENT_STARTUP_MODE=Automatic`) when the service is missing, treat msiexec exit codes 3010 and 1641 as success, run the initial `puppet agent --test` with the service stopped and check its exit code, and exit non-zero with remediation steps when the service is not running (T28)
- Stop the Windows bootstrap from clobbering unrelated `*_server` settings such as `ca_server` in an existing `puppet.conf` (T28)

## [0.40.4] - 2026-07-28

### Fixed
- Fix RPM package builds failing with `nothing provides openssl-libs(x86-64) = ...` by pulling BaseOS, AppStream and Extras from `dl.rockylinux.org` instead of the mirrorlist, since independently syncing mirrors could make the dnf transaction unsolvable; this is the actual cause of the failure 0.40.3 attempted to fix (T24)

## [0.40.3] - 2026-07-28

### Fixed
- Fix RPM package builds failing with an `openssl-libs` dependency error by using the maintained `rockylinux/rockylinux:9` image instead of the deprecated docker-library `rockylinux:9` image (T24)
- Verify that the package builder images are `linux/amd64` and executable on the host, so builds on Apple Silicon or ARM hosts fail fast with an actionable message (T24)
- Drop `baseUrl` from `frontend/tsconfig.json` in favour of a `./src/*` path mapping, which TypeScript 7 requires

## [0.40.2] - 2026-07-28

### Changed
- Update dependencies with no functional changes: the Rust minor group (14 crates), the npm minor group (17 packages) and TypeScript from 6.0.3 to 7.0.2

## [0.40.1] - 2026-07-21

### Added
- Highlight group-sourced facts on the node detail Facts tab by sorting them to the top, styling them with an accent background, tagging them with a "Group fact" badge and showing their count in the footer stats (T13)

## [0.40.0] - 2026-07-21

### Changed
- List only the groups whose classification rules match the node's facts on the node detail Groups tab, evaluating rules server-side through `GET /nodes/{certname}/classification` instead of filtering client-side on rule count (T13)

### Fixed
- Fix the node autocomplete search box rendering its placeholder under the search icon because the `.input` padding shorthand overrode the icon-clearing left padding

## [0.39.1] - 2026-07-21

### Changed
- Replace the fixed node dropdowns with searchable autocompletes backed by server-side search, so nodes beyond the configured page size can be selected: Groups → Pin Node queries as you type, excludes already-pinned nodes and only accepts existing nodes, and Facter Templates → Generate searches existing nodes while still accepting any certname (T13)

## [0.39.0] - 2026-07-20

### Added
- Add an `Update Job` alert rule type that fires when update jobs fail or time out, matches on `job.status`, `job.operation_type`, `job.timed_out`, `job.failed_targets`, `job.total_targets` and `job.runtime_minutes`, is evaluated by the update scheduler, and lets a `job.runtime_minutes` condition reference the configured maximum runtime (T41)
- Add an "Update Jobs" Settings tab with a configurable maximum update-job runtime (default 240 minutes, previously a hard-coded 120-minute per-target timeout) after which running jobs are failed automatically (T41)

### Fixed
- Show triggered alerts in the notification bell of every user instead of only the rule's creator, including alerts from rules without a creator that were filed under a `"system"` user nobody could see (T19)

## [0.38.0] - 2026-07-17

### Added
- Add a clickable list icon next to the Node Groups matched-nodes summary that opens a modal listing every matched node, each linking to its node detail page (T13)

## [0.37.20] - 2026-07-14

### Fixed
- Restore a compiling build after dependency updates by pinning `samael` to `=0.0.19` (0.0.21+ gate `quick-xml` imports behind `xmlsec` and fail with `default-features = false`) and migrating `services::backup_encryption` to the `chacha20poly1305` 0.11 / `aead` 0.6 API, and verify the full backend and frontend test suites and the production build pass

## [0.37.19] - 2026-07-01

### Added
- Add Windows (Server 2019/2022) to the module's `operatingsystem_support` metadata (T38)
- Add unit tests for `openvox_webui::client` covering the Windows, Linux and macOS code paths for config dir, SSL dir and file ownership defaults (T38)

### Changed
- Migrate the module's development tooling from PDK to Regent, removing `pdk.yaml` and the `pdk-version`, `template-url` and `template-ref` keys from `metadata.json` (T25)
- Render the `openvox_webui::enc` fact file from a plain interpolated string instead of a heredoc, with identical output, because Regent's manifest parser cannot parse the heredoc (T32)

### Fixed
- Support Windows agents in `openvox_webui::client` by defaulting `config_dir` to `C:/ProgramData/PuppetLabs/facter` on Windows and `/etc/puppetlabs/facter` elsewhere, and by resolving the Puppet SSL directory and `File` owner, group and mode per platform, leaving POSIX ownership unmanaged on Windows (T38)

## [0.37.18] - 2026-07-01

### Changed
- Pass `-SkipCertificateCheck` to `Invoke-WebRequest` in the Windows bootstrap command when "Ignore SSL certificates" is enabled instead of setting a permissive `ServerCertificateValidationCallback`, and remind users to run it with PowerShell 7 (`pwsh`) because Windows PowerShell 5.1 lacks that parameter (T28)

## [0.37.17] - 2026-07-01

### Fixed
- Fix the Windows bootstrap script reporting "OpenVox Server URL is not configured" when it is, by splitting the `{{OPENVOX_SERVER}}` sentinel so placeholder substitution no longer turns the guard into an always-failing self-comparison, matching the Linux script (T28)

## [0.37.16] - 2026-06-30

### Added
- Add an "Ignore SSL certificates" checkbox (default unchecked) to the Add Node page that makes the Linux bootstrap command use `curl -k` and the Windows command skip certificate validation, so nodes can enroll against servers with self-signed certificates (T27, T28)

## [0.37.15] - 2026-06-26

### Fixed
- Dispatch group update jobs to all of the group's matched nodes instead of only its pinned nodes, using the same classification-aware resolver as the UI for manual dispatch, preview, manual schedule run and the schedule scheduler (T59)

## [0.37.14] - 2026-06-26

### Fixed
- Fix dispatching updates to "All outdated nodes" failing with a 400 error by sending a `target_all_outdated` flag that the backend resolves to every currently outdated node when creating or previewing an update job (T59, #151)

## [0.37.13] - 2026-06-26

### Fixed
- Repair the backend test suite by seeding an active auth session in the alert rule integration tests and updating the `require_permission_middleware` doctest to the current `AppState`/`AppConfig` shape
- Pin `samael` back to 0.0.19 because 0.0.21 fails to compile without the `xmlsec` feature, which broke the default build

## [0.37.12] - 2026-06-26

### Added
- Return a fleet-wide `by_health` breakdown (healthy, warning, critical, unknown) from `GET /api/v1/nodes/stats`, computed by PuppetDB and mirroring the dashboard's per-node health logic (T34)

### Fixed
- Compute the dashboard status overview cards and Node Status Distribution chart from fleet-wide server-side aggregates instead of the paginated list capped at 100 nodes (T34, #150)

## [0.37.11] - 2026-06-18

### Added
- Add `pagination_default_limit` and `pagination_max_limit` class parameters that render the `pagination` section of `config.yaml`, so page sizes for `/nodes` and `/facts` can be managed through Puppet and Hiera (T34, T25)

## [0.37.10] - 2026-06-18

### Added
- Add server-side pagination with Previous/Next controls to the Nodes page, showing the true fleet-wide count through a new `X-Total-Count` header and filtering by search and status in PuppetDB instead of over a list truncated at 100 (T34, #142)
- Add a `GET /api/v1/nodes/stats` endpoint returning aggregate node counts (total, by status, by environment) computed by PuppetDB, used by the Analytics overview so "Total Nodes" and "Environments" stay accurate regardless of fleet size (T34)
- Add a `pagination` configuration section (`default_limit`, `max_limit`) to override the default and maximum page size of the `/nodes` and `/facts` endpoints (T34)

### Changed
- Use the configurable `pagination.default_limit` (default 100) in the `/nodes` and `/facts` list endpoints and clamp client-requested `limit` values to `pagination.max_limit` (default 5000) (T34)

## [0.37.9] - 2026-06-16

### Fixed
- Complete the 0.37.8 fix for Code Deploy "Failed to fetch from remote" over HTTPS by enabling the `https` and `ssh` features of `git2` 0.21 (which defaults to none, producing `there is no TLS stream available; class=Ssl (16)`) alongside `vendored-libgit2` and `vendored-openssl`, and by installing `perl-core` in the RPM and DEB builder images to compile the vendored OpenSSL (T30)

## [0.37.8] - 2026-06-16

### Fixed
- Fix Code Deploy "Failed to fetch from remote" over HTTPS by building `git2` with `vendored-libgit2` and `vendored-openssl`, because the bundled libgit2 had no TLS backend; this was incomplete until 0.37.9 enabled the `https` and `ssh` transports (T30)

## [0.37.7] - 2026-06-15

### Fixed
- Fix alert rules with simple-format conditions (Node Status connection state, Compliance status, Report Failure status) never matching, by matching a literal flat key such as `node.status` before falling back to nested traversal in field lookups (T19)

## [0.37.6] - 2026-06-15

### Added
- Make the dashboard Patch Age chart interactive, opening a drill-down of the nodes in a clicked bucket with each node's age and last-patched time (T57)
- Remember which Node Groups tree groups are expanded or collapsed across visits in a cookie, and add collapse-all and expand-all controls to the "All Groups" header (T13)

### Fixed
- Show feedback for "Evaluate Rules" on the Alerting page: a result banner with the number of triggered alerts, a hint when there are no enabled rules, and an error message on failure (T19)
- Restore a compiling backend after the `sqlx` 0.8.6 → 0.9.0 and `git2` 0.20 → 0.21 bumps by wrapping dynamic SQL strings with `AssertSqlSafe` and handling the `Option` to `Result` accessor changes (`Remote::url`, `Signature::name`/`email`, `Commit::message`, `Reference::shorthand`) with `.ok()`

## [0.37.5] - 2026-06-15

### Changed
- Pre-populate the "Parent Group" field with the selected group when creating a new node group (T13)

## [0.37.4] - 2026-05-29

### Security
- Remove the deprecated `X-XSS-Protection` response header, since modern browsers ignore it and the legacy filter could itself be abused, leaving XSS protection to the Content-Security-Policy and React's default output escaping (T23)

## [0.37.3] - 2026-05-21

### Fixed
- Fix the Activity Heatmap rendering all white because Tailwind purged the dynamically interpolated `bg-success-*`, `bg-warning-*` and `bg-danger-*` classes, by using inline `backgroundColor` styles with an explicit color ramp and a `min-w-[12px]` cell floor (T15)

## [0.37.2] - 2026-05-21

### Added
- Add an hourly report summary table (`report_hourly_summary`) with `GET /api/v1/reports/hourly-summary?hours=N` and `GET /api/v1/reports/activity-heatmap?days=N`, so "Report Metrics Over Time" (24h/7d/30d) and the Activity Heatmap read pre-aggregated data instead of fetching up to 10,000 reports per page load (T33)

### Changed
- Refresh the daily and hourly report summary tables in a single PQL fetch per cycle (one `reports[end_time, status]` projection over the rolling 31-day window), cutting the per-cycle PuppetDB roundtrips by about 56× (T33)

## [0.37.1] - 2026-05-21

### Fixed
- Replace the deprecated `quick_xml::Attribute::unescape_value` with `normalized_value(XmlVersion::Implicit1_0)` in the repository checker to silence quick-xml 0.40 deprecation warnings (T52)

## [0.37.0] - 2026-05-21
<!-- Pre-split accumulation: the previous puppet/CHANGELOG.md filed every change made between 0.28.0 and 0.37.0 here; the individual 0.29.0 to 0.36.x releases were never recorded. Phase 10.x, ENC watchdog and inventory-database entries were moved to 0.28.0 and 0.36.1 on git-date evidence. -->

### Added
- Add a pre-aggregated daily report summary (`report_daily_summary` table) populated hourly from PuppetDB, read by `GET /api/v1/reports/daily-summary` for the Dashboard "Weekly Activity Trend" chart instead of fetching up to 5000 reports per page load (T33)
- Prune inventory rows (snapshots, packages, applications, web, runtime, container and user data, update status, repo configs) for certnames no longer active in PuppetDB from the inventory maintenance scheduler, fixing "Inventory Coverage / reporting nodes" exceeding "Total Nodes" (T44)
- Add a cancel button to the Update Jobs UI for jobs in `pending_approval`, `approved` or `in_progress`, backed by `POST /api/v1/inventory/updates/{job_id}/cancel` (T61)
- Add an inline compliance rule editor to create and edit compliance baselines, with add and remove rule controls and fact, operator and value configuration (T39)
- Add edit support for compliance baselines (PUT endpoint and edit modal), a help guide with tips on naming, severity levels and rules, and placeholder text in the form fields (T39)
- Add server-backed auth session tracking so idle timeout enforcement survives page refreshes and token reuse (T36)
- Add optional node group selection to drift detection baselines in the create and edit modals (T39)
- Add a `classification.disable_authentication` config flag (default `false`) that disables authentication only for the public node environment and classification endpoints (T31)
- Add repository-based package version checking, where nodes report their configured repository metadata (YUM, APT, Zypper, Winget) and the server periodically fetches the latest package versions from the repositories (T52)
- Add the `fleet_repository_configs` and `node_repository_configs` tables, a `RepoCheckerService` parsing YUM (`repodata/primary.xml`) and APT (`Packages.gz`) metadata, and a background repo checker scheduler with a configurable interval (default 24h) (T52)
- Add the `GET /inventory/repositories` and `POST /inventory/repositories/check` endpoints to list fleet repositories and trigger a manual check (T52)
- Add facter collector methods for YUM, APT, Zypper and Winget repository discovery, and make `winget` the default Windows package repository source (T52)
- Add a `source_kind` field to `OutdatedInventoryItem` to distinguish "repo-checked" from "fleet-observed" outdated determinations (T53)
- Add an "Update Schedules" tab to Node Groups for one-time and recurring (cron) update tasks with optional approval workflows, backed by the `group_update_schedules` table, CRUD endpoints (`GET/POST /groups/:id/update-schedules`, `PUT/DELETE /groups/:id/update-schedules/:scheduleId`, `POST .../run`) and a background scheduler that creates UpdateJobs when schedules are due (T60)
- Add an Updates analytics dashboard tab with fleet update metrics, compliance charts, patch age distribution, top outdated software and update job history (T55, T56)
- Add drill-downs for outdated software (affected nodes with versions), compliance categories (node lists) and update jobs (per-target results), with backend endpoints for outdated software and compliance category queries (T56)
- Add drill-down modals to the Dashboard Inventory Compliance cards and pie chart and to Top Outdated Software (T56)
- Add Windows bootstrap support through a PowerShell script served from `/api/v1/bootstrap/windows-script` that discovers and installs the latest OpenVox Agent MSI from downloads.voxpupuli.org, with a copy-to-clipboard, non-interactive and dry-run command section on the Add Node page (T28)
- Add container inventory collection for Docker CE, Docker Enterprise and Podman with status, image, ports, mounts and runtime type, and local user inventory with UID, groups, shell, home directory, lock status and last login on Linux, macOS and Windows, shown in new containers and users tabs with filtering and search (T49)
- Make the inventory client poll for and execute pending update jobs (system patch, security patch and package operations), and refresh the version catalog and update statuses after each inventory submission (T61)
- Add an Updates management page with Update Status, Update Jobs, Version Catalog and Vulnerabilities tabs, and an update dispatcher with All Updates, Selected Packages and Security Updates Only scopes (T58, T59)
- Add CVE vulnerability detection with NVD 2.0 and CISA KEV feeds, configurable feed sources in Settings with add, edit, delete and sync controls, and background feed sync and vulnerability match schedulers with configurable intervals (T40)
- Add a Dashboard vulnerability view with severity distribution and affected nodes, and node-level vulnerability warnings in the Inventory tab with an expandable CVE table (T40)
- Add a `SecurityPatch` update operation type that resolves vulnerable packages from CVE matches, and a dry-run preview endpoint showing per-node package changes for update jobs (T40, T58)
- Add a `Vulnerability` alert rule type with notifications for critical and KEV matches (T40, T19)
- Add the `openvox_inventory_status` fact and `openvox_webui::client` options to enable inventory collection, control submission and cap collected item counts (T43)
- Add an environment group feature that assigns environments instead of filtering by them, and a Match All Nodes option controlling groups without rules (T13)
- Add a shared key authentication option for the classification endpoint through `classification.shared_key` or `CLASSIFICATION_SHARED_KEY`, with `classification_key` and `auto_generate_classification_key` support in the facter client, ENC script, `openvox_webui` class and `openvox_webui::client` class (T31)
- Add an unauthenticated `/api/v1/nodes/:certname/environment` endpoint returning only the environment assignment, for early environment detection before certificates are available, and fall back to it in the ENC script when `/classify` authentication fails (T31)

### Changed
- Use the dedicated `/environment` endpoint, which needs no authentication, in the `openvox_environment` fact (T31)
- Use async database queries for group permission checks for better performance and consistency (T6)
- Move the frontend lint toolchain to ESLint 10 with updated TypeScript and React plugin dependencies
- Restyle the "New Baseline" button as outlined and show edit (pencil) and delete icons side by side on baseline cards (T39)
- Raise the `inventory_max_items` default from 500 to 10,000 and its upper bound from 5,000 to 50,000 (T45)

### Fixed
- Fix node deletion authorization so users with `nodes:delete` granted through RBAC roles can delete nodes, the Delete button is disabled without the permission and 403 errors show a clear message (T29)
- Fix the package post-install script to use the agent `certname` (or FQDN) instead of `localhost` for the PuppetDB URL when PuppetDB is on the same host so TLS matches the Jetty certificate, and make a failed connection check only warn instead of aborting `dpkg`/`apt` (T24)
- Fix login session persistence after a browser refresh by waiting for auth store hydration before protected-route redirects (T36)
- Retry authenticated API requests with the refresh token when the access token expires (T36)
- Fix intermittent WebUI freezes by hardening browser storage access, showing a hydration loader and adding API request timeout defaults
- Fix SQLite contention spikes during inventory ingestion by serializing version-catalog refreshes and debouncing post-ingest refresh triggers (T44)
- Fix frontend lint and hook-order issues across Node Detail, Alerting, Login, Notifications and the auth and session stores
- Fix the report metrics over time chart by fetching a 30-day window (`since`) with a 10,000-report limit and linear interpolation to avoid misleading smoothing (T15)
- Fail hung update jobs by marking dispatched targets with no agent response within 2 hours as failed and rolling up the parent job status (T61)
- Fix the compliance baseline modal action buttons to use the shared design-system button styles (T39)
- Log users out automatically after 30 minutes of inactivity in both the GUI and protected API session validation (T36)
- Show the selected group name instead of the raw group ID on drift baseline cards, and an explicit "All nodes" scope when no group is selected (T39)
- Fix the Oracle Linux and RedHat collector missing `Last Successful Update` when `dnf history` reports abbreviated actions like `E, I, U`, and detect the last successful system update time from dnf/yum history, apt logs and zypper history instead of always reporting "Never recorded" (T45)
- Fix false-positive outdated package detection on fleets mixing OS major versions (e.g. el8 and el9) by adding an `os_version_pattern` dimension to the version catalog (T53)
- Fix the Update Compliance donut chart legend showing "value" and always include the Compliant, Outdated and Stale categories even at zero (T56)
- Fix RPM and DEB package builds failing on the missing `react-is` peer dependency required by `recharts` v3 (T24)
- Fix inventory submission 422 errors by making `image` optional in `HostContainerInventoryItem`, so runtime-only entries such as Docker Engine are accepted (T49)
- Install the base64 gem for r10k compatibility with Ruby 3.2, since puppet_forge requires >= 0.2.0 but Ruby 3.2 ships 0.1.1 (T30)
- Fix the dashboard stats grid so the 5 status cards fit on one row (T15)
- Fix the Activity Heatmap showing "No activity data available" by parsing PuppetDB raw metrics format (`{data: [...], href: "..."}`) as well as the pre-parsed format (T15)
- Fix the outdated software "Affected Nodes" count being inflated by duplicate package entries such as gpg-pubkey by counting unique nodes per package (T56)
- Fix the dashboard stats cards not summing to Total Nodes by adding a Warning category for stale nodes and aligning all cards to health-based classification (T15)
- Add the openvox-webui user to the puppet group in the ENC manifest for r10k cache directory access (T30)
- Fix r10k deployments being marked failed when killed by SIGSYS or SIGPIPE after completing successfully (T30)
- Collect all packages instead of capping at 500 items per category, and remove the frontend 250-item display cap for packages and applications in favour of progressive loading (T45)
- Fix update jobs staying in "queued" by executing them on the client's next Puppet run (T61)
- Populate the version catalog without requiring `inventory.enabled: true` in the server config (T52)
- Default r10k deployments to `pool_size: 1` to work around a Ruby 3.2 segfault in `File.chown` under multithreading, through the new `r10k_pool_size` config option and `code_deploy_r10k_pool_size` Puppet parameter, which is written to the generated `r10k.yaml` (T30)
- Remove the invalid `--pool-size` CLI argument from r10k invocations (T30)
- Fix intermittent r10k deployment corruption leaving partial modules by sending SIGTERM before escalating to SIGKILL after 30 s on timeout, no longer marking signal-terminated r10k processes successful, setting `/tmp` as the working directory of the spawned process and adding a `purge_allowlist` to the generated `r10k.yaml` that protects deployment marker files (T30)
- Fix pinned nodes not matching their group when parent groups do not match via rules, and make child groups match only nodes that also match their parent for non-pinned nodes (T13)
- Fix the bootstrap script to use Vox Pupuli release packages from apt.voxpupuli.org with a manual configuration fallback, detect and work around broken release packages that create invalid APT sources, clean up broken openvox repository configurations before a fresh install, and disable automatic service restarts (needrestart) during package installation (T27)
- Keep the current rule values visible when opening the classification rule inline edit form (T13)

### Security
- Require client certificate authentication (mTLS) on the classification endpoint `/api/v1/nodes/:certname/classify` (T31)
- Add an optional shared key authentication alternative to mTLS for debugging (T31)

## [0.36.1] - 2026-04-27
<!-- Date and release attribution inferred from git history (commits aea33b9, f96fe04 dated 2026-04-27); the old root CHANGELOG.md accumulated these entries under Unreleased/0.41.0 and puppet/CHANGELOG.md under 0.37.0. -->

### Added
- Add a self-healing ENC watchdog to the Puppet module: `openvox_webui::enc` deploys a sibling `${enc_script_path}.template` (byte-identical to the live script), `/opt/openvox/enc-watchdog.sh` and a systemd timer (`openvox-enc-watchdog.timer`, every 5 minutes) that probes the ENC end-to-end, restores the script from the template when it is missing or broken and restarts puppetserver when the JVM exec wedge ("Cannot run program ... error=2") is detected in the journal, breaking the deadlock where a broken ENC stops puppetserver from compiling the catalogs that would repair it, configurable through `enable_watchdog`, `puppetserver_service_name`, `puppet_user`, `watchdog_allow_restart` and `watchdog_journal_lookback_min` (T32)
- Add an inventory maintenance scheduler that prunes snapshots past the retention policy, checkpoints the WAL and runs `PRAGMA optimize`, plus a longer-cadence job that runs `VACUUM` on the inventory database (T44)
- Add a `--reset-inventory-migration` CLI flag that clears the migration marker so the one-shot migrator runs again on the next startup (T44)
- Add an `AppState::inventory_repository()` helper that builds an `InventoryRepository` bound to the correct pool and config (T44)

### Changed
- Breaking: move inventory data (snapshots, packages, applications, containers, users, update jobs, repository configs, group update schedules) to a dedicated SQLite database at `/var/lib/openvox-webui/inventory.db`, with a one-shot migrator that moves existing rows out of the main database on first start, truncates the legacy tables and schedules a background `VACUUM`, while inventory endpoints return 503 until it completes, to stop SQLite write-lock contention from heavy inventory POSTs blocking all UI reads and pushing the service past its cgroup memory soft limit (T44)
- Add the `database_url`, `keep_raw_payload`, `snapshot_retention_per_node`, `maintenance_interval_secs` and `vacuum_interval_secs` knobs to `InventoryConfig`, all with defaults so existing deployments need no change (T44)
- Raise the packaged systemd unit to `MemoryHigh=3G` / `MemoryMax=6G` (was 1G/2G) for SQLite page cache and request buffers, and move `StartLimitIntervalSec` and `StartLimitBurst` from `[Service]` to `[Unit]` to silence systemd warnings (T24)
- Write `host_inventory_snapshots.raw_payload` as NULL by default, restorable with `inventory.keep_raw_payload: true`, because the normalized per-item tables carry everything the UI reads (T44)

### Fixed
- Show pinned nodes in every ancestor group by propagating pinned membership up the ancestor chain with `MatchType::Inherited`, so ancestors such as `Develop` and `Linux` no longer report "0 nodes matched" for a node pinned to a leaf group, and evaluate ancestor-level rules for pinned nodes instead of short-circuiting (T13)

## [0.28.0] - 2026-03-19
<!-- Phase 10 delivery entries moved here on git-date evidence (commit 96d1786 "Implement phase 10 inventory and update management", tag v0.28.0, 2026-03-19). -->

### Added
- Add the Phase 10.1 inventory foundation with an inventory snapshot schema, an authenticated node ingestion endpoint, a node inventory history API and an inventory UI on the node detail page (T42, T43)
- Add Phase 10.2 collector support to the Puppet client module for OS-specific inventory gathering and submission to the node inventory APIs, covering Linux, Windows and macOS package and application, website and runtime discovery (T45, T46, T47, T48)
- Add Phase 10.3 node inventory UX with searchable per-node inventory sections, filter controls and richer history views (T50, T51)
- Add Phase 10.4 inventory intelligence with a scheduled version catalog refresh, stale inventory detection, per-node outdated status and inventory summary and catalog APIs (T52, T53, T54)
- Add Phase 10.5 fleet inventory dashboard reporting with compliance, patch age, platform distribution and top outdated software analytics (T55, T56, T57)
- Add the Phase 10.6 update orchestration foundation with persisted update jobs, an approval workflow, node polling and result APIs, and frontend client types for update operations (T58, T59, T60, T61)

### Fixed
- Rebuild PuppetDB dotted fact paths into nested JSON and include `catalog_environment` in group matched-node lookups, so hierarchical classification rules return the expected nodes (T13)
- Keep dispatching queued targets while a multi-node update job is `in_progress` instead of stranding them after the first node claims it (T59)
- Use the full last 7 days of reports for the dashboard weekly activity trend, and fall back to observed install timestamps for patch age analytics when explicit update history is missing (T15, T57)
- Make the Puppet inventory collector work on older Ruby runtimes by replacing `filter_map` and add collector normalization tests (T45)

## [0.26.0] - 2026-01-22
<!-- Release attribution inferred: the old root CHANGELOG.md accumulated these entries under Unreleased/0.41.0; git shows them added on 2026-01-22, when tags v0.25.0 and v0.26.0 were both created. -->

### Added
- Add global SMTP configuration through a key-value settings table and repository, an "Email/SMTP" tab in Admin Settings (host, port, username, password, sender, TLS) and `GET/PUT /api/v1/settings/smtp` endpoints (T19)
- Simplify email notification channels to a recipient address only, with SMTP settings injected automatically from the global configuration (T19)
- Add a warning in the Alerting page when no notification channels are configured, and disable the "New Rule" button with a warning tooltip (T19)
- Send real emails with the lettre library for test notifications and alerts, using HTML templates with severity-based styling, and show recipient addresses on channel cards and loading, success and error feedback on the test button (T19)
- Create system notifications in the Notifications panel when alerts trigger, and log alert rule evaluation in detail (enabled rule count, PuppetDB status, empty rules lists, silenced rules, cooldown status and evaluation errors) to debug why rules do not trigger (T19)
- Support simple and advanced condition formats in alert rules on the backend, with a frontend toggle between them and automatic handling of both during deserialization (T19, S24)
- Add a comprehensive alert condition evaluation engine with 11 condition types (NodeStatus, NodeFact, ReportMetric, EnvironmentFilter, GroupFilter, NodeCountThreshold, TimeWindowFilter, LastReportTime, ConsecutiveFailures, ConsecutiveChanges, ClassChangeFrequency), operators `=`, `!=`, `~`, `!~`, `>`, `>=`, `<`, `<=`, `in`, `not_in`, `exists`, `not_exists`, `contains`, `not_contains`, AND/OR logic, String, Integer, Float and Boolean data types, a `RuleEvaluator` running every 5 minutes, a `/api/v1/alerting/rules/:id/test` endpoint and the alert rule, condition and trigger database schema (T19, S24)
- Add a frontend condition builder with add, remove, duplicate and per-condition enable toggles, context-sensitive inputs, AND/OR operators, real-time validation and summary, dark mode support, updated TypeScript types and integration into the alert rule create and edit modals (T19)
- Add advanced infrastructure health conditions (LastReportTime to detect stale nodes, ConsecutiveFailures, ConsecutiveChanges, ClassChangeFrequency) with consecutive-metric evaluation functions, report caching and query batching, documented real-world scenarios, integration tests with 4 cases for matches and non-matches and test data factory functions (T19, S24)
- Add the feature documentation set for all 9 phases (19 files) with an index in `docs/implemented-features/README.md`, organized by phase (S1, S19)

### Changed
- Separate the email recipient (per channel) from the SMTP configuration (system-wide in Admin Settings), so email channel configs hold only the recipient list and `CreateChannelRequest` accepts partial configs (T19)
- Accept a `NotificationService` in `AlertingService` so triggered alerts create system notifications (T19)
- Compact `ROADMAP.md` from a single 1100+ line document into a roughly 300-line quick reference linking to the 19 detailed feature documents, each listing completed tasks, implementation details, API endpoints and key files (S1, S19)
- Use `docker buildx --load` for the RPM and DEB builders to avoid container-driver EOF issues on Docker Desktop (T24)
- Enhance the alerting documentation with the complete condition structure and evaluation logic, 11 condition types, 7 examples from simple to multi-condition rules, rule testing and manual evaluation endpoints, the database schema, best practices and common scenarios (T19, S24)

### Removed
- Remove the non-functional pre-configured system-email and system-webhook notification channels through a new migration (T19)

### Fixed
- Fix Rust compilation errors in `evaluate_condition` by handling the optional `AlertCondition` field and value (T19)
- Fix alert rule channel associations not being saved by generating an ID in `add_channel` for `alert_rule_channels` inserts (T19)
- Make alert rule conditions easy to add and remove with dedicated buttons, allow notification channels to be selected independently (unique IDs and `stopPropagation`), require condition fields with per-condition error messages, and change `condition_operator` from 'AND'/'OR' to 'all'/'any' to match the backend and stop 422 errors (T19)
- Standardize condition inputs on select boxes for type, field and operator in simple and advanced modes, with descriptive labels (e.g. "Node Status - Node connection state", "= (equals) - Exact match", "Last Report Time - Detect stale nodes"), value placeholder examples (failed, 24, 2024-01-22), channel type dropdown descriptions and an advanced mode help section explaining the config JSON with examples (T19)
- Improve notification channel creation with dynamic labels and hints per channel type ("SMTP URL" for Email, "Slack Webhook URL" for Slack), type-specific placeholder examples and user-friendly errors, and make the Email channel accept a recipient address instead of an SMTP URL (T19)

## [0.22.0] - 2026-01-15
<!-- Date corrected from 2025-01-15 to the v0.22.0 tag date; the old year predates the first commit (2025-12-16). -->

### Added
- Add node deletion with the `nodes:delete` RBAC permission, a Delete button with a confirmation dialog on the node detail page, and removal of pinned associations, certificate revocation and PuppetDB deactivation on delete (T29)

### Fixed
- Fix the parent group dropdown in the create dialog so it shows all available groups (T13)
- Fix classification inheritance for child groups without rules (T13)

## [0.21.1] - 2026-01-15
<!-- Date corrected from 2025-01-14 to the puppet-v0.21.1 tag date. -->

### Added
- Add a node bootstrap feature for adding new Puppet agents through a curl command, with a bootstrap script that detects RHEL-based and Debian-based systems (T27)

### Changed
- Rename `puppet_server_url` to `openvox_server_url` in the bootstrap configuration (T27)

### Fixed
- Include the scripts directory in Docker package builds (T24)

## [0.21.0] - 2026-01-15
<!-- Date corrected from 2025-01-13 to the v0.21.0 tag date. -->

### Fixed
- Include the documentation, puppet and tests directories in Docker package builds (T24)
- Make the `run-scheduled-reports` binary required in packages (T24)
- Align the Facts Explorer search input style with the Nodes page

## [0.20.0] - 2026-01-14
<!-- Date corrected from 2025-01-12 to the v0.20.0 tag date. -->

### Added
- Add a deployment cancel feature with process kill support to Code Deploy (T30)
- Add incremental Docker builds for faster package creation (T24)

## [0.9.1] - 2025-12-18
<!-- Consolidated release: the old roadmap lists releases 0.2.x to 0.9.x as shipped between 2025-12-16 and 2025-12-18 (commit b09df32 "v0.9.1") but the changelog never recorded them separately. Entries for T3 to T26 are reconstructed from the old ROADMAP.md and docs/implemented-features/; entries from the old root CHANGELOG.md Unreleased section are dated by git (2025-12-17/18). -->

### Added
- Add the RBAC foundation (T3)
- Add user management and JWT authentication (T5)
- Add RBAC with five built-in roles, scoped permissions and permission caching, and RBAC management tools and API (T6, T7)
- Add PuppetDB integration with full query support, a data caching layer and PuppetDB API endpoints (T8, T9, T10)
- Add Puppet CA certificate management with backend services, API endpoints and a management UI (T11, T12, T17)
- Add rule-based node classification with group hierarchy and 10 comparison operators (T13)
- Add Facter integration with custom fact templates and multi-format export in JSON, YAML and Shell (T14)
- Add a real-time dashboard with visualizations and node status and health indicators (T15)
- Add YAML-based application configuration with a validated settings UI, versioning and history, import and export, per-user dashboard preferences and support for PuppetDB, PostgreSQL and SQLite (T16)
- Add report generation (compliance, drift detection, custom) with scheduled execution (T18)
- Add alert rules with multiple notification channels, acknowledgment and silencing (T19)
- Add multi-tenancy with tenant isolation and API key management for programmatic access (T20)
- Add comprehensive audit logging (T21)
- Add Tailwind dark mode (`darkMode: 'class'`) and a ThemeToggle component in the sidebar (T22)
- Add database batch loading methods to eliminate N+1 query patterns in GroupRepository and the RBAC services (T22)
- Add selective permission cache invalidation using a role-to-users reverse lookup mapping (T22)
- Add frontend lazy loading with `React.lazy()` and `Suspense` for all page components (T22)
- Add native package building infrastructure: version auto-detection, Docker builds and binary tarballs in `scripts/build-packages.sh`, an RPM spec file for RHEL, CentOS, Fedora and Rocky, DEB packaging with libssl3 and libssl1.1 alternatives, a hardened systemd unit (SystemCallFilter, MemoryDenyWriteExecute and more), `/etc/default/openvox-webui` and `/etc/sysconfig/openvox-webui` environment files, GPG package signing and packaging documentation in `packaging/README.md` (T24)
- Add the `openvox-webui` Puppet module with 50+ parameters, a `config.yaml.epp` template, a `puppetdb_connection` custom fact with automatic SSL certificate detection, support for 8 OS distributions (RHEL, CentOS, Rocky, Alma, Fedora, Debian, Ubuntu), Hiera data defaults, example manifests and a module README (T25)
- Add the production documentation suite: `docs/INSTALLATION.md` (RPM, DEB and Puppet), `docs/CONFIGURATION.md`, `docs/UPGRADE.md` with version-specific migration notes and rollback procedures, `docs/BACKUP.md` with automated scripts and disaster recovery, troubleshooting sections and security best practices (T26)

### Changed
- Fix clippy lints across config, middleware, models and services
- Split API routing into public and protected routes so the auth middleware applies only to protected routes (T23)
- Scope groups, fact templates and users to the tenant in repository queries (T20)
- Apply consistent dark theme styles across layout, navigation, cards, inputs and alerting UI (T22)
- Optimize `GroupRepository.get_all()` from 1+2N queries to 3 queries with batch loading, and RBAC `get_all_roles()` and `get_user_roles()` from N+1 to 2 queries each (T22)
- Split the frontend bundle into vendor chunks (react, query, charts, ui) for better caching (T22)

### Fixed
- Stop the global auth middleware blocking `/api/v1/auth/*` endpoints (T23)
- Clear ESLint warnings in the Settings page (`no-console`)

### Security
- Add IP-based rate limiting middleware with stricter configurable limits for auth endpoints than for API endpoints (T23)
- Add a security headers middleware setting HSTS, CSP, X-Frame-Options, X-Content-Type-Options, X-XSS-Protection, Referrer-Policy and Permissions-Policy (T23)
- Add Cache-Control headers to API responses to prevent caching of sensitive data (T23)
- Set TLS 1.3 as the default minimum version, configurable through `server.tls.min_version` (T23)
- Support ALPN for HTTP/2 and HTTP/1.1 over TLS (T23)

## [0.1.0] - 2025-12-16
<!-- Date corrected from 2024-12-16 (before the first commit, 2025-12-16). No v0.1.0 tag exists. The old root CHANGELOG.md listed this release as "TBD" with the core items below, and puppet/CHANGELOG.md with the Puppet module items. -->

### Added
- Set up the initial project release (T1)
- Add the core Axum server implementation with basic API routing and logging and tracing infrastructure (T2)
- Add a JWT authentication system (T5)
- Add YAML configuration management (T2)
- Add a unit and integration test framework (T4)
- Add a Docker development environment (T1)
- Add package installation for RPM and DEB systems to the Puppet module, with systemd service management and configuration file templating (T25)
- Add PuppetDB connection support with SSL and Hiera integration for all parameters to the Puppet module (T25)
- Add initial admin account setup to the Puppet module (T25)
- Support RHEL 8/9, CentOS 8/9, Rocky 8/9, AlmaLinux 8/9, Fedora 38/39/40, Debian 11/12 and Ubuntu 22.04/24.04 in the Puppet module (T25)

[Unreleased]: https://github.com/openvoxproject/openvox-webui/compare/v0.43.0...HEAD
[0.43.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.43.0
[0.42.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.42.0
[0.41.6]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.41.6
[0.41.5]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.41.5
[0.41.3]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.41.3
[0.41.2]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.41.2
[0.41.1]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.41.1
[0.41.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.41.0
[0.37.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.37.0
[0.36.1]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.36.1
[0.28.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.28.0
[0.22.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.22.0
[0.21.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.21.0
[0.20.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.20.0
[0.1.0]: https://github.com/openvoxproject/openvox-webui/releases/tag/v0.1.0
