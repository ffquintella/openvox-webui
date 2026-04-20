# Example: OpenVox WebUI with PuppetDB integration
#
# This example shows how to configure OpenVox WebUI to connect to
# an existing PuppetDB instance using SSL certificates.

class { 'openvox_webui':
  # Network settings - listen on all interfaces
  listen_address => '0.0.0.0',
  listen_port    => 3000,

  # PuppetDB connection
  puppetdb_url      => 'https://puppetdb.example.com:8081',
  puppetdb_ssl_cert => '/etc/puppetlabs/puppet/ssl/certs/webui.example.com.pem',
  puppetdb_ssl_key  => '/etc/puppetlabs/puppet/ssl/private_keys/webui.example.com.pem',
  puppetdb_ssl_ca   => '/etc/puppetlabs/puppet/ssl/certs/ca.pem',
  puppetdb_timeout  => 60,

  # Authentication
  jwt_expiry       => '12h',
  session_timeout  => 7200,

  # Initial admin account
  admin_username => 'admin',
  admin_password => Sensitive(lookup('openvox_webui::admin_password')),
  admin_email    => 'admin@example.com',

  # Logging
  log_level => 'info',
}
