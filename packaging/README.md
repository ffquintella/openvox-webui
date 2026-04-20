# OpenVox WebUI Packaging

This directory contains packaging scripts and configurations for building native Linux packages.

## Supported Distributions

- **RPM-based**: RHEL 8/9, Rocky Linux 8/9, AlmaLinux 8/9, Fedora 38+, CentOS Stream 8/9
- **DEB-based**: Debian 11/12, Ubuntu 22.04/24.04

## Building Packages

### Quick Start

```bash
# Build all packages (uses version from Cargo.toml)
./scripts/build-packages.sh

# Build specific package type
./scripts/build-packages.sh rpm
./scripts/build-packages.sh deb

# Build with specific version
./scripts/build-packages.sh -v 1.0.0 all

# Build using Docker (for cross-distro builds)
./scripts/build-packages.sh --docker rpm
```

### Build Output

Packages are placed in `build/output/`:
- `openvox-webui-<version>.<arch>.rpm` - Binary RPM
- `openvox-webui-<version>-<release>.src.rpm` - Source RPM
- `openvox-webui_<version>_<arch>.deb` - Debian package
- `openvox-webui-<version>-linux-<arch>.tar.gz` - Binary tarball

### Build Options

| Option | Description |
|--------|-------------|
| `-v VERSION` | Set package version |
| `-r RELEASE` | Set release number |
| `-o DIR` | Output directory |
| `-a ARCH` | Target architecture |
| `--docker` | Use Docker for building |
| `--clean` | Clean build directory first |
| `--sign` | Sign packages (requires GPG_KEY_ID) |

### Build Targets

| Target | Description |
|--------|-------------|
| `all` | Build all packages (default) |
| `rpm` | Build RPM only |
| `deb` | Build DEB only |
| `source` | Create source tarball only |
| `binary` | Create binary tarball only |

## Package Contents

The packages install the following:

| Path | Description |
|------|-------------|
| `/usr/bin/openvox-webui` | Main binary |
| `/etc/openvox-webui/config.yaml` | Configuration file |
| `/etc/openvox-webui/ssl/` | SSL certificates directory |
| `/usr/share/openvox-webui/static/` | Frontend assets |
| `/var/lib/openvox-webui/` | Data directory (SQLite database) |
| `/var/log/openvox/webui/` | Log files |
| `/lib/systemd/system/openvox-webui.service` | Systemd unit |

## Post-Installation

After installing the package:

1. Edit the configuration file:
   ```bash
   sudo vi /etc/openvox-webui/config.yaml
   ```

2. Set up TLS certificates (recommended for production):
   ```bash
   # Option 1: Use existing certificates
   sudo cp /path/to/cert.pem /etc/openvox-webui/ssl/server.crt
   sudo cp /path/to/key.pem /etc/openvox-webui/ssl/server.key
   sudo chmod 640 /etc/openvox-webui/ssl/server.key
   sudo chown root:openvox-webui /etc/openvox-webui/ssl/server.key

   # Option 2: Generate self-signed certificates
   sudo openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
     -keyout /etc/openvox-webui/ssl/server.key \
     -out /etc/openvox-webui/ssl/server.crt \
     -subj "/CN=openvox-webui"
   ```

3. Start the service:
   ```bash
   sudo systemctl enable openvox-webui
   sudo systemctl start openvox-webui
   ```

4. Check status:
   ```bash
   sudo systemctl status openvox-webui
   sudo journalctl -u openvox-webui -f
   ```

## Package Signing

### Setting Up GPG Key

```bash
# Generate a new GPG key
gpg --full-generate-key

# List keys to get the key ID
gpg --list-keys

# Export public key for repository
gpg --armor --export YOUR_KEY_ID > openvox-webui.gpg
```

### Building Signed Packages

```bash
export GPG_KEY_ID="YOUR_KEY_ID"
./scripts/build-packages.sh --sign all
```

## Setting Up a Package Repository

### YUM/DNF Repository (RPM)

1. Create repository structure:
   ```bash
   mkdir -p /var/www/html/repos/openvox-webui/{el8,el9}/x86_64
   ```

2. Copy packages:
   ```bash
   cp build/output/*.rpm /var/www/html/repos/openvox-webui/el9/x86_64/
   ```

3. Create repository metadata:
   ```bash
   createrepo /var/www/html/repos/openvox-webui/el9/x86_64/
   ```

4. Create repo file for clients (`/etc/yum.repos.d/openvox-webui.repo`):
   ```ini
   [openvox-webui]
   name=OpenVox WebUI
   baseurl=https://your-server/repos/openvox-webui/el$releasever/$basearch
   enabled=1
   gpgcheck=1
   gpgkey=https://your-server/repos/openvox-webui/openvox-webui.gpg
   ```

### APT Repository (DEB)

1. Create repository structure:
   ```bash
   mkdir -p /var/www/html/repos/openvox-webui/pool/main
   mkdir -p /var/www/html/repos/openvox-webui/dists/stable/main/binary-amd64
   ```

2. Copy packages:
   ```bash
   cp build/output/*.deb /var/www/html/repos/openvox-webui/pool/main/
   ```

3. Create package index:
   ```bash
   cd /var/www/html/repos/openvox-webui
   apt-ftparchive packages pool/main > dists/stable/main/binary-amd64/Packages
   gzip -k dists/stable/main/binary-amd64/Packages
   apt-ftparchive release dists/stable > dists/stable/Release
   gpg --armor --detach-sign -o dists/stable/Release.gpg dists/stable/Release
   gpg --armor --clearsign -o dists/stable/InRelease dists/stable/Release
   ```

4. Create sources.list entry for clients:
   ```
   deb [signed-by=/usr/share/keyrings/openvox-webui.gpg] https://your-server/repos/openvox-webui stable main
   ```

## Directory Structure

```
packaging/
├── README.md           # This file
├── rpm/
│   └── openvox-webui.spec  # RPM spec file
├── deb/
│   └── debian/
│       ├── changelog   # Package changelog
│       ├── control     # Package metadata
│       ├── copyright   # License information
│       ├── postinst    # Post-installation script
│       ├── postrm      # Post-removal script
│       ├── rules       # Build rules
│       └── openvox-webui.dirs  # Directory list
└── systemd/
    └── openvox-webui.service  # Systemd unit file
```

## Troubleshooting

### Build Errors

1. **Missing Rust toolchain**: Install via rustup
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Missing Node.js**: Install Node.js 20+
   ```bash
   # RHEL/Rocky
   dnf module enable nodejs:20 && dnf install nodejs

   # Debian/Ubuntu
   curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
   apt-get install -y nodejs
   ```

3. **Missing build tools**:
   ```bash
   # RHEL/Rocky
   dnf install rpm-build rpmdevtools gcc openssl-devel sqlite-devel

   # Debian/Ubuntu
   apt-get install build-essential debhelper devscripts libssl-dev libsqlite3-dev
   ```

### Service Issues

1. Check logs:
   ```bash
   journalctl -u openvox-webui -n 100 --no-pager
   ```

2. Verify configuration:
   ```bash
   /usr/bin/openvox-webui --check-config
   ```

3. Test manually:
   ```bash
   sudo -u openvox-webui /usr/bin/openvox-webui
   ```
