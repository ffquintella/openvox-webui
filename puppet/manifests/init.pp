# @summary Main class for OpenVox WebUI
#
# Installs and configures OpenVox WebUI, a web interface for managing
# and monitoring OpenVox infrastructure.
#
# @param ensure
#   Whether to install or remove the package. Valid values: 'present', 'absent', 'latest'.
#
# @param package_name
#   Name of the package to install.
#
# @param service_name
#   Name of the systemd service.
#
# @param service_ensure
#   Desired state of the service. Valid values: 'running', 'stopped'.
#
# @param service_enable
#   Whether to enable the service at boot.
#
# @param config_dir
#   Directory where configuration files are stored.
#
# @param data_dir
#   Directory where data files (database, etc.) are stored.
#
# @param user
#   User account that runs the service.
#
# @param group
#   Group account that runs the service.
#
# @param listen_address
#   IP address to bind the server to.
#
# @param listen_port
#   Port to listen on.
#
# @param database_path
#   Path to the SQLite database file.
#
# @param log_level
#   Logging verbosity. Valid values: 'trace', 'debug', 'info', 'warn', 'error'.
#
# @param puppetdb_url
#   URL of the PuppetDB server to connect to.
#
# @param puppetdb_ssl_cert
#   Path to SSL certificate for PuppetDB connection.
#
# @param puppetdb_ssl_key
#   Path to SSL private key for PuppetDB connection.
#
# @param puppetdb_ssl_ca
#   Path to SSL CA certificate for PuppetDB connection.
#
# @param puppetdb_timeout
#   Timeout in seconds for PuppetDB requests.
#
# @param jwt_secret
#   Secret key for JWT token signing. Should be at least 32 characters.
#
# @param jwt_expiry
#   JWT token expiry time (e.g., '24h', '7d').
#
# @param session_timeout
#   Session timeout in seconds.
#
# @param admin_username
#   Username for the initial admin account.
#
# @param admin_password
#   Password for the initial admin account (will be hashed).
#
# @param admin_email
#   Email for the initial admin account.
#
# @param manage_package
#   Whether to manage package installation.
#
# @param manage_service
#   Whether to manage the systemd service.
#
# @param manage_config
#   Whether to manage configuration files.
#
# @example Basic usage with defaults
#   include openvox_webui
#
# @example Configuring PuppetDB connection
#   class { 'openvox_webui':
#     puppetdb_url      => 'https://puppetdb.example.com:8081',
#     puppetdb_ssl_cert => '/etc/puppetlabs/puppet/ssl/certs/webui.pem',
#     puppetdb_ssl_key  => '/etc/puppetlabs/puppet/ssl/private_keys/webui.pem',
#     puppetdb_ssl_ca   => '/etc/puppetlabs/puppet/ssl/certs/ca.pem',
#   }
#
# @example Using Hiera
#   # In your hiera data:
#   # openvox_webui::listen_port: 8080
#   # openvox_webui::puppetdb_url: 'https://puppetdb.example.com:8081'
#   include openvox_webui
#
class openvox_webui (
  Enum['present', 'absent', 'latest'] $ensure         = 'present',
  String[1]                           $package_name   = 'openvox-webui',
  String[1]                           $service_name   = 'openvox-webui',
  Enum['running', 'stopped']          $service_ensure = 'running',
  Boolean                             $service_enable = true,
  Stdlib::Absolutepath                $config_dir     = '/etc/openvox-webui',
  Stdlib::Absolutepath                $data_dir       = '/var/lib/openvox-webui',
  String[1]                           $user           = 'openvox-webui',
  String[1]                           $group          = 'openvox-webui',

  # Server settings
  Stdlib::Host                        $listen_address = '127.0.0.1',
  Stdlib::Port                        $listen_port    = 3000,
  Stdlib::Absolutepath                $database_path  = '/var/lib/openvox-webui/openvox.db',
  Enum['trace', 'debug', 'info', 'warn', 'error'] $log_level = 'info',

  # PuppetDB settings
  Optional[Stdlib::HTTPUrl]           $puppetdb_url      = undef,
  Optional[Stdlib::Absolutepath]      $puppetdb_ssl_cert = undef,
  Optional[Stdlib::Absolutepath]      $puppetdb_ssl_key  = undef,
  Optional[Stdlib::Absolutepath]      $puppetdb_ssl_ca   = undef,
  Integer[1, 300]                     $puppetdb_timeout  = 30,

  # Authentication settings
  String[32]                          $jwt_secret       = fqdn_rand_string(64),
  String[1]                           $jwt_expiry       = '24h',
  Integer[60, 86400]                  $session_timeout  = 3600,

  # Initial admin account
  String[1]                           $admin_username   = 'admin',
  Optional[Sensitive[String[8]]]      $admin_password   = undef,
  Optional[String[1]]                 $admin_email      = undef,

  # Management options
  Boolean                             $manage_package   = true,
  Boolean                             $manage_service   = true,
  Boolean                             $manage_config    = true,
) {
  contain openvox_webui::install
  contain openvox_webui::config
  contain openvox_webui::service

  Class['openvox_webui::install']
  -> Class['openvox_webui::config']
  ~> Class['openvox_webui::service']
}
