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
#   Directory where the client configuration file is stored.
#
# @param template_name
#   Name of the facter template to use for fact generation.
#
# @param manage_config
#   Whether to manage the configuration file.
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
  Stdlib::Absolutepath                $config_dir       = '/etc/puppetlabs/facter',
  String[1]                           $template_name    = 'classification',
  Boolean                             $manage_config    = true,
) {
  # Validate that we have some form of authentication
  if !$use_puppet_certs and !$api_token and !$api_key {
    fail('openvox_webui::client requires either use_puppet_certs, api_token, or api_key')
  }

  # Determine SSL paths
  if $use_puppet_certs {
    # Use Puppet's SSL directory
    $puppet_ssldir = $facts['puppet_settings'] ? {
      undef   => '/etc/puppetlabs/puppet/ssl',
      default => $facts['puppet_settings']['main']['ssldir'],
    }
    $certname = $facts['clientcert'] ? {
      undef   => $facts['networking']['fqdn'],
      default => $facts['clientcert'],
    }

    $effective_ssl_ca = pick($ssl_ca, "${puppet_ssldir}/certs/ca.pem")
    $effective_ssl_cert = pick($ssl_cert, "${puppet_ssldir}/certs/${certname}.pem")
    $effective_ssl_key = pick($ssl_key, "${puppet_ssldir}/private_keys/${certname}.pem")
  } else {
    $effective_ssl_ca = $ssl_ca
    $effective_ssl_cert = $ssl_cert
    $effective_ssl_key = $ssl_key
  }

  # Determine the appropriate root group (wheel on macOS/BSD, root on Linux)
  $root_group = $facts['os']['family'] ? {
    'Darwin' => 'wheel',
    'FreeBSD' => 'wheel',
    'OpenBSD' => 'wheel',
    default  => 'root',
  }

  # Ensure config directory exists
  file { $config_dir:
    ensure => directory,
    owner  => 'root',
    group  => $root_group,
    mode   => '0755',
  }

  # Create client configuration file
  if $manage_config {
    file { "${config_dir}/openvox-client.yaml":
      ensure  => file,
      owner   => 'root',
      group   => $root_group,
      mode    => '0640',
      content => epp('openvox_webui/client.yaml.epp', {
          api_url    => $api_url,
          api_token  => $api_token,
          api_key    => $api_key,
          ssl_ca     => $effective_ssl_ca,
          ssl_cert   => $effective_ssl_cert,
          ssl_key    => $effective_ssl_key,
          ssl_verify => $ssl_verify,
          timeout    => $timeout,
          template   => $template_name,
      }),
      require => File[$config_dir],
    }
  }
}
