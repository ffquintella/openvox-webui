# OpenVox WebUI Scripts

This directory contains utility scripts for managing and troubleshooting OpenVox WebUI and Puppet integration.

## Puppet Environment Management

### 1. Setup Puppet ENC Integration

**[setup-puppet-enc.sh](setup-puppet-enc.sh)** - Configure Puppet Server to use OpenVox WebUI as External Node Classifier

```bash
# Run on Puppet Server (requires root)
sudo ./setup-puppet-enc.sh

# Or specify a custom WebUI URL
sudo WEBUI_URL=https://puppet.example.com ./setup-puppet-enc.sh
```

**What it does:**
- Auto-detects OpenVox WebUI URL (HTTP:8080 or HTTPS:443)
- Creates ENC script at `/opt/openvox/enc.sh`
- Configures Puppet Server's `puppet.conf`
- Installs PyYAML dependency
- Tests the ENC configuration
- Restarts Puppet Server

**Requirements:**
- Root access on Puppet Server
- OpenVox WebUI running and accessible
- Python 3 installed

---

### 2. Diagnose Environment Issues

**[diagnose-puppet-environment.sh](diagnose-puppet-environment.sh)** - Comprehensive diagnostic tool for environment mismatches

```bash
# Run on Puppet Server or agent
sudo ./diagnose-puppet-environment.sh

# Or specify a certname
sudo ./diagnose-puppet-environment.sh node.example.com

# With custom WebUI URL
sudo WEBUI_URL=https://puppet.example.com ./diagnose-puppet-environment.sh
```

**What it checks:**
- Puppet agent configuration (puppet.conf)
- ENC configuration and script execution
- OpenVox WebUI classification API
- Available environments on Puppet Server
- Puppet Server logs
- Catalog compilation tests

**Output includes:**
- ✓ Success indicators
- ✗ Error indicators
- ℹ Information messages
- Actionable recommendations

---

### 3. Auto-Fix Environment Issues

**[fix-puppet-environment.sh](fix-puppet-environment.sh)** - Interactive tool to automatically fix common problems

```bash
# Run on Puppet Server (requires root)
sudo ./fix-puppet-environment.sh

# Or specify a certname
sudo ./fix-puppet-environment.sh node.example.com

# With custom WebUI URL
sudo WEBUI_URL=https://puppet.example.com ./fix-puppet-environment.sh
```

**What it fixes:**
- Creates missing environment directories
- Removes conflicting `environment` settings from puppet.conf
- Sets up basic environment structure (manifests, modules, data)
- Tests catalog compilation

**Interactive features:**
- Asks for confirmation before making changes
- Creates backups of modified files
- Provides clear progress indicators

---

## Notification System

### 4. Create Test Notifications

**[create-test-notifications.sh](create-test-notifications.sh)** - Generate sample notifications for UI testing

```bash
# Run on any machine with access to OpenVox WebUI
./create-test-notifications.sh

# Specify user
./create-test-notifications.sh user-id-123

# With custom WebUI URL
WEBUI_URL=https://puppet.example.com ./create-test-notifications.sh
```

**Creates:**
- Info notification
- Success notification
- Warning notification
- Error notification

**Use cases:**
- Testing notification UI
- Demonstrating notification features
- Verifying SSE stream

---

### 5. Test Notifications and ENC

**[test-notifications-and-enc.sh](test-notifications-and-enc.sh)** - Diagnose ENC issues and create diagnostic notifications

```bash
# Run on any machine
./test-notifications-and-enc.sh

# With custom WebUI URL
WEBUI_URL=https://puppet.example.com ./test-notifications-and-enc.sh
```

**What it does:**
- Checks ENC configuration
- Tests classification endpoint
- Diagnoses environment mismatches
- Creates notifications about issues found
- Generates actionable recommendations

---

## Common Usage Patterns

### Initial Setup (Fresh Install)

