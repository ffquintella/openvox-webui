# OpenVox WebUI Installation Guide

This guide covers installation of OpenVox WebUI using various methods.

## Table of Contents

1. [System Requirements](#system-requirements)
2. [Installation Methods](#installation-methods)
   - [Package Installation (RPM)](#rpm-installation)
   - [Package Installation (DEB)](#deb-installation)
   - [Puppet Module Installation](#puppet-module-installation)
3. [Initial Configuration](#initial-configuration)
4. [Post-Installation](#post-installation)
5. [Troubleshooting](#troubleshooting)

## System Requirements

### Operating System Support

**RHEL/CentOS/Rocky/AlmaLinux Family:**
- RHEL/CentOS/Rocky/AlmaLinux 8, 9
- Fedora 38, 39, 40

**Debian Family:**
- Debian 11 (Bullseye), 12 (Bookworm)
- Ubuntu 22.04 LTS (Jammy), 24.04 LTS (Noble)

### Hardware Requirements

**Minimum:**
- 2 CPU cores
- 2 GB RAM
- 10 GB disk space

**Recommended:**
- 4+ CPU cores
- 4+ GB RAM
- 20+ GB disk space (for logs and database)

### Software Dependencies

**Runtime:**
- OpenSSL 3.x (RHEL 9, Debian 12, Ubuntu 24.04) or 1.1.1 (RHEL 8, Debian 11, Ubuntu 22.04)
- SQLite 3.x
- systemd

## Installation Methods

### RPM Installation

#### 1. Add OpenVox Repository (Recommended)

Create `/etc/yum.repos.d/openvox.repo`:

```ini
[openvox]
name=OpenVox Repository
baseurl=https://packages.openvoxproject.org/rpm/el/$releasever/$basearch/
enabled=1
gpgcheck=1
gpgkey=https://packages.openvoxproject.org/RPM-GPG-KEY-openvox
```

Install the package:

```bash
sudo dnf install openvox-webui
# or
sudo yum install openvox-webui
```

#### 2. Direct RPM Installation

Download the RPM package:

```bash
# For RHEL 9 / Rocky 9 / AlmaLinux 9
wget https://github.com/openvoxproject/openvox-webui/releases/download/v0.9.0/openvox-webui-0.9.0-1.el9.x86_64.rpm

# For RHEL 8 / Rocky 8 / AlmaLinux 8
wget https://github.com/openvoxproject/openvox-webui/releases/download/v0.9.0/openvox-webui-0.9.0-1.el8.x86_64.rpm

# For Fedora 40
wget https://github.com/openvoxproject/openvox-webui/releases/download/v0.9.0/openvox-webui-0.9.0-1.fc40.x86_64.rpm
```

Install:

```bash
sudo dnf install ./openvox-webui-*.rpm
# or
sudo yum localinstall ./openvox-webui-*.rpm
```

### DEB Installation

#### 1. Add OpenVox Repository (Recommended)

```bash
# Add GPG key
curl -fsSL https://packages.openvoxproject.org/DEB-GPG-KEY-openvox | sudo gpg --dearmor -o /usr/share/keyrings/openvox-archive-keyring.gpg

# Add repository (Debian 12)
echo "deb [signed-by=/usr/share/keyrings/openvox-archive-keyring.gpg] https://packages.openvoxproject.org/deb bookworm main" | sudo tee /etc/apt/sources.list.d/openvox.list

# Add repository (Ubuntu 24.04)
echo "deb [signed-by=/usr/share/keyrings/openvox-archive-keyring.gpg] https://packages.openvoxproject.org/deb noble main" | sudo tee /etc/apt/sources.list.d/openvox.list

# Update and install
sudo apt update
sudo apt install openvox-webui
```

#### 2. Direct DEB Installation

Download the DEB package:

```bash
# For Debian 12 / Ubuntu 24.04
wget https://github.com/openvoxproject/openvox-webui/releases/download/v0.9.0/openvox-webui_0.9.0-1_amd64.deb
```

Install:

```bash
sudo dpkg -i openvox-webui_0.9.0-1_amd64.deb
sudo apt-get install -f  # Install dependencies if needed
```

### Puppet Module Installation

#### From Puppet Forge

```bash
puppet module install openvox-webui
```

#### From Git Repository

```bash
cd /etc/puppetlabs/code/environments/production/modules
git clone https://github.com/openvoxproject/openvox-webui.git openvox_webui
```

#### Puppetfile

Add to your `Puppetfile`:

```ruby
mod 'openvox-webui',
  :git => 'https://github.com/openvoxproject/openvox-webui.git',
  :tag => 'v0.9.0'
```

## Initial Configuration

### 1. Basic Configuration

Edit `/etc/openvox-webui/config.yaml`:

```yaml
server:
  host: "0.0.0.0"  # Listen on all interfaces
  port: 5051

database:
  url: "sqlite:///var/lib/openvox-webui/openvox.db"

logging:
  level: "info"
```

### 2. PuppetDB Configuration

Add PuppetDB connection details:

```yaml
puppetdb:
  url: "https://puppetdb.example.com:8081"
  timeout: 30
  ssl:
    cert_path: "/etc/puppetlabs/puppet/ssl/certs/webui.pem"
    key_path: "/etc/puppetlabs/puppet/ssl/private_keys/webui.pem"
    ca_path: "/etc/puppetlabs/puppet/ssl/certs/ca.pem"
    verify: true
```

### 3. Authentication Configuration

Set JWT secret and admin credentials:

```yaml
auth:
  jwt_secret: "your-secret-key-min-32-characters-long"
  jwt_expiry: "24h"
  session_timeout: 3600

# Initial admin account (used on first start)
initial_admin:
  username: "admin"
  password: "ChangeMe123!"
  email: "admin@example.com"
```

**Important:** Change the admin password immediately after first login!

### 4. Using Puppet Module

Create a Puppet manifest:

```puppet
class { 'openvox_webui':
  listen_address    => '0.0.0.0',
  listen_port       => 5051,
  
  # PuppetDB connection
  puppetdb_url      => 'https://puppetdb.example.com:8081',
  puppetdb_ssl_cert => '/etc/puppetlabs/puppet/ssl/certs/webui.pem',
  puppetdb_ssl_key  => '/etc/puppetlabs/puppet/ssl/private_keys/webui.pem',
  puppetdb_ssl_ca   => '/etc/puppetlabs/puppet/ssl/certs/ca.pem',
  
  # Authentication
  admin_password    => Sensitive('SecurePassword123!'),
  admin_email       => 'admin@example.com',
  
  # Logging
  log_level         => 'info',
}
```

Or use Hiera:

```yaml
# In your Hiera data:
openvox_webui::listen_address: '0.0.0.0'
openvox_webui::listen_port: 5051
openvox_webui::puppetdb_url: 'https://puppetdb.example.com:8081'
openvox_webui::admin_password: >
  ENC[PKCS7,...]  # Use eyaml for sensitive data
```

Then apply:

```bash
include openvox_webui
```

## Post-Installation

### 1. Enable and Start Service

```bash
# Enable service to start on boot
sudo systemctl enable openvox-webui

# Start the service
sudo systemctl start openvox-webui

# Check status
sudo systemctl status openvox-webui
```

### 2. Verify Installation

Check that the service is listening:

```bash
ss -tlnp | grep 5051
```

Test the health endpoint:

```bash
curl http://localhost:5051/api/v1/health
```

Expected response:

```json
{
  "status": "healthy",
  "version": "0.9.0"
}
```

### 3. Access Web Interface

Open your browser and navigate to:

```
http://your-server:5051
```

Login with the admin credentials you configured.

### 4. Firewall Configuration

**For firewalld (RHEL/CentOS/Fedora):**

```bash
sudo firewall-cmd --permanent --add-port=5051/tcp
sudo firewall-cmd --reload
```

**For UFW (Ubuntu/Debian):**

```bash
sudo ufw allow 5051/tcp
sudo ufw reload
```

### 5. Reverse Proxy Setup (Optional)

#### Nginx

```nginx
server {
    listen 80;
    server_name openvox.example.com;

    location / {
        proxy_pass http://localhost:5051;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

#### Apache

```apache
<VirtualHost *:80>
    ServerName openvox.example.com
    
    ProxyPreserveHost On
    ProxyPass / http://localhost:5051/
    ProxyPassReverse / http://localhost:5051/
    
    RequestHeader set X-Forwarded-Proto "http"
</VirtualHost>
```

## Troubleshooting

### Service Won't Start

Check logs:

```bash
sudo journalctl -u openvox-webui -n 100 --no-pager
```

Check configuration:

```bash
sudo openvox-webui --check-config
```

### Cannot Connect to PuppetDB

Verify SSL certificates:

```bash
# Check certificate validity
openssl x509 -in /path/to/cert.pem -text -noout

# Test PuppetDB connection
curl --cert /path/to/cert.pem \
     --key /path/to/key.pem \
     --cacert /path/to/ca.pem \
     https://puppetdb.example.com:8081/status/v1/services
```

### Database Errors

Check database file permissions:

```bash
ls -la /var/lib/openvox-webui/openvox.db
# Should be owned by openvox-webui:openvox-webui
```

Reset database (WARNING: deletes all data):

```bash
sudo systemctl stop openvox-webui
sudo rm /var/lib/openvox-webui/openvox.db
sudo systemctl start openvox-webui
```

### Port Already in Use

Find what's using the port:

```bash
sudo ss -tlnp | grep :5051
```

Change the port in configuration:

```yaml
server:
  port: 8080  # Use different port
```

### Permission Denied Errors

Check file permissions:

```bash
# Configuration directory
sudo chown -R root:openvox-webui /etc/openvox-webui
sudo chmod 750 /etc/openvox-webui
sudo chmod 640 /etc/openvox-webui/config.yaml

# Data directory
sudo chown -R openvox-webui:openvox-webui /var/lib/openvox-webui
sudo chmod 750 /var/lib/openvox-webui

# Log directory
sudo chown -R openvox-webui:openvox-webui /var/log/openvox-webui
sudo chmod 755 /var/log/openvox-webui
```

### SELinux Issues (RHEL/CentOS/Fedora)

If SELinux is blocking the service:

```bash
# Check for denials
sudo ausearch -m avc -ts recent

# Allow necessary permissions
sudo setsebool -P httpd_can_network_connect 1

# Or create custom policy
sudo audit2allow -a -M openvox-webui
sudo semodule -i openvox-webui.pp
```

## Next Steps

- [Configuration Reference](CONFIGURATION.md) - Detailed configuration options
- [Upgrade Guide](UPGRADE.md) - Upgrading between versions
- [Backup and Restore](BACKUP.md) - Backup and disaster recovery procedures
- [Puppet Module Documentation](puppet/README.md) - Puppet module usage guide

## Support

- GitHub Issues: https://github.com/openvoxproject/openvox-webui/issues
- Documentation: https://docs.openvoxproject.org
- Community: https://community.openvoxproject.org
