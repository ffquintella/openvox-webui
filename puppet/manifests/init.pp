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
# @param tls_ciphers
#   List of TLS cipher suites to allow. If empty, uses secure defaults.
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
# @param puppet_ca_url
#   URL of the Puppet CA server to connect to (e.g., 'https://puppet:8140').
#
# @param puppet_ca_ssl_cert
#   Path to SSL certificate for Puppet CA connection.
#
# @param puppet_ca_ssl_key
#   Path to SSL private key for Puppet CA connection.
#
# @param puppet_ca_ssl_ca
#   Path to SSL CA certificate for Puppet CA connection.
#
# @param puppet_ca_timeout
#   Timeout in seconds for Puppet CA requests.
#
# @param puppet_ca_auto_discover
#   Whether to auto-discover Puppet CA connection from local Puppet agent.
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
# @param manage_puppetserver_auth
#   Whether to manage Puppet Server auth.conf for OpenVox WebUI access.
#   Creates a drop-in config file granting CA API access to the client certificate.
#
# @param manage_puppetdb_auth
#   Whether to manage PuppetDB auth.conf for OpenVox WebUI access.
#   Creates a drop-in config file granting query/command API access to the client certificate.
#
# @param auth_client_certname
#   The certificate CN to authorize in auth.conf files. If not specified,
#   auto-discovers from puppet_settings fact or uses the node's FQDN.
#
# @param puppetserver_confdir
#   Directory where Puppet Server configuration files are stored.
#
# @param puppetdb_confdir
#   Directory where PuppetDB configuration files are stored.
#
# @param puppetserver_service
#   Name of the Puppet Server service (for notifications on auth.conf changes).
#
# @param puppetdb_service
#   Name of the PuppetDB service (for notifications on auth.conf changes).
#
# @param manage_puppetserver_ca_conf
#   Whether to manage Puppet Server ca.conf to enable the certificate_status endpoint.
#   This is required for OpenVox WebUI to manage certificates via the CA API.
#   The certificate_status endpoint is disabled by default in Puppet Server.
#
# @param ca_allow_subject_alt_names
#   Whether to allow Subject Alternative Names in certificate requests.
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
# @example Managing auth.conf for Puppet Server and PuppetDB
#   class { 'openvox_webui':
#     manage_puppetserver_auth => true,
#     manage_puppetdb_auth     => true,
#     auth_client_certname     => 'webui.example.com',
#   }
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
  Stdlib::Host                        $listen_address     = '127.0.0.1',
  Stdlib::Port                        $listen_port        = 5051,
  Stdlib::Absolutepath                $database_path      = '/var/lib/openvox-webui/openvox.db',
  Enum['trace', 'debug', 'info', 'warn', 'error'] $log_level = 'info',
  Stdlib::Absolutepath                $log_dir            = '/var/log/openvox/webui',
  Boolean                             $serve_frontend     = true,
  Stdlib::Absolutepath                $static_dir         = '/usr/share/openvox-webui/static',

  # TLS settings
  Boolean                             $enable_tls         = false,
  Optional[Stdlib::Absolutepath]      $tls_cert_file      = undef,
  Optional[Stdlib::Absolutepath]      $tls_key_file       = undef,
  Enum['1.2', '1.3']                  $tls_min_version    = '1.3',
  Array[String[1]]                    $tls_ciphers        = [],

  # PuppetDB settings
  Optional[Stdlib::HTTPUrl]           $puppetdb_url           = undef,
  Optional[Stdlib::Absolutepath]      $puppetdb_ssl_cert      = undef,
  Optional[Stdlib::Absolutepath]      $puppetdb_ssl_key       = undef,
  Optional[Stdlib::Absolutepath]      $puppetdb_ssl_ca        = undef,
  Integer[1, 300]                     $puppetdb_timeout       = 30,
  Boolean                             $puppetdb_auto_discover = true,

  # Puppet CA settings
  Optional[Stdlib::HTTPUrl]           $puppet_ca_url          = undef,
  Optional[Stdlib::Absolutepath]      $puppet_ca_ssl_cert     = undef,
  Optional[Stdlib::Absolutepath]      $puppet_ca_ssl_key      = undef,
  Optional[Stdlib::Absolutepath]      $puppet_ca_ssl_ca       = undef,
  Integer[1, 300]                     $puppet_ca_timeout      = 30,
  Boolean                             $puppet_ca_auto_discover = true,

  # Authentication settings
  String[32]                          $jwt_secret             = fqdn_rand_string(64),
  String[1]                           $jwt_expiry             = '24h',
  Integer[60, 86400]                  $session_timeout        = 3600,
  Integer[1, 20]                      $max_login_attempts     = 5,
  Integer[60, 3600]                   $lockout_duration       = 900,

  # Initial admin account
  String[1]                           $admin_username         = 'admin',
  Optional[Sensitive[String[8]]]      $admin_password         = undef,
  Optional[String[1]]                 $admin_email            = undef,

  # Cache settings
  Integer[0]                          $cache_ttl              = 300,
  Integer[10]                         $cache_max_entries      = 1000,

  # Dashboard settings
  String[1]                           $dashboard_theme        = 'light',
  Integer[10, 1000]                   $dashboard_page_size    = 25,
  Integer[5, 300]                     $dashboard_refresh      = 30,

  # Classification settings
  Integer[1, 500]                     $max_rules_per_group    = 100,

  # Management options
  Boolean                             $manage_package         = true,
  Boolean                             $manage_service         = true,
  Boolean                             $manage_config          = true,
  Boolean                             $manage_firewall        = false,

  # Auth configuration management
  # These options manage auth.conf files for Puppet Server and PuppetDB
  Boolean                             $manage_puppetserver_auth   = false,
  Boolean                             $manage_puppetdb_auth       = false,
  Optional[String[1]]                 $auth_client_certname       = undef,
  Stdlib::Absolutepath                $puppetserver_confdir       = '/etc/puppetlabs/puppetserver/conf.d',
  Stdlib::Absolutepath                $puppetdb_confdir           = '/etc/puppetlabs/puppetdb/conf.d',
  String[1]                           $puppetserver_service       = 'puppetserver',
  String[1]                           $puppetdb_service           = 'puppetdb',

  # CA configuration management
  # Manages ca.conf to enable the certificate_status endpoint (disabled by default)
  Boolean                             $manage_puppetserver_ca_conf = false,
  Boolean                             $ca_allow_subject_alt_names  = true,
) {
  contain openvox_webui::install
  contain openvox_webui::config
  contain openvox_webui::service

  # Optionally manage auth.conf and ca.conf files
  if $manage_puppetserver_auth or $manage_puppetdb_auth or $manage_puppetserver_ca_conf {
    contain openvox_webui::auth

    Class['openvox_webui::config']
    -> Class['openvox_webui::auth']
  }

  Class['openvox_webui::install']
  -> Class['openvox_webui::config']
  ~> Class['openvox_webui::service']
}
