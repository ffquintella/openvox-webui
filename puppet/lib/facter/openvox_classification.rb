# frozen_string_literal: true

# Custom facts to fetch classification data from OpenVox WebUI.
# These facts contact the OpenVox WebUI API to get:
# - Groups the node belongs to
# - Classes assigned via classification
# - Variables/parameters from matched groups
# - Environment assignment
#
# Configuration is read from the first of these that exists:
#   Unix:    /etc/openvox-webui/client.yaml
#            /etc/puppetlabs/facter/openvox-client.yaml
#            /etc/puppetlabs/puppet/openvox-client.yaml
#   Windows: %PROGRAMDATA%\PuppetLabs\facter\openvox-client.yaml
#            %PROGRAMDATA%\PuppetLabs\puppet\etc\openvox-client.yaml
#            %PROGRAMDATA%\OpenVox-WebUI\client.yaml

require 'json'
require 'net/http'
require 'openssl'
require 'time'
require 'uri'
require 'yaml'

# Pluginsynced to $vardir/lib/puppet_x/openvox_webui/paths.rb alongside this
# file, so resolve it relative to our own location rather than via $LOAD_PATH.
require_relative '../puppet_x/openvox_webui/paths'

module OpenVoxClassification
  # windows?, config_paths, puppet_conf_paths, puppet_ssl_dirs,
  # classification_key_paths
  extend PuppetX::OpenVoxWebui::Paths

  module_function

  USER_AGENT = 'OpenVox-Facter/1.0'

  def config_file
    config_paths.find { |path| File.exist?(path) }
  end

  def config_available?
    !config_file.nil?
  end

  def load_config
    path = config_file
    return nil unless path

    YAML.load_file(path)
  rescue StandardError => e
    Facter.warn("openvox_classification: Failed to load config from #{path}: #{e.message}")
    nil
  end

  def api_url(config)
    config['api_url'] || config['url']
  end

  # Certname resolution, in priority order:
  # 1. Config file override
  # 2. Facter clientcert (set by the Puppet agent)
  # 3. puppet.conf certname setting
  # 4. FQDN as a fallback
  def discover_certname(config)
    certname = config['certname'] || Facter.value(:clientcert)
    return certname unless certname.nil? || certname.to_s.empty?

    puppet_conf_paths.each do |conf_path|
      next unless File.exist?(conf_path)

      begin
        File.readlines(conf_path).each do |line|
          # Match certname = value (allowing for spaces and comments)
          next unless line =~ /^\s*certname\s*=\s*(\S+)/

          found = Regexp.last_match(1)
          Facter.debug("openvox_classification: Found certname '#{found}' in #{conf_path}")
          return found
        end
      rescue StandardError => e
        Facter.debug("openvox_classification: Could not read #{conf_path}: #{e.message}")
      end
    end

    Facter.value(:fqdn)
  end

  # The classification shared key, used when no client certificate is available
  # (debug mode). Only generates a new key when the caller opts in, so the
  # read-only fact paths never write to disk.
  def resolve_classification_key(config, generate: false)
    configured = config['classification_key']
    return configured if configured

    stored = stored_classification_key
    return stored if stored
    return nil unless generate && config['auto_generate_classification_key'] == true

    generate_classification_key
  end

  def stored_classification_key
    path = classification_key_paths.find { |candidate| File.exist?(candidate) }
    return nil unless path

    key = File.read(path).strip
    return nil if key.empty?

    Facter.debug("openvox_classification: Using existing classification key from #{path}")
    key
  rescue StandardError => e
    Facter.debug("openvox_classification: Could not read classification key: #{e.message}")
    nil
  end

  def generate_classification_key
    require 'fileutils'
    require 'securerandom'

    path = classification_key_paths.first
    key = SecureRandom.hex(16)

    FileUtils.mkdir_p(File.dirname(path))
    File.write(path, key)
    # Windows has no POSIX modes; the file inherits the ProgramData ACL there.
    File.chmod(0o600, path) unless windows?
    Facter.debug("openvox_classification: Generated new classification key in #{path}")
    key
  rescue StandardError => e
    Facter.warn("openvox_classification: Could not save classification key: #{e.message}")
    nil
  end

  # Explicit ssl_ca wins; otherwise fall back to the agent's CA bundle.
  def ca_file(config)
    configured = config['ssl_ca']
    return configured if configured && File.exist?(configured)

    puppet_ssl_dirs.map { |dir| "#{dir}/certs/ca.pem" }.find { |path| File.exist?(path) }
  end

  # Auto-detects the Puppet agent certificate when ssl_cert/ssl_key are unset.
  # Returns [cert_path, key_path] or nil when no usable pair exists.
  def client_certificate_pair(config, certname)
    ssl_cert = config['ssl_cert']
    ssl_key = config['ssl_key']

    if ssl_cert.nil? || ssl_key.nil?
      ssl_cert ||= puppet_ssl_dirs.map { |dir| "#{dir}/certs/#{certname}.pem" }.find { |path| File.exist?(path) }
      ssl_key ||= puppet_ssl_dirs.map { |dir| "#{dir}/private_keys/#{certname}.pem" }.find { |path| File.exist?(path) }
    end

    return nil unless ssl_cert && ssl_key && File.exist?(ssl_cert) && File.exist?(ssl_key)

    [ssl_cert, ssl_key]
  end

  # Returns [http, has_client_cert]. Pass client_cert: false for endpoints that
  # do not authenticate the node, so no certificate is offered needlessly.
  def build_http(uri, config, certname, client_cert: true, open_timeout: nil, read_timeout: nil)
    http = Net::HTTP.new(uri.host, uri.port)
    http.open_timeout = open_timeout || config['timeout'] || 10
    http.read_timeout = read_timeout || config['timeout'] || 30

    return [http, false] unless uri.scheme == 'https'

    http.use_ssl = true

    if config['ssl_verify'] == false
      Facter.debug('openvox_classification: SSL verification disabled')
      http.verify_mode = OpenSSL::SSL::VERIFY_NONE
    else
      http.verify_mode = OpenSSL::SSL::VERIFY_PEER

      ca = ca_file(config)
      if ca
        Facter.debug("openvox_classification: Using CA file: #{ca}")
        http.ca_file = ca
      else
        Facter.debug('openvox_classification: No CA file found, using system defaults')
      end
    end

    # Presenting our own certificate is independent of how we verify the
    # server's, so it happens even when ssl_verify is disabled.
    return [http, false] unless client_cert

    pair = client_certificate_pair(config, certname)
    return [http, false] unless pair

    Facter.debug("openvox_classification: Using client cert: #{pair.first}")
    http.cert = OpenSSL::X509::Certificate.new(File.read(pair.first))
    http.key = OpenSSL::PKey::RSA.new(File.read(pair.last))
    [http, true]
  end

  # A client certificate or a classification key is required whenever the
  # transport actually verifies who we are.
  def authentication_required?(uri, config)
    uri.scheme == 'https' && config['ssl_verify'] != false
  end

  def build_request(uri, config, classification_key = nil, auth: true)
    request = Net::HTTP::Get.new(uri.request_uri)
    request['Accept'] = 'application/json'
    request['User-Agent'] = USER_AGENT
    return request unless auth

    api_token = config['api_token'] || config['token']
    api_key = config['api_key']
    request['Authorization'] = "Bearer #{api_token}" if api_token
    request['X-API-Key'] = api_key if api_key
    request['X-Classification-Key'] = classification_key if classification_key
    request
  end

  # Use the public /classify endpoint, which accepts client certificate auth.
  # (The /classification endpoint requires JWT authentication.)
  def classify_uri(config, certname)
    url = "#{api_url(config).chomp('/')}/api/v1/nodes/#{certname}/classify"
    organization_id = config['organization_id']
    url += "?organization_id=#{organization_id}" if organization_id
    URI.parse(url)
  end

  def environment_uri(config, certname)
    URI.parse("#{api_url(config).chomp('/')}/api/v1/nodes/#{certname}/environment")
  end
