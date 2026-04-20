# OpenVox WebUI - Complete User Guide

This comprehensive guide covers all features and workflows in OpenVox WebUI.

## Table of Contents

- [Getting Started](#getting-started)
- [Authentication](#authentication)
- [Dashboard](#dashboard)
- [Node Management](#node-management)
- [Node Groups & Classification](#node-groups--classification)
- [Facts & Facter Templates](#facts--facter-templates)
- [Reports](#reports)
- [Analytics](#analytics)
- [Alerting](#alerting)
- [Puppet CA Management](#puppet-ca-management)
- [Code Deploy](#code-deploy)
- [User Management](#user-management)
- [Roles & Permissions](#roles--permissions)
- [Settings](#settings)
- [User Profile](#user-profile)

---

## Getting Started

### First Login

1. Navigate to your OpenVox WebUI instance (default: `https://<hostname>:443`)
2. Log in with your credentials:
   - **Local authentication**: Use username and password
   - **SAML SSO**: Click "Login with SSO" button

### Interface Overview

The interface consists of:
- **Top Navigation Bar**: Access to all main sections
- **User Menu**: Profile, settings, and logout (top-right corner)
- **Theme Toggle**: Switch between light and dark themes
- **Breadcrumbs**: Current location in the application

---

## Authentication

### Local Authentication

Standard username/password authentication.

**Features:**
- Password complexity requirements (minimum 8 characters)
- Account lockout after failed attempts
- Force password change on first login
- Session timeout management

### SAML 2.0 SSO

Single Sign-On integration with enterprise identity providers.

**Setup Requirements:**
1. Admin configures SAML in `config.yaml`
2. IdP metadata is imported
3. Users must be pre-provisioned in OpenVox WebUI
4. User's `auth_provider` must be set to `saml` or `both`

**Login Flow:**
1. Click "Login with SSO"
2. Authenticate with your Identity Provider
3. Return to OpenVox WebUI with active session

**Auth Provider Types:**
- **local**: Password authentication only
- **saml**: SSO authentication only
- **both**: Can use either method

---

## Dashboard

The Dashboard provides real-time infrastructure monitoring.

### Widgets

**Node Status Overview**
- Total node count
- Nodes by status (changed, unchanged, failed, unreported)
- Click any status to filter nodes

**Recent Reports**
- Latest Puppet runs across your infrastructure
- Status indicators for success/failure
- Direct links to detailed reports

**Environment Distribution**
- Visual breakdown of nodes by environment
- Quick way to see infrastructure segmentation

**Compliance Status**
- Track compliance across your fleet
- Identify non-compliant nodes

**Performance Metrics**
- Average run times
- Resource usage statistics
- Trend analysis

### Refresh and Auto-Update

- Manual refresh button in top-right
- Configurable auto-refresh interval in Settings
- Real-time updates without page reload

---

## Node Management

### Nodes List

View and manage all nodes in your infrastructure.

**Features:**
- Search by certname
- Filter by environment, status, or report timestamp
- Sort by any column
- Pagination controls

**Status Indicators:**
- 🟢 **Unchanged**: Last run made no changes
- 🟡 **Changed**: Last run applied changes successfully
- 🔴 **Failed**: Last run encountered errors
- ⚫ **Unreported**: No recent reports (configured threshold)

### Node Detail View

Click any node to see comprehensive details:

#### Overview Tab
- Certname and current status
- Environment assignment
- Catalog and report timestamps
- Deactivated/expired status

#### Facts Tab
- All facts from PuppetDB
- Structured fact navigation
- Search and filter capabilities
- Export fact data

#### Reports Tab
- Complete run history
- Success/failure status
- Resource counts
- Corrective vs. intentional changes
- Click to view detailed report

#### Classification Tab
- Groups this node belongs to
- Classification rules that matched
- Applied classes and parameters
- Variables available to the node
- Pinned group memberships

**Actions:**
- Deactivate/Reactivate node
- Trigger new Puppet run (if configured)
- Export node data
- View node certificate status

---

## Node Groups & Classification

Node groups enable Puppet Enterprise-style classification.

### Creating a Node Group

1. Navigate to **Groups**
2. Click **Create Group**
3. Configure:
   - **Name**: Unique group identifier
   - **Description**: Purpose of this group
   - **Parent Group**: Optional hierarchy (inherits rules/classes)
   - **Environment**: Scope to specific Puppet environment
   - **Rule Match Type**:
     - `all`: Node must match ALL rules
     - `any`: Node must match ANY rule

### Classification Rules

Rules automatically match nodes based on facts.

**Rule Components:**
- **Fact Path**: Dot-notation path to fact (e.g., `os.family`, `trusted.extensions.pp_role`)
- **Operator**: Comparison method
  - `=`: Equals
  - `!=`: Not equals
  - `~`: Regex match
  - `!~`: Regex does not match
  - `>`, `>=`, `<`, `<=`: Numeric comparisons
  - `in`: Value in list
  - `not_in`: Value not in list
- **Value**: Expected value or pattern

**Examples:**
```yaml
# Match all CentOS nodes
fact_path: os.family
operator: =
value: RedHat

# Match web servers by role
fact_path: trusted.extensions.pp_role
operator: =
value: webserver

# Match memory greater than 8GB
fact_path: memory.system.total_bytes
operator: >
value: 8589934592
```

### Pinned Nodes

Manually assign nodes to groups regardless of rules.

1. Open a group
2. Go to **Pinned Nodes** tab
3. Click **Add Node**
4. Enter certname
5. Click **Pin**

**Use Cases:**
- Exception handling
- Testing classification changes
- Temporary assignments

### Classes and Parameters

Define Puppet classes to apply to matching nodes.

**Format:**
```json
{
  "class_name": {
    "parameter1": "value1",
    "parameter2": "value2"
  },
  "another_class": {}
}
```

**Examples:**
```json
{
  "apache": {
    "default_vhost": false,
    "mpm_module": "prefork"
  },
  "firewall": {
    "ensure": "running"
  }
}
```

### Variables

Key-value pairs available to external facts via facter templates.

**Format:**
```json
{
  "datacenter": "us-east-1",
  "backup_window": "02:00-04:00",
  "monitoring_enabled": true
}
```

### Testing Classification

1. Go to **Nodes** → Select a node → **Classification** tab
2. View matched groups and why they matched
3. See effective classes, parameters, and variables
4. Verify inheritance from parent groups

---

## Facts & Facter Templates

### Browsing Facts

Navigate to **Facts** to explore PuppetDB facts.

**Features:**
- Browse all fact names
- View fact values across nodes
- Search for specific facts
- Filter by environment
- Export fact data

**Common Fact Paths:**
- `os.*`: Operating system information
- `networking.*`: Network interfaces and configuration
- `memory.*`: Memory statistics
- `processors.*`: CPU information
- `trusted.*`: Certificate trusted data
- `custom.*`: Your custom facts

### Facter Templates

Generate external facts for nodes based on classification and existing facts.

#### Creating a Template

1. Navigate to **Facter Templates**
2. Click **Create Template**
3. Configure:
   - **Name**: Template identifier
   - **Description**: Template purpose
   - **Format**: YAML or JSON output
   - **Content**: Template definition

#### Template Syntax

Templates support Handlebars-style templating:

**Available Variables:**
- `{{certname}}`: Node certname
- `{{environment}}`: Node environment
- `{{groups}}`: Array of group names
- `{{variables.key}}`: Group variables
- `{{fact.path.to.value}}`: Existing facts

**Example Template:**
```yaml
# config.yaml
datacenter: {{variables.datacenter}}
role: {{variables.role}}
environment: {{environment}}
node_class: {{fact.trusted.extensions.pp_role}}
is_production: {{#if (eq environment "production")}}true{{else}}false{{/if}}
```

#### Exporting Facts

**Per Node:**
1. Go to **Nodes** → Select node
2. Click **Export Facts**
3. Select template
4. Download generated facts file

**API Export:**
```bash
curl -H "Authorization: Bearer <token>" \
  https://<host>/api/v1/facter/export/<certname>?template=<template_name>
```

**Bulk Export:**
Use the API to generate facts for multiple nodes in CI/CD pipelines.

---

## Reports

View and analyze Puppet run reports.

### Reports List

**Columns:**
- Certname
- Environment
- Transaction UUID
- Report timestamp
- Status (success/failure/pending)
- Resource counts (changed, failed, total)
- Runtime duration

**Filters:**
- Date range
- Environment
- Status
- Certname search

### Report Detail

Click any report to see:

**Summary:**
- Run timestamp and duration
- Catalog version
- Configuration version
- Puppet version
- Report format

**Resources:**
- All resources in the catalog
- Resource type and title
- Status (unchanged, changed, failed, skipped)
- Events and messages
- File line numbers for debugging

**Logs:**
- Structured log messages
- Log level (debug, info, notice, warning, err)
- Source location in manifests
- Timestamps

**Metrics:**
- Resource timing breakdown
- Catalog compilation time
- Plugin sync time
- Total run time

**Events:**
- Chronological event timeline
- Resource changes
- Before/after values for changed properties

### Report Analysis

**Common Workflows:**

**Troubleshoot Failed Run:**
1. Filter reports by Status: Failed
2. Open failing report
3. Check **Resources** tab for failed resources
4. Review error messages in **Logs**
5. Identify manifest file and line number

**Track Changes:**
1. Filter by Status: Changed
2. Review **Events** to see what changed
3. Verify changes were expected
4. Check for corrective changes (unmanaged drift)

**Performance Analysis:**
1. Sort by Runtime (descending)
2. Identify slow runs
3. Check **Metrics** for bottlenecks
4. Review resource timing

---

## Analytics

Advanced analytics and visualizations.

### Available Charts

**Node Status Trends**
- Historical node status over time
- Identify patterns in failures
- Track infrastructure growth

**Environment Distribution**
- Pie chart of nodes per environment
- Quickly see environment balance

**Puppet Version Distribution**
- Track agent version compliance
- Plan upgrades based on adoption

**Run Time Analysis**
- Average, min, max run times
- Identify performance degradation
- Compare environments

**Resource Statistics**
- Most common resource types
- Resource count trends
- Catalog size growth

### Time Range Selection

- Last 24 hours
- Last 7 days
- Last 30 days
- Custom date range

### Exporting Analytics

- Export charts as PNG
- Export raw data as CSV
- Schedule automated reports

---

## Alerting

Proactive monitoring with customizable alerts.

### Alert Channels

Configure notification channels for alerts.

#### Webhook Channel
```json
{
  "name": "Ops Webhook",
  "type": "webhook",
  "url": "https://your-webhook.example.com/alerts",
  "headers": {
    "Authorization": "Bearer <token>"
  },
  "method": "POST"
}
```

#### Email Channel
```json
{
  "name": "Ops Team Email",
  "type": "email",
  "recipients": ["ops@example.com", "oncall@example.com"],
  "smtp_server": "smtp.example.com",
  "smtp_port": 587,
  "use_tls": true
}
```

#### Slack Channel
```json
{
  "name": "Slack #ops",
  "type": "slack",
  "webhook_url": "https://hooks.slack.com/services/YOUR/WEBHOOK/URL",
  "channel": "#ops",
  "username": "OpenVox Alerts"
}
```

#### Microsoft Teams Channel
```json
{
  "name": "Teams Ops",
  "type": "teams",
  "webhook_url": "https://outlook.office.com/webhook/YOUR/WEBHOOK/URL"
}
```

### Alert Rules

Create rules to trigger notifications.

**Rule Types:**

**Node Status Alert**
- Trigger when node status changes
- Configure: which status, duration threshold
- Example: Alert if node fails for > 30 minutes

**Unreported Nodes**
- Trigger when nodes stop reporting
- Configure: time threshold
- Example: Alert if no report in 2 hours

**Compliance Alert**
- Trigger on compliance violations
- Configure: compliance rules
- Example: Alert on unauthorized package installation

**Failed Resources**
- Trigger on resource failures
- Configure: resource pattern, failure count
- Example: Alert on any service restart failure

**Custom Metric**
- Trigger on fact values or metrics
- Configure: fact path, operator, threshold
- Example: Alert if disk usage > 90%

#### Creating an Alert Rule

1. Navigate to **Alerting** → **Rules**
2. Click **Create Rule**
3. Configure:
   - **Name**: Descriptive rule name
   - **Type**: Select alert type
   - **Conditions**: Set thresholds and criteria
   - **Channels**: Select notification channels
   - **Severity**: Info, Warning, Critical
   - **Enabled**: Active/inactive toggle

### Active Alerts

View currently firing alerts.

**Actions:**
- **Acknowledge**: Mark as seen, stops repeat notifications
- **Resolve**: Manually clear the alert
- **Silence**: Temporarily disable notifications (set duration)
- **View Details**: See alert history and related data

### Alert History

- View resolved and expired alerts
- Analyze alert patterns
- Track MTTR (Mean Time To Resolution)
- Export alert data for analysis

### Silences

Temporarily suppress alerts during maintenance.

**Creating a Silence:**
1. Go to **Alerting** → **Silences**
2. Click **Create Silence**
3. Configure:
   - **Matcher**: Alert rule or pattern to silence
   - **Duration**: How long to silence (e.g., 2 hours)
   - **Reason**: Why you're silencing (required)
   - **Created By**: Automatically filled

**Use Cases:**
- Scheduled maintenance windows
- Known issues being worked on
- Testing/development activities

---

## Puppet CA Management

Manage Certificate Authority operations.

### Certificate Signing Requests (CSRs)

**Viewing Pending CSRs:**
1. Navigate to **CA** → **Requests** tab
2. See all pending certificate requests
3. View request details (certname, fingerprint, requested_at)

**Signing Requests:**
1. Review the certname and fingerprint
2. Verify this is a legitimate request
3. Click **Sign**
4. Node can now check in to Puppet

**Rejecting Requests:**
1. Click **Reject** on a CSR
2. Node must regenerate CSR to resubmit

**Cleaning Requests:**
- Remove rejected or abandoned CSRs
- Click **Clean** to delete from CA

### Certificate Management

**Viewing Signed Certificates:**
1. Go to **CA** → **Certificates** tab
2. See all signed certificates
3. View expiration dates
4. Check certificate status

**Revoking Certificates:**
1. Find the certificate
2. Click **Revoke**
3. Confirm revocation
4. Node can no longer authenticate

**Certificate Renewal:**
- Monitor certificate expiration dates
- Renew certificates before expiry
- Automated renewal workflows (if configured)

### CA Certificate Status

**Dashboard Widget:**
- CA certificate expiration date
- Days until expiration
- Warning indicators (< 90 days)

**Renewing CA Certificate:**
1. Go to **CA** → **Settings**
2. Click **Renew CA Certificate**
3. Follow renewal workflow
4. Update clients with new CA cert

### Certificate Auto-Signing

Configure auto-signing policies (if enabled):

1. Go to **CA** → **Settings** → **Auto-Sign**
2. Configure patterns:
   - Domain-based: `*.internal.example.com`
   - IP-based: `10.0.0.0/8`
   - Challenge-based: Shared secret validation

**Security Note:** Auto-signing should only be used in trusted networks with additional verification.

---

## Code Deploy

Manage and deploy Puppet code to environments.

### Code Repository Integration

**Setup:**
1. Navigate to **Code Deploy**
2. Click **Configure Repository**
3. Enter:
   - Repository URL (Git)
   - Branch mapping to environments
   - Deploy credentials (if required)

### Deploying Code

**Manual Deployment:**
1. Go to **Code Deploy**
2. Select environment
3. Select branch/tag/commit
4. Click **Deploy**
5. Monitor deployment progress

**Deployment Status:**
- Pending: Queued for deployment
- In Progress: Currently deploying
- Success: Deployed successfully
- Failed: Deployment error (check logs)

**Deployment History:**
- Who deployed
- When deployed
- What was deployed (commit hash)
- Deployment duration
- Success/failure status

### Environment Management

**Creating Environments:**
1. Go to **Code Deploy** → **Environments**
2. Click **Create Environment**
3. Configure:
   - Name
   - Git branch mapping
   - Deploy on commit (auto-deploy)

**Environment Promotion:**
- Deploy to dev/test first
- Validate changes
- Promote to production

### Rollback

**Rolling Back Deployments:**
1. Go to deployment history
2. Find previous successful deployment
3. Click **Rollback to This Version**
4. Confirm rollback

---

## User Management

Manage users and their access.

### Creating Users

1. Navigate to **Users**
2. Click **Create User**
3. Fill in:
   - **Username**: Unique identifier
   - **Email**: User email address
   - **Authentication Provider**:
     - `local`: Password authentication
     - `saml`: SSO only
     - `both`: Can use either method
   - **Organization**: Multi-tenant scope
   - **Roles**: Assign roles (see Roles & Permissions)

4. Click **Create**
5. User receives initial password (local) or can login via SSO (saml)

### User Details

**Viewing User Information:**
- Username and email
- Assigned roles
- Organization membership
- Authentication method
- Last login timestamp
- Account status (active/locked)

**Editing Users:**
1. Open user detail
2. Click **Edit**
3. Modify:
   - Email address
   - Roles
   - Authentication provider
   - Force password change flag

### User Actions

**Reset Password:**
1. Open user detail
2. Click **Reset Password**
3. Choose:
   - Generate random password
   - Set specific password
4. User must change on next login

**Lock/Unlock Account:**
- Temporarily disable access without deleting
- Useful for security incidents or departures

**Delete User:**
- Permanently removes user
- Audit logs are retained
- Cannot be undone

### Bulk Operations

**Import Users:**
1. Go to **Users** → **Import**
2. Upload CSV file with columns:
   - username, email, auth_provider, roles
3. Review import preview
4. Confirm import

**Export Users:**
- Export user list as CSV
- Useful for auditing or backup

---

## Roles & Permissions

Fine-grained access control with Role-Based Access Control (RBAC).

### Built-in Roles

**Super Admin**
- Full system access across all organizations
- Can create/manage organizations
- Can impersonate other users (audit logged)
- Can view all audit logs

**Admin**
- Full access within their organization
- Cannot manage other organizations
- Can manage users and roles
- Can configure settings

**Operator**
- Read/write access to nodes, groups, facts
- Cannot manage users or roles
- Cannot change system settings
- Can create and manage alerts

**Viewer**
- Read-only access
- Cannot make changes
- Can view nodes, facts, reports
- Cannot create alerts or deploy code

**Group Admin**
- Can manage specific node groups
- Cannot access other groups
- Limited to assigned groups only

**Auditor**
- Read-only access to audit logs
- Can view all security events
- Cannot view or modify resources

### Custom Roles

**Creating a Custom Role:**
1. Navigate to **Roles**
2. Click **Create Role**
3. Configure:
   - **Name**: Role identifier
   - **Display Name**: Human-readable name
   - **Description**: Role purpose
   - **Permissions**: Select specific permissions

### Permissions Structure

Permissions follow the pattern: `resource:action`

**Resources:**
- `nodes`: Puppet nodes
- `groups`: Node groups
- `facts`: Puppet facts
- `reports`: Puppet reports
- `users`: User accounts
- `roles`: Role definitions
- `settings`: System configuration
- `ca`: Certificate authority
- `code_deploy`: Code deployment
- `alerts`: Alerting rules and history

**Actions:**
- `read`: View resource
- `create`: Create new resource
- `update`: Modify existing resource
- `delete`: Remove resource
- `execute`: Perform special actions (e.g., sign cert, deploy code)

**Examples:**
- `nodes:read`: Can view nodes
- `groups:create`: Can create node groups
- `ca:execute`: Can sign certificates
- `settings:update`: Can change configuration

### Permission Scopes

Permissions can be scoped:

**Global Scope:**
- Applies across entire organization
- No additional restrictions

**Environment Scope:**
- Limit to specific Puppet environments
- User only sees nodes/groups in those environments

**Group Scope:**
- Limit to specific node groups
- Useful for Group Admin role

### Assigning Roles

**To a User:**
1. Go to **Users** → Select user
2. Click **Assign Role**
3. Select role(s)
4. Optionally add scope restrictions
5. Save

**Multiple Roles:**
- Users can have multiple roles
- Permissions are combined (union)
- Most permissive wins

### Testing Permissions

**As Admin:**
1. Go to **Users** → Select user
2. Click **Preview Permissions**
3. See effective permissions for this user
4. Test specific actions

---

## Settings

Configure system-wide settings.

### PuppetDB Configuration

**Connection Settings:**
- **URL**: PuppetDB base URL (e.g., `https://puppetdb:8081`)
- **Timeout**: Request timeout in seconds
- **SSL Verify**: Enable certificate verification
- **Certificate**: Client certificate for auth
- **Private Key**: Client key for auth
- **CA Certificate**: CA cert for verification

**Testing Connection:**
1. Configure settings
2. Click **Test Connection**
3. Verify successful connection
4. Check PuppetDB version and status

### Puppet CA Configuration

**Connection Settings:**
- **URL**: Puppet Server CA API URL
- **Certificate Auth**: Client cert and key
- **Timeout**: Request timeout

**Operations:**
- Test connectivity
- View CA certificate status
- Configure auto-signing (if enabled)

### Authentication Settings

**JWT Token Configuration:**
- **Token Expiry**: Access token lifetime (default: 24 hours)
- **Refresh Token Expiry**: Refresh token lifetime (default: 7 days)
- **Secret Key**: JWT signing secret (auto-generated)

**Password Policy:**
- **Minimum Length**: Default 8 characters
- **Complexity Requirements**: Upper, lower, number, special char
- **Max Login Attempts**: Before account lockout (default: 5)
- **Lockout Duration**: Account lock time (default: 15 minutes)

**SAML SSO Configuration:**
- Configured via `config.yaml` (see Configuration docs)
- View SAML metadata
- Test SAML connection
- User mapping settings

### Cache Configuration

**Cache Settings:**
- **Enable Caching**: Toggle cache on/off
- **Default TTL**: Time-to-live for cached data (seconds)
- **Max Entries**: Maximum cache size

**Cache Types:**
- Node data cache
- Fact cache
- Report cache

**Clear Cache:**
- Clear all cached data
- Force fresh data from PuppetDB

### Dashboard Settings

**Refresh Interval:**
- Auto-refresh frequency (seconds)
- Set to 0 to disable auto-refresh

**Widget Configuration:**
- Enable/disable specific dashboard widgets
- Set default time ranges
- Configure chart colors

### Notification Settings

**Email Configuration:**
- SMTP server and port
- TLS/SSL settings
- Authentication credentials
- From address and name

**Test Notifications:**
1. Configure notification channel
2. Click **Send Test Notification**
3. Verify receipt

### System Maintenance

**Database Maintenance:**
- Vacuum database
- Rebuild indexes
- View database size and stats

**Log Management:**
- View application logs
- Set log level (debug, info, warn, error)
- Download log files
- Configure log rotation

---

## User Profile

Manage your personal account settings.

### Viewing Profile

Navigate to **Profile** (user menu → Profile)

**Displayed Information:**
- Username
- Email address
- Assigned roles
- Authentication provider (LOCAL, SAML, or BOTH)
- Organization membership

### Changing Password

**For Local or Both Auth Users:**

1. Go to **Profile** → **Change Password** section
2. Enter:
   - Current password
   - New password (min 8 characters)
   - Confirm new password
3. Click **Change Password**

**Notes:**
- SSO-only users (auth_provider: saml) do not see this section
- Passwords must meet complexity requirements
- Cannot reuse current password

### Theme Preference

**Selecting Theme:**
1. Go to **Profile** → **Appearance** section
2. Choose theme:
   - **Light**: Light color scheme
   - **Dark**: Dark color scheme
3. Theme applies immediately
4. Preference is saved to your browser

### Viewing Assigned Roles

See your effective roles and permissions:
1. Go to **Profile**
2. View **Role** badge showing your primary role
3. See **Authentication** method badge

### Session Information

- Last login timestamp
- Current session expiry
- Active API keys (if any)

---

## Best Practices

### Node Classification

1. **Use descriptive group names**: `webservers-prod-us-east` not `group1`
2. **Leverage parent groups**: Create hierarchies (e.g., `all` → `production` → `webservers`)
3. **Start with simple rules**: Test with one or two rules before adding complexity
4. **Use pinning sparingly**: Prefer rules for scalability
5. **Document your variables**: Add descriptions in group notes

### Security

1. **Principle of least privilege**: Give users minimum necessary permissions
2. **Use SSO when available**: Centralize authentication management
3. **Rotate API keys regularly**: Set expiration dates
4. **Review audit logs**: Check for suspicious activity
5. **Enable MFA**: If your SSO provider supports it
6. **Lock accounts promptly**: When users leave or roles change

### Alert Management

1. **Start with critical alerts**: Add more as you tune thresholds
2. **Set appropriate thresholds**: Avoid alert fatigue
3. **Use silences for maintenance**: Better than disabling rules
4. **Group similar alerts**: Create grouped notification channels
5. **Review alert history**: Identify patterns and improve rules

### Performance

1. **Enable caching**: Reduces PuppetDB load
2. **Use environment filters**: Narrow data queries
3. **Limit report retention**: Archive old reports
4. **Monitor dashboard refresh rate**: Don't set too aggressive
5. **Use API pagination**: When fetching large datasets

### Backup and Recovery

1. **Back up SQLite database regularly**: Contains all classification and config
2. **Export critical groups**: Keep YAML backups of important groups
3. **Document custom roles**: Export role definitions
4. **Version control config.yaml**: Track configuration changes
5. **Test restore procedures**: Verify backups are valid

---

## Troubleshooting

### Cannot Login

**Local Auth:**
- Verify username and password
- Check if account is locked (contact admin)
- Verify force_password_change flag isn't set

**SAML SSO:**
- Verify SAML is enabled in Settings
- Check IdP connectivity
- Verify user exists and has `auth_provider` = `saml` or `both`
- Check SAML metadata is current
- Review browser console for errors

### Nodes Not Appearing

1. Verify PuppetDB connection in Settings
2. Check PuppetDB query permissions
3. Verify node reported recently to Puppet
4. Check environment filters
5. Review application logs for errors

### Classification Not Working

1. Verify rules match node facts (check node Facts tab)
2. Test rule syntax with a single known node
3. Check rule match type (all vs any)
4. Verify parent group rules aren't conflicting
5. Review classification in Node Detail → Classification tab

### Alerts Not Firing

1. Verify alert rule is enabled
2. Check alert conditions are met
3. Test notification channel
4. Check for active silences
5. Review alert history for errors
6. Verify channel configuration (webhook URL, email settings, etc.)

### Performance Issues

1. Check PuppetDB response times
2. Enable caching if disabled
3. Reduce dashboard refresh frequency
4. Limit report history retention
5. Check database size and vacuum if needed

### Permission Denied

1. Verify your role assignments
2. Check required permission for the action
3. Verify environment scope if set
4. Contact admin to request additional permissions
5. Check audit logs for permission failures

---

## Keyboard Shortcuts

### Global

- `?`: Show help dialog (if implemented)
- `/`: Focus search box
- `Esc`: Close modals/dialogs

### Navigation

- `g` then `d`: Go to Dashboard
- `g` then `n`: Go to Nodes
- `g` then `g`: Go to Groups
- `g` then `f`: Go to Facts
- `g` then `r`: Go to Reports

### List Views

- `j`: Next item
- `k`: Previous item
- `Enter`: Open selected item
- `Ctrl+R` or `Cmd+R`: Refresh list

---

## API Access

All features are available via REST API.

### Authentication

**JWT Token:**
```bash
# Login to get token
curl -X POST https://<host>/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"user","password":"pass"}'

# Use token in subsequent requests
curl -H "Authorization: Bearer <token>" \
  https://<host>/api/v1/nodes
```

**API Key:**
```bash
# Use API key in header
curl -H "X-API-Key: <api-key>" \
  https://<host>/api/v1/nodes

# Or in Authorization header
curl -H "Authorization: ApiKey <api-key>" \
  https://<host>/api/v1/nodes
```

### API Documentation

Complete API reference available at:
- OpenAPI/Swagger UI: `https://<host>/api/docs`
- API spec: `https://<host>/api/v1/openapi.json`

---

## Getting Help

### Documentation

- **Installation**: `docs/INSTALLATION.md`
- **Configuration**: `docs/CONFIGURATION.md`
- **API Reference**: `docs/api/`
- **Architecture**: `docs/architecture/`

### Support

- **GitHub Issues**: https://github.com/ffquintella/openvox-webui/issues
- **Discussions**: https://github.com/ffquintella/openvox-webui/discussions

### Contributing

See `CONTRIBUTING.md` for how to contribute to the project.

---

## Glossary

- **Certname**: Unique identifier for a Puppet node (usually FQDN)
- **Classification**: Process of determining which classes/parameters apply to a node
- **External Facts**: Facts generated outside Puppet agent (e.g., from scripts)
- **Facter**: Puppet's system profiling tool that collects facts
- **IdP**: Identity Provider (for SAML SSO)
- **PuppetDB**: Central storage for Puppet data (facts, catalogs, reports)
- **RBAC**: Role-Based Access Control
- **SSO**: Single Sign-On
- **Catalog**: Compiled set of resources that should be applied to a node
- **Report**: Record of a Puppet agent run
- **Resource**: Manageable item in Puppet (file, package, service, etc.)
- **CSR**: Certificate Signing Request
- **CA**: Certificate Authority
