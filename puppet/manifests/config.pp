# @summary Configures OpenVox WebUI
#
# @api private
#
class openvox_webui::config {
  assert_private()

  if $openvox_webui::manage_config {
    # Auto-discover PuppetDB connection if enabled and not explicitly configured
    if $openvox_webui::puppetdb_auto_discover and !$openvox_webui::puppetdb_url {
      $pdb_conn = $facts['puppetdb_connection']
      if $pdb_conn {
        $effective_puppetdb_url      = pick($openvox_webui::puppetdb_url, $pdb_conn['url'])
        $effective_puppetdb_ssl_cert = pick($openvox_webui::puppetdb_ssl_cert, $pdb_conn['ssl_cert'])
        $effective_puppetdb_ssl_key  = pick($openvox_webui::puppetdb_ssl_key, $pdb_conn['ssl_key'])
        $effective_puppetdb_ssl_ca   = pick($openvox_webui::puppetdb_ssl_ca, $pdb_conn['ssl_ca'])
      } else {
        $effective_puppetdb_url      = $openvox_webui::puppetdb_url
        $effective_puppetdb_ssl_cert = $openvox_webui::puppetdb_ssl_cert
        $effective_puppetdb_ssl_key  = $openvox_webui::puppetdb_ssl_key
        $effective_puppetdb_ssl_ca   = $openvox_webui::puppetdb_ssl_ca
      }
    } else {
      $effective_puppetdb_url      = $openvox_webui::puppetdb_url
      $effective_puppetdb_ssl_cert = $openvox_webui::puppetdb_ssl_cert
      $effective_puppetdb_ssl_key  = $openvox_webui::puppetdb_ssl_key
      $effective_puppetdb_ssl_ca   = $openvox_webui::puppetdb_ssl_ca
    }

    # Auto-discover Puppet CA connection if enabled and not explicitly configured
    if $openvox_webui::puppet_ca_auto_discover and !$openvox_webui::puppet_ca_url {
      $puppet_settings = $facts['puppet_settings']
      if $puppet_settings {
        $effective_puppet_ca_url      = pick($openvox_webui::puppet_ca_url, "https://${puppet_settings['server']}:8140")
        $effective_puppet_ca_ssl_cert = pick($openvox_webui::puppet_ca_ssl_cert, $puppet_settings['hostcert'])
        $effective_puppet_ca_ssl_key  = pick($openvox_webui::puppet_ca_ssl_key, $puppet_settings['hostprivkey'])
        $effective_puppet_ca_ssl_ca   = pick($openvox_webui::puppet_ca_ssl_ca, $puppet_settings['localcacert'])
      } else {
        $effective_puppet_ca_url      = $openvox_webui::puppet_ca_url
        $effective_puppet_ca_ssl_cert = $openvox_webui::puppet_ca_ssl_cert
        $effective_puppet_ca_ssl_key  = $openvox_webui::puppet_ca_ssl_key
        $effective_puppet_ca_ssl_ca   = $openvox_webui::puppet_ca_ssl_ca
      }
    } else {
      $effective_puppet_ca_url      = $openvox_webui::puppet_ca_url
      $effective_puppet_ca_ssl_cert = $openvox_webui::puppet_ca_ssl_cert
      $effective_puppet_ca_ssl_key  = $openvox_webui::puppet_ca_ssl_key
      $effective_puppet_ca_ssl_ca   = $openvox_webui::puppet_ca_ssl_ca
    }

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

    # Create parent log directory /var/log/openvox first
    file { '/var/log/openvox':
      ensure => directory,
      owner  => $openvox_webui::user,
      group  => $openvox_webui::group,
      mode   => '0750',
    }

    file { $openvox_webui::log_dir:
      ensure  => directory,
      owner   => $openvox_webui::user,
      group   => $openvox_webui::group,
      mode    => '0750',
      require => File['/var/log/openvox'],
    }

    file { "${openvox_webui::config_dir}/config.yaml":
      ensure  => file,
      owner   => 'root',
      group   => $openvox_webui::group,
      mode    => '0640',
      content => epp('openvox_webui/config.yaml.epp', {
        listen_address          => $openvox_webui::listen_address,
        listen_port             => $openvox_webui::listen_port,
        database_path           => $openvox_webui::database_path,
        log_level               => $openvox_webui::log_level,
        log_dir                 => $openvox_webui::log_dir,
        serve_frontend          => $openvox_webui::serve_frontend,
        static_dir              => $openvox_webui::static_dir,
        enable_tls              => $openvox_webui::enable_tls,
        tls_cert_file           => $openvox_webui::tls_cert_file,
        tls_key_file            => $openvox_webui::tls_key_file,
        tls_min_version         => $openvox_webui::tls_min_version,
        tls_ciphers             => $openvox_webui::tls_ciphers,
        puppetdb_url            => $effective_puppetdb_url,
        puppetdb_ssl_cert       => $effective_puppetdb_ssl_cert,
        puppetdb_ssl_key        => $effective_puppetdb_ssl_key,
        puppetdb_ssl_ca         => $effective_puppetdb_ssl_ca,
        puppetdb_timeout        => $openvox_webui::puppetdb_timeout,
        puppet_ca_url           => $effective_puppet_ca_url,
        puppet_ca_ssl_cert      => $effective_puppet_ca_ssl_cert,
        puppet_ca_ssl_key       => $effective_puppet_ca_ssl_key,
        puppet_ca_ssl_ca        => $effective_puppet_ca_ssl_ca,
        puppet_ca_timeout       => $openvox_webui::puppet_ca_timeout,
        jwt_secret              => $openvox_webui::jwt_secret,
        jwt_expiry              => $openvox_webui::jwt_expiry,
        session_timeout         => $openvox_webui::session_timeout,
        max_login_attempts      => $openvox_webui::max_login_attempts,
        lockout_duration        => $openvox_webui::lockout_duration,
        admin_username          => $openvox_webui::admin_username,
        admin_password          => $openvox_webui::admin_password,
        admin_email             => $openvox_webui::admin_email,
        cache_ttl               => $openvox_webui::cache_ttl,
        cache_max_entries       => $openvox_webui::cache_max_entries,
        dashboard_theme         => $openvox_webui::dashboard_theme,
        dashboard_page_size     => $openvox_webui::dashboard_page_size,
        dashboard_refresh       => $openvox_webui::dashboard_refresh,
        max_rules_per_group     => $openvox_webui::max_rules_per_group,
        # SAML settings
        saml_enabled                   => $openvox_webui::saml_enabled,
        saml_sp_entity_id              => $openvox_webui::saml_sp_entity_id,
        saml_sp_acs_url                => $openvox_webui::saml_sp_acs_url,
        saml_sp_certificate_file       => $openvox_webui::saml_sp_certificate_file,
        saml_sp_private_key_file       => $openvox_webui::saml_sp_private_key_file,
        saml_sign_requests             => $openvox_webui::saml_sign_requests,
        saml_require_signed_assertions => $openvox_webui::saml_require_signed_assertions,
        saml_idp_metadata_url          => $openvox_webui::saml_idp_metadata_url,
        saml_idp_metadata_file         => $openvox_webui::saml_idp_metadata_file,
        saml_idp_entity_id             => $openvox_webui::saml_idp_entity_id,
        saml_idp_sso_url               => $openvox_webui::saml_idp_sso_url,
        saml_idp_certificate_file      => $openvox_webui::saml_idp_certificate_file,
        saml_username_attribute        => $openvox_webui::saml_username_attribute,
        saml_email_attribute           => $openvox_webui::saml_email_attribute,
        saml_require_existing_user     => $openvox_webui::saml_require_existing_user,
        saml_allow_idp_initiated       => $openvox_webui::saml_allow_idp_initiated,
        saml_request_max_age           => $openvox_webui::saml_request_max_age,
      }),
    }
  }
}
