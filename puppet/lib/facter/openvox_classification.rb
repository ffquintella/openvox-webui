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

    # Get authentication
    api_token = config['api_token'] || config['token']
    api_key = config['api_key']
    ssl_cert = config['ssl_cert']
    ssl_key = config['ssl_key']

    # Authentication can be via token, API key, or client certificate
    has_token_auth = api_token || api_key
    has_cert_auth = ssl_cert && ssl_key && File.exist?(ssl_cert) && File.exist?(ssl_key)

    unless has_token_auth || has_cert_auth
      Facter.warn('openvox_classification: No authentication configured (api_token, api_key, or ssl_cert/ssl_key required)')
      return nil
    end

    # Get certname (use facter value or config override)
    certname = config['certname'] || Facter.value(:clientcert) || Facter.value(:fqdn)
    unless certname
      Facter.warn('openvox_classification: Could not determine certname')
      return nil
    end

    # Get template name (defaults to 'classification')
    template = config['template'] || 'classification'

    # Build the API URL for classification endpoint
    # Use the nodes classification endpoint which returns full classification
    classification_url = "#{api_url.chomp('/')}/api/v1/nodes/#{certname}/classification"

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
          http.verify_mode = OpenSSL::SSL::VERIFY_NONE
        else
          http.verify_mode = OpenSSL::SSL::VERIFY_PEER

          # Custom CA certificate
          if config['ssl_ca'] && File.exist?(config['ssl_ca'])
            http.ca_file = config['ssl_ca']
          end

          # Client certificate authentication
          if config['ssl_cert'] && config['ssl_key']
            if File.exist?(config['ssl_cert']) && File.exist?(config['ssl_key'])
              http.cert = OpenSSL::X509::Certificate.new(File.read(config['ssl_cert']))
              http.key = OpenSSL::PKey::RSA.new(File.read(config['ssl_key']))
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

        # Return structured classification data
        result = {
          'certname'    => data['certname'] || certname,
          'groups'      => data['groups']&.map { |g| g['name'] } || [],
          'classes'     => data['classes'] || [],
          'environment' => data['environment'],
          'variables'   => data['variables'] || {},
          'parameters'  => data['parameters'] || {},
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

Facter.add(:openvox_parameters) do
  setcode do
    classification = Facter.value(:openvox_classification)
    classification['parameters'] if classification
  end
end
