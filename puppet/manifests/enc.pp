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
# @param enable_watchdog
#   Deploy a systemd-timer driven watchdog (every 5 minutes) that detects
#   a broken or unrunnable ENC and self-heals: restores the script from a
#   sibling `.template` file managed by this class, and/or restarts
#   puppetserver if its JVM has wedged on `Cannot run program ... error=2`.
#   Default: true. Disable only if you have external monitoring covering
#   the same failure modes.
#
# @param puppetserver_service_name
#   Name of the puppetserver systemd unit the watchdog will restart.
#   Default: 'puppetserver'.
#
# @param puppet_user
#   Local user that owns the puppetserver process. The watchdog probes the
#   ENC by running it as this user (matches puppetserver's runtime context).
#   Default: 'puppet'.
#
# @param watchdog_allow_restart
#   When the watchdog detects puppetserver's JVM cannot exec the ENC, may it
#   `systemctl restart puppetserver`? Set to false in environments where the
#   restart must be performed by an operator. Default: true.
#
# @param watchdog_journal_lookback_min
#   How many minutes of puppetserver journal the watchdog scans for the
#   "Cannot run program" exec failure. Default: 10 (twice the timer cadence).
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
  Optional[Stdlib::HTTPUrl] $webui_url                       = undef,
  Stdlib::Absolutepath      $enc_script_path                 = '/opt/openvox/enc.sh',
  Boolean                   $manage_puppet_conf              = true,
  Boolean                   $manage_dependencies             = true,
  Boolean                   $restart_puppetserver            = true,
  Boolean                   $ssl_verify                      = false,
  Stdlib::Absolutepath      $puppet_conf_path                = '/etc/puppetlabs/puppet/puppet.conf',
  Boolean                   $remove_agent_environment        = true,
  Optional[String]          $classification_key              = undef,
  # Self-healing watchdog. Periodically tests the ENC end-to-end and
  # restores it from an on-disk template / restarts puppetserver if its
  # JVM has wedged. See templates/enc-watchdog.sh.epp for details.
  Boolean                   $enable_watchdog                 = true,
  String                    $puppetserver_service_name       = 'puppetserver',
  String                    $puppet_user                     = 'puppet',
  Boolean                   $watchdog_allow_restart          = true,
  Integer[1, 1440]          $watchdog_journal_lookback_min   = 10,
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

  # Render the ENC script content once so both the live script and its
  # sibling .template file (used by the watchdog for offline recovery) stay
  # byte-identical even if the EPP changes.
  $enc_script_content = epp('openvox_webui/enc.sh.epp', {
      webui_url          => $effective_webui_url,
      ssl_verify         => $ssl_verify,
      classification_key => $classification_key,
  })

  # Create ENC script
  file { $enc_script_path:
    ensure  => file,
    owner   => 'root',
    group   => 'root',
    mode    => '0755',
    content => $enc_script_content,
    require => File[$enc_dir],
  }

  # Sibling template that the self-healing watchdog uses to restore the ENC
  # script if it ever goes missing or is replaced with broken content.
  # Kept byte-identical to the live script via the shared variable above.
  $enc_template_path = "${enc_script_path}.template"
  file { $enc_template_path:
    ensure  => file,
    owner   => 'root',
    group   => 'root',
    mode    => '0644',
    content => $enc_script_content,
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

  # Ensure openvox-webui group exists and contains puppet user
  group { 'openvox-webui':
    ensure => present,
  }

  user { 'puppet':
    groups  => ['openvox-webui'],
    require => Group['openvox-webui'],
  }

  # Ensure openvox-webui user is in the puppet group
  # This allows openvox-webui to access the r10k cache directory
  # (/opt/puppetlabs/puppet/cache/r10k) which is under puppet:puppet ownership
  user { 'openvox-webui':
    groups  => ['puppet'],
    require => Group['openvox-webui'],
  }

  # Ensure Puppet code environments directory exists
  # This prevents Puppet Server from failing to start if environments don't exist
  file { '/etc/puppetlabs/code/environments':
    ensure  => directory,
    owner   => 'puppet',
    group   => 'openvox-webui',
    mode    => '0775',
    require => [Group['openvox-webui'], User['puppet']],
  }

  # Ensure production environment exists (minimum requirement)
  file { '/etc/puppetlabs/code/environments/production':
    ensure  => directory,
    owner   => 'puppet',
    group   => 'openvox-webui',
    mode    => '0775',
    require => File['/etc/puppetlabs/code/environments'],
  }

  # Create production manifests directory
  file { '/etc/puppetlabs/code/environments/production/manifests':
    ensure  => directory,
    owner   => 'puppet',
    group   => 'openvox-webui',
    mode    => '0775',
    require => File['/etc/puppetlabs/code/environments/production'],
  }

  # Create production modules directory
  file { '/etc/puppetlabs/code/environments/production/modules':
    ensure  => directory,
    owner   => 'puppet',
    group   => 'openvox-webui',
    mode    => '0775',
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

  # ---------------------------------------------------------------------------
  # Self-healing watchdog
  #
  # Once the ENC is broken, no node (including this puppetserver) can compile
  # a catalog, so Puppet itself can't repair the ENC — a chicken-and-egg
  # deadlock seen in production. The watchdog runs out-of-band via a systemd
  # timer (independent of puppet agent runs) and recovers from two failure
  # modes: corrupted/missing script (restored from `${enc_script_path}.template`)
  # and JVM exec wedge (restart of `${puppetserver_service_name}`).
  # ---------------------------------------------------------------------------
  if $enable_watchdog {
    $watchdog_script_path = "${enc_dir}/enc-watchdog.sh"

    file { $watchdog_script_path:
      ensure  => file,
      owner   => 'root',
      group   => 'root',
      mode    => '0755',
      content => epp('openvox_webui/enc-watchdog.sh.epp', {
          enc_script_path             => $enc_script_path,
          enc_template_path           => $enc_template_path,
          puppet_user                 => $puppet_user,
          puppetserver_service        => $puppetserver_service_name,
          allow_restart_puppetserver  => $watchdog_allow_restart,
          journal_lookback_minutes    => $watchdog_journal_lookback_min,
      }),
      require => [File[$enc_dir], File[$enc_script_path], File[$enc_template_path]],
    }

    file { '/etc/systemd/system/openvox-enc-watchdog.service':
      ensure => file,
      owner  => 'root',
      group  => 'root',
      mode   => '0644',
      source => 'puppet:///modules/openvox_webui/openvox-enc-watchdog.service',
      notify => Exec['openvox_enc_watchdog_daemon_reload'],
    }

    file { '/etc/systemd/system/openvox-enc-watchdog.timer':
      ensure => file,
      owner  => 'root',
      group  => 'root',
      mode   => '0644',
      source => 'puppet:///modules/openvox_webui/openvox-enc-watchdog.timer',
      notify => Exec['openvox_enc_watchdog_daemon_reload'],
    }

    exec { 'openvox_enc_watchdog_daemon_reload':
      command     => '/bin/systemctl daemon-reload',
      refreshonly => true,
    }

    service { 'openvox-enc-watchdog.timer':
      ensure  => running,
      enable  => true,
      require => [
        File[$watchdog_script_path],
        File['/etc/systemd/system/openvox-enc-watchdog.service'],
        File['/etc/systemd/system/openvox-enc-watchdog.timer'],
        Exec['openvox_enc_watchdog_daemon_reload'],
      ],
    }
  } else {
    # Operator opted out — make sure any previously installed timer is gone.
    service { 'openvox-enc-watchdog.timer':
      ensure => stopped,
      enable => false,
    }

    file { ['/etc/systemd/system/openvox-enc-watchdog.timer',
            '/etc/systemd/system/openvox-enc-watchdog.service',
            "${enc_dir}/enc-watchdog.sh"]:
      ensure  => absent,
      require => Service['openvox-enc-watchdog.timer'],
    }
  }
}
