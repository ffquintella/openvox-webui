# frozen_string_literal: true

require 'English'
require 'base64'
require 'json'
require 'net/http'
require 'openssl'
require 'rbconfig'
require 'time'
require 'uri'
require 'shellwords'
require 'yaml'

# Pluginsynced to $vardir/lib/puppet_x/openvox_webui/paths.rb alongside this
# file, so resolve it relative to our own location rather than via $LOAD_PATH.
require_relative '../puppet_x/openvox_webui/paths'

module OpenVoxInventory
  # windows?, program_data, config_paths, puppet_conf_paths, puppet_ssl_dirs
  extend PuppetX::OpenVoxWebui::Paths

  module_function

  UNIX_CONFIG_PATHS = PuppetX::OpenVoxWebui::Paths::UNIX_CONFIG_PATHS

  # Kept for backwards compatibility with anything referencing the old constant.
  CONFIG_PATHS = UNIX_CONFIG_PATHS

  def load_config
    config_file = config_paths.find { |path| File.exist?(path) }
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

    puppet_conf_paths.each do |conf_path|
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
                 collect_windows_packages(config)
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
      when 'windows', 'Windows'
        collect_windows_runtimes(config)
      else
        []
      end

    containers = collect_containers(config)

    users = case family
            when 'RedHat', 'Suse', 'Debian'
              collect_linux_users(config)
            when 'Darwin'
              collect_macos_users(config)
            when 'windows', 'Windows'
              collect_windows_users(config)
            else
              []
            end

    repositories = case family
                   when 'RedHat', 'Suse'
                     collect_yum_repos(os_fact)
                   when 'Debian'
                     collect_apt_repos
                   when 'windows', 'Windows'
                     collect_windows_repos
                   else
                     []
                   end

    payload = {
      'collector_version' => 'phase10.4-puppet',
      'collected_at' => collected_at,
      'is_full_snapshot' => true,
      'os' => collect_os_inventory(os_fact, collected_at),
      'packages' => trim(normalize_packages(packages), config),
      'applications' => trim(normalize_applications(applications), config),
      'websites' => trim(normalize_websites(websites), config),
      'runtimes' => trim(normalize_runtimes(runtimes), config),
      'containers' => trim(normalize_containers(containers), config),
      'users' => trim(normalize_users(users), config),
      'repositories' => repositories
    }

    payload['os']['update_channel'] ||= infer_update_channel(payload)
    payload
  end

  def collect_os_inventory(os_fact, collected_at)
    windows = %w[windows Windows].include?(os_fact['family'])
    build = windows ? collect_windows_build_info : {}

    {
      'os_family' => os_fact['family'] || 'Unknown',
      'distribution' => os_fact['name'] || 'Unknown',
      'edition' => windows ? windows_edition(os_fact, build) : (os_fact['distro'] && os_fact['distro']['description']),
      'architecture' => Facter.value(:architecture),
      'kernel_version' => Facter.value(:kernelrelease),
      'os_version' => os_fact.dig('release', 'full') || os_fact.dig('release', 'major') || 'Unknown',
      'patch_level' => windows ? windows_patch_level(os_fact, build) : os_fact.dig('release', 'minor'),
      'package_manager' => detect_package_manager(os_fact),
      'update_channel' => windows ? blank_to_nil(build['display_version']) : nil,
      'last_inventory_at' => collected_at,
      'last_successful_update_at' => detect_last_successful_update(os_fact)
    }
  end

  # Windows has no distro description; the product name is the closest analogue.
  def windows_edition(os_fact, build)
    blank_to_nil(os_fact.dig('windows', 'product_name')) ||
      blank_to_nil(build['product_name']) ||
      blank_to_nil(os_fact.dig('windows', 'edition_id')) ||
      blank_to_nil(build['edition_id'])
  end

  # On Windows the meaningful "patch level" is the Update Build Revision, e.g.
  # 10.0.20348.2582 -> "2582". Fall back to the release minor when unavailable.
  def windows_patch_level(os_fact, build)
    ubr = build['ubr']
    return ubr.to_s unless blankish?(ubr)

    full = os_fact.dig('release', 'full').to_s
    revision = full.split('.')[3]
    blank_to_nil(revision) || os_fact.dig('release', 'minor')
  end

  def collect_windows_build_info
    script = <<~'POWERSHELL'
      $key = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
      $v = Get-ItemProperty -Path $key -ErrorAction SilentlyContinue
      [pscustomobject]@{
        product_name    = $v.ProductName
        edition_id      = $v.EditionID
        display_version = if ($v.DisplayVersion) { $v.DisplayVersion } else { $v.ReleaseId }
        ubr             = $v.UBR
        build           = $v.CurrentBuildNumber
      } | ConvertTo-Json -Compress
    POWERSHELL

    output = run_powershell(script)
    return {} if blankish?(output)

    parsed = JSON.parse(output)
    parsed.is_a?(Hash) ? parsed : {}
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to read Windows build info: #{e.message}")
    {}
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
      detect_windows_package_manager
    else
      nil
    end
  end

  # Report what is actually installed rather than assuming Chocolatey. Many
  # Windows Servers only ever get patched through Windows Update.
  def detect_windows_package_manager
    return 'choco' if command_available?('choco')
    return 'winget' if command_available?('winget')

    'windowsupdate'
  end

  def detect_last_successful_update(os_fact)
    family = os_fact['family']
    timestamp = case family
                when 'RedHat'
                  detect_last_update_rpm
                when 'Debian'
                  detect_last_update_apt
                when 'Suse'
                  detect_last_update_zypper
                when 'windows', 'Windows'
                  detect_last_update_windows
                else
                  nil
                end
    timestamp&.utc&.iso8601
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to detect last update time: #{e.message}")
    nil
  end

  def detect_last_update_rpm
    # Try dnf first, fall back to yum
    output = run_command('dnf history list 2>/dev/null') ||
             run_command('yum history list all 2>/dev/null')
    return nil if output.nil? || output.empty?

    timestamps = []
    output.each_line do |line|
      timestamp = parse_rpm_history_timestamp(line)
      timestamps << timestamp if timestamp
    end
    timestamps.max
  rescue StandardError
    nil
  end

  def parse_rpm_history_timestamp(line)
    parts = line.split('|').map(&:strip)
    return nil if parts.length < 4

    command = parts[1]
    timestamp = parts[2]
    actions = parts[3]

    return nil unless rpm_history_update_transaction?(command, actions)

    Time.parse(timestamp)
  rescue StandardError
    nil
  end

  def rpm_history_update_transaction?(command, actions)
    combined = [command, actions].compact.join(' ')

    return true if combined.match?(/\b(update|upgrade|distrosync|distro-sync|security)\b/i)

    action_codes = actions.to_s.upcase.scan(/[A-Z]+/)
    update_codes = %w[U UPGRADE UPDATE]
    install_codes = %w[I INSTALL]
    erase_codes = %w[E ERASE REMOVE]

    (action_codes & update_codes).any? ||
      ((action_codes & install_codes).any? && (action_codes & erase_codes).any?)
  end

  def detect_last_update_apt
    # Check /var/log/apt/history.log for the latest End-Date of an upgrade
    last_date = nil

    if File.exist?('/var/log/apt/history.log')
      File.readlines('/var/log/apt/history.log').each do |line|
        if line =~ /^End-Date:\s+(.+)/
          begin
            last_date = Time.parse(Regexp.last_match(1).strip)
          rescue StandardError
            nil
          end
        end
      end
    end

    # Fallback: check dpkg.log for last configure/install action
    if last_date.nil? && File.exist?('/var/log/dpkg.log')
      File.readlines('/var/log/dpkg.log').reverse_each do |line|
        if line =~ /^(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})\s+(configure|install)/
          begin
            last_date = Time.parse(Regexp.last_match(1))
            break
          rescue StandardError
            nil
          end
        end
      end
    end

    last_date
  rescue StandardError
    nil
  end

  def detect_last_update_zypper
    # /var/log/zypp/history format: "timestamp|action|..."
    return nil unless File.exist?('/var/log/zypp/history')

    last_date = nil
    File.readlines('/var/log/zypp/history').each do |line|
      next if line.start_with?('#')
      parts = line.split('|')
      next if parts.length < 2
      action = parts[1].to_s.strip.downcase
      next unless action =~ /install|update/
      begin
        last_date = Time.parse(parts[0].strip)
      rescue StandardError
        nil
      end
    end
    last_date
  rescue StandardError
    nil
  end

  # Prefer the Windows Update agent history (covers cumulative updates that
  # never show up as hotfixes) and fall back to Win32_QuickFixEngineering.
  def detect_last_update_windows
    script = <<~'POWERSHELL'
      $result = $null
      try {
        $session = New-Object -ComObject 'Microsoft.Update.Session'
        $searcher = $session.CreateUpdateSearcher()
        $count = $searcher.GetTotalHistoryCount()
        if ($count -gt 0) {
          $latest = $null
          foreach ($entry in $searcher.QueryHistory(0, $count)) {
            # Operation 1 = Installation, ResultCode 2/3 = Succeeded(WithErrors)
            if ($entry.Operation -eq 1 -and ($entry.ResultCode -eq 2 -or $entry.ResultCode -eq 3)) {
              if ($null -eq $latest -or $entry.Date -gt $latest) { $latest = $entry.Date }
            }
          }
          # IUpdateHistoryEntry.Date is already UTC.
          if ($latest) { $result = $latest.ToString('yyyy-MM-ddTHH:mm:ss') + 'Z' }
        }
      } catch { }
      if (-not $result) {
        $hotfix = Get-CimInstance -ClassName Win32_QuickFixEngineering -ErrorAction SilentlyContinue |
          Where-Object { $_.InstalledOn } |
          Sort-Object InstalledOn -Descending |
          Select-Object -First 1
        if ($hotfix) {
          $result = ([datetime]$hotfix.InstalledOn).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss') + 'Z'
        }
      }
      if ($result) { Write-Output $result }
    POWERSHELL

    output = run_powershell(script)
    return nil if blankish?(output)

    Time.parse(output.strip)
  rescue StandardError
    nil
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

  # Windows package managers are the analogue of rpm/dpkg here: things installed
  # from a known source with a resolvable upstream version. Everything else
  # surfaces through collect_windows_applications (the uninstall registry).
  def collect_windows_packages(config)
    trim(collect_chocolatey_packages + collect_winget_packages, config)
  end

  def collect_chocolatey_packages
    return [] unless command_available?('choco')

    # Chocolatey 2.x removed --local-only (it lists local packages by default),
    # so try the 1.x form first and fall back.
    output = run_command('choco list --local-only --limit-output 2>NUL') ||
             run_command('choco list --limit-output 2>NUL')
    return [] if blankish?(output)

    architecture = Facter.value(:architecture)

    output.each_line.map do |line|
      name, version = line.strip.split('|', 2)
      next if blankish?(name) || blankish?(version)

      {
        'name' => name,
        'epoch' => nil,
        'version' => version,
        'release' => nil,
        'architecture' => architecture,
        'repository_source' => 'chocolatey',
        'install_path' => nil,
        'install_time' => nil
      }
    end.compact
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Chocolatey packages: #{e.message}")
    []
  end

  # `winget export` emits machine-readable JSON, unlike `winget list` which is a
  # fixed-width table. Note that winget is a per-user MSIX app and is frequently
  # unavailable when the Puppet agent runs as LocalSystem; that degrades to an
  # empty list rather than an error.
  def collect_winget_packages
    return [] unless command_available?('winget')

    script = <<~'POWERSHELL'
      $tmp = Join-Path $env:TEMP ('openvox-winget-' + [guid]::NewGuid().ToString() + '.json')
      try {
        $null = & winget export --output $tmp --include-versions --accept-source-agreements --disable-interactivity
        if (Test-Path $tmp) { Get-Content -Raw -Path $tmp }
      } catch {
      } finally {
        if (Test-Path $tmp) { Remove-Item -Force -Path $tmp -ErrorAction SilentlyContinue }
      }
    POWERSHELL

    output = run_powershell(script)
    return [] if blankish?(output)

    architecture = Facter.value(:architecture)
    data = JSON.parse(output)

    Array(data['Sources']).flat_map do |source|
      source_name = blank_to_nil(source.dig('SourceDetails', 'Name')) || 'winget'

      Array(source['Packages']).map do |pkg|
        name = pkg['PackageIdentifier']
        next if blankish?(name)

        {
          'name' => name,
          'epoch' => nil,
          'version' => blank_to_nil(pkg['Version']) || 'unknown',
          'release' => nil,
          'architecture' => architecture,
          'repository_source' => source_name,
          'install_path' => nil,
          'install_time' => nil
        }
      end.compact
    end
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect winget packages: #{e.message}")
    []
  end

  # ---- Repository configuration collection ----

  def collect_yum_repos(os_fact)
    repos = []
    repo_dir = '/etc/yum.repos.d'
    return repos unless Dir.exist?(repo_dir)

    releasever = os_fact.dig('release', 'major') || ''
    basearch = Facter.value(:architecture) || ''

    Dir.glob(File.join(repo_dir, '*.repo')).each do |repo_file|
      parse_yum_repo_file(repo_file, releasever, basearch, repos)
    end

    repos
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect YUM repos: #{e.message}")
    []
  end

  def parse_yum_repo_file(path, releasever, basearch, repos)
    current = nil

    File.readlines(path).each do |line|
      line = line.strip
      next if line.empty? || line.start_with?('#')

      if line =~ /^\[(.+)\]$/
        # Save previous section if it exists
        repos << current if current && current['enabled']
        current = {
          'repo_id' => Regexp.last_match(1),
          'repo_name' => nil,
          'repo_type' => 'yum',
          'base_url' => nil,
          'mirror_list_url' => nil,
          'distribution_path' => nil,
          'components' => nil,
          'architectures' => basearch,
          'enabled' => true,
          'gpg_check' => nil
        }
      elsif current
        key, value = line.split('=', 2).map(&:strip)
        next unless key && value

        value = substitute_yum_vars(value, releasever, basearch)

        case key
        when 'name'
          current['repo_name'] = value
        when 'baseurl'
          current['base_url'] = value.split(/\s+/).first
        when 'mirrorlist', 'metalink'
          current['mirror_list_url'] = value
        when 'enabled'
          current['enabled'] = value != '0'
        when 'gpgcheck'
          current['gpg_check'] = value != '0'
        end
      end
    end

    repos << current if current && current['enabled']
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to parse #{path}: #{e.message}")
  end

  def substitute_yum_vars(value, releasever, basearch)
    value.gsub('$releasever', releasever)
         .gsub('$basearch', basearch)
  end

  def collect_apt_repos
    repos = []

    # Parse sources.list
    sources_list = '/etc/apt/sources.list'
    parse_apt_sources_list(sources_list, repos) if File.exist?(sources_list)

    # Parse sources.list.d/*.list
    list_dir = '/etc/apt/sources.list.d'
    if Dir.exist?(list_dir)
      Dir.glob(File.join(list_dir, '*.list')).each do |f|
        parse_apt_sources_list(f, repos)
      end

      # Parse DEB822 .sources files
      Dir.glob(File.join(list_dir, '*.sources')).each do |f|
        parse_apt_deb822_sources(f, repos)
      end
    end

    repos
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect APT repos: #{e.message}")
    []
  end

  def parse_apt_sources_list(path, repos)
    File.readlines(path).each do |line|
      line = line.strip
      next if line.empty? || line.start_with?('#')

      # Format: deb [options] URI distribution [component...]
      next unless line =~ /^deb\s+/

      # Strip 'deb' prefix and optional [options]
      remainder = line.sub(/^deb\s+/, '')
      remainder = remainder.sub(/\[.*?\]\s*/, '')

      parts = remainder.split(/\s+/)
      next if parts.size < 2

      uri = parts[0]
      distribution = parts[1]
      components = parts[2..].join(' ')

      repo_id = "#{uri}_#{distribution}".gsub(%r{[^a-zA-Z0-9._-]}, '_')

      repos << {
        'repo_id' => repo_id,
        'repo_name' => nil,
        'repo_type' => 'apt',
        'base_url' => uri,
        'mirror_list_url' => nil,
        'distribution_path' => distribution,
        'components' => components.empty? ? nil : components,
        'architectures' => Facter.value(:architecture),
        'enabled' => true,
        'gpg_check' => nil
      }
    end
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to parse #{path}: #{e.message}")
  end

  def parse_apt_deb822_sources(path, repos)
    # DEB822 format: blocks separated by blank lines
    current = {}

    File.readlines(path).each do |line|
      line = line.rstrip

      if line.empty?
        add_deb822_entry(current, repos) unless current.empty?
        current = {}
        next
      end

      next if line.start_with?('#')

      if line =~ /^(\S+):\s*(.*)/
        current[Regexp.last_match(1)] = Regexp.last_match(2).strip
      end
    end

    add_deb822_entry(current, repos) unless current.empty?
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to parse DEB822 #{path}: #{e.message}")
  end

  def add_deb822_entry(entry, repos)
    types = (entry['Types'] || '').split
    return unless types.include?('deb')

    uris = (entry['URIs'] || '').split
    suites = (entry['Suites'] || '').split
    components = (entry['Components'] || '').strip

    uris.each do |uri|
      suites.each do |suite|
        repo_id = "#{uri}_#{suite}".gsub(%r{[^a-zA-Z0-9._-]}, '_')
        repos << {
          'repo_id' => repo_id,
          'repo_name' => nil,
          'repo_type' => 'apt',
          'base_url' => uri,
          'mirror_list_url' => nil,
          'distribution_path' => suite,
          'components' => components.empty? ? nil : components,
          'architectures' => entry['Architectures'] || Facter.value(:architecture),
          'enabled' => (entry['Enabled'] || 'yes') != 'no',
          'gpg_check' => nil
        }
      end
    end
  end

  def collect_windows_repos
    collect_winget_repos + collect_chocolatey_repos
  end

  def collect_winget_repos
    return [] unless command_available?('winget')

    repos = parse_winget_source_export || parse_winget_source_table
    repos || []
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect winget repos: #{e.message}")
    []
  end

  # winget >= 1.6 can emit its source list as JSON, which avoids guessing at
  # column widths in the localised table output.
  def parse_winget_source_export
    output = run_powershell('& winget source export --disable-interactivity')
    return nil if blankish?(output)

    json = output[/\[.*\]/m] || output[/\{.*\}/m]
    return nil if json.nil?

    parsed = JSON.parse(json)
    parsed = [parsed] if parsed.is_a?(Hash)
    architecture = Facter.value(:architecture)

    parsed.map do |source|
      name = blank_to_nil(source['Name'])
      next unless name

      winget_repo_entry(name, blank_to_nil(source['Arg']) || blank_to_nil(source['Argument']), architecture)
    end.compact
  rescue StandardError
    nil
  end

  def parse_winget_source_table
    output = run_command('winget source list 2>NUL')
    return [] if blankish?(output)

    architecture = Facter.value(:architecture)
    data_started = false

    output.lines.map(&:strip).reject(&:empty?).map do |line|
      if line.start_with?('-')
        data_started = true
        next
      end
      next unless data_started

      parts = line.split(/\s{2,}/)
      next if parts.size < 2

      winget_repo_entry(parts[0].strip, parts[1].strip, architecture)
    end.compact
  end

  def winget_repo_entry(name, url, architecture)
    {
      'repo_id' => name,
      'repo_name' => name,
      'repo_type' => 'winget',
      'base_url' => url,
      'mirror_list_url' => nil,
      'distribution_path' => nil,
      'components' => nil,
      'architectures' => architecture,
      'enabled' => true,
      'gpg_check' => nil
    }
  end

  def collect_chocolatey_repos
    return [] unless command_available?('choco')

    output = run_command('choco source list --limit-output 2>NUL')
    return [] if blankish?(output)

    architecture = Facter.value(:architecture)

    output.each_line.map do |line|
      # name|url|disabled|username|certificate|priority|bypassproxy|...
      parts = line.strip.split('|')
      next if parts.size < 2 || blankish?(parts[0])

      {
        'repo_id' => parts[0],
        'repo_name' => parts[0],
        'repo_type' => 'chocolatey',
        'base_url' => blank_to_nil(parts[1]),
        'mirror_list_url' => nil,
        'distribution_path' => nil,
        'components' => nil,
        'architectures' => architecture,
        'enabled' => parts[2].to_s.strip.downcase != 'true',
        'gpg_check' => nil
      }
    end.compact
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Chocolatey sources: #{e.message}")
    []
  end

  # ---- End repository collection ----

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
    # The Puppet agent runs as LocalSystem, so HKCU is the service profile and
    # is effectively always empty. Walking the loaded HKEY_USERS hives is what
    # actually surfaces per-user installs (Chrome, Teams, VS Code, ...).
    script = <<~'POWERSHELL'
      $arch = $env:PROCESSOR_ARCHITEW6432
      if (-not $arch) { $arch = $env:PROCESSOR_ARCHITECTURE }
      if ($arch -eq 'AMD64') { $arch = 'x64' }
      $suffix = 'Microsoft\Windows\CurrentVersion\Uninstall\*'
      $targets = [System.Collections.ArrayList]::new()
      [void]$targets.Add(@{ Path = "HKLM:\Software\$suffix"; Scope = 'system'; Arch = $arch })
      [void]$targets.Add(@{ Path = "HKLM:\Software\WOW6432Node\$suffix"; Scope = 'system'; Arch = 'x86' })
      Get-ChildItem -Path 'Registry::HKEY_USERS' -ErrorAction SilentlyContinue | ForEach-Object {
        $sid = $_.PSChildName
        if ($sid -match '^S-1-5-21-[\d-]+$') {
          [void]$targets.Add(@{ Path = "Registry::HKEY_USERS\$sid\Software\$suffix"; Scope = 'user'; Arch = $arch })
          [void]$targets.Add(@{ Path = "Registry::HKEY_USERS\$sid\Software\WOW6432Node\$suffix"; Scope = 'user'; Arch = 'x86' })
        }
      }
      $updateTypes = @('Security Update', 'Update Rollup', 'Hotfix', 'ServicePack')
      $apps = foreach ($entry in $targets) {
        Get-ItemProperty -Path $entry.Path -ErrorAction SilentlyContinue |
          Where-Object {
            $_.DisplayName -and $_.DisplayVersion -and
            $_.SystemComponent -ne 1 -and
            -not $_.ParentKeyName -and
            ($updateTypes -notcontains $_.ReleaseType)
          } |
          ForEach-Object {
            [pscustomobject]@{
              DisplayName = $_.DisplayName
              Publisher = $_.Publisher
              DisplayVersion = $_.DisplayVersion
              InstallDate = $_.InstallDate
              UninstallString = $_.UninstallString
              InstallLocation = $_.InstallLocation
              Scope = $entry.Scope
              Architecture = $entry.Arch
            }
          }
      }
      $apps | ConvertTo-Json -Depth 4
    POWERSHELL
    output = run_powershell(script)
    return [] if blankish?(output)

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
    script = <<~'POWERSHELL'
      Import-Module WebAdministration -ErrorAction SilentlyContinue
      if (Get-Command Get-Website -ErrorAction SilentlyContinue) {
        $sites = @(Get-Website | ForEach-Object {
          $certs = @($_.Bindings.Collection | Where-Object { $_.protocol -eq 'https' } | ForEach-Object { $_.certificateHash })
          [pscustomobject]@{
            site_name = $_.Name
            bindings = @($_.Bindings.Collection | ForEach-Object { $_.bindingInformation })
            document_root = $_.PhysicalPath
            app_pool = $_.applicationPool
            state = $_.State
            certs = $certs
          }
        })
        if ($sites.Count -gt 0) { $sites | ConvertTo-Json -Depth 5 }
      }
    POWERSHELL
    output = run_powershell(script)
    return [] if blankish?(output)

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
          'certificate_hashes' => Array(site['certs']).compact,
          'state' => blank_to_nil(site['state'])
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

  def collect_windows_runtimes(config)
    trim(collect_iis_app_pools + collect_windows_framework_runtimes, config)
  end

  def collect_iis_app_pools
    script = <<~'POWERSHELL'
      Import-Module WebAdministration -ErrorAction SilentlyContinue
      if (Test-Path 'IIS:\AppPools') {
        $sites = @()
        if (Get-Command Get-Website -ErrorAction SilentlyContinue) { $sites = @(Get-Website) }
        $apps = @()
        if (Get-Command Get-WebApplication -ErrorAction SilentlyContinue) { $apps = @(Get-WebApplication) }
        $pools = @(Get-ChildItem 'IIS:\AppPools' -ErrorAction SilentlyContinue | ForEach-Object {
          $pool = $_
          $deployed = @(
            ($sites | Where-Object { $_.applicationPool -eq $pool.Name } | ForEach-Object { $_.Name }) +
            ($apps  | Where-Object { $_.applicationPool -eq $pool.Name } | ForEach-Object { $_.path })
          )
          [pscustomobject]@{
            name = $pool.Name
            runtime_version = $pool.managedRuntimeVersion
            pipeline_mode = [string]$pool.managedPipelineMode
            state = [string]$pool.state
            enable_32bit = [bool]$pool.enable32BitAppOnWin64
            identity = [string]$pool.processModel.identityType
            deployed = $deployed
          }
        })
        if ($pools.Count -gt 0) { $pools | ConvertTo-Json -Depth 4 }
      }
    POWERSHELL

    output = run_powershell(script)
    return [] if blankish?(output)

    parsed = JSON.parse(output)
    parsed = [parsed] if parsed.is_a?(Hash)

    parsed.map do |pool|
      next if blankish?(pool['name'])

      # An empty managedRuntimeVersion means "No Managed Code" in IIS.
      version = blank_to_nil(pool['runtime_version']) || 'nomanagedcode'

      {
        'runtime_type' => 'iis_app_pool',
        'runtime_name' => pool['name'],
        'runtime_version' => version,
        'install_path' => nil,
        'management_endpoint' => nil,
        'deployed_units' => Array(pool['deployed']),
        'metadata' => compact_hash({
          'pipeline_mode' => blank_to_nil(pool['pipeline_mode']),
          'state' => blank_to_nil(pool['state']),
          'identity_type' => blank_to_nil(pool['identity']),
          'enable_32bit' => pool['enable_32bit'].to_s
        })
      }
    end.compact
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect IIS application pools: #{e.message}")
    []
  end

  def collect_windows_framework_runtimes
    script = <<~'POWERSHELL'
      $result = [System.Collections.ArrayList]::new()
      $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
      if ($dotnet) {
        & $dotnet.Source --list-runtimes 2>$null | ForEach-Object {
          if ($_ -match '^(\S+)\s+(\S+)\s+\[(.+)\]$') {
            [void]$result.Add([pscustomobject]@{ type = 'dotnet'; name = $Matches[1]; version = $Matches[2]; path = $Matches[3] })
          }
        }
      }
      $ndp = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full' -ErrorAction SilentlyContinue
      if ($ndp -and $ndp.Version) {
        [void]$result.Add([pscustomobject]@{ type = 'dotnet_framework'; name = '.NET Framework'; version = $ndp.Version; path = $null })
      }
      foreach ($root in @('HKLM:\SOFTWARE\JavaSoft', 'HKLM:\SOFTWARE\WOW6432Node\JavaSoft')) {
        foreach ($kind in @('JDK', 'JRE', 'Java Development Kit', 'Java Runtime Environment')) {
          Get-ChildItem -Path (Join-Path $root $kind) -ErrorAction SilentlyContinue | ForEach-Object {
            $javaHome = (Get-ItemProperty -Path $_.PSPath -ErrorAction SilentlyContinue).JavaHome
            [void]$result.Add([pscustomobject]@{ type = 'java'; name = $kind; version = $_.PSChildName; path = $javaHome })
          }
        }
      }
      if ($result.Count -gt 0) { $result | ConvertTo-Json -Depth 4 }
    POWERSHELL

    output = run_powershell(script)
    return [] if blankish?(output)

    parsed = JSON.parse(output)
    parsed = [parsed] if parsed.is_a?(Hash)

    parsed.map do |entry|
      name = blank_to_nil(entry['name'])
      next unless name

      {
        'runtime_type' => blank_to_nil(entry['type']) || 'unknown',
        'runtime_name' => name,
        'runtime_version' => blank_to_nil(entry['version']) || 'unknown',
        'install_path' => blank_to_nil(entry['path']),
        'management_endpoint' => nil,
        'deployed_units' => [],
        'metadata' => nil
      }
    end.compact
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Windows framework runtimes: #{e.message}")
    []
  end

  def collect_containers(config)
    containers = []

    # Docker
    if command_available?('docker')
      begin
        output = run_command('docker ps -a --no-trunc --format "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.CreatedAt}}\t{{.Ports}}\t{{.Mounts}}"')
        if output
          output.each_line do |line|
            parts = line.strip.split("\t", 7)
            next if parts.length < 4
            containers << {
              'container_id' => parts[0].to_s.strip,
              'name' => parts[1].to_s.strip,
              'image' => parts[2].to_s.strip,
              'status' => parse_container_status(parts[3].to_s.strip),
              'status_detail' => parts[3].to_s.strip,
              'created_at' => parts[4].to_s.strip.empty? ? nil : parts[4].to_s.strip,
              'ports' => parts[5].to_s.strip.split(',').map(&:strip).reject(&:empty?),
              'mounts' => parts[6].to_s.strip.split(',').map(&:strip).reject(&:empty?),
              'runtime_type' => 'docker'
            }
          end
        end

        # Docker runtime info
        version = run_command('docker version --format "{{.Server.Version}}"')&.strip
        if version && !version.empty?
          containers << {
            'container_id' => '_runtime_docker',
            'name' => 'docker-engine',
            'image' => '',
            'status' => 'runtime',
            'status_detail' => "Docker Engine #{version}",
            'ports' => [],
            'mounts' => [],
            'runtime_type' => 'docker'
          }
        end
      rescue => e
        # Docker not accessible, skip silently
      end
    end

    # Podman
    if command_available?('podman')
      begin
        output = run_command('podman ps -a --no-trunc --format "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.CreatedAt}}\t{{.Ports}}\t{{.Mounts}}"')
        if output
          output.each_line do |line|
            parts = line.strip.split("\t", 7)
            next if parts.length < 4
            containers << {
              'container_id' => parts[0].to_s.strip,
              'name' => parts[1].to_s.strip,
              'image' => parts[2].to_s.strip,
              'status' => parse_container_status(parts[3].to_s.strip),
              'status_detail' => parts[3].to_s.strip,
              'created_at' => parts[4].to_s.strip.empty? ? nil : parts[4].to_s.strip,
              'ports' => parts[5].to_s.strip.split(',').map(&:strip).reject(&:empty?),
              'mounts' => parts[6].to_s.strip.split(',').map(&:strip).reject(&:empty?),
              'runtime_type' => 'podman'
            }
          end
        end

        # Podman runtime info
        version = run_command('podman version --format "{{.Version}}"')&.strip
        if version && !version.empty?
          containers << {
            'container_id' => '_runtime_podman',
            'name' => 'podman-engine',
            'image' => '',
            'status' => 'runtime',
            'status_detail' => "Podman #{version}",
            'ports' => [],
            'mounts' => [],
            'runtime_type' => 'podman'
          }
        end
      rescue => e
        # Podman not accessible, skip silently
      end
    end

    containers
  end

  def parse_container_status(raw)
    return 'unknown' if raw.nil? || raw.empty?
    word = raw.split(/\s+/).first.to_s.downcase
    case word
    when 'up' then 'running'
    when 'exited' then 'exited'
    when 'created' then 'created'
    when 'paused' then 'paused'
    when 'restarting' then 'restarting'
    when 'removing' then 'removing'
    when 'dead' then 'dead'
    else 'unknown'
    end
  end

  def collect_linux_users(config)
    users = []
    output = run_command('getent passwd')
    return users unless output

    output.each_line do |line|
      parts = line.strip.split(':', 7)
      next if parts.length < 7
      username = parts[0]
      uid = parts[2].to_i
      gid = parts[3].to_i
      gecos = parts[4]
      home = parts[5]
      shell = parts[6]

      users << {
        'username' => username,
        'uid' => uid,
        'gid' => gid,
        'home_directory' => home,
        'shell' => shell,
        'user_type' => classify_user_type(uid, shell),
        'groups' => get_user_groups(username),
        'last_login' => get_last_login(username),
        'locked' => user_locked?(username),
        'gecos' => (gecos.nil? || gecos.strip.empty?) ? nil : gecos.strip,
        'sid' => nil
      }
    end

    users
  end

  def collect_macos_users(config)
    users = []
    output = run_command('dscl . list /Users UniqueID')
    return users unless output

    output.each_line do |line|
      parts = line.strip.split(/\s+/, 2)
      next if parts.length < 2
      username = parts[0]
      uid = parts[1].to_i
      next if username.start_with?('_') # Skip system daemon accounts

      # Get additional details
      details = run_command("dscl . read '/Users/#{username}' PrimaryGroupID NFSHomeDirectory UserShell RealName 2>/dev/null")
      gid = nil
      home = nil
      shell = nil
      gecos = nil

      if details
        gid = $1.to_i if details =~ /PrimaryGroupID:\s*(\d+)/
        home = $1.strip if details =~ /NFSHomeDirectory:\s*(.+)/
        shell = $1.strip if details =~ /UserShell:\s*(.+)/
        gecos = $1.strip if details =~ /RealName:\s*\n\s*(.+)/
      end

      users << {
        'username' => username,
        'uid' => uid,
        'gid' => gid,
        'home_directory' => home,
        'shell' => shell,
        'user_type' => classify_user_type(uid, shell),
        'groups' => get_user_groups(username),
        'last_login' => nil,
        'locked' => nil,
        'gecos' => gecos,
        'sid' => nil
      }
    end

    users
  end

  def collect_windows_users(_config)
    users = collect_windows_local_users
    users = collect_windows_cim_users if users.empty?
    users
  end

  def collect_windows_local_users
    script = <<~'POWERSHELL'
      if (-not (Get-Command Get-LocalUser -ErrorAction SilentlyContinue)) { return }
      $groupMap = @{}
      Get-LocalGroup -ErrorAction SilentlyContinue | ForEach-Object {
        $groupName = $_.Name
        Get-LocalGroupMember -Group $groupName -ErrorAction SilentlyContinue | ForEach-Object {
          $memberSid = $_.SID.Value
          if ($memberSid) {
            if (-not $groupMap.ContainsKey($memberSid)) { $groupMap[$memberSid] = [System.Collections.ArrayList]::new() }
            [void]$groupMap[$memberSid].Add($groupName)
          }
        }
      }
      $profiles = @{}
      Get-CimInstance -ClassName Win32_UserProfile -ErrorAction SilentlyContinue | ForEach-Object {
        if ($_.SID) { $profiles[$_.SID] = $_.LocalPath }
      }
      $users = @(Get-LocalUser -ErrorAction SilentlyContinue | ForEach-Object {
        $sid = $_.SID.Value
        [pscustomobject]@{
          Name = $_.Name
          SID = $sid
          Enabled = [bool]$_.Enabled
          Description = $_.Description
          LastLogon = if ($_.LastLogon) { $_.LastLogon.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss') + 'Z' } else { $null }
          Groups = @($groupMap[$sid])
          ProfilePath = $profiles[$sid]
        }
      })
      if ($users.Count -gt 0) { $users | ConvertTo-Json -Depth 4 }
    POWERSHELL

    parse_windows_users(run_powershell(script))
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Windows local users: #{e.message}")
    []
  end

  # Fallback for hosts without the LocalAccounts PowerShell module.
  def collect_windows_cim_users
    script = <<~'POWERSHELL'
      $users = @(Get-CimInstance -ClassName Win32_UserAccount -Filter 'LocalAccount=True' -ErrorAction SilentlyContinue |
        ForEach-Object {
          [pscustomobject]@{
            Name = $_.Name
            SID = $_.SID
            Enabled = (-not $_.Disabled)
            Description = $_.Description
            LastLogon = $null
            Groups = @()
            ProfilePath = $null
          }
        })
      if ($users.Count -gt 0) { $users | ConvertTo-Json -Depth 4 }
    POWERSHELL

    parse_windows_users(run_powershell(script))
  rescue StandardError => e
    Facter.debug("openvox_inventory: Failed to collect Windows users via CIM: #{e.message}")
    []
  end

  def parse_windows_users(output)
    return [] if blankish?(output)

    parsed = JSON.parse(output)
    parsed = [parsed] if parsed.is_a?(Hash)

    parsed.map do |user|
      username = blank_to_nil(user['Name'])
      next unless username

      sid = user['SID'].is_a?(Hash) ? user['SID']['Value'] : blank_to_nil(user['SID'])
      enabled = user['Enabled']

      {
        'username' => username,
        'uid' => nil,
        'sid' => sid,
        'gid' => nil,
        'home_directory' => blank_to_nil(user['ProfilePath']),
        'shell' => nil,
        'user_type' => classify_windows_user_type(sid),
        'groups' => Array(user['Groups']).compact,
        'last_login' => blank_to_nil(user['LastLogon']),
        'locked' => enabled.nil? ? nil : enabled == false,
        'gecos' => blank_to_nil(user['Description'])
      }
    end.compact
  rescue JSON::ParserError
    []
  end

  # Built-in accounts are identified by their well-known RID, not by whether
  # they happen to be disabled.
  WINDOWS_BUILTIN_RIDS = %w[500 501 502 503 504].freeze

  def classify_windows_user_type(sid)
    rid = sid.to_s.split('-').last
    WINDOWS_BUILTIN_RIDS.include?(rid) ? 'system' : 'regular'
  end

  def classify_user_type(uid, shell)
    nologin_shells = ['/sbin/nologin', '/bin/false', '/usr/sbin/nologin', '/bin/nologin', '/usr/bin/false']
    if uid < 1000
      nologin_shells.include?(shell.to_s) ? 'system' : 'service'
    else
      'regular'
    end
  end

  def get_user_groups(username)
    output = run_command("id -Gn #{Shellwords.escape(username)} 2>/dev/null")
    return [] unless output
    output.strip.split(/\s+/).reject(&:empty?)
  rescue
    []
  end

  def get_last_login(username)
    output = run_command("lastlog -u #{Shellwords.escape(username)} 2>/dev/null")
    return nil unless output
    lines = output.strip.split("\n")
    return nil if lines.length < 2
    line = lines[1]
    return nil if line.include?('**Never logged in**')
    # Extract the timestamp portion (after username and port columns)
    parts = line.split(/\s+/, 4)
    parts.length >= 4 ? parts[3].strip : nil
  rescue
    nil
  end

  def user_locked?(username)
    output = run_command("passwd -S #{Shellwords.escape(username)} 2>/dev/null")
    return nil unless output
    fields = output.strip.split(/\s+/)
    return nil if fields.length < 2
    ['L', 'LK'].include?(fields[1])
  rescue
    nil
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
        ca_file = puppet_ssl_dirs.map { |dir| "#{dir}/certs/ca.pem" }.find { |path| File.exist?(path) }
      end
      http.ca_file = ca_file if ca_file && File.exist?(ca_file)
    end

    ssl_cert = config['ssl_cert']
    ssl_key = config['ssl_key']
    if ssl_cert.nil? || ssl_key.nil?
      ssl_cert ||= puppet_ssl_dirs.map { |dir| "#{dir}/certs/#{certname}.pem" }.find { |path| File.exist?(path) }
      ssl_key ||= puppet_ssl_dirs.map { |dir| "#{dir}/private_keys/#{certname}.pem" }.find { |path| File.exist?(path) }
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
          when 'windows', 'Windows'
            return execute_windows_updates(false)
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
          when 'windows', 'Windows'
            return execute_windows_updates(true)
          else
            return { 'status' => 'failed', 'summary' => "Unsupported OS family: #{family}", 'output' => '' }
          end

    run_update_command(cmd)
  end

  # Drives the Windows Update agent through COM so that no third-party module
  # (PSWindowsUpdate) has to be present on the node.
  def execute_windows_updates(security_only)
    unless windows?
      return { 'status' => 'failed', 'summary' => 'Windows update operations require a Windows host', 'output' => '' }
    end

    script = <<~POWERSHELL
      $ErrorActionPreference = 'Stop'
      $securityOnly = $#{security_only}
      try {
        $session = New-Object -ComObject 'Microsoft.Update.Session'
        $searcher = $session.CreateUpdateSearcher()
        $found = $searcher.Search("IsInstalled=0 and Type='Software' and IsHidden=0")
        $selected = New-Object -ComObject 'Microsoft.Update.UpdateColl'
        foreach ($update in $found.Updates) {
          if ($securityOnly) {
            $isSecurity = $false
            foreach ($category in $update.Categories) {
              if ($category.Name -match 'Security') { $isSecurity = $true }
            }
            if (-not $isSecurity) { continue }
          }
          if (-not $update.EulaAccepted) { $update.AcceptEula() }
          [void]$selected.Add($update)
        }
        if ($selected.Count -eq 0) {
          Write-Output 'No applicable updates found.'
          exit 0
        }
        foreach ($update in $selected) { Write-Output ('Pending: ' + $update.Title) }
        $downloader = $session.CreateUpdateDownloader()
        $downloader.Updates = $selected
        $null = $downloader.Download()
        $installer = $session.CreateUpdateInstaller()
        $installer.Updates = $selected
        $outcome = $installer.Install()
        Write-Output ('ResultCode: ' + $outcome.ResultCode)
        Write-Output ('RebootRequired: ' + $outcome.RebootRequired)
        if ($outcome.ResultCode -eq 2 -or $outcome.ResultCode -eq 3) { exit 0 }
        exit 1
      } catch {
        Write-Output ('ERROR: ' + $_.Exception.Message)
        exit 1
      }
    POWERSHELL

    run_update_command(powershell_command(script))
  end

  def powershell_command(script)
    encoded = Base64.strict_encode64(script.encode('UTF-16LE'))
    %("#{powershell_executable}" #{POWERSHELL_ARGS} #{encoded} 2>&1)
  end

  # Chocolatey is preferred because it works under the LocalSystem account the
  # Puppet agent runs as; winget is a per-user MSIX app and often is not
  # resolvable there.
  def windows_package_tool
    return 'choco' if command_available?('choco')
    return 'winget' if command_available?('winget')

    nil
  end

  WINGET_FLAGS = '--silent --accept-package-agreements --accept-source-agreements --disable-interactivity'

  def execute_windows_package_operation(operation, packages)
    tool = windows_package_tool
    unless tool
      return {
        'status' => 'failed',
        'summary' => 'No supported Windows package manager found (chocolatey or winget)',
        'output' => ''
      }
    end

    if tool == 'choco'
      subcommand = { update: 'upgrade', install: 'install', remove: 'uninstall' }[operation]
      return run_update_command("choco #{subcommand} -y #{packages.join(' ')} 2>&1")
    end

    subcommand = { update: 'upgrade', install: 'install', remove: 'uninstall' }[operation]
    # winget handles one package identifier per invocation.
    results = packages.map do |package|
      run_update_command("winget #{subcommand} --id #{package} --exact #{WINGET_FLAGS} 2>&1")
    end

    merge_update_results(results)
  end

  def merge_update_results(results)
    return { 'status' => 'failed', 'summary' => 'No packages processed', 'output' => '' } if results.empty?

    failed = results.reject { |result| result['status'] == 'succeeded' }
    {
      'status' => failed.empty? ? 'succeeded' : 'failed',
      'summary' => if failed.empty?
                     "#{results.size} package operation(s) completed successfully"
                   else
                     "#{failed.size} of #{results.size} package operation(s) failed"
                   end,
      'output' => results.map { |result| result['output'] }.join("\n").slice(0, 10_000)
    }
  end

  def sanitize_package_names(packages)
    packages.select { |p| p.is_a?(String) && p.match?(/\A[a-zA-Z0-9._+\-:]+\z/) }
  end

  def execute_package_update(family, packages, _config)
    safe_packages = sanitize_package_names(packages)
    return { 'status' => 'failed', 'summary' => 'No valid packages specified', 'output' => '' } if safe_packages.empty?

    return execute_windows_package_operation(:update, safe_packages) if %w[windows Windows].include?(family)

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

    return execute_windows_package_operation(:install, safe_packages) if %w[windows Windows].include?(family)

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

    return execute_windows_package_operation(:remove, safe_packages) if %w[windows Windows].include?(family)

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

  def normalize_containers(items)
    items.map do |item|
      next unless present?(item['container_id']) && present?(item['name'])
      item['ports'] = Array(item['ports']).map(&:to_s).reject(&:empty?).uniq
      item['mounts'] = Array(item['mounts']).map(&:to_s).reject(&:empty?).uniq
      compact_hash(item)
    end.compact.uniq { |item| [item['runtime_type'], item['container_id']] }
  end

  def normalize_users(items)
    items.map do |item|
      next unless present?(item['username'])
      item['groups'] = Array(item['groups']).map(&:to_s).reject(&:empty?).uniq.sort
      compact_hash(item)
    end.compact.uniq { |item| [item['username'], item['uid']] }
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
    if windows?
      system("where #{command} >NUL 2>NUL")
    else
      system("command -v #{command} >/dev/null 2>&1")
    end
  end

  POWERSHELL_ARGS = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand'

  def powershell_executable
    return @powershell_executable if defined?(@powershell_executable)

    system_root = ENV['SystemRoot'] || 'C:/Windows'
    # Sysnative only resolves from a 32-bit process on 64-bit Windows, and it is
    # checked first on purpose: a 32-bit Ruby launching the System32 (i.e.
    # SysWOW64) PowerShell would read the WOW6432Node registry view and miss
    # every 64-bit application.
    candidates = [
      File.join(system_root, 'Sysnative', 'WindowsPowerShell', 'v1.0', 'powershell.exe'),
      File.join(system_root, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe')
    ]

    @powershell_executable = candidates.find { |path| File.exist?(path) } || 'powershell.exe'
  end

  # Scripts are passed as base64-encoded UTF-16LE so that quoting, newlines and
  # backslashes survive the trip through cmd.exe untouched. Exit status is not
  # checked because native tools invoked from the script (winget, choco) return
  # non-zero for warnings; callers validate the payload instead.
  def run_powershell(script)
    return nil unless windows?

    encoded = Base64.strict_encode64(script.encode('UTF-16LE'))
    output = `"#{powershell_executable}" #{POWERSHELL_ARGS} #{encoded} 2>NUL`
    blankish?(output) ? nil : output
  rescue StandardError => e
    Facter.debug("openvox_inventory: PowerShell invocation failed: #{e.message}")
    nil
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
      'container_count' => payload['containers'].size,
      'user_count' => payload['users'].size,
      'submitted' => submitted,
      'status_code' => status_code,
      'update_jobs_executed' => update_jobs_executed
    }
  end
end
