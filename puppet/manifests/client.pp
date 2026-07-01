# @summary Configures Puppet agents to fetch classification facts from OpenVox WebUI
#
# This class configures Puppet agents to automatically fetch their classification
# data from OpenVox WebUI and make it available as custom facts. The classification
# includes groups, classes, variables, and environment assignment.
#
# The client uses the node's Puppet certificate for authentication, ensuring
# that each node can only fetch its own classification data.
#
# @param api_url
#   URL of the OpenVox WebUI API (e.g., 'https://openvox.example.com:5051')
#
# @param api_token
#   API token for authentication. Either api_token or api_key is required.
#   Ignored when using certificate authentication.
#
# @param api_key
#   API key for authentication. Either api_token or api_key is required.
#   Ignored when using certificate authentication.
#
# @param use_puppet_certs
#   Whether to use Puppet certificates for authentication (mTLS).
#   When enabled, the node's Puppet certificate is used to authenticate
#   with the OpenVox WebUI API. This is the recommended approach.
#
# @param ssl_ca
#   Path to CA certificate for SSL verification.
#   Defaults to Puppet's CA certificate when use_puppet_certs is true.
#
# @param ssl_cert
#   Path to client certificate for mTLS.
#   Defaults to node's Puppet certificate when use_puppet_certs is true.
#
# @param ssl_key
#   Path to client private key for mTLS.
#   Defaults to node's Puppet private key when use_puppet_certs is true.
#
# @param ssl_verify
#   Whether to verify the server's SSL certificate.
#
# @param timeout
#   Request timeout in seconds.
#
# @param config_dir
#   Directory where the client configuration file is stored. When left undef,
#   defaults to '/etc/puppetlabs/facter' on *nix and
#   'C:/ProgramData/PuppetLabs/facter' on Windows.
#
# @param template_name
#   Name of the facter template to use for fact generation.
#
# @param manage_config
#   Whether to manage the configuration file.
#
# @param classification_key
#   Shared key for authenticating to the classification API (/classify endpoint).
#   This is an alternative to mTLS client certificate authentication.
#   Must match the key configured on the OpenVox WebUI server.
#
# @param inventory_enabled
#   Whether to collect and submit application/package inventory to OpenVox WebUI.
#
# @param inventory_submit
#   Whether collected inventory should be posted back to the WebUI API.
#
# @param inventory_max_items
#   Maximum number of records collected per inventory category to avoid oversized payloads.
#
# @example Basic usage with Puppet certificates (recommended)
#   class { 'openvox_webui::client':
#     api_url          => 'https://openvox.example.com:5051',
#     use_puppet_certs => true,
#   }
#
# @example Using API token authentication
#   class { 'openvox_webui::client':
#     api_url   => 'https://openvox.example.com:5051',
#     api_token => Sensitive('your-api-token-here'),
#   }
#
# @example Custom configuration with Hiera
#   # hiera data:
#   openvox_webui::client::api_url: 'https://openvox.example.com:5051'
#   openvox_webui::client::use_puppet_certs: true
#   openvox_webui::client::template_name: 'classification'
#
# @example Using shared key authentication (alternative to mTLS)
#   class { 'openvox_webui::client':
#     api_url            => 'https://openvox.example.com:5051',
#     use_puppet_certs   => false,
#     classification_key => 'my-secret-shared-key',
#   }
#
class openvox_webui::client (
  Stdlib::HTTPUrl                     $api_url,
  Optional[Sensitive[String[1]]]      $api_token        = undef,
  Optional[Sensitive[String[1]]]      $api_key          = undef,
  Boolean                             $use_puppet_certs = true,
  Optional[Stdlib::Absolutepath]      $ssl_ca           = undef,
  Optional[Stdlib::Absolutepath]      $ssl_cert         = undef,
  Optional[Stdlib::Absolutepath]      $ssl_key          = undef,
  Boolean                             $ssl_verify       = true,
  Integer[1, 120]                     $timeout          = 30,
  Optional[Stdlib::Absolutepath]      $config_dir         = undef,
  String[1]                           $template_name      = 'classification',
  Boolean                             $manage_config      = true,
  Optional[String[1]]                 $classification_key = undef,
  Boolean                             $inventory_enabled  = false,
  Boolean                             $inventory_submit   = true,
  Integer[10, 50000]                  $inventory_max_items = 10000,
) {
  # Validate that we have some form of authentication
  if !$use_puppet_certs and !$api_token and !$api_key and !$classification_key {
    fail('openvox_webui::client requires either use_puppet_certs, api_token, api_key, or classification_key')
  }

  $is_windows = $facts['os']['family'] == 'windows'

  # Resolve the config directory default per platform. An explicit config_dir
  # always wins; otherwise pick the platform-appropriate location.
  if $config_dir {
    $effective_config_dir = $config_dir
  } elsif $is_windows {
    $effective_config_dir = 'C:/ProgramData/PuppetLabs/facter'
  } else {
    $effective_config_dir = '/etc/puppetlabs/facter'
  }

  # Determine SSL paths
  if $use_puppet_certs {
    # Use Puppet's SSL directory
    if $is_windows {
      $default_ssldir = 'C:/ProgramData/PuppetLabs/puppet/etc/ssl'
    } else {
      $default_ssldir = '/etc/puppetlabs/puppet/ssl'
    }
    if $facts['puppet_settings'] {
      $puppet_ssldir = $facts['puppet_settings']['main']['ssldir']
    } else {
      $puppet_ssldir = $default_ssldir
    }
    if $facts['clientcert'] {
      $certname = $facts['clientcert']
    } else {
      $certname = $facts['networking']['fqdn']
    }

    if $ssl_ca {
      $effective_ssl_ca = $ssl_ca
    } else {
      $effective_ssl_ca = "${puppet_ssldir}/certs/ca.pem"
    }
    if $ssl_cert {
      $effective_ssl_cert = $ssl_cert
    } else {
      $effective_ssl_cert = "${puppet_ssldir}/certs/${certname}.pem"
    }
    if $ssl_key {
      $effective_ssl_key = $ssl_key
    } else {
      $effective_ssl_key = "${puppet_ssldir}/private_keys/${certname}.pem"
    }
  } else {
    $effective_ssl_ca = $ssl_ca
    $effective_ssl_cert = $ssl_cert
    $effective_ssl_key = $ssl_key
  }

  # Determine ownership/permissions per platform. Windows has no 'root' user
  # or POSIX modes, so those attributes are left unmanaged there.
  if $is_windows {
    $dir_owner  = undef
    $dir_group  = undef
    $dir_mode   = undef
    $file_mode  = undef
  } elsif $facts['os']['family'] in ['Darwin', 'FreeBSD', 'OpenBSD'] {
    # wheel on macOS/BSD, root on Linux
    $dir_owner = 'root'
    $dir_group = 'wheel'
    $dir_mode  = '0755'
    $file_mode = '0640'
  } else {
    $dir_owner = 'root'
    $dir_group = 'root'
    $dir_mode  = '0755'
    $file_mode = '0640'
  }

  # Ensure config directory exists
  file { $effective_config_dir:
    ensure => directory,
    owner  => $dir_owner,
    group  => $dir_group,
    mode   => $dir_mode,
  }

  # Create client configuration file
  if $manage_config {
    file { "${effective_config_dir}/openvox-client.yaml":
      ensure  => file,
      owner   => $dir_owner,
      group   => $dir_group,
      mode    => $file_mode,
      content => epp('openvox_webui/client.yaml.epp', {
          api_url            => $api_url,
          api_token          => $api_token,
          api_key            => $api_key,
          ssl_ca             => $effective_ssl_ca,
          ssl_cert           => $effective_ssl_cert,
          ssl_key            => $effective_ssl_key,
          ssl_verify         => $ssl_verify,
          timeout            => $timeout,
          template           => $template_name,
          classification_key => $classification_key,
          inventory_enabled  => $inventory_enabled,
          inventory_submit   => $inventory_submit,
          inventory_max_items => $inventory_max_items,
      }),
      require => File[$effective_config_dir],
    }
  }
}
