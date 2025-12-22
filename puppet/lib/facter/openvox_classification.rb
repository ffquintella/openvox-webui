# frozen_string_literal: true

# Custom fact to fetch classification data from OpenVox WebUI
# This fact contacts the OpenVox WebUI API to get:
# - Groups the node belongs to
# - Classes assigned via classification
# - Variables/parameters from matched groups
# - Environment assignment
#
# Configuration is read from /etc/openvox-webui/client.yaml or
# /etc/puppetlabs/facter/openvox-client.yaml
#
Facter.add(:openvox_classification) do
  confine do
    # Only run if we can find a config file
    config_paths = [
      '/etc/openvox-webui/client.yaml',
      '/etc/puppetlabs/facter/openvox-client.yaml',
      '/etc/puppetlabs/puppet/openvox-client.yaml'
    ]
    config_paths.any? { |p| File.exist?(p) }
  end

  setcode do
    require 'net/http'
    require 'uri'
    require 'json'
    require 'yaml'
    require 'openssl'

    # Find and load configuration
    config_paths = [
      '/etc/openvox-webui/client.yaml',
      '/etc/puppetlabs/facter/openvox-client.yaml',
      '/etc/puppetlabs/puppet/openvox-client.yaml'
    ]

    config_file = config_paths.find { |p| File.exist?(p) }
    return nil unless config_file

    begin
      config = YAML.load_file(config_file)
    rescue StandardError => e
      Facter.warn("openvox_classification: Failed to load config from #{config_file}: #{e.message}")
      return nil
    end

    # Validate required config
    api_url = config['api_url'] || config['url']
    unless api_url
      Facter.warn('openvox_classification: api_url not configured')
      return nil
    end

    # Get authentication (optional - the public /classify endpoint doesn't require auth)
    api_token = config['api_token'] || config['token']
    api_key = config['api_key']
    ssl_cert = config['ssl_cert']
    ssl_key = config['ssl_key']

    # Get certname from multiple sources (in priority order):
    # 1. Config file override
    # 2. Facter clientcert (set by Puppet agent)
    # 3. puppet.conf certname setting
    # 4. FQDN as fallback
    certname = config['certname'] || Facter.value(:clientcert)

    # Try to read certname from puppet.conf if not found yet
    if certname.nil? || certname.empty?
      puppet_conf_paths = [
        '/etc/puppetlabs/puppet/puppet.conf',
        '/etc/puppet/puppet.conf'
      ]
      puppet_conf_paths.each do |conf_path|
        next unless File.exist?(conf_path)

        begin
          File.readlines(conf_path).each do |line|
            # Match certname = value (allowing for spaces and comments)
            if line =~ /^\s*certname\s*=\s*(\S+)/
              certname = Regexp.last_match(1)
              Facter.debug("openvox_classification: Found certname '#{certname}' in #{conf_path}")
              break
            end
          end
        rescue StandardError => e
          Facter.debug("openvox_classification: Could not read #{conf_path}: #{e.message}")
        end
        break if certname
      end
    end

    # Fall back to FQDN if still not found
    certname ||= Facter.value(:fqdn)

    unless certname
      Facter.warn('openvox_classification: Could not determine certname')
      return nil
    end

    # Get template name (defaults to 'classification')
    template = config['template'] || 'classification'

    # Get organization ID (optional - uses server default if not specified)
    organization_id = config['organization_id']

    # Build the API URL for classification endpoint
    # Use the public /classify endpoint which accepts client certificate auth
    # (the /classification endpoint requires JWT authentication)
    classification_url = "#{api_url.chomp('/')}/api/v1/nodes/#{certname}/classify"
    classification_url += "?organization_id=#{organization_id}" if organization_id

    begin
      uri = URI.parse(classification_url)

      http = Net::HTTP.new(uri.host, uri.port)
      http.open_timeout = config['timeout'] || 10
      http.read_timeout = config['timeout'] || 30

      # Configure SSL if using HTTPS
      if uri.scheme == 'https'
        http.use_ssl = true

        # SSL verification
        if config['ssl_verify'] == false
          Facter.debug('openvox_classification: SSL verification disabled')
          http.verify_mode = OpenSSL::SSL::VERIFY_NONE
        else
          http.verify_mode = OpenSSL::SSL::VERIFY_PEER

          # Custom CA certificate - try multiple locations
          ca_file = config['ssl_ca']

          # If not configured, try common Puppet CA locations
          if ca_file.nil? || !File.exist?(ca_file)
            puppet_ca_paths = [
              '/etc/puppetlabs/puppet/ssl/certs/ca.pem',
              '/etc/puppet/ssl/certs/ca.pem',
              '/var/lib/puppet/ssl/certs/ca.pem'
            ]
            ca_file = puppet_ca_paths.find { |p| File.exist?(p) }
          end

          if ca_file && File.exist?(ca_file)
            Facter.debug("openvox_classification: Using CA file: #{ca_file}")
            http.ca_file = ca_file
          else
            Facter.debug('openvox_classification: No CA file found, using system defaults')
          end

          # Client certificate authentication
          if ssl_cert && ssl_key
            if File.exist?(ssl_cert) && File.exist?(ssl_key)
              Facter.debug("openvox_classification: Using client cert: #{ssl_cert}")
              http.cert = OpenSSL::X509::Certificate.new(File.read(ssl_cert))
              http.key = OpenSSL::PKey::RSA.new(File.read(ssl_key))
            else
              Facter.warn("openvox_classification: Client cert/key files not found: #{ssl_cert}, #{ssl_key}")
            end
          end
        end
      end

      # Build request
      request = Net::HTTP::Get.new(uri.request_uri)
      request['Accept'] = 'application/json'
      request['User-Agent'] = 'OpenVox-Facter/1.0'

      # Add authentication header
      if api_token
        request['Authorization'] = "Bearer #{api_token}"
      elsif api_key
        request['X-API-Key'] = api_key
      end

      # Make the request
      response = http.request(request)

      case response.code.to_i
      when 200
        data = JSON.parse(response.body)

        # Classes are now in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
        classes_data = data['classes'] || {}

        # Return structured classification data
        result = {
          'certname'    => data['certname'] || certname,
          'groups'      => data['groups']&.map { |g| g['name'] } || [],
          'classes'     => classes_data,
          'environment' => data['environment'],
          'variables'   => data['variables'] || {},
          'timestamp'   => Time.now.utc.iso8601
        }

        result
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
  setcode do
    classification = Facter.value(:openvox_classification)
    classification['environment'] if classification
  end
