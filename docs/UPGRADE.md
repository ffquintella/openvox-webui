# OpenVox WebUI Upgrade Guide

This guide covers upgrading OpenVox WebUI between versions.

## Before You Upgrade

### 1. Backup Your Data

**Always backup before upgrading!** See [Backup and Restore Guide](BACKUP.md) for details.

Minimal backup:

```bash
# Stop the service
sudo systemctl stop openvox-webui

# Backup database
sudo cp /var/lib/openvox-webui/openvox.db /var/lib/openvox-webui/openvox.db.backup

# Backup configuration
sudo cp /etc/openvox-webui/config.yaml /etc/openvox-webui/config.yaml.backup
```

### 2. Review Release Notes

Check the [CHANGELOG.md](../CHANGELOG.md) for breaking changes and new features.

### 3. Check System Requirements

Verify your system meets the requirements for the new version.

## Upgrade Methods

### Method 1: Package Manager (Recommended)

#### RPM-based Systems (RHEL/CentOS/Rocky/Fedora)

```bash
# Update package index
sudo dnf check-update

# Upgrade openvox-webui
sudo dnf upgrade openvox-webui

# or for older systems
sudo yum update openvox-webui
```

#### DEB-based Systems (Debian/Ubuntu)

```bash
# Update package index
sudo apt update

# Upgrade openvox-webui
sudo apt upgrade openvox-webui
```

### Method 2: Direct Package Installation

#### RPM

```bash
# Download new version
wget https://github.com/openvoxproject/openvox-webui/releases/download/v1.0.0/openvox-webui-1.0.0-1.el9.x86_64.rpm

# Stop service
sudo systemctl stop openvox-webui

# Upgrade package
sudo dnf upgrade ./openvox-webui-1.0.0-1.el9.x86_64.rpm

# Start service
sudo systemctl start openvox-webui
```

#### DEB

```bash
# Download new version
wget https://github.com/openvoxproject/openvox-webui/releases/download/v1.0.0/openvox-webui_1.0.0-1_amd64.deb

# Stop service
sudo systemctl stop openvox-webui

# Upgrade package
sudo dpkg -i openvox-webui_1.0.0-1_amd64.deb
sudo apt-get install -f  # Fix dependencies if needed

# Start service
sudo systemctl start openvox-webui
```

### Method 3: Puppet Module

Update module version in Puppetfile:

```ruby
mod 'openvox-webui',
  :git => 'https://github.com/openvoxproject/openvox-webui.git',
  :tag => 'v1.0.0'  # Update version
```

Then run Puppet:

```bash
r10k puppetfile install
puppet agent -t
```

## Post-Upgrade Steps

### 1. Verify Service Status

```bash
sudo systemctl status openvox-webui
```

### 2. Check Logs

```bash
sudo journalctl -u openvox-webui -n 50 --no-pager
```

### 3. Test API

```bash
curl http://localhost:5051/api/v1/health
```

### 4. Verify Configuration

```bash
# Check for configuration warnings
sudo openvox-webui --check-config
```

### 5. Test Web Interface

Open browser and verify:
- Login works
- Dashboard loads
- PuppetDB connection active
- No JavaScript console errors

## Database Migrations

Database migrations are automatic. The application will:

1. Detect current schema version
2. Apply pending migrations on startup
3. Log migration progress

To manually check migrations:

```bash
# View migration status
sudo -u openvox-webui openvox-webui --migrate-status

# Run migrations manually (advanced)
sudo -u openvox-webui openvox-webui --migrate
```

## Version-Specific Upgrade Notes

### Upgrading to 1.0.0 (from 0.9.x)

**Breaking Changes:**
- Default port changed from 3000 to 5051
- Configuration format changes (see below)

**Required Actions:**

1. Update port in firewall rules
2. Update reverse proxy configuration
3. Review new configuration options

**Configuration Changes:**

