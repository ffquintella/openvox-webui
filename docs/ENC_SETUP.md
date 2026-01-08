# External Node Classifier (ENC) Setup Guide

This guide explains how to configure Puppet Server to use OpenVox WebUI as an External Node Classifier (ENC).

## Overview

An External Node Classifier allows Puppet Server to query an external service (OpenVox WebUI) to determine:
- Which environment a node should use
- Which classes should be applied to a node
- What parameters/variables should be set

OpenVox WebUI provides centralized node classification through its web interface and API.

## Prerequisites

- Puppet Server installed and running
- OpenVox WebUI installed and accessible
- Puppet module installed: `openvox_webui`

## Setup Methods

### Method 1: Using Puppet (Recommended)

The recommended approach is to use the `openvox_webui::enc` Puppet class to configure the ENC automatically.

#### Basic Setup

Add to your Puppet Server node's manifest or site.pp:

```puppet
# Basic setup with auto-detection
include openvox_webui::enc
```

#### Custom Configuration

```puppet
class { 'openvox_webui::enc':
  webui_url                => 'https://segdc1vpr0018.fgv.br',
  manage_puppet_conf       => true,
  restart_puppetserver     => true,
  ssl_verify               => false,  # For self-signed certs
  remove_agent_environment => true,   # Remove conflicting settings
}
```

**Note:** The ENC uses the public `/api/v1/nodes/:certname/classify` endpoint which does not require authentication.

#### Using Hiera

```yaml
# hiera/common.yaml or nodes/puppetserver.yaml
---
openvox_webui::enc::webui_url: 'https://segdc1vpr0018.fgv.br'
openvox_webui::enc::manage_puppet_conf: true
openvox_webui::enc::restart_puppetserver: true
openvox_webui::enc::ssl_verify: false
openvox_webui::enc::remove_agent_environment: true
```

Then in your manifest:

```puppet
include openvox_webui::enc
```

#### What It Does

The `openvox_webui::enc` class:

1. ✓ Creates ENC script at `/opt/openvox/enc.sh`
2. ✓ Installs dependencies (python3-pyyaml)
3. ✓ Configures `puppet.conf` [master] section:
   - `node_terminus = exec`
   - `external_nodes = /opt/openvox/enc.sh`
4. ✓ Removes conflicting `environment` from [agent] section
5. ✓ Validates the ENC script
6. ✓ Restarts Puppet Server

---

### Method 2: Manual Setup

If you prefer manual configuration or can't use the Puppet class:

#### 1. Install Dependencies

```bash
# RHEL/CentOS/Rocky
sudo dnf install python3-pyyaml

# Debian/Ubuntu
sudo apt-get install python3-yaml
```

#### 2. Create ENC Script

```bash
sudo mkdir -p /opt/openvox
sudo vi /opt/openvox/enc.sh
```

Add this content (replace `YOUR_WEBUI_URL`):

```bash
#!/bin/bash
set -e

WEBUI_URL="https://openvox.example.com"  # CHANGE THIS
CERTNAME="$1"

if [ -z "$CERTNAME" ]; then
    echo "Error: No certname provided" >&2
    exit 1
fi

# Query OpenVox WebUI (-k allows self-signed certs)
CLASSIFICATION=$(curl -k -s "${WEBUI_URL}/api/v1/nodes/${CERTNAME}/classification" 2>&1)

if echo "$CLASSIFICATION" | grep -q "^{"; then
    echo "$CLASSIFICATION" | python3 -c '
import sys, json, yaml
try:
    data = json.load(sys.stdin)
    output = {
        "environment": data.get("environment", "production"),
        "classes": data.get("classes", {}),
    }
    if "parameters" in data:
        output["parameters"] = data["parameters"]
    print(yaml.dump(output, default_flow_style=False, explicit_start=True))
except Exception as e:
    print(f"Error: {e}", file=sys.stderr)
    sys.exit(1)
'
else
    echo "---"
    echo "environment: production"
    echo "classes: {}"
fi
```

Make it executable:

```bash
sudo chmod +x /opt/openvox/enc.sh
```

#### 3. Configure Puppet Server

