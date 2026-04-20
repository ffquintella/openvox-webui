# Example: Node Classification with OpenVox WebUI
#
# This example shows how to configure Puppet agents to receive
# their classification from OpenVox WebUI and automatically
# apply the classified classes with their parameters.
#
# Prerequisites:
# 1. OpenVox WebUI server running and accessible
# 2. Node groups configured in OpenVox WebUI with classes and rules
# 3. Puppet certificates for mTLS authentication (recommended)
#
# Usage in site.pp or role class:
#   include openvox_webui::client
#   include openvox_webui::classification
#

# Step 1: Configure the client to fetch classification data
# This sets up the custom fact that contacts the OpenVox WebUI API
class { 'openvox_webui::client':
  # URL of the OpenVox WebUI API
  api_url          => 'https://openvox.example.com:5051',

  # Use Puppet agent certificates for mTLS authentication (recommended)
  use_puppet_certs => true,

  # Alternative: Use API token authentication
  # api_token => Sensitive('your-api-token'),

  # SSL verification (default: true)
  ssl_verify       => true,

  # Request timeout in seconds
  timeout          => 30,
}

# Step 2: Apply classified classes with their parameters
# This reads the openvox_classification fact and includes all classes
class { 'openvox_webui::classification':
  # Apply all classified classes (default: true)
  apply_classes          => true,

  # Don't fail if a classified class is not available (default: false)
  # Set to true in production for strict mode
  fail_on_missing_class  => false,

  # Optional: Prefix all class names
  # Useful when OpenVox uses short names like 'webserver'
  # that should map to 'profile::webserver'
  # class_prefix => 'profile::',

  # Exclude specific classes or patterns
  excluded_classes       => [
    'deprecated::*',      # Exclude all deprecated classes
    'test::experimental', # Exclude specific class
  ],

  # Don't fail if no classification data is available
  require_classification => false,

  # Log level for classification messages
  log_level              => 'info',
}

# Alternative: Minimal configuration using defaults
# class { 'openvox_webui::client':
#   api_url => 'https://openvox.example.com:5051',
# }
# include openvox_webui::classification

# The classification data is also available as facts for custom logic:
#
# if 'webservers' in $facts['openvox_groups'] {
#   notify { 'This node is a webserver': }
# }
#
# $role = $facts['openvox_variables']['role']
# if $role == 'database' {
#   include profile::database_extras
# }
