# OpenVox WebUI Architecture

This document explains how the application is put together so operators and contributors can reason about behavior, scale, and security.

## High-Level Components
- **Frontend**: React + TypeScript (Vite) single-page app served by the backend or any static host.
- **Backend**: Rust (Axum) API that handles auth, RBAC, multi-tenancy, PuppetDB access, and business logic.
- **Database**: SQLite for metadata, RBAC, alerting state, and cached artifacts.
- **Puppet Integrations**:
  - PuppetDB client for nodes, facts, reports.
  - Puppet CA client for CSR lifecycle (status, sign, revoke, renew).
- **Background/Async**: Report scheduling, alert evaluation, and cache invalidation run in async tasks.

## Backend Layout (Rust)
- `src/api/*`: HTTP route handlers grouped by resource (auth, nodes, groups, facts, facter, reports, roles, users, permissions, alerting, analytics, CA, query).
- `src/services/*`: Business logic layers (auth, rbac, classification, facter, puppetdb, puppet_ca, alerting, reporting, scheduler, cache).
- `src/db/*`: Repositories and migrations for all persisted entities (users, roles/permissions, organizations, node_groups, fact_templates, alerting, reporting).
- `src/middleware/*`:
  - `auth`: JWT + API key authentication; injects `AuthUser`.
  - `rbac`: Permission enforcement helpers.
- `src/models/*`: Shared data structures (RBAC enums, groups, facts, reports, alerting models, orgs, audit entries, API keys).

## Request Flow
1. **Public router** handles `/api/v1/health*` and `/api/v1/auth/*` without auth.
2. **Protected router** applies auth middleware:
   - Accepts `Authorization: Bearer <jwt>` (access token) or `Authorization: ApiKey <ovk_id_secret>` / `X-API-Key`.
   - Resolves user + roles, attaches `AuthUser { id, organization_id, roles, role_ids }`.
3. **RBAC checks** occur in handlers/services; permissions are scoped (all/specific/environment/group/self/owned).
4. **Repositories** enforce tenant filters (`organization_id`) for core tables (users, groups, fact templates, saved reports, alerting/reporting tables, api_keys, audit_log).
5. **Responses** serialized via Axum/serde.

## Authentication & Authorization
- **JWT**: Issued at login/refresh; carries `sub` (user id), `roles`, `organization_id`.
- **API keys**: Stored hashed; each key has explicit role scope (`api_key_roles`). Last-used and expiry tracked.
- **System roles**: `super_admin` (cross-tenant), `admin`, `operator`, `viewer`, `group_admin`, `auditor`.
- **RBAC resources/actions** cover nodes, groups, facts, facter templates, reports, users, roles, settings, alerting, audit_logs, api_keys.

## Multi-Tenancy Model
- **organizations** table; default organization id `00000000-0000-0000-0000-000000000010`.
- Each tenant-owned record stores `organization_id` (users, node_groups, fact_templates, saved_reports, report schedules/executions, compliance/drift baselines, api_keys, audit_log).
- **Tenant overrides**: Selected endpoints accept `?organization_id=` but only for `super_admin`.
- **Cross-tenant admin**: `super_admin` has full scope and can manage organizations and keys across tenants.

## Data Stores & Migrations
- SQLite is used via SQLx. Migrations live in `migrations/` and run on startup.
- Key tables:
  - Identity/RBAC: `users`, `roles`, `permissions`, `user_roles`, `api_keys`, `api_key_roles`
  - Tenancy: `organizations`
  - Classification: `node_groups`, `classification_rules`, `pinned_nodes`
  - Facter templates: `fact_templates`
  - Reporting/analytics: `saved_reports`, `report_schedules`, `report_executions`, `compliance_baselines`, `drift_baselines`, `report_templates`
  - Alerting: `notification_channels`, `alert_rules`, `alert_rule_channels`, `alerts`, `notification_history`, `alert_silences`
  - Audit: `audit_log`

## Async & Background Work
- **Scheduler**: Runs report schedules and alert evaluations.
- **Cache**: Optional caching for PuppetDB responses; TTL-configurable.
- **Audit**: Writes on sensitive operations (e.g., API key create/delete, org changes).

## Deployment Topology
- **All-in-one**: Axum API + static frontend + SQLite on the same host (default).
- **Split frontend**: Serve `frontend/dist` via CDN; point it at the API URL.
- **External PuppetDB/CA**: Configure endpoints and SSL materials in `config.yaml`.
- **Scaling**: Horizontal scaling is constrained by SQLite; for HA, place behind a reverse proxy and keep a single-writer node or migrate to an external DB in future phases.

## Observability & Operations
- Logging: level/format/target configurable; supports JSON for ingestion.
- Health endpoints: `/api/v1/health`, `/api/v1/health/detailed`, `/api/v1/health/live`, `/api/v1/health/ready`.
- Audit logs: persisted and queriable via `/api/v1/audit-logs`.
- Metrics: future work; current focus is logs + audit + health checks.

## Frontend Structure (React/Vite)
- Pages: Dashboard, Nodes, Groups, Facts/Facter, Reports, Alerting, Settings, RBAC (users/roles/permissions), API Keys, Audit Logs.
- State: Auth via local storage token; data via API client (`frontend/src/services/api.ts`).
- Styling: TailwindCSS; components under `frontend/src/components`.

