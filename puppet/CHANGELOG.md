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

### Fixed
- r10k deployments now default to `pool_size: 1` to work around Ruby 3.2 segfault in `File.chown` under multithreaded execution
  - New `r10k_pool_size` config option and `code_deploy_r10k_pool_size` Puppet parameter
  - Also passes `--pool-size` on the r10k CLI and includes `pool_size` in generated `r10k.yaml`

### Added
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
