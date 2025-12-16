# Basic example - install with defaults
# This will install the package, configure the service to listen on localhost:3000
include openvox_webui

# Example with PuppetDB integration
# class { 'openvox_webui':
#   listen_address    => '0.0.0.0',
#   listen_port       => 8080,
#   puppetdb_url      => 'https://puppetdb.example.com:8081',
#   puppetdb_ssl_cert => '/etc/puppetlabs/puppet/ssl/certs/webui.pem',
#   puppetdb_ssl_key  => '/etc/puppetlabs/puppet/ssl/private_keys/webui.pem',
#   puppetdb_ssl_ca   => '/etc/puppetlabs/puppet/ssl/certs/ca.pem',
#   admin_password    => Sensitive('SecurePassword123!'),
#   admin_email       => 'admin@example.com',
# }
