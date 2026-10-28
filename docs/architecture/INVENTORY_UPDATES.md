# Application Inventory & Update Management (Phase 10 plan)

> Moved verbatim from the pre-PTF `ROADMAP.md` (Phase 10) during the migration to the Project Tracking Format. Delivery status is now tracked in `ROADMAP.md` milestones M12-M17.

Status: In progress. Phases 10.1 through 10.5 are implemented, and Phase 10.6 is in progress with update-job orchestration foundations in place.

## Goal
- Turn OpenVox WebUI into a central inventory and update management console for Windows, Linux, and macOS nodes managed through the OpenVox Puppet module.
- Collect application, website, runtime, package, and OS version data from agents, store historical inventory on the server, identify outdated software from default repositories, and provide controlled update operations for single nodes or groups of nodes.

## Scope Summary
- Inventory collection from Windows, Linux, and macOS nodes
- Inventory transport through the OpenVox Puppet module back to the server
- Database schema and APIs for inventory snapshots, application metadata, and update state
- Node-level UI cards showing OS version, OS patch state, and installed applications
- Fleet statistics UI for updated and outdated nodes, stale inventory, and update compliance
- Background jobs to resolve latest versions from vendor-default repositories and package sources
- On-demand and bulk update workflows with auditability and safety controls

## Inventory Coverage

### Operating System Data
- OS family, distribution, edition, architecture, kernel version, OS version, and patch level
- Last inventory collection time and last successful update time
- Host package manager and update channel metadata

### Linux Inventory
- Installed RPM packages for Oracle Linux, Red Hat, and SUSE families
- Installed DEB packages for Ubuntu and Debian families
- Package epoch, version, release, architecture, repository source, and install time when available
- Service-backed application discovery for Apache HTTPD, NGINX, Tomcat, and JBoss
- Website and application metadata:
  - Apache virtual hosts, site roots, enabled modules, bound ports, TLS certificate references
  - NGINX server blocks, upstreams, document roots, bound ports, TLS certificate references
  - Tomcat deployed WARs, context paths, app base, connector ports, runtime version
  - JBoss / WildFly deployed applications, server groups, runtime version, management bindings

### Windows Inventory
- Installed applications from registered Windows uninstall sources
- Product name, publisher, version, install date, uninstall identity, architecture, and install scope
- IIS website and application inventory:
  - Sites, applications, virtual directories, bindings, app pools, physical paths, and certificate bindings
- OS version and patch metadata from Windows management facts
- Update source metadata aligned to Chocolatey for version intelligence

### macOS Inventory
- Installed `.app` bundles from standard application locations
- Bundle identifier, display name, version, short version, install path, signing metadata when available
- Homebrew formula and cask inventory with installed versions, tap/source, and install paths
- macOS version, build number, and available software update metadata

## Delivery Architecture

### Agent Collection via Puppet Module
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

### Server-Side Processing
- Validate, deduplicate, and persist host inventory snapshots
- Maintain both current-state tables and historical snapshots for trend reporting
- Correlate package inventory with service/application inventory on the same host
- Mark stale records when a host has not reported within the expected collection interval

## Database Plan

### Core Tables
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

### Data Model Principles
- Separate canonical software identity from host-specific installed instances
- Store raw collector payloads for troubleshooting and parser migrations
- Support multiple inventory records per host for the same product when installed in different locations
- Preserve historical snapshots for compliance and trend views
- Record source confidence and discovery method for each inventory item

## API Plan
- Inventory ingestion endpoint for Puppet module submissions
- Node inventory endpoints for current state and history
- Fleet statistics endpoints for update posture and inventory coverage
- Version intelligence endpoints for latest-known versions by platform and package source
- Update orchestration endpoints for single-node and bulk update requests
- Audit endpoints for update job actions, approvals, and outcomes

## UI Plan

### Node Detail Enhancements
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

