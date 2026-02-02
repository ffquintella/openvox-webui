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
4. [Client Configuration](#client-configuration)
    * [Why Configure Clients?](#why-configure-clients)
    * [Setting Up the Client](#setting-up-the-client)
    * [How Classification Works](#how-classification-works)
    * [Using Groups in the Frontend](#using-groups-in-the-frontend)
5. [Reference](#reference)
6. [Limitations](#limitations)

## Description

This module installs and configures OpenVox WebUI, a web interface for managing and monitoring OpenVox infrastructure.

Features:

- Package installation (RPM/DEB)
- Service management via systemd
- Configuration file management via templates
- PuppetDB connection configuration
- RBAC initial setup
- Hiera integration
- **Client-side fact reporting** for node classification
- **Node group management** with rule-based classification

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

### External Node Classifier (Server-Side)

Configure Puppet Server to use OpenVox WebUI as an External Node Classifier (ENC):

```puppet
# On Puppet Server node
class { 'openvox_webui::enc':
  webui_url                => 'https://openvox.example.com',
  manage_puppet_conf       => true,
  restart_puppetserver     => true,
  ssl_verify               => false,  # For self-signed certificates
  remove_agent_environment => true,   # Remove conflicting environment settings
}
```

This configures Puppet Server to query OpenVox WebUI for node classification, including environment assignment. See [ENC Setup Guide](../docs/ENC_SETUP.md) for detailed documentation.

### Node Classification (Client-Side)

Alternatively, configure Puppet agents to fetch and apply classification from OpenVox WebUI:

```puppet
# Step 1: Configure the client to fetch classification data
class { 'openvox_webui::client':
  api_url          => 'https://openvox.example.com:5051',
  use_puppet_certs => true,
}

# Step 2: Apply classified classes with their parameters
include openvox_webui::classification
```

This enables centralized node classification similar to Puppet Enterprise's Node Classifier.
Classes defined in OpenVox WebUI groups will be automatically included with their parameters.

#### How It Works

1. **openvox_webui::client** - Configures the custom fact that fetches classification
   data from the OpenVox WebUI API at `/api/v1/nodes/{certname}/classify`

2. **openvox_webui::classification** - Reads the classification fact and uses
   `create_resources('class', ...)` to declare all classified classes with parameters

#### Available Facts

After configuring the client, these facts become available:

- `$facts['openvox_classification']` - Full classification data (groups, classes, variables)
- `$facts['openvox_groups']` - Array of group names the node belongs to
- `$facts['openvox_classes']` - Hash of classes with their parameters
- `$facts['openvox_variables']` - Variables defined in matched groups
- `$facts['openvox_environment']` - Classified environment name
- Top-level facts for each variable (e.g., `$facts['role']`, `$facts['datacenter']`)

#### Classification Options

```puppet
class { 'openvox_webui::classification':
  apply_classes          => true,           # Apply classified classes
  fail_on_missing_class  => false,          # Don't fail if class not found
  class_prefix           => 'profile::',    # Prefix all class names
  excluded_classes       => ['deprecated*'], # Exclude matching classes
  require_classification => false,          # Don't fail if no classification
  log_level              => 'debug',        # Logging level
}
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

## Client Configuration

This section explains how to configure Puppet agents (clients) to report facts to OpenVox WebUI and use the node classification system.

### Why Configure Clients?

The OpenVox WebUI provides powerful features that require client-side configuration to work properly:

1. **Fact Reporting**: Clients report their facts to PuppetDB, which OpenVox WebUI queries. The `openvox_webui::client` class adds custom facts that enable bidirectional communication with the WebUI.

2. **Node Classification**: OpenVox WebUI can classify nodes into groups based on their facts. For this classification to be applied during Puppet runs, clients need to fetch and apply the classification data.

3. **Group-Based Management**: In the OpenVox WebUI frontend, you can create node groups with rules (e.g., "all nodes where `os.family` equals `RedHat`"). For nodes to appear in these groups and receive their assigned classes/parameters, the client module must be installed.

**Without client configuration**, you can still:
- View nodes and their facts in the WebUI (via PuppetDB)
- Create groups and rules
- Browse reports

**With client configuration**, you additionally get:
- Automatic class assignment based on group membership
- Group variables available as facts
- Environment assignment from WebUI
- Full node classification similar to Puppet Enterprise

### Setting Up the Client

Install the `openvox_webui::client` class on all Puppet agents that should participate in the classification system:

```puppet
# In your site.pp or a profile class applied to all nodes
class { 'openvox_webui::client':
  api_url          => 'https://openvox.example.com:5051',
  use_puppet_certs => true,  # Use existing Puppet SSL certificates
}
```

Or via Hiera (recommended):

```yaml
# In your common.yaml or equivalent
classes:
  - openvox_webui::client

openvox_webui::client::api_url: 'https://openvox.example.com:5051'
openvox_webui::client::use_puppet_certs: true
```

This installs a custom fact script that contacts the OpenVox WebUI API during each Puppet run to fetch the node's classification.

### How Classification Works

The classification system follows this flow:

```text
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Puppet Agent   │────▶│  OpenVox WebUI  │────▶│    PuppetDB     │
│  (with client)  │     │     Server      │     │                 │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │
        │ 1. Request            │ 2. Query node facts
        │    classification     │
        │                       ▼
        │               ┌─────────────────┐
        │               │  Evaluate rules │
        │               │  against facts  │
        │               └─────────────────┘
        │                       │
        │ 3. Return groups,     │
        │    classes, variables │
        ◀───────────────────────┘
        │
        ▼
┌─────────────────┐
│ Apply classes   │
│ via classification │
│ class           │
└─────────────────┘
```

1. **Fact Collection**: When Puppet runs, the custom fact contacts the WebUI API endpoint `/api/v1/nodes/{certname}/classification`

2. **Rule Evaluation**: The WebUI evaluates all group rules against the node's facts from PuppetDB

3. **Classification Response**: The WebUI returns:
   - Groups the node belongs to
   - Classes to apply (with parameters)
   - Variables defined in those groups
   - Environment assignment (if configured)

4. **Class Application**: The `openvox_webui::classification` class reads the classification fact and declares all assigned classes

To apply the classified classes, include the classification class:

```puppet
# After configuring the client
include openvox_webui::classification
```

### Using Groups in the Frontend

The OpenVox WebUI provides a visual interface for managing node groups. Here's how to use them effectively:

#### Creating Groups

1. Navigate to **Groups** in the WebUI sidebar
2. Click **Create Group**
3. Enter a name and optional description
4. Add classification rules

#### Understanding Rules

Rules determine which nodes belong to a group. Each rule consists of:

- **Fact**: The fact path to evaluate (e.g., `os.family`, `networking.ip`, `role`)
- **Operator**: Comparison type (`=`, `!=`, `~` for regex, `>`, `<`, `in`)
- **Value**: The value to compare against

Examples:

| Rule | Description |
|------|-------------|
| `os.family = RedHat` | All RHEL-based systems |
| `networking.domain ~ .*\.prod\.example\.com` | Production domain nodes |
| `virtual != physical` | All virtual machines |
| `processorcount > 4` | Nodes with more than 4 CPUs |
| `role in [webserver, appserver]` | Nodes with specific roles |

Multiple rules in a group use AND logic (all must match).

#### Assigning Classes and Parameters

Once a group is created, you can assign:

1. **Classes**: Puppet classes to include on matching nodes
   - Navigate to the group detail page
   - Click **Add Class**
   - Enter the class name (e.g., `profile::webserver`)
   - Optionally add parameters as key-value pairs

2. **Variables**: Custom variables available as facts
   - These become top-level facts on matching nodes
   - Useful for setting `role`, `environment`, `datacenter`, etc.

#### Pinned Nodes

You can manually pin specific nodes to a group regardless of rules:

1. Open the group detail page
2. Click **Pin Node**
3. Enter the node's certname

Pinned nodes always belong to the group, even if they don't match the rules.

#### Group Hierarchy

Groups can have parent-child relationships:

- Child groups inherit classes and variables from parents
- Rules are evaluated independently (a node must match the child's rules)
- This allows for layered configuration (e.g., `base` → `production` → `webservers`)

#### Viewing Group Membership

To see which nodes belong to a group:

1. Open the group detail page
2. View the **Nodes** tab
3. See both rule-matched and pinned nodes

To see which groups a node belongs to:

1. Navigate to **Nodes**
2. Click on a node's certname
3. View the **Groups** section

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
