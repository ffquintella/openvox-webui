# Puppet Environment Troubleshooting Guide

This guide helps diagnose and fix environment mismatch issues between Puppet agents and the Puppet Server when using OpenVox WebUI as an External Node Classifier (ENC).

## The Problem

When running `puppet agent -t --environment=pserver`, you may see:

```
Notice: Local environment: 'pserver' doesn't match server specified environment 'production',
restarting agent run with environment 'production'
```

This indicates a mismatch between:
1. What the agent requests (`--environment=pserver`)
2. What the ENC/server assigns (`production`)

## Root Causes

### 1. Environment Directory Doesn't Exist
The ENC returns `environment: pserver`, but `/etc/puppetlabs/code/environments/pserver/` doesn't exist on the Puppet Server.

### 2. Agent Config Conflicts with ENC
The `puppet.conf` has `environment = production` in the `[agent]` section, which conflicts with ENC's environment assignment.

### 3. Pinned Nodes Ignoring Environment Filter
In OpenVox WebUI, pinned nodes are assigned to groups regardless of environment filtering. If a node is pinned to a "production" group, it gets the production environment even if you request `pserver`.

### 4. ENC Not Returning Correct Environment
The ENC script may have bugs or the classification logic in OpenVox WebUI may not be working as expected.

## Diagnostic Tools

### Script 1: Diagnose Environment Issues

Run the diagnostic script to identify the problem:

```bash
# On the Puppet agent node
sudo /path/to/openvox-webui/scripts/diagnose-puppet-environment.sh

# Or specify a certname
sudo /path/to/openvox-webui/scripts/diagnose-puppet-environment.sh node.example.com
```

**What it checks:**
- Puppet agent configuration (puppet.conf)
- ENC configuration and script execution
- OpenVox WebUI classification API
- Available environments on Puppet Server
- Puppet Server logs for environment issues
- Catalog compilation tests

**Sample Output:**
```
═══════════════════════════════════════════════════════════
   Puppet Environment Diagnostic Tool
═══════════════════════════════════════════════════════════

▶ Puppet Agent Configuration
───────────────────────────────────────────────────────────
✓ Found puppet.conf

Environment setting in puppet.conf:
  environment = production

▶ OpenVox WebUI Classification
───────────────────────────────────────────────────────────
ℹ Querying: http://localhost:8080/api/v1/nodes/node.example.com/classification
✓ Retrieved classification from OpenVox WebUI

{
  "environment": "pserver",
  "classes": {...}
}

ℹ Environment from WebUI: pserver

▶ Available Environments on Puppet Server
───────────────────────────────────────────────────────────
✓ Found environments directory: /etc/puppetlabs/code/environments

Available environments:
  ✓ production (has environment.conf)
  ✗ pserver (MISSING)
```

### Script 2: Auto-Fix Common Issues

Run the fix script to automatically resolve common problems:

```bash
# On the Puppet Server (must run as root)
sudo /path/to/openvox-webui/scripts/fix-puppet-environment.sh

# Or specify a certname
sudo /path/to/openvox-webui/scripts/fix-puppet-environment.sh node.example.com
```

**What it does:**
1. Queries OpenVox WebUI for node classification
2. Creates missing environment directories
3. Removes conflicting `environment` settings from puppet.conf
4. Tests catalog compilation
5. Provides next steps

**Interactive Example:**
```
═══════════════════════════════════════════════════════════
   Puppet Environment Auto-Fix Tool
═══════════════════════════════════════════════════════════

▶ Fetching Node Classification
───────────────────────────────────────────────────────────
ℹ Querying: http://localhost:8080/api/v1/nodes/node.example.com/classification
✓ Retrieved classification
✓ Node environment: pserver

▶ Checking Environment Directory
───────────────────────────────────────────────────────────
✗ Environment directory does not exist: /etc/puppetlabs/code/environments/pserver

Create environment directory for 'pserver'? [y/N]: y

ℹ Creating environment directory...
✓ Environment directory created: /etc/puppetlabs/code/environments/pserver
✓ Created manifests/site.pp
✓ Created environment.conf

▶ Checking puppet.conf Configuration
───────────────────────────────────────────────────────────
✗ Found environment setting in [agent] section: production

Remove environment setting from puppet.conf [agent] section? [y/N]: y

ℹ Backing up puppet.conf...
ℹ Removing environment setting from [agent] section...
✓ Environment setting removed from puppet.conf
```

