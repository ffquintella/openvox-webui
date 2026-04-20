# OpenVox WebUI User Guide

This guide walks operators and viewers through daily usage of the WebUI. It assumes the application is installed and configured (see INSTALLATION.md and CONFIGURATION.md).

## 1. First Login
1. Open the WebUI in your browser (default: `https://<host>:5051`).
2. Sign in with the admin credentials configured in `config.yaml` (`initial_admin`).
3. Immediately change the password (`Settings → Security → Change password`).
4. If you will run multiple tenants, create organizations and additional admins (see RBAC & Tenants).

## 2. Navigation Map
- **Dashboard**: Environment and node summaries, alert widgets.
- **Nodes**: PuppetDB-backed node list; drill into facts and reports per node.
- **Groups**: Rule-based and pinned classification; manage classes/parameters/variables.
- **Facts / Facter Templates**: Browse facts; create templates to export external facts.
- **Reports**: Saved reports, schedules, executions, and report templates.
- **Alerting**: Rules, channels (Webhook/Email/Slack/Teams), active alerts, silences, history.
- **RBAC**: Users, roles, permissions; assign roles.
- **API Keys**: Create/manage API keys scoped to roles and tenant.
- **Audit Logs**: View security-sensitive actions.
- **Settings**: Auth settings, PuppetDB/CA connectivity, caching, theme preferences.

## 3. RBAC, Tenants, and Roles
- **Tenants (organizations)**: Data is isolated by `organization_id`. The default organization is created automatically.
- **Super Admin**: Cross-tenant role; can specify `?organization_id=<uuid>` on supported APIs and manage organizations and keys across tenants.
- **Admins/Operators/Viewers/Group Admin/Auditor**: Built-in roles with progressively narrower permissions.
- **Assigning roles**:
  1. Go to **RBAC → Users**.
  2. Create or select a user.
  3. Assign roles (system roles or custom roles).
- **Creating organizations** (super_admin):
  1. Go to **RBAC → Organizations**.
  2. Create a new organization (name + slug).
  3. Create or move users/keys within that organization.

## 4. API Keys
- Navigate to **API Keys**.
- Click **Create API Key**:
  - Name the key.
  - Optional expiry.
  - Optional role scope (default: your own roles). Super admins can scope keys to other users/tenants.
- Copy the plaintext key once; it will not be shown again.
- Use in clients via `Authorization: ApiKey <key>` or `X-API-Key: <key>`.

## 5. Classification & Groups
1. Go to **Groups**.
2. Create groups with optional parent hierarchy and environment scope.
3. Add rules (fact path + operator + value) or pin nodes directly.
4. Classes/parameters/variables are applied to matching nodes; variables are available to facter exports.
5. Use **Nodes → Classify** to view where a node lands and why.

## 6. Facts & Facter Templates
- **Facts**: Browse PuppetDB facts; filter by environment or search.
- **Facter Templates**: Create templates that emit external facts from:
  - Static values
  - Classification attributes (environment, groups, variables)
  - Existing facts (`fact.path`)
- Export facts per node: **Facts → Export** or `GET /api/v1/facter/export/:certname?template=<name>`.

## 7. Reports & Analytics
- **Saved Reports**: Create saved queries with parameters (compliance, drift, change tracking).
- **Schedules**: Set cron, timezone, output format (json/csv/pdf), recipients.
- **Executions**: View history, status, runtime, outputs.
- **Templates**: Built-in report templates are marked system and cannot be deleted.

## 8. Alerting
1. Configure channels (Webhook/Email/Slack/Teams) and test them.
2. Create alert rules (node status, compliance, drift, report failure, or custom).
3. View active alerts; acknowledge/resolve; apply silences with durations.
4. Check alert history and statistics for triage.

## 9. Audit Logs
- Accessible to admins/auditors/super_admin.
- Shows action, resource, user, organization, IP, timestamp.
- Filter by action, resource type, user, or organization (super_admin only).

## 10. Settings & Connectivity
- **PuppetDB**: Configure URL, TLS materials, timeouts. Verify connectivity in **Settings**.
- **Puppet CA**: Manage CSRs (list, sign, reject, revoke, renew) from **CA** or API.
- **Cache**: Enable/disable cache and TTLs to reduce PuppetDB load.
- **Auth**: Adjust token lifetimes, password policy, and login protections.

## 11. Environments & Scoping Tips
- Use environment-scoped permissions (`scope_type: environment`) to limit read/write access by environment.
- Group admins manage only their assigned groups; operators can create/update but not delete everything.
- For tenant overrides in API calls, only `super_admin` can set `organization_id` explicitly.

## 12. Common Workflows
- **Onboard a team**:
  1. Create an organization (optional if not multi-tenant).
  2. Add users; assign `operator` or `viewer`; add `auditor` for audit visibility.
  3. Create API keys for automation with limited roles.
- **Classify a new service**:
  1. Create a group (e.g., `webservers`) with rules on `trusted.extensions.pp_role` or `os.family`.
  2. Add classes/parameters; pin initial nodes if needed.
  3. Validate via node classification view.
- **Troubleshoot a node**:
  1. Open **Nodes** → select node.
  2. Check facts, reports, and classification matches.
  3. Review alerts and audit entries for recent actions.