Edit `/etc/puppetlabs/puppet/puppet.conf`:

```bash
sudo vi /etc/puppetlabs/puppet/puppet.conf
```

Add to `[master]` section:

```ini
[master]
node_terminus = exec
external_nodes = /opt/openvox/enc.sh
```

Remove from `[agent]` section (if present):

```ini
[agent]
# environment = production  # Comment out or remove this line
```

#### 4. Test ENC Script

```bash
sudo /opt/openvox/enc.sh $(hostname -f)
```

Expected output:

```yaml
---
environment: production
classes:
  base: {}
  apache:
    port: 8080
parameters:
  role: webserver
```

#### 5. Restart Puppet Server

```bash
sudo systemctl restart puppetserver
sudo systemctl status puppetserver
```

---

## Verification

### 1. Check ENC Script

```bash
# Test ENC with your node
sudo /opt/openvox/enc.sh your-node.example.com

# Should return YAML with environment and classes
```

### 2. Check Puppet Configuration

```bash
# View [master] section
sudo grep -A5 "\[master\]" /etc/puppetlabs/puppet/puppet.conf

# Should show:
# node_terminus = exec
# external_nodes = /opt/openvox/enc.sh
```

### 3. Test Puppet Agent

```bash
# Run puppet agent (without --environment flag!)
puppet agent -t

# Check the log output
# Should show: "Using environment 'X'" where X comes from OpenVox
```

### 4. Check Puppet Server Logs

```bash
sudo tail -f /var/log/puppetlabs/puppetserver/puppetserver.log

# Look for environment messages
sudo grep -i environment /var/log/puppetlabs/puppetserver/puppetserver.log | tail -20
```

---

## OpenVox WebUI Configuration

### 1. Access the Web Interface

Navigate to your OpenVox WebUI:

```
https://your-openvox-server
```

### 2. Create Node Groups

1. Go to **Groups** → **Create Group**
2. Set group properties:
   - Name: e.g., "Web Servers"
   - Environment: e.g., "production"
   - Description

3. Add classification rules:
   - Fact: `operatingsystem`
   - Operator: `=`
   - Value: `RedHat`

4. Add classes:
   - Class: `apache`
   - Parameters: `{ "port": 8080 }`

### 3. Classify Nodes

Nodes are automatically classified based on:
- **Rules**: Fact-based matching (e.g., OS, location, custom facts)
- **Pinning**: Manually assign specific nodes to groups
- **Priority**: Higher priority groups override lower ones

### 4. View Node Classification

1. Go to **Nodes** → Select a node
2. View "Classification" tab
3. See:
   - Assigned environment
   - Applied classes
   - Effective parameters
   - Group memberships

---

## Troubleshooting

### Problem: "Puppet Not Found: Could not find environment 'X'"

**Cause:** Environment doesn't exist on Puppet Server

**Solution:** Create the environment directory

```bash
sudo mkdir -p /etc/puppetlabs/code/environments/pserver
sudo mkdir -p /etc/puppetlabs/code/environments/pserver/{manifests,modules,data}

# Create basic site.pp
sudo cat > /etc/puppetlabs/code/environments/pserver/manifests/site.pp <<'EOF'
node default {
  # Classification via ENC
}
EOF

# Set permissions
sudo chown -R puppet:puppet /etc/puppetlabs/code/environments/pserver
```

### Problem: "Environment mismatch between agent and server"

**Cause:** Agent's `puppet.conf` has `environment = X` in [agent] section

**Solution:** Remove environment setting

```bash
# Backup first
sudo cp /etc/puppetlabs/puppet/puppet.conf /etc/puppetlabs/puppet/puppet.conf.backup

# Remove environment line
sudo sed -i '/\[agent\]/,/\[/ { /^\s*environment\s*=/d; }' /etc/puppetlabs/puppet/puppet.conf

# Verify
grep -A5 "\[agent\]" /etc/puppetlabs/puppet/puppet.conf
```

### Problem: ENC Returns 404 Error

**Cause:** Classification endpoint doesn't exist or OpenVox WebUI isn't accessible

**Solution:**

