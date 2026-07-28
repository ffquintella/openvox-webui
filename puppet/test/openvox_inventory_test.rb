# frozen_string_literal: true

require 'minitest/autorun'
require 'json'

$LOAD_PATH.unshift(File.expand_path('support', __dir__))
require 'facter'

require_relative '../lib/facter/openvox_inventory'

class OpenVoxInventoryTest < Minitest::Test
  def with_stubbed_run_command(result)
    original = OpenVoxInventory.method(:run_command)
    OpenVoxInventory.define_singleton_method(:run_command) do |_command|
      result
    end
    yield
  ensure
    OpenVoxInventory.define_singleton_method(:run_command, original)
  end

  # Replaces module methods for the duration of the block. Values that respond
  # to #call are invoked with the original arguments, everything else is
  # returned as-is.
  def with_stubs(stubs)
    originals = {}
    stubs.each do |name, value|
      originals[name] = OpenVoxInventory.method(name)
      callable = value.respond_to?(:call) ? value : ->(*_args) { value }
      OpenVoxInventory.define_singleton_method(name) { |*args| callable.call(*args) }
    end
    yield
  ensure
    originals.each { |name, original| OpenVoxInventory.define_singleton_method(name, original) }
  end

  def with_facter_values(values)
    original = Facter.method(:value)
    Facter.define_singleton_method(:value) { |name| values[name] }
    yield
  ensure
    Facter.define_singleton_method(:value, original)
  end

  def test_normalize_packages_filters_invalid_and_deduplicates
    items = [
      { 'name' => 'httpd', 'version' => '2.4.62', 'release' => '1.el9', 'architecture' => 'x86_64' },
      { 'name' => 'httpd', 'version' => '2.4.62', 'release' => '1.el9', 'architecture' => 'x86_64' },
      { 'name' => 'curl', 'version' => '', 'release' => '1.el9' },
      { 'name' => '', 'version' => '1.0.0' }
    ]

    normalized = OpenVoxInventory.normalize_packages(items)

    assert_equal 1, normalized.length
    assert_equal 'httpd', normalized.first['name']
    assert_equal '2.4.62', normalized.first['version']
  end

  def test_normalize_applications_keeps_distinct_install_locations
    items = [
      { 'name' => 'MyApp', 'version' => '1.0.0', 'install_path' => '/opt/myapp' },
      { 'name' => 'MyApp', 'version' => '1.0.0', 'install_path' => '/Applications/MyApp.app' }
    ]

    normalized = OpenVoxInventory.normalize_applications(items)

    assert_equal 2, normalized.length
  end

  def test_infer_update_channel_uses_first_repository_source
    payload = {
      'packages' => [
        { 'name' => 'nginx', 'repository_source' => 'baseos' },
        { 'name' => 'curl', 'repository_source' => 'appstream' }
      ]
    }

    assert_equal 'baseos', OpenVoxInventory.infer_update_channel(payload)
  end

  def test_trim_respects_inventory_max_items
    items = (1..10).map { |index| { 'name' => "pkg#{index}", 'version' => '1.0.0' } }

    trimmed = OpenVoxInventory.trim(items, { 'inventory_max_items' => 3 })

    assert_equal 3, trimmed.length
    assert_equal 'pkg1', trimmed.first['name']
    assert_equal 'pkg3', trimmed.last['name']
  end

  def test_detect_last_update_rpm_parses_abbreviated_dnf_actions
    history = <<~HISTORY
      ID     | Command line             | Date and time    | Action(s)      | Altered
      --------------------------------------------------------------------------------
      51     | upgrade -y               | 2026-04-14 09:23 | E, I, U        |       42
      50     | install vim              | 2026-04-13 08:10 | Install        |        1
    HISTORY

    with_stubbed_run_command(history) do
      timestamp = OpenVoxInventory.detect_last_update_rpm
      assert_equal Time.parse('2026-04-14 09:23').utc, timestamp.utc
    end
  end

  def test_detect_last_successful_update_returns_iso8601_for_redhat_nodes
    history = <<~HISTORY
      ID     | Command line             | Date and time    | Action(s)      | Altered
      --------------------------------------------------------------------------------
      77     | dnf -y update            | 2026-04-12 17:45 | Upgrade        |       12
    HISTORY

    with_stubbed_run_command(history) do
      timestamp = OpenVoxInventory.detect_last_successful_update('family' => 'RedHat')
      assert_equal Time.parse('2026-04-12 17:45').utc.iso8601, timestamp
    end
  end

  # ---- Windows support ----

  def test_config_paths_use_programdata_on_windows
    with_stubs(windows?: true, program_data: 'D:/ProgramData') do
      paths = OpenVoxInventory.config_paths

      assert_includes paths, 'D:/ProgramData/PuppetLabs/facter/openvox-client.yaml'
      # The manifest default is a literal C:/ProgramData, so it must also be checked.
      assert_includes paths, 'C:/ProgramData/PuppetLabs/facter/openvox-client.yaml'
      refute_includes paths, '/etc/puppetlabs/facter/openvox-client.yaml'
    end
  end

  def test_config_paths_unchanged_on_unix
    with_stubs(windows?: false) do
      assert_equal OpenVoxInventory::UNIX_CONFIG_PATHS, OpenVoxInventory.config_paths
    end
  end

  def test_puppet_conf_and_ssl_paths_on_windows
    with_stubs(windows?: true, program_data: 'C:/ProgramData') do
      assert_equal ['C:/ProgramData/PuppetLabs/puppet/etc/puppet.conf'], OpenVoxInventory.puppet_conf_paths
      assert_equal ['C:/ProgramData/PuppetLabs/puppet/etc/ssl'], OpenVoxInventory.puppet_ssl_dirs
    end
  end

  def test_detect_windows_package_manager_prefers_chocolatey
    with_stubs(command_available?: ->(cmd) { cmd == 'choco' }) do
      assert_equal 'choco', OpenVoxInventory.detect_windows_package_manager
    end

    with_stubs(command_available?: ->(cmd) { cmd == 'winget' }) do
      assert_equal 'winget', OpenVoxInventory.detect_windows_package_manager
    end

    with_stubs(command_available?: false) do
      assert_equal 'windowsupdate', OpenVoxInventory.detect_windows_package_manager
    end
  end

  def test_collect_chocolatey_packages_parses_limit_output
    output = <<~OUTPUT
      chocolatey|2.2.2
      git|2.43.0
      badline
    OUTPUT

    with_stubs(command_available?: true, run_command: output) do
      packages = OpenVoxInventory.collect_chocolatey_packages

      assert_equal %w[chocolatey git], packages.map { |pkg| pkg['name'] }
      assert_equal '2.43.0', packages.last['version']
      assert_equal 'chocolatey', packages.last['repository_source']
    end
  end

  def test_collect_chocolatey_packages_returns_empty_without_choco
    with_stubs(command_available?: false) do
      assert_empty OpenVoxInventory.collect_chocolatey_packages
    end
  end

  def test_collect_winget_packages_parses_export_json
    export = JSON.generate(
      'Sources' => [
        {
          'SourceDetails' => { 'Name' => 'winget' },
          'Packages' => [
            { 'PackageIdentifier' => 'Git.Git', 'Version' => '2.43.0' },
            { 'PackageIdentifier' => 'Microsoft.PowerShell' }
          ]
        }
      ]
    )

    with_stubs(command_available?: true, run_powershell: export) do
      packages = OpenVoxInventory.collect_winget_packages

      assert_equal ['Git.Git', 'Microsoft.PowerShell'], packages.map { |pkg| pkg['name'] }
      assert_equal '2.43.0', packages.first['version']
      assert_equal 'unknown', packages.last['version']
      assert_equal 'winget', packages.last['repository_source']
    end
  end

  def test_collect_chocolatey_repos_tracks_disabled_sources
    output = <<~OUTPUT
      chocolatey|https://community.chocolatey.org/api/v2/|False|||0||False|False
      internal|https://nexus.example.com/repository/choco/|True|||1||False|False
    OUTPUT

    with_stubs(command_available?: true, run_command: output) do
      repos = OpenVoxInventory.collect_chocolatey_repos

      assert_equal 2, repos.length
      assert_equal true, repos.first['enabled']
      assert_equal false, repos.last['enabled']
      assert_equal 'chocolatey', repos.last['repo_type']
      assert_equal 'https://nexus.example.com/repository/choco/', repos.last['base_url']
    end
  end

  def test_parse_winget_source_export_reads_json
    export = JSON.generate([{ 'Name' => 'winget', 'Arg' => 'https://cdn.winget.microsoft.com/cache' }])

    with_stubs(run_powershell: export) do
      repos = OpenVoxInventory.parse_winget_source_export

      assert_equal 1, repos.length
      assert_equal 'winget', repos.first['repo_id']
      assert_equal 'https://cdn.winget.microsoft.com/cache', repos.first['base_url']
    end
  end

  def test_parse_windows_users_maps_builtin_and_regular_accounts
    payload = JSON.generate(
      [
        {
          'Name' => 'Administrator',
          'SID' => 'S-1-5-21-1-2-3-500',
          'Enabled' => false,
          'Description' => 'Built-in administrator',
          'LastLogon' => '2026-07-01T10:00:00Z',
          'Groups' => ['Administrators'],
          'ProfilePath' => 'C:\\Users\\Administrator'
        },
        {
          'Name' => 'svc_app',
          'SID' => 'S-1-5-21-1-2-3-1001',
          'Enabled' => true,
          'Description' => nil,
          'LastLogon' => nil,
          'Groups' => [],
          'ProfilePath' => nil
        }
      ]
    )

    users = OpenVoxInventory.parse_windows_users(payload)

    assert_equal %w[Administrator svc_app], users.map { |user| user['username'] }

    admin = users.first
    assert_equal 'system', admin['user_type']
    assert_equal true, admin['locked']
    assert_equal ['Administrators'], admin['groups']
    assert_equal 'C:\\Users\\Administrator', admin['home_directory']
    assert_equal '2026-07-01T10:00:00Z', admin['last_login']

    service = users.last
    assert_equal 'regular', service['user_type']
    assert_equal false, service['locked']
    assert_nil service['last_login']
  end

  def test_parse_windows_users_accepts_single_object
    payload = JSON.generate('Name' => 'guest', 'SID' => 'S-1-5-21-1-2-3-501', 'Enabled' => false)

    users = OpenVoxInventory.parse_windows_users(payload)

    assert_equal 1, users.length
    assert_equal 'system', users.first['user_type']
  end

  def test_parse_windows_users_tolerates_garbage
    assert_empty OpenVoxInventory.parse_windows_users('not json at all')
    assert_empty OpenVoxInventory.parse_windows_users(nil)
  end

  def test_windows_patch_level_prefers_update_build_revision
    os_fact = { 'release' => { 'full' => '10.0.20348.1234', 'minor' => '0' } }

    assert_equal '2582', OpenVoxInventory.windows_patch_level(os_fact, 'ubr' => 2582)
    assert_equal '1234', OpenVoxInventory.windows_patch_level(os_fact, {})
    assert_equal '0', OpenVoxInventory.windows_patch_level({ 'release' => { 'full' => '10.0', 'minor' => '0' } }, {})
  end

  def test_windows_edition_falls_back_through_sources
    os_fact = { 'windows' => { 'product_name' => 'Windows Server 2022 Standard' } }
    assert_equal 'Windows Server 2022 Standard', OpenVoxInventory.windows_edition(os_fact, {})

    assert_equal 'ServerStandard', OpenVoxInventory.windows_edition({}, 'edition_id' => 'ServerStandard')
    assert_nil OpenVoxInventory.windows_edition({}, {})
  end

  def test_collect_os_inventory_uses_windows_sources
    os_fact = {
      'family' => 'windows',
      'name' => 'windows',
      'release' => { 'full' => '10.0.20348.2582' },
      'windows' => { 'product_name' => 'Windows Server 2022 Datacenter' }
    }
    build = { 'ubr' => 2582, 'display_version' => '21H2' }

    with_stubs(collect_windows_build_info: build,
               detect_windows_package_manager: 'choco',
               detect_last_successful_update: nil) do
      os = OpenVoxInventory.collect_os_inventory(os_fact, '2026-07-28T00:00:00Z')

      assert_equal 'windows', os['os_family']
      assert_equal 'Windows Server 2022 Datacenter', os['edition']
      assert_equal '2582', os['patch_level']
      assert_equal '21H2', os['update_channel']
      assert_equal 'choco', os['package_manager']
    end
  end

  def test_detect_last_update_windows_parses_iso8601
    with_stubs(run_powershell: "2026-07-20T03:15:00Z\n") do
      assert_equal Time.parse('2026-07-20T03:15:00Z'), OpenVoxInventory.detect_last_update_windows
    end
  end

  def test_detect_last_update_windows_returns_nil_when_unavailable
    with_stubs(run_powershell: nil) do
      assert_nil OpenVoxInventory.detect_last_update_windows
    end
  end

  def test_collect_inventory_dispatches_windows_collectors
    os_fact = { 'family' => 'windows', 'name' => 'windows', 'release' => { 'full' => '10.0.20348.2582' } }

    stubs = {
      collect_windows_packages: [{ 'name' => 'git', 'version' => '2.43.0' }],
      collect_windows_applications: [{ 'name' => 'Notepad++', 'version' => '8.6.2' }],
      collect_windows_iis_sites: [{ 'site_name' => 'Default Web Site', 'server_type' => 'iis' }],
      collect_windows_runtimes: [{ 'runtime_type' => 'iis_app_pool', 'runtime_name' => 'DefaultAppPool' }],
      collect_windows_users: [{ 'username' => 'Administrator' }],
      collect_windows_repos: [{ 'repo_id' => 'winget' }],
      collect_containers: [],
      collect_windows_build_info: {},
      detect_windows_package_manager: 'choco',
      detect_last_successful_update: nil
    }

    with_facter_values(os: os_fact) do
      with_stubs(stubs) do
        payload = OpenVoxInventory.collect_inventory({})

        assert_equal 1, payload['packages'].length
        assert_equal 1, payload['applications'].length
        assert_equal 1, payload['websites'].length
        assert_equal 1, payload['runtimes'].length
        assert_equal 1, payload['users'].length
        assert_equal 1, payload['repositories'].length
        assert_equal 'windows', payload.dig('os', 'os_family')
      end
    end
  end

  def test_execute_windows_package_operation_reports_missing_tooling
    with_stubs(windows_package_tool: nil) do
      result = OpenVoxInventory.execute_windows_package_operation(:install, ['git'])

      assert_equal 'failed', result['status']
      assert_match(/chocolatey or winget/, result['summary'])
    end
  end

  def test_execute_windows_package_operation_batches_chocolatey
    seen = []

    with_stubs(windows_package_tool: 'choco',
               run_update_command: ->(cmd) {
                 seen << cmd
                 { 'status' => 'succeeded', 'summary' => 'ok', 'output' => '' }
               }) do
      result = OpenVoxInventory.execute_windows_package_operation(:update, %w[git curl])

      assert_equal 'succeeded', result['status']
      assert_equal 1, seen.length
      assert_match(/choco upgrade -y git curl/, seen.first)
    end
  end

  def test_execute_windows_package_operation_loops_winget_per_package
    seen = []

    with_stubs(windows_package_tool: 'winget',
               run_update_command: ->(cmd) {
                 seen << cmd
                 { 'status' => 'succeeded', 'summary' => 'ok', 'output' => '' }
               }) do
      OpenVoxInventory.execute_windows_package_operation(:remove, %w[Git.Git Mozilla.Firefox])

      assert_equal 2, seen.length
      assert_match(/winget uninstall --id Git\.Git --exact/, seen.first)
      assert_match(/winget uninstall --id Mozilla\.Firefox --exact/, seen.last)
    end
  end

  def test_execute_package_update_routes_windows_family
    with_stubs(execute_windows_package_operation: ->(operation, packages) {
      { 'status' => 'succeeded', 'summary' => "#{operation}:#{packages.join(',')}", 'output' => '' }
    }) do
      result = OpenVoxInventory.execute_package_update('windows', ['git'], {})

      assert_equal 'update:git', result['summary']
    end
  end

  def test_merge_update_results_fails_when_any_package_fails
    results = [
      { 'status' => 'succeeded', 'output' => 'a' },
      { 'status' => 'failed', 'output' => 'b' }
    ]

    merged = OpenVoxInventory.merge_update_results(results)

    assert_equal 'failed', merged['status']
    assert_match(/1 of 2/, merged['summary'])
    assert_equal "a\nb", merged['output']
  end

  def test_execute_windows_updates_requires_a_windows_host
    with_stubs(windows?: false) do
      result = OpenVoxInventory.execute_windows_updates(true)

      assert_equal 'failed', result['status']
      assert_match(/require a Windows host/, result['summary'])
    end
  end

  def test_execute_windows_updates_encodes_security_only_flag
    seen = []

    with_stubs(windows?: true,
               powershell_executable: 'powershell.exe',
               run_update_command: ->(cmd) {
                 seen << cmd
                 { 'status' => 'succeeded', 'summary' => 'ok', 'output' => '' }
               }) do
      OpenVoxInventory.execute_windows_updates(true)
      OpenVoxInventory.execute_windows_updates(false)
    end

    decoded = seen.map do |cmd|
      encoded = cmd[/-EncodedCommand (\S+)/, 1]
      refute_nil encoded, "command is missing -EncodedCommand: #{cmd}"
      Base64.strict_decode64(encoded).force_encoding('UTF-16LE').encode('UTF-8')
    end

    assert_match(/\$securityOnly = \$true/, decoded.first)
    assert_match(/\$securityOnly = \$false/, decoded.last)
    assert_match(/Microsoft\.Update\.Session/, decoded.first)
  end

  def test_windows_powershell_scripts_stay_within_cmd_command_limit
    captured = []
    recorder = ->(script) { captured << script and nil }

    with_stubs(windows?: true, command_available?: true, run_powershell: recorder) do
      OpenVoxInventory.collect_windows_applications({})
      OpenVoxInventory.collect_windows_iis_sites({})
      OpenVoxInventory.collect_windows_local_users
      OpenVoxInventory.collect_iis_app_pools
      OpenVoxInventory.collect_windows_framework_runtimes
      OpenVoxInventory.collect_windows_build_info
      OpenVoxInventory.collect_winget_packages
    end

    assert_equal 7, captured.length

    captured.each do |script|
      encoded = Base64.strict_encode64(script.encode('UTF-16LE'))
      # cmd.exe caps a command line at 8191 characters; leave headroom for the
      # interpreter path and switches.
      assert_operator encoded.length, :<, 7_000,
                      "PowerShell script too long for -EncodedCommand:\n#{script}"
    end
  end

  def test_windows_powershell_scripts_use_literal_backslashes
    captured = []
    recorder = ->(script) { captured << script and nil }

    with_stubs(windows?: true, command_available?: true, run_powershell: recorder) do
      OpenVoxInventory.collect_windows_applications({})
      OpenVoxInventory.collect_windows_build_info
      OpenVoxInventory.collect_iis_app_pools
    end

    # A doubled backslash would mean the heredoc escaped it and the registry or
    # IIS path would be wrong on the node.
    captured.each do |script|
      refute_match(/\\\\/, script, "script contains an escaped backslash:\n#{script}")
    end

    assert_match(/HKLM:\\Software\\/, captured[0])
    assert_match(/HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion/, captured[1])
    assert_match(/IIS:\\AppPools/, captured[2])
  end
end