end

Facter.add(:openvox_classification) do
  confine do
    # Only run if we can find a config file
    OpenVoxClassification.config_available?
  end

  setcode do
    config = OpenVoxClassification.load_config
    next nil unless config

    # Validate required config
    unless OpenVoxClassification.api_url(config)
      Facter.warn('openvox_classification: api_url not configured')
      next nil
    end

    # Authentication priority: client certificate (most secure), then the
    # classification shared key (debug mode). The key may be auto-generated and
    # persisted for reuse across runs.
    classification_key = OpenVoxClassification.resolve_classification_key(config, generate: true)

    certname = OpenVoxClassification.discover_certname(config)
    if certname.nil? || certname.to_s.empty?
      Facter.warn('openvox_classification: Could not determine certname')
      next nil
    end

    begin
      uri = OpenVoxClassification.classify_uri(config, certname)
      http, has_client_cert = OpenVoxClassification.build_http(uri, config, certname)

      if OpenVoxClassification.authentication_required?(uri, config) &&
         !has_client_cert && classification_key.nil?
        Facter.warn('openvox_classification: Authentication required. '\
                    'Configure ssl_cert/ssl_key, classification_key, or ensure Puppet agent certificates exist.')
        next nil
      end

      request = OpenVoxClassification.build_request(uri, config, classification_key)
      response = http.request(request)

      case response.code.to_i
      when 200
        data = JSON.parse(response.body)

        # Classes are in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
        {
          'certname'    => data['certname'] || certname,
          'groups'      => data['groups']&.map { |g| g['name'] } || [],
          'classes'     => data['classes'] || {},
          'environment' => data['environment'],
          'variables'   => data['variables'] || {},
          'timestamp'   => Time.now.utc.iso8601
        }
      when 401, 403
        Facter.warn("openvox_classification: Authentication failed (#{response.code})")
        nil
      when 404
        Facter.debug("openvox_classification: Node #{certname} not found or not classified")
        nil
      else
        Facter.warn("openvox_classification: API request failed with status #{response.code}: #{response.body}")
        nil
      end
    rescue Net::OpenTimeout, Net::ReadTimeout => e
      Facter.warn("openvox_classification: Request timeout: #{e.message}")
      nil
    rescue OpenSSL::SSL::SSLError => e
      Facter.warn("openvox_classification: SSL error: #{e.message}")
      nil
    rescue StandardError => e
      Facter.warn("openvox_classification: Failed to fetch classification: #{e.message}")
      nil
    end
  end
