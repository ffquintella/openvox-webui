# frozen_string_literal: true

require 'minitest/autorun'

module Facter
  module_function

  def add(_name)
    return unless block_given?

    shim = Object.new
    def shim.confine
      true
    end

    def shim.setcode
      true
    end

    shim.instance_eval(&Proc.new)
  end

  def warn(_message); end

  def debug(_message); end

  def value(_name)
    nil
  end
end

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
end