```bash
# Test OpenVox WebUI access
curl -k https://your-openvox-server/api/v1/health

# Test classification endpoint
curl -k https://your-openvox-server/api/v1/nodes/$(hostname -f)/classification

# Check OpenVox WebUI logs
sudo journalctl -u openvox-webui -n 50
```

### Problem: ENC Script Fails with Python Error

**Cause:** PyYAML not installed

**Solution:**

```bash
# RHEL/CentOS/Rocky
sudo dnf install python3-pyyaml

# Debian/Ubuntu
sudo apt-get install python3-yaml

# Test
python3 -c "import yaml; print('OK')"
```

### Problem: Node Ignores ENC Classification

**Cause:** ENC not properly configured in puppet.conf

**Solution:**

```bash
# Verify [master] section
sudo grep -A5 "\[master\]" /etc/puppetlabs/puppet/puppet.conf

# Should have:
# node_terminus = exec
# external_nodes = /opt/openvox/enc.sh

# If missing, add them
sudo puppet config set node_terminus exec --section master
sudo puppet config set external_nodes /opt/openvox/enc.sh --section master

# Restart
sudo systemctl restart puppetserver
```

---

## Best Practices

### 1. Don't Set Environment on Agent

❌ **Don't do this:**

```ini
[agent]
environment = production  # Remove this!
```

✅ **Do this:**

```ini
[agent]
server = puppet.example.com
# Let ENC control environment
```

### 2. Create All Required Environments

Before classifying nodes to an environment, ensure it exists:

```bash
ls -la /etc/puppetlabs/code/environments/
# Should show: production, development, staging, etc.
```

### 3. Test ENC Before Agent Runs

```bash
# Test ENC first
sudo /opt/openvox/enc.sh $(hostname -f)

# Then test catalog compilation
sudo puppet agent -t --noop
```

### 4. Monitor Puppet Server Logs

```bash
# Watch for environment issues
sudo tail -f /var/log/puppetlabs/puppetserver/puppetserver.log | grep -i environment
```

### 5. Use Version Control for Environments

```bash
# Link to git repositories
cd /etc/puppetlabs/code/environments/production
sudo git init
sudo git remote add origin https://git.example.com/puppet-prod.git
```

---

## Advanced Configuration

### Custom ENC Script Location

```puppet
class { 'openvox_webui::enc':
  enc_script_path => '/usr/local/bin/openvox-enc.sh',
}
```

### Skip SSL Verification (Self-Signed Certs)

```puppet
class { 'openvox_webui::enc':
  ssl_verify => false,
}
```

### Don't Restart Puppet Server Automatically

```puppet
class { 'openvox_webui::enc':
  restart_puppetserver => false,
}

# Manual restart later:
# sudo systemctl restart puppetserver
```

### Environment-Specific Classification

Use different groups with environment filters:

1. Create "Production Web Servers" group with `environment = production`
2. Create "Development Web Servers" group with `environment = development`
3. Use same rules, but different environments

---

## Migration from Node Definitions

If you're migrating from traditional node definitions:

### Before (site.pp):

```puppet
node 'web01.example.com' {
  class { 'apache':
    port => 8080,
  }
  class { 'mysql': }
}
```

### After (OpenVox WebUI):

1. Create group "Web Servers"
2. Add classification rule: `hostname = web01.example.com`
3. Add classes: `apache`, `mysql`
4. Set parameters: `apache::port = 8080`
5. Deploy via ENC

---

## API Integration

For programmatic classification, use the OpenVox WebUI API:

```bash
# Get node classification
curl -k https://openvox.example.com/api/v1/nodes/web01.example.com/classification

# Response:
{
  "environment": "production",
  "classes": {
    "apache": {"port": 8080},
    "mysql": {}
  },
  "parameters": {
    "role": "webserver"
  }
}
```

---

## See Also

- [Puppet Environment Troubleshooting Guide](PUPPET_ENVIRONMENT_TROUBLESHOOTING.md)
- [Notification System Documentation](NOTIFICATIONS.md)
- [OpenVox WebUI Main README](../README.md)
- [Puppet External Node Classifiers](https://puppet.com/docs/puppet/latest/nodes_external.html)
