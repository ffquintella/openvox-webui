# @summary Manages OpenVox WebUI service
#
# @api private
#
class openvox_webui::service {
  assert_private()

  if $openvox_webui::manage_service {
    service { $openvox_webui::service_name:
      ensure     => $openvox_webui::service_ensure,
      enable     => $openvox_webui::service_enable,
      hasstatus  => true,
      hasrestart => true,
    }
  }
}
