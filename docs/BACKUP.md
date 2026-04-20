# OpenVox WebUI Backup and Restore Guide

This guide covers backing up and restoring OpenVox WebUI data.

## What to Backup

### Critical Data

1. **Database**: `/var/lib/openvox-webui/openvox.db` - All application data
2. **Configuration**: `/etc/openvox-webui/config.yaml` - Application settings
3. **SSL Certificates**: Custom TLS certificates (if used)
4. **Facter Templates**: `/etc/openvox-webui/templates/*` - Custom fact templates

### Optional Data

1. **Logs**: `/var/log/openvox-webui/*` - Historical logs (if needed)
2. **Groups Configuration**: `/etc/openvox-webui/groups.yaml` - Node group definitions

## Backup Methods

### Method 1: Manual Backup (Recommended for One-Time)

#### Full Backup

```bash
#!/bin/bash
# Stop service for consistent backup
sudo systemctl stop openvox-webui

# Create backup directory
BACKUP_DIR="/backup/openvox-webui/$(date +%Y%m%d_%H%M%S)"
sudo mkdir -p "$BACKUP_DIR"

# Backup database
sudo cp -a /var/lib/openvox-webui/openvox.db "$BACKUP_DIR/"

# Backup configuration
sudo cp -a /etc/openvox-webui "$BACKUP_DIR/config"

# Backup templates
if [ -d /etc/openvox-webui/templates ]; then
    sudo cp -a /etc/openvox-webui/templates "$BACKUP_DIR/"
fi

# Create backup archive
sudo tar -czf "/backup/openvox-webui-backup-$(date +%Y%m%d_%H%M%S).tar.gz" -C "$BACKUP_DIR" .

# Start service
sudo systemctl start openvox-webui

echo "Backup completed: $BACKUP_DIR"
```

#### Hot Backup (without stopping service)

SQLite supports hot backups using the `.backup` command:

```bash
#!/bin/bash
BACKUP_FILE="/backup/openvox-webui/openvox-$(date +%Y%m%d_%H%M%S).db"
sudo sqlite3 /var/lib/openvox-webui/openvox.db ".backup $BACKUP_FILE"
sudo gzip "$BACKUP_FILE"
echo "Hot backup completed: ${BACKUP_FILE}.gz"
```

### Method 2: Automated Backup Script

Create `/usr/local/bin/backup-openvox-webui.sh`:

```bash
#!/bin/bash
set -e

# Configuration
BACKUP_ROOT="/backup/openvox-webui"
RETENTION_DAYS=30
DB_PATH="/var/lib/openvox-webui/openvox.db"
CONFIG_DIR="/etc/openvox-webui"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="$BACKUP_ROOT/$TIMESTAMP"

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Backup database (hot backup)
echo "Backing up database..."
sqlite3 "$DB_PATH" ".backup $BACKUP_DIR/openvox.db"

# Backup configuration
echo "Backing up configuration..."
cp -a "$CONFIG_DIR" "$BACKUP_DIR/config"

# Create compressed archive
echo "Creating archive..."
tar -czf "$BACKUP_ROOT/openvox-backup-$TIMESTAMP.tar.gz" \
    -C "$BACKUP_DIR" .

# Remove temporary directory
rm -rf "$BACKUP_DIR"

# Clean old backups
echo "Cleaning old backups (older than $RETENTION_DAYS days)..."
find "$BACKUP_ROOT" -name "openvox-backup-*.tar.gz" \
    -mtime +$RETENTION_DAYS -delete

# Calculate backup size
BACKUP_SIZE=$(du -h "$BACKUP_ROOT/openvox-backup-$TIMESTAMP.tar.gz" | cut -f1)
echo "Backup completed: openvox-backup-$TIMESTAMP.tar.gz ($BACKUP_SIZE)"

# Verify backup
if tar -tzf "$BACKUP_ROOT/openvox-backup-$TIMESTAMP.tar.gz" &>/dev/null; then
    echo "Backup verification: OK"
else
    echo "Backup verification: FAILED"
    exit 1
fi
```

Make it executable:

```bash
sudo chmod +x /usr/local/bin/backup-openvox-webui.sh
```

### Method 3: Scheduled Backups with Cron

#### Daily Backup

```bash
# Edit crontab
sudo crontab -e

# Add daily backup at 2 AM
0 2 * * * /usr/local/bin/backup-openvox-webui.sh 2>&1 | logger -t openvox-backup
```

#### Weekly Backup

