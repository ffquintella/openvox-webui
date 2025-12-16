# openvox_webui

## Table of Contents

1. [Description](#description)
2. [Setup](#setup)
    * [Requirements](#requirements)
    * [Installation](#installation)
3. [Usage](#usage)
    * [Basic Usage](#basic-usage)
    * [PuppetDB Integration](#puppetdb-integration)
    * [Hiera Configuration](#hiera-configuration)
4. [Reference](#reference)
5. [Limitations](#limitations)

## Description

This module installs and configures OpenVox WebUI, a web interface for managing and monitoring OpenVox infrastructure.

Features:
- Package installation (RPM/DEB)
- Service management via systemd
- Configuration file management via templates
- PuppetDB connection configuration
- RBAC initial setup
- Hiera integration

## Setup

### Requirements

- Puppet 7.x or 8.x
- puppetlabs/stdlib >= 8.0.0

### Installation

Install from Puppet Forge:

```bash
puppet module install openvox-webui
```

Or add to your Puppetfile:

```ruby
mod 'openvox-webui', :latest
```

## Usage

### Basic Usage

Install with default settings (listens on localhost:3000):

```puppet
include openvox_webui
```

### PuppetDB Integration

Configure connection to PuppetDB with SSL:

```puppet
class { 'openvox_webui':
  listen_address    => '0.0.0.0',
  listen_port       => 3000,
  puppetdb_url      => 'https://puppetdb.example.com:8081',
  puppetdb_ssl_cert => '/etc/puppetlabs/puppet/ssl/certs/webui.pem',
  puppetdb_ssl_key  => '/etc/puppetlabs/puppet/ssl/private_keys/webui.pem',
  puppetdb_ssl_ca   => '/etc/puppetlabs/puppet/ssl/certs/ca.pem',
}
```

### Hiera Configuration

All parameters can be configured via Hiera:

```yaml
# common.yaml
openvox_webui::listen_address: '0.0.0.0'
openvox_webui::listen_port: 8080
openvox_webui::puppetdb_url: 'https://puppetdb.example.com:8081'
openvox_webui::log_level: 'debug'

# Sensitive data should use eyaml or similar
openvox_webui::admin_password: ENC[PKCS7,...]
```

## Reference

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ensure` | Enum | `present` | Package state: present, absent, latest |
| `package_name` | String | `openvox-webui` | Package name to install |
| `service_name` | String | `openvox-webui` | Systemd service name |
| `service_ensure` | Enum | `running` | Service state: running, stopped |
| `service_enable` | Boolean | `true` | Enable service at boot |
| `listen_address` | String | `127.0.0.1` | IP address to bind |
| `listen_port` | Integer | `3000` | Port to listen on |
| `database_path` | String | `/var/lib/openvox-webui/openvox.db` | SQLite database path |
| `log_level` | Enum | `info` | Log level: trace, debug, info, warn, error |
| `puppetdb_url` | String | `undef` | PuppetDB URL |
| `puppetdb_ssl_cert` | String | `undef` | SSL certificate path |
| `puppetdb_ssl_key` | String | `undef` | SSL private key path |
| `puppetdb_ssl_ca` | String | `undef` | SSL CA certificate path |
| `puppetdb_timeout` | Integer | `30` | PuppetDB request timeout |
| `jwt_secret` | String | random | JWT signing secret (min 32 chars) |
| `jwt_expiry` | String | `24h` | JWT token expiry |
| `session_timeout` | Integer | `3600` | Session timeout in seconds |
| `admin_username` | String | `admin` | Initial admin username |
| `admin_password` | Sensitive | `undef` | Initial admin password |
| `admin_email` | String | `undef` | Initial admin email |
| `manage_package` | Boolean | `true` | Manage package installation |
| `manage_service` | Boolean | `true` | Manage systemd service |
| `manage_config` | Boolean | `true` | Manage configuration files |

## Limitations

- Requires systemd (no SysV init support)
- SQLite database only (no external database support yet)
- Tested on:
  - RHEL/CentOS/Rocky/AlmaLinux 8, 9
  - Fedora 38, 39, 40
  - Debian 11, 12
  - Ubuntu 22.04, 24.04
