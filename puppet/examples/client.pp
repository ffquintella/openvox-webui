# Example: Configure Puppet agents to fetch classification from OpenVox WebUI
#
# This example shows how to configure Puppet agents to automatically
# fetch their classification data from OpenVox WebUI.

# Basic configuration using Puppet certificates (recommended)
# The node's Puppet certificate is used for authentication,
# ensuring nodes can only fetch their own classification.
class { 'openvox_webui::client':
  api_url          => 'https://openvox.example.com:5051',
  use_puppet_certs => true,
}

# After applying this class, the node will have these facts available:
#
# $facts['openvox_classification'] - Full classification result with:
#   - certname: The node's certificate name
#   - groups: Array of group names the node belongs to
#   - classes: Array of Puppet classes assigned
#   - variables: Hash of variables from matched groups
#   - parameters: Hash of parameters from matched groups
#   - environment: Assigned environment
#
# $facts['openvox_groups'] - Array of group names
# $facts['openvox_classes'] - Array of class names
# $facts['openvox_variables'] - Hash of variables
# $facts['openvox_parameters'] - Hash of parameters
# $facts['openvox_environment'] - Environment string
#
# Example usage in Puppet manifests:
#
#   if 'production_webservers' in $facts['openvox_groups'] {
#     include profile::webserver
#   }
#
#   $datacenter = $facts['openvox_variables']['datacenter']
#   notify { "This node is in datacenter: ${datacenter}": }