## Manual Fix Steps

If you prefer to fix issues manually:

### Fix 1: Create Missing Environment

```bash
# On Puppet Server
sudo mkdir -p /etc/puppetlabs/code/environments/pserver/{manifests,modules,data}

# Create basic site.pp
sudo cat > /etc/puppetlabs/code/environments/pserver/manifests/site.pp <<EOF
node default {
  # Classification handled by ENC
}
EOF

# Create environment.conf
sudo cat > /etc/puppetlabs/code/environments/pserver/environment.conf <<EOF
modulepath = modules:\$basemodulepath
manifest = manifests/site.pp
EOF

# Set permissions
sudo chown -R puppet:puppet /etc/puppetlabs/code/environments/pserver
sudo chmod -R 755 /etc/puppetlabs/code/environments/pserver
```

### Fix 2: Remove Agent Environment Setting

```bash
# On Puppet agent node
sudo cp /etc/puppetlabs/puppet/puppet.conf /etc/puppetlabs/puppet/puppet.conf.backup

# Edit puppet.conf and remove the 'environment =' line from [agent] section
sudo vi /etc/puppetlabs/puppet/puppet.conf

# Or use sed
sudo sed -i.bak '/\[agent\]/,/\[/ { /^\s*environment\s*=/d; }' /etc/puppetlabs/puppet/puppet.conf
```

### Fix 3: Check Node Classification in WebUI

1. Open OpenVox WebUI: `http://your-server:8080`
2. Navigate to **Nodes** → Find your node
3. Check which groups the node belongs to
4. Verify the environment assignment
5. Check if the node is **pinned** to any groups
6. If pinned, verify the group's environment matches your expectation

### Fix 4: Test ENC Script Directly

```bash
# On Puppet Server
sudo /opt/openvox/enc.sh node.example.com

# Expected output (YAML):
---
environment: pserver
classes:
  base: {}
  apache: {}
parameters:
  foo: bar
```

## Understanding the Flow

```
┌─────────────────┐
│  Puppet Agent   │
│   (on node)     │
└────────┬────────┘
         │
         │ 1. Request catalog
         │    (may specify --environment)
         ▼
┌─────────────────┐
│ Puppet Server   │
└────────┬────────┘
         │
         │ 2. Query ENC for classification
         ▼
┌─────────────────┐
│  OpenVox WebUI  │
│      (ENC)      │
└────────┬────────┘
         │
         │ 3. Return classification
         │    (includes environment)
         ▼
┌─────────────────┐
│ Puppet Server   │
│  - Loads code   │
│    from env dir │
│  - Compiles     │
│    catalog      │
└────────┬────────┘
         │
         │ 4. Send catalog
         ▼
┌─────────────────┐
│  Puppet Agent   │
│  (applies it)   │
└─────────────────┘
```

## Best Practices

### 1. Don't Set Environment in puppet.conf
Let the ENC control environment assignment:

**Bad (in puppet.conf):**
```ini
[agent]
environment = production  # Don't do this when using ENC!
```

**Good (in puppet.conf):**
```ini
[agent]
server = puppet.example.com
# No environment setting - let ENC decide
```

### 2. Create All Required Environments
Before classifying nodes to an environment, ensure it exists:

```bash
ls -la /etc/puppetlabs/code/environments/
drwxr-xr-x. 5 puppet puppet  56 Jan  1 12:00 production
drwxr-xr-x. 5 puppet puppet  56 Jan  1 12:00 pserver
drwxr-xr-x. 5 puppet puppet  56 Jan  1 12:00 development
```