end

Facter.add(:openvox_variables) do
  setcode do
    classification = Facter.value(:openvox_classification)
    classification['variables'] if classification
  end
end

# Register dynamic facts for variables as top-level facts
# This runs at load time to discover and register facts from classification
begin
  require 'net/http'
  require 'uri'
  require 'json'
  require 'yaml'

  config_paths = [
    '/etc/openvox-webui/client.yaml',
    '/etc/puppetlabs/facter/openvox-client.yaml',
    '/etc/puppetlabs/puppet/openvox-client.yaml'
  ]

  config_file = config_paths.find { |p| File.exist?(p) }

  if config_file
    config = YAML.load_file(config_file)
    api_url = config['api_url'] || config['url']

    if api_url
      # Get certname
      certname = config['certname']

      if certname.nil? || certname.to_s.empty?
        puppet_conf_paths = [
          '/etc/puppetlabs/puppet/puppet.conf',
          '/etc/puppet/puppet.conf'
        ]
        puppet_conf_paths.each do |conf_path|
          next unless File.exist?(conf_path)

          File.readlines(conf_path).each do |line|
            if line =~ /^\s*certname\s*=\s*(\S+)/
              certname = Regexp.last_match(1)
              break
            end
          end
          break if certname
        end
      end

      certname ||= Facter.value(:fqdn)

      if certname
        # Fetch classification to discover variable/parameter names
        uri = URI.parse("#{api_url.chomp('/')}/api/v1/nodes/#{certname}/classify")
        http = Net::HTTP.new(uri.host, uri.port)
        http.open_timeout = 5
        http.read_timeout = 10

        if uri.scheme == 'https'
          http.use_ssl = true
          http.verify_mode = OpenSSL::SSL::VERIFY_NONE # Quick check, full verification in main fact
        end

        request = Net::HTTP::Get.new(uri.request_uri)
        request['Accept'] = 'application/json'

        response = http.request(request)

        if response.code.to_i == 200
          data = JSON.parse(response.body)

          # Register each variable as a top-level fact
          (data['variables'] || {}).each do |key, value|
            Facter.add(key.to_sym) do
              setcode { value }
            end
          end
        end
      end
    end
  end
rescue StandardError
  # Silently ignore errors during dynamic fact registration
  # The main openvox_classification fact will report any issues
end