end

# Individual facts derived from classification
# These make it easier to use classification data in Puppet manifests

Facter.add(:openvox_groups) do
  setcode do
    classification = Facter.value(:openvox_classification)
    classification['groups'] if classification
  end
end

Facter.add(:openvox_classes) do
  setcode do
    classification = Facter.value(:openvox_classification)
    classification['classes'] if classification
  end
end

Facter.add(:openvox_environment) do
  # This fact uses a separate unauthenticated endpoint to get the environment.
  # That allows it to work early in the Puppet agent run, before certificates
  # are available.
  setcode do
    config = OpenVoxClassification.load_config
    next nil unless config
    next nil unless OpenVoxClassification.api_url(config)

    certname = OpenVoxClassification.discover_certname(config)
    next nil if certname.nil? || certname.to_s.empty?

    begin
      uri = OpenVoxClassification.environment_uri(config, certname)
      # No client certificate needed - this endpoint is unauthenticated.
      http, = OpenVoxClassification.build_http(uri, config, certname, client_cert: false)
      response = http.request(OpenVoxClassification.build_request(uri, config, auth: false))

      if response.code.to_i == 200
        JSON.parse(response.body)['environment']
      else
        Facter.debug("openvox_environment: API returned #{response.code}")
        nil
      end
    rescue StandardError => e
      Facter.debug("openvox_environment: Failed to fetch environment: #{e.message}")
      nil
    end
  end
end

Facter.add(:openvox_variables) do
  setcode do
    classification = Facter.value(:openvox_classification)
    classification['variables'] if classification
  end
end

# Register classification variables as top-level facts. This runs at load time
# to discover them, so it cannot go through Facter.value(:openvox_classification).
begin
  config = OpenVoxClassification.load_config

  if config && OpenVoxClassification.api_url(config)
    certname = OpenVoxClassification.discover_certname(config)

    if certname && !certname.to_s.empty?
      # Never generate a key here; the openvox_classification fact owns that.
      classification_key = OpenVoxClassification.resolve_classification_key(config)

      uri = OpenVoxClassification.classify_uri(config, certname)
      http, has_client_cert = OpenVoxClassification.build_http(
        uri, config, certname, open_timeout: 5, read_timeout: 10
      )

      # Only proceed if we have auth (client cert or classification key)
      if has_client_cert || classification_key
        response = http.request(OpenVoxClassification.build_request(uri, config, classification_key))

        if response.code.to_i == 200
          (JSON.parse(response.body)['variables'] || {}).each do |key, value|
            Facter.add(key.to_sym) do
              setcode { value }
            end
          end
        end
      end
    end
  end
rescue StandardError
  # Silently ignore errors during dynamic fact registration.
  # The main openvox_classification fact will report any issues.
end
