# frozen_string_literal: true

require 'English'
require 'json'
require 'net/http'
require 'openssl'
require 'time'
require 'uri'
require 'shellwords'
require 'yaml'

module OpenVoxInventory
  module_function

  CONFIG_PATHS = [
    '/etc/openvox-webui/client.yaml',
    '/etc/puppetlabs/facter/openvox-client.yaml',
    '/etc/puppetlabs/puppet/openvox-client.yaml'
  ].freeze

  def load_config
    config_file = CONFIG_PATHS.find { |path| File.exist?(path) }
    return nil unless config_file

    YAML.load_file(config_file)
  rescue StandardError => e
    Facter.warn("openvox_inventory: Failed to load config: #{e.message}")
    nil
  end

  def inventory_enabled?(config)
    config && config['inventory_enabled'] == true
  end

  def discover_certname(config)
    certname = config['certname'] || Facter.value(:clientcert)
    return certname unless certname.nil? || certname.empty?

    [
      '/etc/puppetlabs/puppet/puppet.conf',
      '/etc/puppet/puppet.conf'
    ].each do |conf_path|
      next unless File.exist?(conf_path)

      begin
        File.readlines(conf_path).each do |line|
          if line =~ /^\s*certname\s*=\s*(\S+)/
            return Regexp.last_match(1)
          end
        end
      rescue StandardError
        nil
      end
    end

    Facter.value(:fqdn)
  end

  def collect_inventory(config)
    require 'facter'

    collected_at = Time.now.utc.iso8601
    os_fact = Facter.value(:os) || {}
    family = os_fact.dig('family') || Facter.value(:kernel)

    packages = case family
               when 'RedHat', 'Suse'
                 collect_linux_rpm_packages(config)
               when 'Debian'
                 collect_linux_deb_packages(config)
               when 'Darwin'
                 collect_macos_homebrew_packages(config)
               when 'windows', 'Windows'
                 []
               else
                 []
               end

    applications =
      case family
      when 'Darwin'
        collect_macos_applications(config)
      when 'windows', 'Windows'
        collect_windows_applications(config)
      else
        collect_linux_applications(config)
      end

    websites =
      case family
      when 'RedHat', 'Suse', 'Debian'
        collect_linux_websites(config)
      when 'windows', 'Windows'
        collect_windows_iis_sites(config)
      else
        []
      end

    runtimes =
      case family
      when 'RedHat', 'Suse', 'Debian'
        collect_linux_runtimes(config)
      else
        []
      end

    payload = {
      'collector_version' => 'phase10.2-puppet',
      'collected_at' => collected_at,
      'is_full_snapshot' => true,
      'os' => collect_os_inventory(os_fact, collected_at),
      'packages' => trim(normalize_packages(packages), config),
      'applications' => trim(normalize_applications(applications), config),
      'websites' => trim(normalize_websites(websites), config),
      'runtimes' => trim(normalize_runtimes(runtimes), config)
    }

    payload['os']['update_channel'] ||= infer_update_channel(payload)
    payload
  end

  def collect_os_inventory(os_fact, collected_at)
    {
      'os_family' => os_fact['family'] || 'Unknown',
      'distribution' => os_fact['name'] || 'Unknown',
      'edition' => os_fact['distro'] && os_fact['distro']['description'],
      'architecture' => Facter.value(:architecture),
      'kernel_version' => Facter.value(:kernelrelease),
      'os_version' => os_fact.dig('release', 'full') || os_fact.dig('release', 'major') || 'Unknown',
      'patch_level' => os_fact.dig('release', 'minor'),
      'package_manager' => detect_package_manager(os_fact),
      'update_channel' => nil,
      'last_inventory_at' => collected_at,
      'last_successful_update_at' => nil
    }
  end

  def detect_package_manager(os_fact)
    case os_fact['family']
    when 'RedHat'
      'dnf'
    when 'Debian'
      'apt'
    when 'Suse'
      'zypper'
    when 'Darwin'
      brew_installed? ? 'brew' : 'softwareupdate'
    when 'windows', 'Windows'
      'choco'
    else
      nil
    end
  end

  def collect_linux_rpm_packages(_config)
    output = run_command(
      "rpm -qa --queryformat '%{NAME}\\t%|EPOCH?{%{EPOCH}}:{}|\\t%{VERSION}\\t%{RELEASE}\\t%{ARCH}\\t%{VENDOR}\\t%{INSTALLTIME}\\n'"
    )
    return [] if output.nil? || output.empty?

    output.each_line.map do |line|
      name, epoch, version, release, arch, vendor, install_time = line.strip.split("\t", 7)
      next if name.to_s.empty? || version.to_s.empty?

      {
        'name' => name,
        'epoch' => blank_to_nil(epoch),
        'version' => version,
        'release' => blank_to_nil(release),
        'architecture' => blank_to_nil(arch),
        'repository_source' => blank_to_nil(vendor),
        'install_path' => nil,
        'install_time' => unix_timestamp_to_iso8601(install_time)
      }
    end.compact
  end

  def collect_linux_deb_packages(_config)
    output = run_command("dpkg-query -W -f='${Package}\\t${Version}\\t${Architecture}\\n'")
    return [] if output.nil? || output.empty?

    output.each_line.map do |line|
      name, version_raw, arch = line.strip.split("\t", 3)
      next if name.to_s.empty? || version_raw.to_s.empty?

      epoch = nil
      version = version_raw
      if version.include?(':')
        epoch, version = version.split(':', 2)
      end

      release = nil
      if version.include?('-')
        version, release = version.split('-', 2)
      end

      {
        'name' => name,
        'epoch' => blank_to_nil(epoch),
        'version' => version,
        'release' => blank_to_nil(release),
        'architecture' => blank_to_nil(arch),
        'repository_source' => nil,
        'install_path' => nil,
        'install_time' => nil
      }
    end.compact
  end

  def collect_macos_homebrew_packages(config)
    packages = []
    return packages unless brew_installed?

    output = run_command('brew info --json=v2 --installed')
    return packages if output.nil? || output.empty?

    data = JSON.parse(output)
    (data['formulae'] || []).each do |formula|
      installed = Array(formula['installed'])
      installed_entry = installed.first || {}
      packages << {
        'name' => formula['name'],
        'epoch' => nil,
        'version' => installed.map { |item| item['version'] }.compact.first || 'unknown',
        'release' => nil,
        'architecture' => blank_to_nil(installed_entry['built_as_bottle'] ? 'bottle' : nil),
        'repository_source' => formula['tap'],
        'install_path' => installed_entry['path'] || formula['linked_keg'],
        'install_time' => nil
      }
    end

    (data['casks'] || []).each do |cask|
      packages << {
        'name' => cask['token'],
        'epoch' => nil,
        'version' => cask['version'] || 'unknown',
        'release' => nil,
        'architecture' => nil,
        'repository_source' => cask['tap'],
        'install_path' => cask['installed']&.first&.dig('installed_artifacts', 0, 0),
        'install_time' => nil
      }
    end

    trim(packages, config)
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Homebrew inventory: #{e.message}")
    []
  end

  def collect_macos_applications(config)
    apps = []
    app_roots = ['/Applications', File.expand_path('~/Applications')]

    app_roots.each do |root|
      next unless Dir.exist?(root)

      Dir.glob(File.join(root, '*.app')).each do |app_path|
        info_plist = File.join(app_path, 'Contents', 'Info.plist')
        next unless File.exist?(info_plist)

        bundle_id = run_command(%(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "#{info_plist}" 2>/dev/null)).to_s.strip
        display_name = run_command(%(/usr/libexec/PlistBuddy -c "Print :CFBundleDisplayName" "#{info_plist}" 2>/dev/null)).to_s.strip
        display_name = run_command(%(/usr/libexec/PlistBuddy -c "Print :CFBundleName" "#{info_plist}" 2>/dev/null)).to_s.strip if display_name.empty?
        version = run_command(%(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "#{info_plist}" 2>/dev/null)).to_s.strip
        build = run_command(%(/usr/libexec/PlistBuddy -c "Print :CFBundleVersion" "#{info_plist}" 2>/dev/null)).to_s.strip

        apps << {
          'name' => display_name.empty? ? File.basename(app_path, '.app') : display_name,
          'publisher' => nil,
          'version' => version.empty? ? (build.empty? ? 'unknown' : build) : version,
          'architecture' => nil,
          'install_scope' => root.start_with?(File.expand_path('~')) ? 'user' : 'system',
          'install_path' => app_path,
          'application_type' => 'app_bundle',
          'bundle_identifier' => blank_to_nil(bundle_id),
          'uninstall_identity' => nil,
          'install_date' => nil,
          'metadata' => compact_hash({
            'bundle_version' => blank_to_nil(build),
            'bundle_root' => app_path
          })
        }
      end
    end

    trim(apps, config)
  end

  def collect_windows_applications(config)
    script = <<~POWERSHELL
      $paths = @(
        @{ path = 'HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*'; scope = 'system'; arch = 'x64' },
        @{ path = 'HKLM:\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*'; scope = 'system'; arch = 'x86' },
        @{ path = 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*'; scope = 'user'; arch = $env:PROCESSOR_ARCHITECTURE }
      )
      $apps = foreach ($entry in $paths) {
        Get-ItemProperty $entry.path -ErrorAction SilentlyContinue |
          Where-Object { $_.DisplayName -and $_.DisplayVersion } |
          ForEach-Object {
            [pscustomobject]@{
              DisplayName = $_.DisplayName
              Publisher = $_.Publisher
              DisplayVersion = $_.DisplayVersion
              InstallDate = $_.InstallDate
              UninstallString = $_.UninstallString
              InstallLocation = $_.InstallLocation
              Scope = $entry.scope
              Architecture = $entry.arch
            }
          }
      }
      $apps | ConvertTo-Json -Depth 4
    POWERSHELL
    output = run_command(%(powershell.exe -NoProfile -NonInteractive -Command "#{escape_powershell(script)}"))
    return [] if output.nil? || output.empty?

    parsed = JSON.parse(output)
    parsed = [parsed] if parsed.is_a?(Hash)
    trim(parsed.map do |app|
      {
        'name' => app['DisplayName'],
        'publisher' => blank_to_nil(app['Publisher']),
        'version' => app['DisplayVersion'] || 'unknown',
        'architecture' => blank_to_nil(app['Architecture']),
        'install_scope' => blank_to_nil(app['Scope']) || 'system',
        'install_path' => blank_to_nil(app['InstallLocation']),
        'application_type' => 'windows_uninstall',
        'bundle_identifier' => nil,
        'uninstall_identity' => blank_to_nil(app['UninstallString']),
        'install_date' => parse_windows_date(app['InstallDate']),
        'metadata' => nil
      }
    end, config)
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Windows applications: #{e.message}")
    []
  end

  def collect_linux_applications(config)
    applications = []

    service_candidates = {
      'Apache HTTP Server' => ['/usr/sbin/httpd', '/usr/sbin/apache2'],
      'NGINX' => ['/usr/sbin/nginx', '/usr/local/sbin/nginx'],
      'Tomcat' => ['/usr/share/tomcat', '/opt/tomcat', '/var/lib/tomcat'],
      'JBoss / WildFly' => ['/opt/wildfly', '/opt/jboss', '/opt/jboss-eap']
    }

    service_candidates.each do |name, paths|
      path = paths.find { |candidate| File.exist?(candidate) }
      next unless path

      applications << {
        'name' => name,
        'publisher' => nil,
        'version' => detect_application_version(name, path),
        'architecture' => Facter.value(:architecture),
        'install_scope' => 'system',
        'install_path' => path,
        'application_type' => 'service',
        'bundle_identifier' => nil,
        'uninstall_identity' => nil,
        'install_date' => nil,
        'metadata' => compact_hash({
          'discovery_path' => path
        })
      }
    end

    trim(applications, config)
  end

  def detect_application_version(name, path)
    case name
    when 'Apache HTTP Server'
      output = run_command(%("#{path}" -v 2>/dev/null))
      output.to_s[/Apache\/([^\s]+)/, 1] || 'unknown'
    when 'NGINX'
      output = run_command(%("#{path}" -v 2>&1))
      output.to_s[/nginx\/([^\s]+)/, 1] || 'unknown'
    else
      'unknown'
    end
  rescue StandardError
    'unknown'
  end

  def collect_linux_websites(config)
    trim(collect_apache_sites + collect_nginx_sites, config)
  end

  def collect_apache_sites
    command = if command_available?('apachectl')
                'apachectl -S 2>/dev/null'
              elsif command_available?('httpd')
                'httpd -S 2>/dev/null'
              else
                nil
              end
    return [] unless command

    output = run_command(command)
    return [] if output.nil? || output.empty?

    output.each_line.map do |line|
      next unless line.include?('namevhost') || line.include?('port ')

      site_name = line[/namevhost\s+([^\s]+)/, 1] || line[/port\s+(\d+)/, 1]
      binding = line[/port\s+(\d+)/, 1]
      conf_hint = line[/\(([^:]+:\d+)\)/, 1]
      next if site_name.nil?

      {
        'server_type' => 'apache',
        'site_name' => site_name,
        'bindings' => binding ? ["*:#{binding}"] : [],
        'document_root' => nil,
        'application_pool' => nil,
        'tls_certificate_reference' => nil,
        'metadata' => compact_hash({
          'source' => 'apachectl -S',
          'config_reference' => blank_to_nil(conf_hint)
        })
      }
    end.compact.uniq { |site| [site['site_name'], site['bindings']] }
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Apache sites: #{e.message}")
    []
  end

  def collect_nginx_sites
    return [] unless command_available?('nginx')

    output = run_command('nginx -T 2>/dev/null')
    return [] if output.nil? || output.empty?

    sites = []
    current = nil
    output.each_line do |line|
      stripped = line.strip
      if stripped.start_with?('server {')
        current = {
          'server_type' => 'nginx',
          'site_name' => 'default',
          'bindings' => [],
          'document_root' => nil,
          'application_pool' => nil,
          'tls_certificate_reference' => nil,
          'metadata' => { 'server_names' => [], 'upstreams' => [] }
        }
      elsif current && stripped.start_with?('server_name ')
        names = stripped.sub('server_name', '').sub(';', '').strip.split(/\s+/)
        current['metadata']['server_names'] = names
        current['site_name'] = names.first || current['site_name']
      elsif current && stripped.start_with?('listen ')
        current['bindings'] << stripped.sub('listen', '').sub(';', '').strip
      elsif current && stripped.start_with?('root ')
        current['document_root'] = stripped.sub('root', '').sub(';', '').strip
      elsif current && stripped.start_with?('ssl_certificate ')
        current['tls_certificate_reference'] = stripped.sub('ssl_certificate', '').sub(';', '').strip
      elsif current && stripped.start_with?('proxy_pass ')
        current['metadata']['upstreams'] << stripped.sub('proxy_pass', '').sub(';', '').strip
      elsif current && stripped == '}'
        current['metadata'] = compact_hash(current['metadata'])
        sites << current
        current = nil
      end
    end

    sites.uniq { |site| [site['site_name'], Array(site['bindings']).sort] }
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect NGINX sites: #{e.message}")
    []
  end

  def collect_windows_iis_sites(_config)
    script = <<~POWERSHELL
      Import-Module WebAdministration -ErrorAction SilentlyContinue
      if (Get-Command Get-Website -ErrorAction SilentlyContinue) {
        Get-Website | ForEach-Object {
          $certs = @($_.Bindings.Collection | Where-Object { $_.protocol -eq 'https' } | ForEach-Object { $_.certificateHash })
          [pscustomobject]@{
            site_name = $_.Name
            bindings = @($_.Bindings.Collection | ForEach-Object { $_.bindingInformation })
            document_root = $_.PhysicalPath
            app_pool = $_.applicationPool
            certs = $certs
          }
        } | ConvertTo-Json -Depth 5
      }
    POWERSHELL
    output = run_command(%(powershell.exe -NoProfile -NonInteractive -Command "#{escape_powershell(script)}"))
    return [] if output.nil? || output.empty?

    parsed = JSON.parse(output)
    parsed = [parsed] if parsed.is_a?(Hash)
    parsed.map do |site|
      {
        'server_type' => 'iis',
        'site_name' => site['site_name'],
        'bindings' => site['bindings'] || [],
        'document_root' => site['document_root'],
        'application_pool' => blank_to_nil(site['app_pool']),
        'tls_certificate_reference' => Array(site['certs']).compact.first,
        'metadata' => compact_hash({
          'certificate_hashes' => Array(site['certs']).compact
        })
      }
    end
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect IIS sites: #{e.message}")
    []
  end

  def collect_linux_runtimes(config)
    runtimes = []

    tomcat_dirs = Dir.glob('/opt/tomcat*') + Dir.glob('/usr/share/tomcat*') + Dir.glob('/var/lib/tomcat*')
    tomcat_dirs.select { |path| File.directory?(path) }.uniq.each do |dir|
      deployed = Dir.glob(File.join(dir, 'webapps', '*'))
                    .reject { |path| path.end_with?('.tmp') }
                    .map { |path| File.basename(path) }
      version = run_command(%(find "#{dir}" -name ServerInfo.properties -exec grep -h 'server.info' {} \\; 2>/dev/null | head -n 1)).to_s[/Apache Tomcat\/([^\s]+)/, 1]
      runtimes << {
        'runtime_type' => 'tomcat',
        'runtime_name' => File.basename(dir),
        'runtime_version' => version || 'unknown',
        'install_path' => dir,
        'management_endpoint' => nil,
        'deployed_units' => deployed,
        'metadata' => compact_hash({
          'webapps_dir' => File.join(dir, 'webapps')
        })
      }
    end

    wildfly_dirs = Dir.glob('/opt/wildfly*') + Dir.glob('/opt/jboss*')
    wildfly_dirs.select { |path| File.directory?(path) }.uniq.each do |dir|
      deployed = Dir.glob(File.join(dir, 'standalone', 'deployments', '*')).map { |path| File.basename(path) }
      version = run_command(%(find "#{dir}" -name product.conf -exec grep -h 'slot=' {} \\; 2>/dev/null | head -n 1)).to_s.split('=').last
      runtimes << {
        'runtime_type' => 'jboss',
        'runtime_name' => File.basename(dir),
        'runtime_version' => blank_to_nil(version&.strip) || 'unknown',
        'install_path' => dir,
        'management_endpoint' => nil,
        'deployed_units' => deployed,
        'metadata' => compact_hash({
          'deployments_dir' => File.join(dir, 'standalone', 'deployments')
        })
      }
    end

    trim(runtimes, config)
  end

  def submit_inventory(config, certname, payload)
    api_url = config['api_url'] || config['url']
    return [false, nil] if api_url.nil? || certname.nil?

    uri = URI.parse("#{api_url.chomp('/')}/api/v1/nodes/#{certname}/inventory")
    http = build_http(uri, config, certname)

    request = Net::HTTP::Post.new(uri.request_uri)
    request['Accept'] = 'application/json'
    request['Content-Type'] = 'application/json'
    request['User-Agent'] = 'OpenVox-InventoryCollector/1.0'
    add_auth_headers(request, config)
    request.body = JSON.generate(payload)

    response = http.request(request)
    [response.code.to_i >= 200 && response.code.to_i < 300, response.code.to_i]
  rescue StandardError => e
    Facter.warn("openvox_inventory: Inventory submission failed: #{e.message}")
    [false, nil]
  end

  def build_http(uri, config, certname)
    http = Net::HTTP.new(uri.host, uri.port)
    http.open_timeout = config['timeout'] || 10
    http.read_timeout = config['timeout'] || 30

    return http unless uri.scheme == 'https'

    http.use_ssl = true
    if config['ssl_verify'] == false
      http.verify_mode = OpenSSL::SSL::VERIFY_NONE
    else
      http.verify_mode = OpenSSL::SSL::VERIFY_PEER
      ca_file = config['ssl_ca']
      if ca_file.nil? || !File.exist?(ca_file)
        ca_file = [
          '/etc/puppetlabs/puppet/ssl/certs/ca.pem',
          '/etc/puppet/ssl/certs/ca.pem',
          '/var/lib/puppet/ssl/certs/ca.pem'
        ].find { |path| File.exist?(path) }
      end
      http.ca_file = ca_file if ca_file && File.exist?(ca_file)
    end

    ssl_cert = config['ssl_cert']
    ssl_key = config['ssl_key']
    if ssl_cert.nil? || ssl_key.nil?
      ssl_cert ||= [
        "/etc/puppetlabs/puppet/ssl/certs/#{certname}.pem",
        "/etc/puppet/ssl/certs/#{certname}.pem"
      ].find { |path| File.exist?(path) }
      ssl_key ||= [
        "/etc/puppetlabs/puppet/ssl/private_keys/#{certname}.pem",
        "/etc/puppet/ssl/private_keys/#{certname}.pem"
      ].find { |path| File.exist?(path) }
    end

    if ssl_cert && ssl_key && File.exist?(ssl_cert) && File.exist?(ssl_key)
      http.cert = OpenSSL::X509::Certificate.new(File.read(ssl_cert))
      http.key = OpenSSL::PKey::RSA.new(File.read(ssl_key))
    end

    http
  end

  def add_auth_headers(request, config)
    api_token = config['api_token'] || config['token']
    api_key = config['api_key']
    classification_key = config['classification_key']

    request['Authorization'] = "Bearer #{api_token}" if api_token
    request['X-API-Key'] = api_key if api_key
    request['X-Classification-Key'] = classification_key if classification_key
  end

  def fetch_pending_update_jobs(config, certname)
    api_url = config['api_url'] || config['url']
    return [] if api_url.nil? || certname.nil?

    uri = URI.parse("#{api_url.chomp('/')}/api/v1/nodes/#{certname}/update-jobs")
    http = build_http(uri, config, certname)

    request = Net::HTTP::Get.new(uri.request_uri)
    request['Accept'] = 'application/json'
    request['User-Agent'] = 'OpenVox-InventoryCollector/1.0'
    add_auth_headers(request, config)

    response = http.request(request)
    return [] unless response.code.to_i >= 200 && response.code.to_i < 300

    JSON.parse(response.body)
  rescue StandardError => e
    Facter.warn("openvox_inventory: Failed to fetch pending update jobs: #{e.message}")
    []
  end

  def execute_update_job(job, config)
    os_fact = Facter.value(:os) || {}
    family = os_fact.dig('family') || Facter.value(:kernel)
    operation = job['operation_type']
    packages = job['package_names'] || []

    case operation
    when 'system_patch', 'SystemPatch'
      execute_system_patch(family, config)
    when 'security_patch', 'SecurityPatch'
      execute_security_patch(family, config)
    when 'package_update', 'PackageUpdate'
      execute_package_update(family, packages, config)
    when 'package_install', 'PackageInstall'
      execute_package_install(family, packages, config)
    when 'package_remove', 'PackageRemove'
      execute_package_remove(family, packages, config)
    else
      { 'status' => 'failed', 'summary' => "Unknown operation type: #{operation}", 'output' => '' }
    end
  end

  def execute_system_patch(family, _config)
    cmd = case family
          when 'RedHat', 'Suse'
            'dnf update -y 2>&1 || yum update -y 2>&1'
          when 'Debian'
            'apt-get update -q && apt-get upgrade -y 2>&1'
          else
            return { 'status' => 'failed', 'summary' => "Unsupported OS family: #{family}", 'output' => '' }
          end

    run_update_command(cmd)
  end

  def execute_security_patch(family, _config)
    cmd = case family
          when 'RedHat', 'Suse'
            'dnf update --security -y 2>&1 || yum update --security -y 2>&1'
          when 'Debian'
            'apt-get update -q && apt-get upgrade -y --only-upgrade 2>&1'
          else
            return { 'status' => 'failed', 'summary' => "Unsupported OS family: #{family}", 'output' => '' }
          end

    run_update_command(cmd)
  end

  def sanitize_package_names(packages)
    packages.select { |p| p.is_a?(String) && p.match?(/\A[a-zA-Z0-9._+\-:]+\z/) }
  end

  def execute_package_update(family, packages, _config)
    safe_packages = sanitize_package_names(packages)
    return { 'status' => 'failed', 'summary' => 'No valid packages specified', 'output' => '' } if safe_packages.empty?

    pkg_list = safe_packages.map { |p| Shellwords.shellescape(p) }.join(' ')
    cmd = case family
          when 'RedHat', 'Suse'
            "dnf update -y #{pkg_list} 2>&1 || yum update -y #{pkg_list} 2>&1"
          when 'Debian'
            "apt-get update -q && apt-get install --only-upgrade -y #{pkg_list} 2>&1"
          else
            return { 'status' => 'failed', 'summary' => "Unsupported OS family: #{family}", 'output' => '' }
          end

    run_update_command(cmd)
  end

  def execute_package_install(family, packages, _config)
    safe_packages = sanitize_package_names(packages)
    return { 'status' => 'failed', 'summary' => 'No valid packages specified', 'output' => '' } if safe_packages.empty?

    pkg_list = safe_packages.map { |p| Shellwords.shellescape(p) }.join(' ')
    cmd = case family
          when 'RedHat', 'Suse'
            "dnf install -y #{pkg_list} 2>&1 || yum install -y #{pkg_list} 2>&1"
          when 'Debian'
            "apt-get update -q && apt-get install -y #{pkg_list} 2>&1"
          else
            return { 'status' => 'failed', 'summary' => "Unsupported OS family: #{family}", 'output' => '' }
          end

    run_update_command(cmd)
  end

  def execute_package_remove(family, packages, _config)
    safe_packages = sanitize_package_names(packages)
    return { 'status' => 'failed', 'summary' => 'No valid packages specified', 'output' => '' } if safe_packages.empty?

    pkg_list = safe_packages.map { |p| Shellwords.shellescape(p) }.join(' ')
    cmd = case family
          when 'RedHat', 'Suse'
            "dnf remove -y #{pkg_list} 2>&1 || yum remove -y #{pkg_list} 2>&1"
          when 'Debian'
            "apt-get remove -y #{pkg_list} 2>&1"
          else
            return { 'status' => 'failed', 'summary' => "Unsupported OS family: #{family}", 'output' => '' }
          end

    run_update_command(cmd)
  end

  def run_update_command(cmd)
    output = `#{cmd}`
    exit_code = $CHILD_STATUS.exitstatus
    {
      'status' => exit_code == 0 ? 'succeeded' : 'failed',
      'summary' => exit_code == 0 ? 'Update completed successfully' : "Update failed with exit code #{exit_code}",
      'output' => output.to_s.slice(0, 10_000)
    }
  rescue StandardError => e
    { 'status' => 'failed', 'summary' => "Execution error: #{e.message}", 'output' => '' }
  end

  def submit_update_job_result(config, certname, job_id, target_id, result, started_at, finished_at)
    api_url = config['api_url'] || config['url']
    return unless api_url

    uri = URI.parse("#{api_url.chomp('/')}/api/v1/nodes/#{certname}/update-jobs/#{job_id}/targets/#{target_id}/results")
    http = build_http(uri, config, certname)

    request = Net::HTTP::Post.new(uri.request_uri)
    request['Accept'] = 'application/json'
    request['Content-Type'] = 'application/json'
    request['User-Agent'] = 'OpenVox-InventoryCollector/1.0'
    add_auth_headers(request, config)

    payload = {
      'status' => result['status'],
      'summary' => result['summary'],
      'output' => result['output'],
      'started_at' => started_at,
      'finished_at' => finished_at
    }
    request.body = JSON.generate(payload)

    response = http.request(request)
    Facter.warn("openvox_inventory: Update job result submission returned #{response.code}") unless response.code.to_i < 300
  rescue StandardError => e
    Facter.warn("openvox_inventory: Failed to submit update job result: #{e.message}")
  end

  def trim(items, config)
    max_items = (config['inventory_max_items'] || 10000).to_i
    items.first(max_items)
  end

  def normalize_packages(items)
    items.map do |item|
      next unless present?(item['name']) && present?(item['version'])
      compact_hash(item)
    end.compact.uniq { |item| [item['name'], item['version'], item['release'], item['architecture'], item['install_path']] }
  end

  def normalize_applications(items)
    items.map do |item|
      next unless present?(item['name']) && present?(item['version'])
      compact_hash(item)
    end.compact.uniq { |item| [item['name'], item['version'], item['install_path'], item['bundle_identifier']] }
  end

  def normalize_websites(items)
    items.map do |item|
      next unless present?(item['site_name'])
      item['bindings'] = Array(item['bindings']).map(&:to_s).map(&:strip).reject(&:empty?).uniq.sort
      compact_hash(item)
    end.compact.uniq { |item| [item['server_type'], item['site_name'], item['bindings']] }
  end

  def normalize_runtimes(items)
    items.map do |item|
      next unless present?(item['runtime_type']) && present?(item['runtime_name'])
      item['deployed_units'] = Array(item['deployed_units']).map(&:to_s).reject(&:empty?).uniq.sort
      compact_hash(item)
    end.compact.uniq { |item| [item['runtime_type'], item['runtime_name'], item['install_path']] }
  end

  def compact_hash(value)
    return value unless value.is_a?(Hash)

    value.each_with_object({}) do |(key, item), result|
      normalized = if item.is_a?(Hash)
                     compact_hash(item)
                   elsif item.is_a?(Array)
                     item.map { |entry| entry.is_a?(Hash) ? compact_hash(entry) : entry }
                         .reject { |entry| blankish?(entry) }
                   else
                     item
                   end
      result[key] = normalized unless blankish?(normalized)
    end
  end

  def infer_update_channel(payload)
    repos = Array(payload['packages']).map { |pkg| pkg['repository_source'] }.compact.uniq
    return nil if repos.empty?

    repos.first
  end

  def brew_installed?
    command_available?('brew')
  end

  def command_available?(command)
    system("command -v #{command} >/dev/null 2>&1")
  end

  def run_command(command)
    output = `#{command}`
    return nil unless $CHILD_STATUS.success?

    output
  rescue StandardError
    nil
  end

  def blank_to_nil(value)
    value.nil? || value.to_s.strip.empty? ? nil : value
  end

  def blankish?(value)
    value.nil? || (value.respond_to?(:empty?) && value.empty?) || value.to_s.strip.empty?
  end

  def present?(value)
    !blankish?(value)
  end

  def unix_timestamp_to_iso8601(value)
    return nil unless value.to_s.match?(/^\d+$/)

    Time.at(value.to_i).utc.iso8601
  rescue StandardError
    nil
  end

  def parse_windows_date(value)
    return nil if value.nil? || value.to_s.empty?
    return nil unless value.to_s.match?(/^\d{8}$/)

    "#{value[0, 4]}-#{value[4, 2]}-#{value[6, 2]}T00:00:00Z"
  end

  def escape_powershell(script)
    script.gsub('"', '\"').gsub(/\r?\n/, '; ')
  end
end

Facter.add(:openvox_inventory_status) do
  confine do
    config = OpenVoxInventory.load_config
    OpenVoxInventory.inventory_enabled?(config)
  end

  setcode do
    require 'json'
    require 'yaml'
    require 'net/http'
    require 'uri'
    require 'openssl'

    config = OpenVoxInventory.load_config
    next nil unless OpenVoxInventory.inventory_enabled?(config)

    certname = OpenVoxInventory.discover_certname(config)
    next nil if certname.nil? || certname.empty?

    payload = OpenVoxInventory.collect_inventory(config)
    submitted = false
    status_code = nil

    if config['inventory_submit'] != false
      submitted, status_code = OpenVoxInventory.submit_inventory(config, certname, payload)
    end

    # Poll for and execute pending update jobs
    update_jobs_executed = 0
    if submitted && config['inventory_updates'] != false
      pending_jobs = OpenVoxInventory.fetch_pending_update_jobs(config, certname)
      pending_jobs.each do |job|
        started_at = Time.now.utc.iso8601
        result = OpenVoxInventory.execute_update_job(job, config)
        finished_at = Time.now.utc.iso8601
        OpenVoxInventory.submit_update_job_result(
          config, certname, job['job_id'], job['target_id'], result, started_at, finished_at
        )
        update_jobs_executed += 1
      end
    end

    {
      'certname' => certname,
      'collector_version' => payload['collector_version'],
      'collected_at' => payload['collected_at'],
      'os_family' => payload.dig('os', 'os_family'),
      'distribution' => payload.dig('os', 'distribution'),
      'os_version' => payload.dig('os', 'os_version'),
      'package_count' => payload['packages'].size,
      'application_count' => payload['applications'].size,
      'website_count' => payload['websites'].size,
      'runtime_count' => payload['runtimes'].size,
      'submitted' => submitted,
      'status_code' => status_code,
      'update_jobs_executed' => update_jobs_executed
    }
  end
end