1. Install and start OpenVox WebUI
2. Run the ENC setup script:
   ```bash
   sudo ./setup-puppet-enc.sh
   ```
3. Verify with diagnostic script:
   ```bash
   sudo ./diagnose-puppet-environment.sh
   ```
4. Test with puppet agent:
   ```bash
   puppet agent -t
   ```

### Troubleshooting Environment Issues

1. Run diagnostic script:
   ```bash
   sudo ./diagnose-puppet-environment.sh node.example.com
   ```
2. Review output and recommendations
3. Run auto-fix if needed:
   ```bash
   sudo ./fix-puppet-environment.sh node.example.com
   ```
4. Test again:
   ```bash
   puppet agent -t
   ```

### Testing Notification System

1. Create test notifications:
   ```bash
   ./create-test-notifications.sh
   ```
2. Open OpenVox WebUI in browser
3. Check notification bell icon (top-right)
4. Verify toasts appear
5. Test marking as read/unread/delete

---

## Environment Variables

All scripts support these environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `WEBUI_URL` | Auto-detected | OpenVox WebUI base URL (http://localhost:8080 or https://localhost) |
| `PUPPET_SERVER` | localhost | Puppet Server hostname |
| `PUPPET_PORT` | 8140 | Puppet Server port |

### Auto-Detection

Scripts automatically detect OpenVox WebUI URL by checking for running processes:

- If `openvox-webui` is listening on port **443**: Uses `https://localhost`
- If `openvox-webui` is listening on port **8080**: Uses `http://localhost:8080`
- Otherwise: Defaults to `http://localhost:8080`

Override with:
```bash
WEBUI_URL=https://puppet.example.com ./script.sh
```

---

## HTTPS and Self-Signed Certificates

All scripts use `curl -k` to allow self-signed certificates when using HTTPS. This is appropriate for internal infrastructure.

For production environments, consider:
- Using proper CA-signed certificates
- Removing `-k` flag from curl commands
- Configuring proper certificate validation

---

## Troubleshooting

### Script Fails to Detect OpenVox WebUI

**Problem:** Script shows "OpenVox WebUI is not accessible"

**Solutions:**
1. Check if service is running:
   ```bash
   systemctl status openvox-webui
   netstat -anp | grep openvox
   ```

2. Manually specify URL:
   ```bash
   WEBUI_URL=https://puppet.example.com ./script.sh
   ```

3. Check firewall rules:
   ```bash
   firewall-cmd --list-all
   ```

### Classification Endpoint Returns 404

**Problem:** API returns "404 Not Found" for classification endpoint

**Cause:** ENC endpoint not configured in backend

**Solution:** Ensure OpenVox WebUI includes the classification API endpoint. The backend should have:
- Route: `/api/v1/nodes/:certname/classification`
- Handler in: `src/api/nodes.rs` or similar

### PyYAML Installation Fails

**Problem:** Setup script can't install PyYAML

**Solutions:**
```bash
# RHEL/CentOS/Rocky
sudo dnf install python3-pyyaml

# Debian/Ubuntu
sudo apt-get install python3-yaml

# Using pip
pip3 install pyyaml
```

### Permission Denied on Scripts

**Problem:** `Permission denied` when running scripts

**Solution:**
```bash
chmod +x scripts/*.sh
```

---

## Documentation

For more detailed information, see:
- [Puppet Environment Troubleshooting Guide](../docs/PUPPET_ENVIRONMENT_TROUBLESHOOTING.md)
- [Notification System Documentation](../docs/NOTIFICATIONS.md)
- [Main README](../README.md)

---

## Contributing

When adding new scripts:
1. Make them executable: `chmod +x script.sh`
2. Add shebang: `#!/bin/bash`
3. Include usage documentation
4. Use color-coded output (GREEN/RED/YELLOW/BLUE)
5. Add to this README
6. Test with both HTTP and HTTPS
7. Handle errors gracefully