```bash
# Weekly backup on Sundays at 3 AM
0 3 * * 0 /usr/local/bin/backup-openvox-webui.sh 2>&1 | logger -t openvox-backup
```

### Method 4: Backup to Remote Location

#### Using rsync

```bash
#!/bin/bash
# Local backup first
/usr/local/bin/backup-openvox-webui.sh

# Sync to remote server
LATEST_BACKUP=$(ls -t /backup/openvox-webui/openvox-backup-*.tar.gz | head -1)
rsync -avz "$LATEST_BACKUP" backup-server:/backups/openvox-webui/

echo "Backup synced to remote server"
```

#### Using S3 (with AWS CLI)

```bash
#!/bin/bash
# Local backup first
/usr/local/bin/backup-openvox-webui.sh

# Upload to S3
LATEST_BACKUP=$(ls -t /backup/openvox-webui/openvox-backup-*.tar.gz | head -1)
aws s3 cp "$LATEST_BACKUP" s3://my-backups/openvox-webui/

echo "Backup uploaded to S3"
```

## Restore Procedures

### Full Restore from Backup

```bash
#!/bin/bash
set -e

# Specify backup file
BACKUP_FILE="/backup/openvox-webui/openvox-backup-20250101_020000.tar.gz"

# Stop service
echo "Stopping service..."
sudo systemctl stop openvox-webui

# Create temporary restore directory
RESTORE_DIR="/tmp/openvox-restore-$$"
mkdir -p "$RESTORE_DIR"

# Extract backup
echo "Extracting backup..."
tar -xzf "$BACKUP_FILE" -C "$RESTORE_DIR"

# Restore database
echo "Restoring database..."
sudo cp -f "$RESTORE_DIR/openvox.db" /var/lib/openvox-webui/openvox.db
sudo chown openvox-webui:openvox-webui /var/lib/openvox-webui/openvox.db
sudo chmod 640 /var/lib/openvox-webui/openvox.db

# Restore configuration
echo "Restoring configuration..."
sudo cp -rf "$RESTORE_DIR/config/"* /etc/openvox-webui/
sudo chown -R root:openvox-webui /etc/openvox-webui
sudo chmod 750 /etc/openvox-webui
sudo chmod 640 /etc/openvox-webui/config.yaml

# Cleanup
rm -rf "$RESTORE_DIR"

# Start service
echo "Starting service..."
sudo systemctl start openvox-webui

# Verify
echo "Verifying service..."
sleep 3
if systemctl is-active --quiet openvox-webui; then
    echo "Restore completed successfully"
else
    echo "ERROR: Service failed to start after restore"
    sudo journalctl -u openvox-webui -n 50 --no-pager
    exit 1
fi
```

### Database-Only Restore

```bash
#!/bin/bash
# Stop service
sudo systemctl stop openvox-webui

# Restore database
sudo cp /backup/openvox-webui/openvox-20250101.db /var/lib/openvox-webui/openvox.db
sudo chown openvox-webui:openvox-webui /var/lib/openvox-webui/openvox.db

# Start service
sudo systemctl start openvox-webui
```

### Configuration-Only Restore

```bash
#!/bin/bash
# Restore configuration without stopping service
sudo cp /backup/openvox-webui/config.yaml /etc/openvox-webui/config.yaml
sudo chown root:openvox-webui /etc/openvox-webui/config.yaml
sudo chmod 640 /etc/openvox-webui/config.yaml

# Restart to apply changes
sudo systemctl restart openvox-webui
```

### Point-in-Time Recovery

If you have incremental backups:

```bash
#!/bin/bash
# List available backups
ls -lh /backup/openvox-webui/openvox-backup-*.tar.gz

# Choose backup by date
BACKUP_DATE="20250101_020000"
BACKUP_FILE="/backup/openvox-webui/openvox-backup-$BACKUP_DATE.tar.gz"

# Restore from that backup
./restore-openvox-webui.sh "$BACKUP_FILE"
```

## Disaster Recovery

### Complete System Rebuild

1. **Install OpenVox WebUI** on new system:
   ```bash
   sudo dnf install openvox-webui
   ```

2. **Stop service**:
   ```bash
   sudo systemctl stop openvox-webui
   ```

3. **Restore from backup**:
   ```bash
   tar -xzf openvox-backup-YYYYMMDD_HHMMSS.tar.gz
   sudo cp openvox.db /var/lib/openvox-webui/
   sudo cp -r config/* /etc/openvox-webui/
   ```

