# OpenVox WebUI Installation Guide

This guide covers installation of OpenVox WebUI using various methods.

## Table of Contents

1. [System Requirements](#system-requirements)
2. [Installation Methods](#installation-methods)
   - [Package Installation (RPM)](#rpm-installation)
   - [Package Installation (DEB)](#deb-installation)
   - [Puppet Module Installation](#puppet-module-installation)
3. [Building Packages from Source](#building-packages-from-source)
   - [Prerequisites](#build-prerequisites)
   - [Building RPM Packages](#building-rpm-packages)
   - [Building DEB Packages](#building-deb-packages)
   - [Setting Up a Local Repository](#setting-up-a-local-repository)
4. [Initial Configuration](#initial-configuration)
5. [Post-Installation](#post-installation)
6. [Troubleshooting](#troubleshooting)

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

OpenVox WebUI is currently distributed via GitHub releases or by building from source. You can either download pre-built packages (when available) or build your own packages and host them in a local repository.

### RPM Installation

#### Download RPM from GitHub Releases

Check the [GitHub Releases page](https://github.com/ffquintella/openvox-webui/releases) for pre-built RPM packages.

```bash
# Download the package for your OS version (example for RHEL 9)
wget https://github.com/ffquintella/openvox-webui/releases/download/v0.27.0/openvox-webui-0.27.0-1.el9.x86_64.rpm

# Install the package
sudo dnf install ./openvox-webui-*.rpm
# or
sudo yum localinstall ./openvox-webui-*.rpm
```

#### Build RPM from Source

If pre-built packages are not available for your platform, or you prefer to build your own, see the [Building Packages from Source](#building-packages-from-source) section below.

#### Use a Local YUM/DNF Repository

For enterprise deployments, you can build packages and host them in your own repository. See [Setting Up a Local Repository](#setting-up-a-local-repository) for instructions.

### DEB Installation

#### Download DEB from GitHub Releases

Check the [GitHub Releases page](https://github.com/ffquintella/openvox-webui/releases) for pre-built DEB packages.

```bash
# Download the package (example)
wget https://github.com/ffquintella/openvox-webui/releases/download/v0.27.0/openvox-webui_0.27.0-1_amd64.deb

# Install the package
sudo dpkg -i openvox-webui_*.deb
sudo apt-get install -f  # Install dependencies if needed
```

#### Build DEB from Source

If pre-built packages are not available, see the [Building Packages from Source](#building-packages-from-source) section below.

#### Use a Local APT Repository

For enterprise deployments, you can build packages and host them in your own repository. See [Setting Up a Local Repository](#setting-up-a-local-repository) for instructions.

### Puppet Module Installation

The Puppet module can install and configure OpenVox WebUI. It requires the package to be available either from a local repository or by placing the package file on the target system.

#### From Git Repository

```bash
cd /etc/puppetlabs/code/environments/production/modules
git clone https://github.com/ffquintella/openvox-webui.git openvox_webui
cd openvox_webui
# The Puppet module is in the puppet/ subdirectory
mv puppet ../openvox_webui_module
cd .. && rm -rf openvox_webui && mv openvox_webui_module openvox_webui
```

#### Puppetfile

Add to your `Puppetfile`:

```ruby
mod 'openvox_webui',
  :git    => 'https://github.com/ffquintella/openvox-webui.git',
  :tag    => 'v0.27.0',
  :install_path => './openvox_webui_tmp'

# Note: The module is located in the puppet/ subdirectory of the repository
```

**Important:** When using the Puppet module, ensure the openvox-webui package is available. You can either:

1. Set up a local package repository (see [Setting Up a Local Repository](#setting-up-a-local-repository))
2. Use the `package_source` parameter to specify a local package file path

## Building Packages from Source

If you prefer to build packages yourself instead of using the official repositories, you can build RPM and DEB packages from source and host them in your own local repository.

### Build Prerequisites

Before building packages, ensure you have the following installed:

**Common Requirements:**

- Git
- Rust toolchain (1.70+)
- Node.js 20+
- npm

**For RPM builds (RHEL/Rocky/AlmaLinux/Fedora):**

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Node.js 20
sudo dnf module enable nodejs:20 -y
sudo dnf install nodejs -y

# Install build tools
sudo dnf install rpm-build rpmdevtools gcc openssl-devel sqlite-devel make -y
```

**For DEB builds (Debian/Ubuntu):**

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Node.js 20
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs

# Install build tools
sudo apt-get install -y build-essential debhelper devscripts libssl-dev libsqlite3-dev
```

### Building RPM Packages

1. Clone the repository:

```bash
git clone https://github.com/ffquintella/openvox-webui.git
cd openvox-webui
```

2. Build the RPM package:

```bash
# Build RPM using the build script
./scripts/build-packages.sh rpm

# Or build with a specific version
./scripts/build-packages.sh -v 1.0.0 rpm

# Or use make
make package-rpm
```

3. The built packages will be in `build/output/`:

```bash
ls build/output/*.rpm
# openvox-webui-<version>-1.el9.x86_64.rpm
# openvox-webui-<version>-1.el9.src.rpm
```

### Building DEB Packages

1. Clone the repository:

```bash
git clone https://github.com/ffquintella/openvox-webui.git
cd openvox-webui
```

2. Build the DEB package:

```bash
# Build DEB using the build script
./scripts/build-packages.sh deb

# Or build with a specific version
./scripts/build-packages.sh -v 1.0.0 deb

# Or use make
make package-deb
```

3. The built packages will be in `build/output/`:

```bash
ls build/output/*.deb
# openvox-webui_<version>-1_amd64.deb
```

### Setting Up a Local Repository

After building your packages, you can host them in a local repository for easy distribution across your infrastructure.

#### Setting Up a YUM/DNF Repository (RPM)

1. Install repository tools:

```bash
sudo dnf install createrepo_c httpd -y
```

2. Create the repository structure:

```bash
# Create directories for each OS version you support
sudo mkdir -p /var/www/html/repos/openvox-webui/{el8,el9}/x86_64
```

3. Copy your built packages:

```bash
# Copy to the appropriate directory based on target OS
sudo cp build/output/*.el9.x86_64.rpm /var/www/html/repos/openvox-webui/el9/x86_64/
sudo cp build/output/*.el8.x86_64.rpm /var/www/html/repos/openvox-webui/el8/x86_64/
```

4. Generate repository metadata:

```bash
sudo createrepo /var/www/html/repos/openvox-webui/el9/x86_64/
sudo createrepo /var/www/html/repos/openvox-webui/el8/x86_64/
```

5. Configure the web server and start it:

```bash
sudo systemctl enable httpd
sudo systemctl start httpd
sudo firewall-cmd --permanent --add-service=http
sudo firewall-cmd --reload
```

6. On client machines, create `/etc/yum.repos.d/openvox-webui-local.repo`:

```ini
[openvox-webui-local]
name=OpenVox WebUI Local Repository
baseurl=http://your-repo-server/repos/openvox-webui/el$releasever/$basearch
enabled=1
gpgcheck=0
```

7. Install from your local repository:

```bash
sudo dnf install openvox-webui
```

#### Setting Up an APT Repository (DEB)

1. Install repository tools:

```bash
sudo apt-get install -y dpkg-dev apt-utils apache2
```

2. Create the repository structure:

```bash
sudo mkdir -p /var/www/html/repos/openvox-webui/pool/main
sudo mkdir -p /var/www/html/repos/openvox-webui/dists/stable/main/binary-amd64
```

3. Copy your built packages:

```bash
sudo cp build/output/*.deb /var/www/html/repos/openvox-webui/pool/main/
```

4. Generate package indices:

```bash
cd /var/www/html/repos/openvox-webui

# Generate Packages file
sudo apt-ftparchive packages pool/main > dists/stable/main/binary-amd64/Packages
sudo gzip -k dists/stable/main/binary-amd64/Packages

# Generate Release file
sudo apt-ftparchive release dists/stable > dists/stable/Release
```

5. Start the web server:

```bash
sudo systemctl enable apache2
sudo systemctl start apache2
```

6. On client machines, add the repository:

```bash
# Add the repository (without GPG signing for local use)
echo "deb [trusted=yes] http://your-repo-server/repos/openvox-webui stable main" | sudo tee /etc/apt/sources.list.d/openvox-webui-local.list

# Update and install
sudo apt-get update
sudo apt-get install openvox-webui
```

#### Optional: Signing Packages for Production Use

For production environments, it's recommended to sign your packages:

1. Generate a GPG key:

```bash
gpg --full-generate-key
gpg --list-keys  # Note your key ID
```

2. Build signed packages:

```bash
export GPG_KEY_ID="YOUR_KEY_ID"
./scripts/build-packages.sh --sign all
```

3. For APT repositories, sign the Release file:

```bash
cd /var/www/html/repos/openvox-webui
gpg --armor --detach-sign -o dists/stable/Release.gpg dists/stable/Release
gpg --armor --clearsign -o dists/stable/InRelease dists/stable/Release

# Export public key for clients
gpg --armor --export YOUR_KEY_ID | sudo tee /var/www/html/repos/openvox-webui/openvox-webui.gpg
```

4. On client machines, import the key and update the sources list:

```bash
curl -fsSL http://your-repo-server/repos/openvox-webui/openvox-webui.gpg | sudo gpg --dearmor -o /usr/share/keyrings/openvox-webui-local.gpg

echo "deb [signed-by=/usr/share/keyrings/openvox-webui-local.gpg] http://your-repo-server/repos/openvox-webui stable main" | sudo tee /etc/apt/sources.list.d/openvox-webui-local.list
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

- GitHub Issues: https://github.com/ffquintella/openvox-webui/issues
- Documentation: https://docs.openvoxproject.org
- Community: https://community.openvoxproject.org
