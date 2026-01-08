# OpenVox WebUI - Quick Start Guides

Quick guides for common workflows and getting started quickly.

## Table of Contents

- [Quick Start: First-Time Setup](#quick-start-first-time-setup)
- [Quick Start: Setting Up SAML SSO](#quick-start-setting-up-saml-sso)
- [Quick Start: Creating Your First Node Group](#quick-start-creating-your-first-node-group)
- [Quick Start: Setting Up Alerts](#quick-start-setting-up-alerts)
- [Quick Start: Generating External Facts](#quick-start-generating-external-facts)
- [Quick Start: Managing Certificates](#quick-start-managing-certificates)
- [Quick Start: Creating Custom Roles](#quick-start-creating-custom-roles)

---

## Quick Start: First-Time Setup

Get OpenVox WebUI up and running in 10 minutes.

### Prerequisites

- OpenVox WebUI installed (see [INSTALLATION.md](INSTALLATION.md))
- PuppetDB accessible
- Admin credentials

### Steps

**1. Initial Login (2 minutes)**

```bash
# Access the WebUI
https://your-server.example.com

# Login with initial admin credentials
Username: admin
Password: <from config.yaml initial_admin.password>
```

**2. Change Admin Password (1 minute)**

- You'll be prompted to change your password on first login
- Choose a strong password (min 8 characters)

**3. Configure PuppetDB Connection (3 minutes)**

```bash
Navigate to: Settings → PuppetDB
```

Configure:
```yaml
URL: https://puppetdb.example.com:8081
SSL Verify: true
Certificate: /path/to/client-cert.pem
Private Key: /path/to/client-key.pem
CA Certificate: /path/to/ca.pem
```

Click **Test Connection** → Should see "✓ Connected successfully"

**4. Verify Nodes Are Visible (2 minutes)**

```bash
Navigate to: Nodes
```

- You should see your Puppet nodes
- If empty, check PuppetDB connection and logs

**5. Create Your First User (2 minutes)**

```bash
Navigate to: Users → Create User
```

```yaml
Username: john.doe
Email: john.doe@example.com
Auth Provider: local
Roles: viewer
```

Click **Create** → User can now login

**Done!** You're ready to start using OpenVox WebUI.

---

## Quick Start: Setting Up SAML SSO

Enable enterprise Single Sign-On in 15 minutes.

### Prerequisites

- Enterprise Identity Provider (Azure AD, Okta, ADFS, etc.)
- Admin access to IdP
- Access to OpenVox WebUI server

### Steps

**1. Get IdP Metadata (5 minutes)**

**For Azure AD:**
```bash
# Azure AD → Enterprise Applications → Your App
# Single sign-on → SAML → Download Federation Metadata XML
```

**For Okta:**
```bash
# Okta → Applications → Your App
# Sign On → View Setup Instructions → IdP metadata
```

**2. Configure OpenVox WebUI (5 minutes)**

Edit `/etc/openvox-webui/config.yaml`:

```yaml
saml:
  enabled: true

  sp:
    entity_id: "https://your-openvox.example.com/saml"
    acs_url: "https://your-openvox.example.com/api/v1/auth/saml/acs"
    sign_requests: false
    require_signed_assertions: true

  idp:
    metadata_file: "/etc/openvox-webui/saml-metadata.xml"

  user_mapping:
    username_attribute: "sAMAccountName"  # or "email" for Okta
    email_attribute: "mail"
    require_existing_user: true
```

Save IdP metadata:
```bash
sudo cp metadata.xml /etc/openvox-webui/saml-metadata.xml
sudo systemctl restart openvox-webui
```

**3. Get SP Metadata (2 minutes)**

```bash
curl https://your-openvox.example.com/api/v1/auth/saml/metadata > sp-metadata.xml
```

**4. Configure IdP (3 minutes)**

Upload `sp-metadata.xml` to your IdP:

**Azure AD:**
```
Enterprise Applications → Your App → Single sign-on →
Upload metadata file → sp-metadata.xml
```

**Okta:**
```
Applications → Your App → General → Edit SAML Settings →
Configure manually or import SP metadata
```

**Set ACS URL:** `https://your-openvox.example.com/api/v1/auth/saml/acs`

**5. Create User in OpenVox (2 minutes)**

Users must exist before SSO login:

```bash
Navigate to: Users → Create User

Username: john.doe  # Must match IdP username attribute
Email: john.doe@example.com
Auth Provider: saml  # or "both" for SSO + password
Roles: operator
```

**6. Test SSO Login (2 minutes)**

1. Logout from OpenVox WebUI
2. Click **Login with SSO**
3. Authenticate with your IdP
4. Should redirect back to OpenVox WebUI dashboard

**Troubleshooting:**

```bash
# Check logs
sudo journalctl -u openvox-webui -f

# Common issues:
# - "User not found": Create user in OpenVox first
# - "SAML not enabled": Check config.yaml saml.enabled: true
# - "Invalid signature": Verify IdP metadata is current
# - "405 error": Ensure using latest version with HTTP 303 fix
```

---

## Quick Start: Creating Your First Node Group

Set up node classification in 10 minutes.

### Scenario

Create a group for all web servers running Apache.

### Steps

**1. Create the Group (3 minutes)**

```bash
Navigate to: Groups → Create Group
```

```yaml
Name: webservers
Description: All Apache web servers
Parent Group: (none)
Environment: production
Rule Match Type: all
```

Click **Create**

**2. Add Classification Rules (3 minutes)**

Click **Add Rule**:

**Rule 1: Match OS Family**
```yaml
Fact Path: os.family
Operator: =
Value: RedHat
```

**Rule 2: Match Custom Role**
```yaml
Fact Path: trusted.extensions.pp_role
Operator: =
Value: webserver
```

Click **Save Rules**

**3. Add Puppet Classes (2 minutes)**

In the group detail, go to **Classes** tab:

```json
{
  "apache": {
    "default_vhost": false,
    "mpm_module": "prefork",
    "default_mods": ["ssl", "rewrite", "headers"]
  },
  "firewall": {
    "port": 80,
    "protocol": "tcp"
  }
}
```

Click **Save**

**4. Add Variables (1 minute)**

Go to **Variables** tab:

```json
{
  "backup_enabled": true,
  "monitoring_enabled": true,
  "datacenter": "us-east-1"
}
```

Click **Save**

**5. Verify Classification (1 minute)**

```bash
Navigate to: Nodes → Pick a web server → Classification tab
```

- Should see "webservers" group listed
- Should see rules that matched
- Should see Apache class and parameters

**Test the classification:**
```bash
# On a Puppet agent
sudo puppet agent -t

# Check external facts
facter --custom-dir=/etc/puppetlabs/facter/facts.d
```

**Done!** Your web servers are now classified.

---

## Quick Start: Setting Up Alerts

Get notified about infrastructure issues in 10 minutes.

### Scenario

Alert when any node fails a Puppet run.

### Steps

**1. Create Notification Channel (3 minutes)**

```bash
Navigate to: Alerting → Channels → Create Channel
```

**For Slack:**
```yaml
Name: Ops Slack
Type: slack
Webhook URL: https://hooks.slack.com/services/YOUR/WEBHOOK/URL
Channel: #ops-alerts
Username: OpenVox Alerts
```

**For Email:**
```yaml
Name: Ops Email
Type: email
Recipients:
  - ops@example.com
  - oncall@example.com
SMTP Server: smtp.example.com
SMTP Port: 587
Use TLS: true
Username: alerts@example.com
Password: <smtp-password>
```

Click **Create** → **Test Channel** to verify

**2. Create Alert Rule (5 minutes)**

```bash
Navigate to: Alerting → Rules → Create Rule
```

```yaml
Name: Node Failed Puppet Run
Type: node_status
Description: Alert when any node has a failed Puppet run

Conditions:
  Status: failed
  Duration: 5 minutes  # Alert after failed for 5 min
  Environment: (all)   # Or specific environment

Notification:
  Channels:
    - Ops Slack
  Severity: critical
  Message Template: |
    Node {{certname}} has failed Puppet run
    Environment: {{environment}}
    Last Report: {{timestamp}}
    Failed Resources: {{failed_resources}}

Enabled: true
```

Click **Create**

**3. Test Alert (2 minutes)**

Option A: Simulate on a test node:
```bash
# On test node, cause a failure
sudo puppet agent -t --noop --detailed-exitcodes || true
```

Option B: Manually trigger:
```bash
# In OpenVox UI
Navigate to: Alerting → Rules → Your Rule → Test Alert
```

Check your notification channel for the alert.

**4. Create Additional Alert Rules (Optional)**

**Unreported Nodes:**
```yaml
Name: Nodes Not Reporting
Type: unreported
Duration: 2 hours
Severity: warning
```

**High Disk Usage:**
```yaml
Name: High Disk Usage
Type: custom_metric
Fact Path: disks./.used_bytes
Operator: >
Threshold: 90% of disks./.size_bytes
Severity: warning
```

**Done!** You're now monitoring your infrastructure.

---

## Quick Start: Generating External Facts

Create external facts from classification in 10 minutes.

### Scenario

Generate a config file for each node with datacenter, role, and backup settings.

### Steps

**1. Ensure Groups Have Variables (2 minutes)**

```bash
Navigate to: Groups → Your Group → Variables
```

Add variables:
```json
{
  "datacenter": "us-east-1",
  "backup_enabled": true,
  "backup_window": "02:00-04:00",
  "monitoring_enabled": true
}
```

**2. Create Facter Template (5 minutes)**

```bash
Navigate to: Facter Templates → Create Template
```

```yaml
Name: node-config
Description: Generate node configuration facts
Format: yaml

Content: |
  # Node Configuration Facts
  # Generated by OpenVox WebUI

  node_certname: {{certname}}
  node_environment: {{environment}}
  node_datacenter: {{variables.datacenter}}
  node_backup_enabled: {{variables.backup_enabled}}
  node_backup_window: {{variables.backup_window}}
  node_monitoring: {{variables.monitoring_enabled}}

  # Classification
  node_groups:
  {{#each groups}}
    - {{this}}
  {{/each}}

  # Hardware Info
  node_os_family: {{fact.os.family}}
  node_os_release: {{fact.os.release.full}}
  node_memory_gb: {{fact.memory.system.total_gb}}
  node_processor_count: {{fact.processors.count}}
```

Click **Create**

**3. Export Facts for Nodes (2 minutes)**

**Single Node:**
```bash
Navigate to: Nodes → Select Node → Export Facts → Select Template

# Or via API
curl -H "Authorization: Bearer <token>" \
  https://openvox.example.com/api/v1/facter/export/node01.example.com?template=node-config \
  -o /tmp/node01_facts.yaml
```

**Bulk Export (All Nodes):**
```bash
#!/bin/bash
# Export facts for all nodes

NODES=$(curl -s -H "Authorization: Bearer $TOKEN" \
  https://openvox.example.com/api/v1/nodes | jq -r '.[].certname')

for node in $NODES; do
  curl -s -H "Authorization: Bearer $TOKEN" \
    "https://openvox.example.com/api/v1/facter/export/${node}?template=node-config" \
    -o "/tmp/facts/${node}.yaml"
done
```

**4. Deploy Facts to Nodes (1 minute)**

**Copy to Puppet external facts directory:**
```bash
# Via Puppet file resource
file { '/etc/puppetlabs/facter/facts.d/node-config.yaml':
  ensure  => file,
  source  => 'puppet:///files/facts/node-config.yaml',
  mode    => '0644',
}

# Or via rsync/scp
for node in $NODES; do
  scp /tmp/facts/${node}.yaml ${node}:/etc/puppetlabs/facter/facts.d/node-config.yaml
done
```

**5. Verify Facts on Node (1 minute)**

```bash
# SSH to a node
facter node_certname
facter node_datacenter
facter node_groups
```

**Done!** Your nodes now have external facts from classification.

---

## Quick Start: Managing Certificates

Sign and manage Puppet certificates in 5 minutes.

### Scenario

New nodes need their certificates signed.

### Steps

**1. View Pending CSRs (1 minute)**

```bash
Navigate to: CA → Requests Tab
```

You'll see pending certificate requests with:
- Certname
- Fingerprint
- Request timestamp

**2. Verify Request is Legitimate (1 minute)**

**Check fingerprint on the node:**
```bash
# On the requesting node
sudo puppet agent --fingerprint
```

**Compare:**
- Fingerprint in WebUI should match node's fingerprint
- Certname should match the node's FQDN

**3. Sign Certificate (1 minute)**

In WebUI:
```bash
CA → Requests → Find the request → Click "Sign"
```

Or via API:
```bash
curl -X POST \
  -H "Authorization: Bearer <token>" \
  https://openvox.example.com/api/v1/ca/certificate/<certname>/sign
```

**4. Verify Signed (1 minute)**

```bash
Navigate to: CA → Certificates Tab
```

Certificate should now appear in signed list.

**On the node:**
```bash
sudo puppet agent -t
# Should now connect successfully
```

**5. Bulk Sign (For Multiple Nodes)**

```bash
# Get all pending requests
curl -H "Authorization: Bearer <token>" \
  https://openvox.example.com/api/v1/ca/requests \
  | jq -r '.[].certname' \
  | while read cert; do
      curl -X POST \
        -H "Authorization: Bearer <token>" \
        https://openvox.example.com/api/v1/ca/certificate/$cert/sign
    done
```

**Managing Certificates:**

**Revoke a certificate:**
```bash
CA → Certificates → Find cert → Click "Revoke"
```

**Clean a rejected request:**
```bash
CA → Requests → Find request → Click "Clean"
```

**Monitor CA expiration:**
```bash
CA → Dashboard Widget shows CA expiration date
```

**Done!** Your nodes are now signed and checking in.

---

## Quick Start: Creating Custom Roles

Create fine-grained access control in 10 minutes.

### Scenario

Create a "Web Team" role that can only manage web server groups.

### Steps

**1. Identify Required Permissions (2 minutes)**

Web Team needs to:
- View all nodes
- View all facts
- View and edit web server groups only
- Cannot create/delete groups
- Cannot access other groups or settings

**2. Create the Role (3 minutes)**

```bash
Navigate to: Roles → Create Role
```

```yaml
Name: web_team
Display Name: Web Team
Description: Team managing web server infrastructure

Permissions:
  # Read-only node access
  - resource: nodes
    action: read
    scope: global

  # Read-only fact access
  - resource: facts
    action: read
    scope: global

  # Read-only report access
  - resource: reports
    action: read
    scope: global

  # Group management (scoped to specific groups)
  - resource: groups
    action: read
    scope: global

  - resource: groups
    action: update
    scope: group
    scope_filter:
      groups:
        - webservers
        - webservers-prod
        - webservers-dev
```

Click **Create**

**3. Assign Role to Users (2 minutes)**

```bash
Navigate to: Users → Select User → Edit
```

Add role:
```yaml
Roles:
  - web_team
```

Click **Save**

**4. Test Permissions (2 minutes)**

Login as the user with web_team role:

**Should be able to:**
- View all nodes ✓
- View facts ✓
- View and edit webservers group ✓
- Add classes to webservers group ✓

**Should NOT be able to:**
- Edit other groups ✗
- Create new groups ✗
- Delete groups ✗
- Access Settings ✗
- Manage users ✗

**5. Create Additional Custom Roles (Examples)**

**Read-Only Auditor:**
```yaml
Name: auditor
Permissions:
  - resource: audit_logs
    action: read
  - resource: nodes
    action: read
  - resource: reports
    action: read
```

**Environment-Specific Operator:**
```yaml
Name: dev_operator
Permissions:
  - resource: nodes
    action: *
    scope: environment
    scope_filter:
      environments: [development, testing]

  - resource: groups
    action: *
    scope: environment
    scope_filter:
      environments: [development, testing]
```

**Certificate Manager:**
```yaml
Name: cert_manager
Permissions:
  - resource: ca
    action: read
  - resource: ca
    action: execute
  - resource: nodes
    action: read
```

**Done!** You now have custom roles tailored to your teams.

---

## Common Patterns and Tips

### Pattern: Gradual Rollout

When deploying new classification:

1. **Create test group** with rules
2. **Pin a few test nodes** manually
3. **Verify classification** on test nodes
4. **Remove pins**, let rules match organically
5. **Monitor for issues**
6. **Expand to production**

### Pattern: Multi-Tier Groups

Create hierarchical groups:

```
all_nodes (base facts, common variables)
├── production (prod-specific classes)
│   ├── webservers-prod
│   └── databases-prod
└── development (dev-specific classes)
    ├── webservers-dev
    └── databases-dev
```

### Pattern: Emergency Node Pinning

Quick temporary classification:

```bash
# Pin node to emergency group
Navigate to: Groups → emergency-patch → Pinned Nodes → Add node01

# Apply emergency changes
# Verify on node
# Remove pin when resolved
```

### Pattern: Fact-Based Auto-Discovery

Use custom facts for auto-classification:

```bash
# On nodes, set custom fact
echo "role: webserver" > /etc/puppetlabs/facter/facts.d/role.yaml

# In OpenVox, create group rule
fact_path: role
operator: =
value: webserver
```

### Pattern: API-Driven Workflows

Integrate with CI/CD:

```bash
#!/bin/bash
# Auto-provision nodes in CI/CD

# Create node in monitoring
create_monitoring_node "$HOSTNAME"

# Wait for Puppet CSR
while ! curl -s -H "Authorization: Bearer $TOKEN" \
  "https://openvox.example.com/api/v1/ca/requests" \
  | jq -e ".[] | select(.certname == \"$HOSTNAME\")"; do
  sleep 5
done

# Auto-sign
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "https://openvox.example.com/api/v1/ca/certificate/$HOSTNAME/sign"

# Trigger Puppet run
ssh $HOSTNAME 'sudo puppet agent -t'
```

---

## Next Steps

After completing these quick starts:

1. **Read the full [User Guide](USER_GUIDE.md)** for comprehensive feature documentation
2. **Review [Configuration Guide](CONFIGURATION.md)** for advanced settings
3. **Explore [API Documentation](api/)** for automation
4. **Join the community** on GitHub Discussions

---

## Getting Help

- **Documentation Issues**: Open an issue on GitHub
- **Feature Requests**: Submit via GitHub Issues
- **Community Support**: GitHub Discussions
- **Bug Reports**: Use the Bug Report template

---

**Quick Reference Links:**

- [Installation Guide](INSTALLATION.md)
- [Configuration Guide](CONFIGURATION.md)
- [Complete User Guide](USER_GUIDE.md)
- [API Documentation](api/)
- [Architecture Overview](architecture/)