### 3. Use Pinning Carefully
Pinned nodes in OpenVox WebUI bypass environment filtering. Only pin nodes when you need them in a specific group regardless of other rules.

### 4. Test Classification Before Agent Runs
Always test ENC output before running puppet agent:

```bash
# Test ENC
sudo /opt/openvox/enc.sh $(hostname -f)

# Test catalog compilation (noop)
sudo puppet agent -t --noop
```

## Common Scenarios

### Scenario 1: New Environment for Development

**Goal:** Create a `development` environment for testing.

**Steps:**
1. Create environment directory (use auto-fix script or manual steps)
2. Deploy your Puppet code to the new environment
3. In OpenVox WebUI, create a group with environment filter `development`
4. Add classification rules to assign nodes to this group
5. Test with a node: `puppet agent -t`

### Scenario 2: Migrating Nodes Between Environments

**Goal:** Move nodes from `production` to `pserver`.

**Steps:**
1. Ensure `pserver` environment exists and has required code
2. In OpenVox WebUI, update classification rules or group assignments
3. On the node, run: `puppet agent -t`
4. Verify the node now uses `pserver` environment

### Scenario 3: Environment Exists But Wrong One is Used

**Goal:** Node should use `pserver` but gets `production`.

**Diagnosis:**
```bash
# Check what ENC returns
sudo /opt/openvox/enc.sh $(hostname -f)

# Check puppet.conf
grep environment /etc/puppetlabs/puppet/puppet.conf

# Check OpenVox WebUI classification
curl http://localhost:8080/api/v1/nodes/$(hostname -f)/classification
```

**Fix:**
- Remove `environment` from puppet.conf [agent] section
- Verify node classification in OpenVox WebUI
- Check if node is pinned to a group with different environment

## Troubleshooting Tips

### Enable Debug Logging

```bash
# Run agent with debug
puppet agent -t --environment=pserver --debug

# Or verbose
puppet agent -t --environment=pserver --verbose
```

### Check Puppet Server Logs

```bash
# Watch logs in real-time
sudo tail -f /var/log/puppetlabs/puppetserver/puppetserver.log

# Filter for environment issues
sudo grep -i environment /var/log/puppetlabs/puppetserver/puppetserver.log | tail -20
```

### Verify ENC in Puppet Server Config

```bash
# On Puppet Server, check puppet.conf
sudo grep -A5 "\[master\]" /etc/puppetlabs/puppet/puppet.conf

# Should have:
# node_terminus = exec
# external_nodes = /opt/openvox/enc.sh
```

### Test API Directly

```bash
# Get node classification
curl -s http://localhost:8080/api/v1/nodes/$(hostname -f)/classification | python3 -m json.tool

# List all groups
curl -s http://localhost:8080/api/v1/groups | python3 -m json.tool

# Get node details
curl -s http://localhost:8080/api/v1/nodes/$(hostname -f) | python3 -m json.tool
```

## Getting Help

If you're still having issues:

1. Run the diagnostic script and save output:
   ```bash
   sudo ./scripts/diagnose-puppet-environment.sh > diagnosis.txt 2>&1
   ```

2. Check OpenVox WebUI logs:
   ```bash
   journalctl -u openvox-webui -f
   ```

3. Verify the node appears in PuppetDB:
   ```bash
   curl http://localhost:8080/api/v1/nodes
   ```

4. Review the [NOTIFICATIONS.md](NOTIFICATIONS.md) documentation for creating diagnostic notifications

## See Also

- [Puppet Environments Documentation](https://puppet.com/docs/puppet/latest/environments.html)
- [External Node Classifiers](https://puppet.com/docs/puppet/latest/nodes_external.html)
- OpenVox WebUI API Documentation
- [Classification Rules Documentation](../README.md#classification-rules)