### Fleet Update Dashboard
- Separate UI section for update and inventory analytics
- Statistics and charts for:
  - Updated vs outdated nodes
  - Nodes missing inventory
  - Nodes missing update checks
  - Time since last patch
  - Top outdated packages and applications
  - Platform and OS distribution
- Filters by OS family, environment, group, application, runtime, and update status

### Update Operations UI
- Single-node update action
- Bulk update action for selected nodes or groups
- Dry-run / preview before execution
- Scheduling, maintenance window, and approval options
- Job progress, success/failure breakdown, and rollback guidance where applicable

## Latest Version Intelligence Plan

### Linux Sources
- Oracle Linux: DNF/YUM repository metadata from configured default repositories
- Red Hat / compatible RPM systems: DNF/YUM repository metadata
- SUSE: Zypper repository metadata
- Ubuntu and Debian: APT repository metadata
- Compare installed versions against latest versions visible in default repositories for each node platform

### Windows Source
- Use Chocolatey package metadata as the default version source for Windows update intelligence
- Map installed applications to Chocolatey packages where possible
- Flag unmatched software separately rather than reporting false update recommendations

### macOS Source
- Homebrew formula and cask metadata for managed Homebrew software
- macOS software update metadata for OS patch visibility
- App bundle version checking only where a trustworthy managed source exists

## Scheduled Background Jobs
- Inventory freshness job to detect stale or missing host submissions
- Repository sync job to refresh latest-version catalogs by platform
- Version comparison job to compute per-node outdated status
- Statistics aggregation job for dashboard performance
- Update execution watcher for long-running update jobs and result rollups

## Update Execution Plan

### Execution Model
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

### Safety Controls
- RBAC permissions for viewing, scheduling, approving, and executing updates
- Group-scoped update permissions
- Maintenance window enforcement
- Maximum concurrency controls
- Dry-run previews and explicit confirmation for bulk actions
- Full audit logging of requested, approved, started, completed, and failed updates

## Delivery Phases

### Phase 10.1: Inventory Schema & Transport (Completed)
- Define canonical inventory schema and ingestion API
- Extend Puppet module with inventory submission support
- Add database tables for OS, packages, applications, websites, runtimes, and snapshots

### Phase 10.2: OS-Specific Collectors (Completed)
- Linux RPM/DEB collectors
- Windows installed applications and IIS collectors
- macOS app bundle and Homebrew collectors
- Apache, NGINX, Tomcat, and JBoss discovery collectors

### Phase 10.3: Node Inventory UI (Completed)
- Node inventory cards and detailed inventory sections
- Search, filter, and history views per node

### Phase 10.4: Version Intelligence & Scheduled Jobs (Completed)
- Repository sync workers for Linux, Windows, and macOS sources
- Outdated-version comparison engine
- Scheduled jobs and stale inventory detection

### Phase 10.5: Fleet Statistics & Compliance Reporting (Completed)
- Separate inventory/update dashboard
- Updated vs outdated statistics
- Patch aging, coverage, and drift metrics

### Phase 10.6: Update Orchestration (In Progress)
- Single-node updates
- Bulk group updates
- Scheduling, approvals, execution tracking, and audit logging

## Open Questions
- Whether unmanaged Windows applications without Chocolatey mapping should be shown as inventory-only or support custom catalog matching
- Whether Tomcat and JBoss application collection should parse deployment manifests deeply or start with runtime and deployment names only
- Whether website inventory should also track TLS expiration, certificates, and reverse-proxy upstream dependencies in the first release
- Whether update execution should use immediate remote actions, queued Puppet runs, or both
- Whether Homebrew-managed updates should be limited to formulae/casks explicitly marked as managed by policy

## Success Criteria
- Every managed node reports OS version and inventory on a predictable schedule
- Operators can see current installed applications and websites directly from the node view
- Fleet dashboard shows accurate updated/outdated status by platform and group
- Latest-version checks use trusted default package sources for each supported OS family
- Operators can safely run single-node and bulk updates with audit trails and RBAC controls
