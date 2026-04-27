# @summary Manages authentication and CA configuration for Puppet Server and PuppetDB
#
# This class configures the auth.conf and ca.conf files for Puppet Server and PuppetDB
# to allow the OpenVox WebUI client certificate to access the required APIs.
#
# The ca.conf management enables the certificate_status endpoint which is disabled
# by default in Puppet Server. This is required for certificate management operations.
#
# @api private
#
class openvox_webui::auth {
  assert_private()

  # Determine the client certificate CN to authorize
  # Priority: explicit parameter > auto-discovered from puppet_settings > hostname
  if $openvox_webui::auth_client_certname {
    $client_certname = $openvox_webui::auth_client_certname
  } elsif $openvox_webui::puppet_ca_auto_discover {
    $puppet_settings = $facts['puppet_settings']
    if $puppet_settings and $puppet_settings['certname'] {
      $client_certname = $puppet_settings['certname']
    } else {
      $client_certname = $facts['networking']['fqdn']
    }
  } else {
    $client_certname = $facts['networking']['fqdn']
  }

  # ---------------------------------------------------------------------------
  # Reload-on-change behavior
  #
  # Earlier versions of this class refreshed puppetserver / puppetdb via a
  # `notify => Service[$name]` (and later, a virtual-resource collector
  # chain `~> Service <| title == $name |>`). Both broke in the field:
  #
  #   * Plain `notify => Service[...]` fails compilation when the host
  #     including this class doesn't itself manage the service.
  #   * The virtual collector matches any Service in the catalog with the
  #     same title — including roles that wrap our class and chain
  #     `Service[$puppetserver_service] -> Class[::openvox_webui::config]`
  #     to enforce "puppetserver up before WebUI configures itself". That
  #     external edge plus our chain creates the cycle:
  #         Class[Config] -> Class[Auth] -> File[auth.conf]
  #             -> Service[$svc] -> Class[Config]
  #
  # Use a refresh-only Exec that calls systemctl directly. Exec is a leaf
  # resource the wrapping role doesn't reference, so it can't participate
  # in the role's Service-level chain. The exec is gated by `onlyif` so it
  # silently no-ops on hosts where the service unit doesn't exist (the
  # original "auth.pp landed on a non-puppetserver" case).
  # ---------------------------------------------------------------------------

  # Manage Puppet Server auth.conf
  if $openvox_webui::manage_puppetserver_auth {
    # Reference the parent class's parameter directly. Aliasing it to a
    # bare local (`$puppetserver_confdir = ...`) would shadow the parent's
    # already-visible `$openvox_webui::puppetserver_confdir`, which Puppet
    # rejects as a reassignment.
    $_puppetserver_auth_conf = "${openvox_webui::puppetserver_confdir}/openvox-webui-auth.conf"

    file { $_puppetserver_auth_conf:
      ensure  => file,
      owner   => 'puppet',
      group   => 'puppet',
      mode    => '0640',
      content => epp('openvox_webui/puppetserver-auth.conf.epp', {
          client_certname => $client_certname,
      }),
    }

    exec { "openvox_webui-reload-${openvox_webui::puppetserver_service}":
      command     => "/usr/bin/systemctl reload-or-restart ${openvox_webui::puppetserver_service}",
      refreshonly => true,
      onlyif      => "/usr/bin/systemctl list-unit-files ${openvox_webui::puppetserver_service}.service --no-legend 2>/dev/null | /usr/bin/grep -q ${openvox_webui::puppetserver_service}",
      path        => '/usr/bin:/bin',
      subscribe   => File[$_puppetserver_auth_conf],
    }
  }

  # Manage PuppetDB auth.conf
  if $openvox_webui::manage_puppetdb_auth {
    $_puppetdb_auth_conf = "${openvox_webui::puppetdb_confdir}/openvox-webui-auth.conf"

    file { $_puppetdb_auth_conf:
      ensure  => file,
      owner   => 'puppetdb',
      group   => 'puppetdb',
      mode    => '0640',
      content => epp('openvox_webui/puppetdb-auth.conf.epp', {
          client_certname => $client_certname,
      }),
    }

    exec { "openvox_webui-reload-${openvox_webui::puppetdb_service}":
      command     => "/usr/bin/systemctl reload-or-restart ${openvox_webui::puppetdb_service}",
      refreshonly => true,
      onlyif      => "/usr/bin/systemctl list-unit-files ${openvox_webui::puppetdb_service}.service --no-legend 2>/dev/null | /usr/bin/grep -q ${openvox_webui::puppetdb_service}",
      path        => '/usr/bin:/bin',
      subscribe   => File[$_puppetdb_auth_conf],
    }
  }

  # Manage Puppet Server ca.conf to enable certificate_status endpoint
  # This endpoint is disabled by default in Puppet Server
  if $openvox_webui::manage_puppetserver_ca_conf {
    $_ca_conf = "${openvox_webui::puppetserver_confdir}/ca.conf"

    file { $_ca_conf:
      ensure  => file,
      owner   => 'puppet',
      group   => 'puppet',
      mode    => '0640',
      content => epp('openvox_webui/ca.conf.epp', {
          client_certname         => $client_certname,
          allow_subject_alt_names => $openvox_webui::ca_allow_subject_alt_names,
      }),
    }

    # Reuse the same exec name when both manage_puppetserver_auth and
    # manage_puppetserver_ca_conf are true, so a single restart picks up
    # both file changes. `ensure_resource` makes the duplicate declaration
    # a no-op when the auth path already declared it.
    ensure_resource('exec', "openvox_webui-reload-${openvox_webui::puppetserver_service}", {
        command     => "/usr/bin/systemctl reload-or-restart ${openvox_webui::puppetserver_service}",
        refreshonly => true,
        onlyif      => "/usr/bin/systemctl list-unit-files ${openvox_webui::puppetserver_service}.service --no-legend 2>/dev/null | /usr/bin/grep -q ${openvox_webui::puppetserver_service}",
        path        => '/usr/bin:/bin',
    })
    File[$_ca_conf]
    ~> Exec["openvox_webui-reload-${openvox_webui::puppetserver_service}"]
  }
}
