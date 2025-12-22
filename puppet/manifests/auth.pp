# @summary Manages authentication configuration for Puppet Server and PuppetDB
#
# This class configures the auth.conf files for Puppet Server and PuppetDB
# to allow the OpenVox WebUI client certificate to access the required APIs.
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

  # Manage Puppet Server auth.conf
  if $openvox_webui::manage_puppetserver_auth {
    # Create a drop-in configuration file for Puppet Server
    # This is the recommended approach for Puppet Server 6+
    $puppetserver_confdir = $openvox_webui::puppetserver_confdir

    file { "${puppetserver_confdir}/openvox-webui-auth.conf":
      ensure  => file,
      owner   => 'puppet',
      group   => 'puppet',
      mode    => '0640',
      content => epp('openvox_webui/puppetserver-auth.conf.epp', {
          client_certname => $client_certname,
      }),
      notify  => Service[$openvox_webui::puppetserver_service],
    }
  }

  # Manage PuppetDB auth.conf
  if $openvox_webui::manage_puppetdb_auth {
    # Create a drop-in configuration file for PuppetDB
    $puppetdb_confdir = $openvox_webui::puppetdb_confdir

    file { "${puppetdb_confdir}/openvox-webui-auth.conf":
      ensure  => file,
      owner   => 'puppetdb',
      group   => 'puppetdb',
      mode    => '0640',
      content => epp('openvox_webui/puppetdb-auth.conf.epp', {
          client_certname => $client_certname,
      }),
      notify  => Service[$openvox_webui::puppetdb_service],
    }
  }
}
