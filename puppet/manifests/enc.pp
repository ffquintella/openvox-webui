# @summary Configures Puppet Server to use OpenVox WebUI as External Node Classifier (ENC)
#
# This class configures the Puppet Server to use OpenVox WebUI as its External Node
# Classifier. It creates the ENC script, installs dependencies, configures puppet.conf,
# and manages the Puppet Server service.
#
# The ENC script queries OpenVox WebUI's classification API and returns node
# classification in YAML format as expected by Puppet.
#
# @param webui_url
#   Base URL of the OpenVox WebUI API (e.g., 'https://puppet.example.com')
#   Auto-detected from the server if not specified.
#
# @param enc_script_path
#   Path where the ENC script will be created.
#   Default: '/opt/openvox/enc.sh'
#
# @param manage_puppet_conf
#   Whether to manage the Puppet Server's puppet.conf file.
#   When true, adds node_terminus and external_nodes settings.
#
# @param manage_dependencies
#   Whether to install required dependencies (python3-pyyaml).
#
# @param restart_puppetserver
#   Whether to restart Puppet Server after configuration changes.
#
# @param ssl_verify
#   Whether the ENC script should verify SSL certificates when connecting to OpenVox WebUI.
#   Set to false if using self-signed certificates.
#
# @param puppet_conf_path
#   Path to the Puppet Server's puppet.conf file.
#
# @param remove_agent_environment
#   Whether to remove environment settings from puppet.conf [agent] section.
#   This prevents conflicts between agent-side environment settings and ENC classification.
#
# @param classification_key
#   Shared key for authenticating to the classification API.
#   Must match the key configured in OpenVox WebUI (CLASSIFICATION_SHARED_KEY).
#   Required when the /classify endpoint requires authentication.
#
# @example Basic usage with auto-detected URL
#   include openvox_webui::enc
#
# @example With custom OpenVox WebUI URL
#   class { 'openvox_webui::enc':
#     webui_url => 'https://segdc1vpr0018.fgv.br',
#   }
#
# @example Full configuration
#   class { 'openvox_webui::enc':
#     webui_url                => 'https://segdc1vpr0018.fgv.br',
#     manage_puppet_conf       => true,
#     restart_puppetserver     => true,
#     ssl_verify               => false,
#     remove_agent_environment => true,
#   }
#
# @example With Hiera
#   # hiera data:
#   openvox_webui::enc::webui_url: 'https://segdc1vpr0018.fgv.br'
#   openvox_webui::enc::manage_puppet_conf: true
#   openvox_webui::enc::ssl_verify: false
#
# @example With shared key authentication
#   class { 'openvox_webui::enc':
#     webui_url          => 'https://openvox.example.com',
#     classification_key => 'my-secret-shared-key',
#   }
#
class openvox_webui::enc (
  Optional[Stdlib::HTTPUrl] $webui_url                = undef,
  Stdlib::Absolutepath      $enc_script_path          = '/opt/openvox/enc.sh',
  Boolean                   $manage_puppet_conf       = true,
  Boolean                   $manage_dependencies      = true,
  Boolean                   $restart_puppetserver     = true,
  Boolean                   $ssl_verify               = false,
  Stdlib::Absolutepath      $puppet_conf_path         = '/etc/puppetlabs/puppet/puppet.conf',
  Boolean                   $remove_agent_environment = true,
  Optional[String]          $classification_key       = undef,
) {
  # Validate we're on a Puppet Server (check for service existence)
  # Only validate if we're managing the service
  if $restart_puppetserver {
    unless $facts['service_provider'] == 'systemd' {
      notify { 'openvox_enc_service_warning':
        message  => 'This system does not use systemd. Puppet Server service management will be skipped.',
        loglevel => 'warning',
      }
    }
  }

  # Determine OpenVox WebUI URL
  # Try to detect from local process if not specified
  $effective_webui_url = $webui_url ? {
    undef   => $facts['openvox_webui_url'] ? {
      undef   => 'https://localhost',
      default => $facts['openvox_webui_url'],
    },
    default => $webui_url,
  }

  # Install dependencies
  if $manage_dependencies {
    case $facts['os']['family'] {
      'RedHat': {
        stdlib::ensure_packages(['python3-pyyaml'])
      }
      'Debian': {
        stdlib::ensure_packages(['python3-yaml'])
      }
      default: {
        notify { 'openvox_enc_dependency_warning':
          message  => 'Unable to automatically install python3-pyyaml for this OS. Please install manually.',
          loglevel => 'warning',
        }
      }
    }
  }

  # Ensure ENC script directory exists
  $enc_dir = dirname($enc_script_path)
  file { $enc_dir:
    ensure => directory,
    owner  => 'root',
    group  => 'root',
    mode   => '0755',
  }

  # Create ENC script
  file { $enc_script_path:
    ensure  => file,
    owner   => 'root',
    group   => 'root',
    mode    => '0755',
    content => epp('openvox_webui/enc.sh.epp', {
        webui_url          => $effective_webui_url,
        ssl_verify         => $ssl_verify,
        classification_key => $classification_key,
    }),
    require => File[$enc_dir],
  }

  # Configure Puppet Server puppet.conf
  if $manage_puppet_conf {
    # Determine notify target
    $config_notify = $restart_puppetserver ? {
      true    => Service['puppetserver'],
      default => undef,
    }

    # Use puppet config set to configure ENC
    exec { 'puppet_config_node_terminus':
      command => '/opt/puppetlabs/bin/puppet config set node_terminus exec --section master',
      unless  => "/opt/puppetlabs/bin/puppet config print node_terminus --section master | grep -q '^exec$'",
      notify  => $config_notify,
    }

    exec { 'puppet_config_external_nodes':
      command => "/opt/puppetlabs/bin/puppet config set external_nodes '${enc_script_path}' --section master",
      unless  => "/opt/puppetlabs/bin/puppet config print external_nodes --section master | grep -q '^${enc_script_path}$'",
      require => Exec['puppet_config_node_terminus'],
      notify  => $config_notify,
    }

    # Optionally remove environment from [agent] section
    if $remove_agent_environment {
      exec { 'puppet_config_remove_agent_environment':
        command => '/opt/puppetlabs/bin/puppet config delete environment --section agent',
        # Only run if environment is explicitly set in [agent] section of puppet.conf
        onlyif  => "/bin/grep -q '^[[:space:]]*environment[[:space:]]*=' ${puppet_conf_path}",
        notify  => $config_notify,
      }
    }
  }

  # Ensure Puppet code environments directory exists
  # This prevents Puppet Server from failing to start if environments don't exist
  file { '/etc/puppetlabs/code/environments':
    ensure => directory,
    owner  => 'puppet',
    group  => 'puppet',
    mode   => '0755',
  }

  # Ensure production environment exists (minimum requirement)
  file { '/etc/puppetlabs/code/environments/production':
    ensure  => directory,
    owner   => 'puppet',
    group   => 'puppet',
    mode    => '0755',
    require => File['/etc/puppetlabs/code/environments'],
  }

  # Create production manifests directory
  file { '/etc/puppetlabs/code/environments/production/manifests':
    ensure  => directory,
    owner   => 'puppet',
    group   => 'puppet',
    mode    => '0755',
    require => File['/etc/puppetlabs/code/environments/production'],
  }

  # Create production modules directory
  file { '/etc/puppetlabs/code/environments/production/modules':
    ensure  => directory,
    owner   => 'puppet',
    group   => 'puppet',
    mode    => '0755',
    require => File['/etc/puppetlabs/code/environments/production'],
  }

  # Manage Puppet Server service (only if requested)
  if $restart_puppetserver {
    service { 'puppetserver':
      ensure    => running,
      enable    => true,
      subscribe => [
        File['/etc/puppetlabs/code/environments/production'],
        File['/etc/puppetlabs/code/environments/production/manifests'],
        File['/etc/puppetlabs/code/environments/production/modules'],
      ],
    }
  }

  # Ensure facter facts.d directory exists
  file { '/etc/facter':
    ensure => directory,
    owner  => 'root',
    group  => 'root',
    mode   => '0755',
  }

  file { '/etc/facter/facts.d':
    ensure  => directory,
    owner   => 'root',
    group   => 'root',
    mode    => '0755',
    require => File['/etc/facter'],
  }

  # Create a fact to indicate ENC is configured
  file { '/etc/facter/facts.d/openvox_enc.yaml':
    ensure  => file,
    owner   => 'root',
    group   => 'root',
    mode    => '0644',
    content => @("YAML"),
      ---
      openvox_enc_enabled: true
      openvox_enc_script: ${enc_script_path}
      openvox_enc_webui_url: ${effective_webui_url}
      | YAML
    require => File['/etc/facter/facts.d'],
  }

  # Validate ENC script after creation
  exec { 'validate_enc_script':
    command     => "${enc_script_path} ${facts['networking']['fqdn']}",
    refreshonly => true,
    subscribe   => File[$enc_script_path],
    logoutput   => true,
  }
}
