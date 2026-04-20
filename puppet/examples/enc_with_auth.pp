# Example: Setup ENC for specific server
# This configures Puppet Server to use OpenVox WebUI as ENC
# Uses the public /classify endpoint (no authentication required)
#
# Usage:
#   puppet apply puppet/examples/enc_with_auth.pp

class { 'openvox_webui::enc':
  webui_url                => 'https://segdc1vpr0018.fgv.br',
  manage_puppet_conf       => true,
  restart_puppetserver     => true,
  ssl_verify               => false,
  remove_agent_environment => true,
}
