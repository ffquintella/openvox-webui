# Example: OpenVox WebUI with SAML SSO enabled
#
# This example shows how to configure OpenVox WebUI with SAML 2.0
# Single Sign-On authentication.
#
# SAML authentication allows users to authenticate via external
# Identity Providers (IdP) such as Okta, Azure AD, Keycloak, or ADFS.
#
# Prerequisites:
# 1. Configure your IdP with the SP metadata from:
#    https://<your-openvox-host>/api/v1/auth/saml/metadata
# 2. Users must be pre-provisioned in OpenVox WebUI before they can
#    log in via SAML (unless auto-provisioning is enabled).
#
# @example Using IdP metadata URL (recommended)
class { 'openvox_webui':
  # Enable SAML SSO
  saml_enabled          => true,

  # Service Provider (SP) configuration
  # These identify your OpenVox WebUI installation to the IdP
  saml_sp_entity_id     => 'https://openvox.example.com/saml',
  saml_sp_acs_url       => 'https://openvox.example.com/api/v1/auth/saml/acs',

  # Identity Provider (IdP) configuration
  # Option 1: Use IdP metadata URL (recommended - auto-fetches settings)
  saml_idp_metadata_url => 'https://idp.example.com/saml/metadata',

  # User mapping - how SAML attributes map to OpenVox users
  saml_username_attribute    => 'NameID',  # Or a custom attribute like 'uid'
  saml_require_existing_user => true,      # Users must be pre-provisioned
}

# @example Using local IdP metadata file
# class { 'openvox_webui':
#   saml_enabled           => true,
#   saml_sp_entity_id      => 'https://openvox.example.com/saml',
#   saml_sp_acs_url        => 'https://openvox.example.com/api/v1/auth/saml/acs',
#   saml_idp_metadata_file => '/etc/openvox-webui/saml/idp-metadata.xml',
# }

# @example Manual IdP configuration (when metadata is not available)
# class { 'openvox_webui':
#   saml_enabled              => true,
#   saml_sp_entity_id         => 'https://openvox.example.com/saml',
#   saml_sp_acs_url           => 'https://openvox.example.com/api/v1/auth/saml/acs',
#   saml_idp_entity_id        => 'https://idp.example.com',
#   saml_idp_sso_url          => 'https://idp.example.com/sso',
#   saml_idp_certificate_file => '/etc/openvox-webui/saml/idp.crt',
# }

# @example With signed SAML requests (more secure)
# class { 'openvox_webui':
#   saml_enabled              => true,
#   saml_sp_entity_id         => 'https://openvox.example.com/saml',
#   saml_sp_acs_url           => 'https://openvox.example.com/api/v1/auth/saml/acs',
#   saml_sp_certificate_file  => '/etc/openvox-webui/saml/sp.crt',
#   saml_sp_private_key_file  => '/etc/openvox-webui/saml/sp.key',
#   saml_sign_requests        => true,
#   saml_idp_metadata_url     => 'https://idp.example.com/saml/metadata',
# }

# @example Using Hiera (recommended for production)
# In your hiera data file (e.g., common.yaml):
#
# openvox_webui::saml_enabled: true
# openvox_webui::saml_sp_entity_id: "https://openvox.example.com/saml"
# openvox_webui::saml_sp_acs_url: "https://openvox.example.com/api/v1/auth/saml/acs"
# openvox_webui::saml_idp_metadata_url: "https://idp.example.com/saml/metadata"
# openvox_webui::saml_username_attribute: "NameID"
# openvox_webui::saml_require_existing_user: true
#
# Then in your manifest:
# include openvox_webui
