# frozen_string_literal: true

require 'minitest/autorun'
require 'fileutils'
require 'openssl'
require 'tmpdir'

$LOAD_PATH.unshift(File.expand_path('support', __dir__))
require 'facter'

require_relative '../lib/puppet_x/openvox_webui/paths'

# openvox_classification.rb registers classification variables as facts at load
# time, which would reach the network if the machine running these tests happens
# to have a real client.yaml. Hide config discovery for the duration of the
# require, then put the real implementation back.
PATHS = PuppetX::OpenVoxWebui::Paths
PATHS.send(:alias_method, :__real_config_paths, :config_paths)
PATHS.send(:define_method, :config_paths) { [] }
require_relative '../lib/facter/openvox_classification'
PATHS.send(:remove_method, :config_paths)
PATHS.send(:alias_method, :config_paths, :__real_config_paths)
PATHS.send(:remove_method, :__real_config_paths)

class OpenVoxClassificationTest < Minitest::Test
  # Replaces module methods for the duration of the block. Values that respond
  # to #call are invoked with the original arguments, everything else is
  # returned as-is.
  def with_stubs(stubs)
    originals = {}
    stubs.each do |name, value|
      originals[name] = OpenVoxClassification.method(name)
      callable = value.respond_to?(:call) ? value : ->(*_args) { value }
      OpenVoxClassification.define_singleton_method(name) { |*args, **kwargs| callable.call(*args, **kwargs) }
    end
    yield
  ensure
    originals.each { |name, original| OpenVoxClassification.define_singleton_method(name, original) }
  end

  def with_facter_values(values)
    original = Facter.method(:value)
    Facter.define_singleton_method(:value) { |name| values[name] }
    yield
  ensure
    Facter.define_singleton_method(:value, original)
  end

  # Lays out a fake %PROGRAMDATA% and runs the block as if this were Windows.
  def on_windows
    Dir.mktmpdir do |dir|
      root = File.join(dir, 'ProgramData')
      FileUtils.mkdir_p(root)
      with_stubs(windows?: true, program_data: root) do
        yield root
      end
    end
  end

  def write_file(path, contents)
    FileUtils.mkdir_p(File.dirname(path))
    File.write(path, contents)
    path
  end

  # ---- Configuration discovery ----

  def test_config_paths_use_programdata_on_windows
    with_stubs(windows?: true, program_data: 'D:/ProgramData') do
      paths = OpenVoxClassification.config_paths

      assert_includes paths, 'D:/ProgramData/PuppetLabs/facter/openvox-client.yaml'
      # The manifest default is a literal C:/ProgramData, so it must also be checked.
      assert_includes paths, 'C:/ProgramData/PuppetLabs/facter/openvox-client.yaml'
      refute_includes paths, '/etc/puppetlabs/facter/openvox-client.yaml'
    end
  end

  def test_config_paths_unchanged_on_unix
    with_stubs(windows?: false) do
      assert_equal PuppetX::OpenVoxWebui::Paths::UNIX_CONFIG_PATHS, OpenVoxClassification.config_paths
    end
  end

  # This is the bug: the confine block used to look only under /etc, so it never
  # matched on Windows and the fact silently never ran.
  def test_config_available_finds_the_config_written_by_the_client_class
    on_windows do |root|
      refute OpenVoxClassification.config_available?, 'no config should be found before one is written'

      write_file("#{root}/PuppetLabs/facter/openvox-client.yaml", "api_url: https://webui.example.com\n")

      assert OpenVoxClassification.config_available?
      assert_equal 'https://webui.example.com', OpenVoxClassification.api_url(OpenVoxClassification.load_config)
    end
  end

  def test_load_config_returns_nil_for_unparsable_yaml
    on_windows do |root|
      write_file("#{root}/PuppetLabs/facter/openvox-client.yaml", "api_url: [unterminated\n")

      assert_nil OpenVoxClassification.load_config
    end
  end

  # ---- Certname discovery ----

  def test_puppet_conf_and_ssl_paths_on_windows
    with_stubs(windows?: true, program_data: 'C:/ProgramData') do
      assert_equal ['C:/ProgramData/PuppetLabs/puppet/etc/puppet.conf'], OpenVoxClassification.puppet_conf_paths
      assert_equal ['C:/ProgramData/PuppetLabs/puppet/etc/ssl'], OpenVoxClassification.puppet_ssl_dirs
    end
  end

  def test_discover_certname_reads_the_windows_puppet_conf
    on_windows do |root|
      write_file("#{root}/PuppetLabs/puppet/etc/puppet.conf", "[main]\n  certname = win-node.example.com\n")

      assert_equal 'win-node.example.com', OpenVoxClassification.discover_certname({})
    end
  end

  def test_discover_certname_prefers_config_then_clientcert
    with_facter_values(clientcert: 'agent.example.com') do
      assert_equal 'pinned.example.com', OpenVoxClassification.discover_certname('certname' => 'pinned.example.com')
      assert_equal 'agent.example.com', OpenVoxClassification.discover_certname({})
    end
  end

  def test_discover_certname_falls_back_to_fqdn
    on_windows do
      with_facter_values(fqdn: 'fallback.example.com') do
        assert_equal 'fallback.example.com', OpenVoxClassification.discover_certname({})
      end
    end
  end

  # ---- mTLS material ----

  def test_ca_file_falls_back_to_the_windows_puppet_ssl_dir
    on_windows do |root|
      ssl_dir = "#{root}/PuppetLabs/puppet/etc/ssl"
      write_file("#{ssl_dir}/certs/ca.pem", 'ca')

      assert_equal "#{ssl_dir}/certs/ca.pem", OpenVoxClassification.ca_file({})
    end
  end

  def test_ca_file_prefers_configured_path_when_it_exists
    on_windows do |root|
      configured = write_file("#{root}/custom-ca.pem", 'ca')
      write_file("#{root}/PuppetLabs/puppet/etc/ssl/certs/ca.pem", 'ca')

      assert_equal configured, OpenVoxClassification.ca_file('ssl_ca' => configured)
      # A configured-but-missing file must not shadow the agent's CA.
      assert_equal "#{root}/PuppetLabs/puppet/etc/ssl/certs/ca.pem",
                   OpenVoxClassification.ca_file('ssl_ca' => "#{root}/absent.pem")
    end
  end

  def test_client_certificate_pair_resolves_windows_agent_certificates
    on_windows do |root|
      ssl_dir = "#{root}/PuppetLabs/puppet/etc/ssl"
      cert = write_file("#{ssl_dir}/certs/win-node.example.com.pem", 'cert')
      key = write_file("#{ssl_dir}/private_keys/win-node.example.com.pem", 'key')

      assert_equal [cert, key], OpenVoxClassification.client_certificate_pair({}, 'win-node.example.com')
    end
  end

  def test_client_certificate_pair_is_nil_when_the_agent_has_no_certificate
    on_windows do
      assert_nil OpenVoxClassification.client_certificate_pair({}, 'win-node.example.com')
    end
  end

  def test_build_http_offers_the_client_certificate_even_with_verification_disabled
    on_windows do |root|
      certname = 'win-node.example.com'
      ssl_dir = "#{root}/PuppetLabs/puppet/etc/ssl"
      key = OpenSSL::PKey::RSA.new(2048)
      cert = OpenSSL::X509::Certificate.new
      cert.version = 2
      cert.serial = 1
      cert.subject = OpenSSL::X509::Name.parse("/CN=#{certname}")
      cert.issuer = cert.subject
      cert.public_key = key.public_key
      cert.not_before = Time.now - 60
      cert.not_after = Time.now + 3600
      cert.sign(key, OpenSSL::Digest.new('SHA256'))

      write_file("#{ssl_dir}/certs/#{certname}.pem", cert.to_pem)
      write_file("#{ssl_dir}/private_keys/#{certname}.pem", key.to_pem)

      uri = URI.parse("https://webui.example.com/api/v1/nodes/#{certname}/classify")
      http, has_client_cert = OpenVoxClassification.build_http(uri, { 'ssl_verify' => false }, certname)

      assert has_client_cert, 'the agent certificate should be presented regardless of ssl_verify'
      assert_equal OpenSSL::SSL::VERIFY_NONE, http.verify_mode
      assert_equal certname, http.cert.subject.to_a.assoc('CN')[1]

      # ...and it must not be offered on the unauthenticated environment endpoint.
      _, unauthenticated = OpenVoxClassification.build_http(uri, {}, certname, client_cert: false)
      refute unauthenticated
    end
  end

  def test_build_http_applies_timeouts
    uri = URI.parse('http://webui.example.com/api/v1/nodes/node/classify')

    http, = OpenVoxClassification.build_http(uri, { 'timeout' => 7 }, 'node')
    assert_equal 7, http.open_timeout
    assert_equal 7, http.read_timeout
    refute http.use_ssl?

    override, = OpenVoxClassification.build_http(uri, {}, 'node', open_timeout: 5, read_timeout: 10)
    assert_equal 5, override.open_timeout
    assert_equal 10, override.read_timeout
  end

  def test_authentication_required_only_when_the_transport_verifies_us
    https = URI.parse('https://webui.example.com/x')
    http = URI.parse('http://webui.example.com/x')

    assert OpenVoxClassification.authentication_required?(https, {})
    refute OpenVoxClassification.authentication_required?(https, 'ssl_verify' => false)
    refute OpenVoxClassification.authentication_required?(http, {})
  end

  # ---- Classification key ----

  def test_classification_key_paths_move_under_programdata_on_windows
    with_stubs(windows?: true, program_data: 'D:/ProgramData') do
      paths = OpenVoxClassification.classification_key_paths

      assert_includes paths, 'D:/ProgramData/PuppetLabs/facter/openvox-classification-key'
      assert_includes paths, 'C:/ProgramData/PuppetLabs/facter/openvox-classification-key'
      refute_includes paths, '/etc/openvox-webui/classification_key'
    end
  end

  def test_classification_key_path_unchanged_on_unix
    with_stubs(windows?: false) do
      # The legacy location stays first so existing agents keep reusing their key.
      assert_equal '/etc/openvox-webui/classification_key',
                   OpenVoxClassification.classification_key_paths.first
    end
  end

  def test_resolve_classification_key_prefers_the_configured_value
    with_stubs(classification_key_paths: ['/nonexistent/key']) do
      assert_equal 'from-config',
                   OpenVoxClassification.resolve_classification_key({ 'classification_key' => 'from-config' })
    end
  end

  def test_resolve_classification_key_generates_and_then_reuses_the_stored_key
    on_windows do |root|
      config = { 'auto_generate_classification_key' => true }

      generated = OpenVoxClassification.resolve_classification_key(config, generate: true)

      key_file = "#{root}/PuppetLabs/facter/openvox-classification-key"
      assert_match(/\A[0-9a-f]{32}\z/, generated)
      assert_equal generated, File.read(key_file)
      # A second run must reuse the persisted key rather than rotating it.
      assert_equal generated, OpenVoxClassification.resolve_classification_key(config, generate: true)
      assert_equal generated, OpenVoxClassification.resolve_classification_key(config)
    end
  end

  def test_generate_classification_key_restricts_permissions_on_unix
    Dir.mktmpdir do |dir|
      key_file = File.join(dir, 'openvox-webui', 'classification_key')

      with_stubs(windows?: false, classification_key_paths: [key_file]) do
        assert_match(/\A[0-9a-f]{32}\z/, OpenVoxClassification.generate_classification_key)
      end

      assert_equal 0o600, File.stat(key_file).mode & 0o777
    end
  end

  def test_resolve_classification_key_does_not_generate_without_opt_in
    on_windows do |root|
      assert_nil OpenVoxClassification.resolve_classification_key({ 'auto_generate_classification_key' => true })
      refute_path_exists "#{root}/PuppetLabs/facter/openvox-classification-key"

      assert_nil OpenVoxClassification.resolve_classification_key({}, generate: true)
      refute_path_exists "#{root}/PuppetLabs/facter/openvox-classification-key"
    end
  end

  def test_stored_classification_key_ignores_an_empty_file
    on_windows do |root|
      write_file("#{root}/PuppetLabs/facter/openvox-classification-key", "\n")

      assert_nil OpenVoxClassification.stored_classification_key
    end
  end

  # ---- Requests ----

  def test_build_request_sets_authentication_headers
    uri = URI.parse('https://webui.example.com/api/v1/nodes/node/classify')
    config = { 'api_token' => 'tok', 'api_key' => 'key' }

    request = OpenVoxClassification.build_request(uri, config, 'shared-key')

    assert_equal 'Bearer tok', request['Authorization']
    assert_equal 'key', request['X-API-Key']
    assert_equal 'shared-key', request['X-Classification-Key']
    assert_equal 'application/json', request['Accept']
  end

  def test_build_request_omits_credentials_for_unauthenticated_endpoints
    uri = URI.parse('https://webui.example.com/api/v1/nodes/node/environment')

    request = OpenVoxClassification.build_request(uri, { 'api_token' => 'tok' }, 'shared-key', auth: false)

    assert_nil request['Authorization']
    assert_nil request['X-Classification-Key']
  end

  def test_classify_uri_includes_the_organization_when_configured
    config = { 'api_url' => 'https://webui.example.com/' }

    assert_equal 'https://webui.example.com/api/v1/nodes/node/classify',
                 OpenVoxClassification.classify_uri(config, 'node').to_s
    assert_equal 'https://webui.example.com/api/v1/nodes/node/classify?organization_id=7',
                 OpenVoxClassification.classify_uri(config.merge('organization_id' => 7), 'node').to_s
    assert_equal 'https://webui.example.com/api/v1/nodes/node/environment',
                 OpenVoxClassification.environment_uri(config, 'node').to_s
  end

  # ---- Shared helper ----

  def test_paths_helper_is_shared_with_the_inventory_fact
    require_relative '../lib/facter/openvox_inventory'

    %i[config_paths puppet_conf_paths puppet_ssl_dirs].each do |name|
      assert_equal OpenVoxInventory.method(name).owner, OpenVoxClassification.method(name).owner,
                   "#{name} should come from the shared PuppetX::OpenVoxWebui::Paths helper"
    end
  end
end
