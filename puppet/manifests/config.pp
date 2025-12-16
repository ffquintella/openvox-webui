# @summary Configures OpenVox WebUI
#
# @api private
#
class openvox_webui::config {
  assert_private()

  if $openvox_webui::manage_config {
    file { $openvox_webui::config_dir:
      ensure => directory,
      owner  => 'root',
      group  => $openvox_webui::group,
      mode   => '0750',
    }

    file { $openvox_webui::data_dir:
      ensure => directory,
      owner  => $openvox_webui::user,
      group  => $openvox_webui::group,
      mode   => '0750',
    }

    file { "${openvox_webui::config_dir}/config.yaml":
      ensure  => file,
      owner   => 'root',
      group   => $openvox_webui::group,
      mode    => '0640',
      content => epp('openvox_webui/config.yaml.epp'),
    }
  }
}
