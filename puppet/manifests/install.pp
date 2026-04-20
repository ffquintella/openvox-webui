# @summary Installs OpenVox WebUI package
#
# @api private
#
class openvox_webui::install {
  assert_private()

  if $openvox_webui::manage_package {
    package { $openvox_webui::package_name:
      ensure => $openvox_webui::ensure,
    }
  }
}