4. **Fix permissions**:
   ```bash
   sudo chown -R openvox-webui:openvox-webui /var/lib/openvox-webui
   sudo chown -R root:openvox-webui /etc/openvox-webui
   sudo chmod 750 /etc/openvox-webui
   sudo chmod 640 /etc/openvox-webui/config.yaml
   ```

5. **Start service**:
   ```bash
   sudo systemctl enable --now openvox-webui
   ```

### Database Corruption Recovery

If database is corrupted:

```bash
# Try integrity check
sqlite3 /var/lib/openvox-webui/openvox.db "PRAGMA integrity_check"

# If corrupted, restore from backup
sudo systemctl stop openvox-webui
sudo mv /var/lib/openvox-webui/openvox.db /var/lib/openvox-webui/openvox.db.corrupt
sudo cp /backup/openvox-webui/latest/openvox.db /var/lib/openvox-webui/
sudo systemctl start openvox-webui
```

## Backup Verification

### Verify Backup Integrity

```bash
# Test archive extraction
tar -tzf /backup/openvox-webui/openvox-backup-*.tar.gz > /dev/null
echo $?  # Should be 0

# Verify database
TEMP_DB=$(mktemp)
tar -xzf openvox-backup-*.tar.gz -O openvox.db > "$TEMP_DB"
sqlite3 "$TEMP_DB" "PRAGMA integrity_check"
rm "$TEMP_DB"
```

### Test Restore (Dry Run)

```bash
# Extract to temporary location
TEMP_DIR=$(mktemp -d)
tar -xzf openvox-backup-*.tar.gz -C "$TEMP_DIR"

# Verify files
ls -la "$TEMP_DIR"

# Cleanup
rm -rf "$TEMP_DIR"
```

## Monitoring Backups

### Backup Status Script

```bash
#!/bin/bash
# Check latest backup age
LATEST=$(ls -t /backup/openvox-webui/openvox-backup-*.tar.gz | head -1)
if [ -z "$LATEST" ]; then
    echo "ERROR: No backups found"
    exit 1
fi

AGE=$(($(date +%s) - $(stat -c %Y "$LATEST")))
AGE_HOURS=$((AGE / 3600))

echo "Latest backup: $LATEST"
echo "Age: $AGE_HOURS hours"

if [ $AGE_HOURS -gt 48 ]; then
    echo "WARNING: Backup is older than 48 hours"
    exit 1
fi
```

### Nagios/Icinga Check

```bash
#!/bin/bash
# Nagios plugin for backup monitoring
BACKUP_DIR="/backup/openvox-webui"
WARN_HOURS=24
CRIT_HOURS=48

LATEST=$(ls -t "$BACKUP_DIR"/openvox-backup-*.tar.gz 2>/dev/null | head -1)
if [ -z "$LATEST" ]; then
    echo "CRITICAL: No backups found"
    exit 2
fi

AGE=$(($(date +%s) - $(stat -c %Y "$LATEST")))
AGE_HOURS=$((AGE / 3600))

if [ $AGE_HOURS -gt $CRIT_HOURS ]; then
    echo "CRITICAL: Backup age $AGE_HOURS hours (> $CRIT_HOURS)"
    exit 2
elif [ $AGE_HOURS -gt $WARN_HOURS ]; then
    echo "WARNING: Backup age $AGE_HOURS hours (> $WARN_HOURS)"
    exit 1
else
    echo "OK: Backup age $AGE_HOURS hours"
    exit 0
fi
```

## Best Practices

1. **Automate backups** - Use cron for regular scheduled backups
2. **Test restores** - Regularly test backup restoration procedures
3. **Off-site backups** - Store copies in remote location or cloud
4. **Retention policy** - Keep backups for appropriate duration (30+ days)
5. **Monitor backups** - Alert on backup failures or missing backups
6. **Encrypt backups** - Encrypt sensitive data in backups
7. **Document procedures** - Keep recovery procedures documented and accessible
8. **Version backups** - Keep multiple backup versions for point-in-time recovery

## Backup Encryption

Encrypt backups for security:

```bash
# Create encrypted backup
/usr/local/bin/backup-openvox-webui.sh
LATEST=$(ls -t /backup/openvox-webui/openvox-backup-*.tar.gz | head -1)
gpg --encrypt --recipient backup@example.com "$LATEST"

# Decrypt for restore
gpg --decrypt openvox-backup-YYYYMMDD_HHMMSS.tar.gz.gpg > openvox-backup.tar.gz
```

## See Also

- [Installation Guide](INSTALLATION.md)
- [Upgrade Guide](UPGRADE.md)
- [Configuration Reference](CONFIGURATION.md)