Old format (0.9.x):
```yaml
server:
  address: "127.0.0.1"
  port: 3000
```

New format (1.0.0):
```yaml
server:
  host: "127.0.0.1"
  port: 5051
```

### Upgrading to 0.9.0 (from 0.8.x)

**New Features:**
- Multi-tenancy support
- API key authentication
- Audit logging enhancements

**Optional Actions:**

1. Enable multi-tenancy if needed
2. Create API keys for automation
3. Review audit log configuration

### Upgrading to 0.8.0 (from 0.7.x)

**New Features:**
- Alerting system
- Advanced reporting
- Performance optimizations

**Database Changes:**
- New tables for alerting and reporting
- Automatic migration on startup

## Rolling Back

If the upgrade fails, rollback to previous version:

### 1. Stop Service

```bash
sudo systemctl stop openvox-webui
```

### 2. Restore Backup

```bash
# Restore database
sudo cp /var/lib/openvox-webui/openvox.db.backup /var/lib/openvox-webui/openvox.db

# Restore configuration
sudo cp /etc/openvox-webui/config.yaml.backup /etc/openvox-webui/config.yaml
```

### 3. Downgrade Package

#### RPM

```bash
# List available versions
sudo dnf list openvox-webui --showduplicates

# Downgrade to specific version
sudo dnf downgrade openvox-webui-0.8.0
```

#### DEB

```bash
# Install old package
sudo dpkg -i /path/to/old/openvox-webui_0.8.0-1_amd64.deb
sudo apt-mark hold openvox-webui  # Prevent auto-upgrade
```

### 4. Start Service

```bash
sudo systemctl start openvox-webui
```

## Troubleshooting Upgrades

### Migration Fails

```bash
# Check migration error
sudo journalctl -u openvox-webui -n 100 --no-pager | grep migration

# Restore backup and retry
sudo systemctl stop openvox-webui
sudo cp /var/lib/openvox-webui/openvox.db.backup /var/lib/openvox-webui/openvox.db
sudo systemctl start openvox-webui
```

### Configuration Errors

```bash
# Validate configuration
sudo openvox-webui --check-config

# Compare with example
diff /etc/openvox-webui/config.yaml /usr/share/doc/openvox-webui/config.example.yaml
```

### Service Won't Start

```bash
# Check detailed error
sudo systemctl status openvox-webui -l

# Check logs
sudo journalctl -u openvox-webui -n 100 --no-pager

# Verify permissions
ls -la /var/lib/openvox-webui/
ls -la /etc/openvox-webui/
```

### API Not Responding

```bash
# Check if port is listening
sudo ss -tlnp | grep :5051

# Check firewall
sudo firewall-cmd --list-ports

# Test locally
curl -v http://localhost:5051/api/v1/health
```

## Best Practices

1. **Always backup** before upgrading
2. **Test upgrades** in non-production environment first
3. **Read release notes** for breaking changes
4. **Schedule maintenance window** for production upgrades
5. **Monitor logs** during and after upgrade
6. **Verify functionality** after upgrade
7. **Keep old packages** for potential rollback
8. **Document** any custom configuration changes

## Automated Upgrades

### Using Puppet

```puppet
class { 'openvox_webui':
  ensure => 'latest',  # Always use latest version
  # ... other parameters
}
```

### Using Ansible

```yaml
- name: Upgrade OpenVox WebUI
  package:
    name: openvox-webui
    state: latest
  notify: restart openvox-webui
```

### Using cron

```bash
# /etc/cron.weekly/upgrade-openvox-webui
#!/bin/bash
dnf upgrade -y openvox-webui
systemctl restart openvox-webui
```

**Note:** Automated upgrades are not recommended for production without testing.

## See Also

- [Installation Guide](INSTALLATION.md)
- [Configuration Reference](CONFIGURATION.md)
- [Backup and Restore](BACKUP.md)
- [CHANGELOG](../CHANGELOG.md)
