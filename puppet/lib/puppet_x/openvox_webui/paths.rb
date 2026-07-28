# frozen_string_literal: true

require 'rbconfig'

module PuppetX
  module OpenVoxWebui
    # Filesystem locations shared by the openvox_webui custom facts.
    #
    # Windows agents keep their configuration under %PROGRAMDATA% and their
    # Puppet confdir at %PROGRAMDATA%\PuppetLabs\puppet\etc, so anything hunting
    # for client.yaml, puppet.conf or the agent's SSL material has to resolve
    # both layouts. openvox_inventory and openvox_classification need identical
    # logic, so it lives here rather than being duplicated per fact.
    #
    # This file is pluginsynced along with lib/facter and lands at
    # $vardir/lib/puppet_x/openvox_webui/paths.rb on the agent. The facts load it
    # with require_relative, which resolves against the requiring file's own
    # location and therefore works whether or not $vardir/lib is on $LOAD_PATH.
    module Paths
      extend self

      UNIX_CONFIG_PATHS = [
        '/etc/openvox-webui/client.yaml',
        '/etc/puppetlabs/facter/openvox-client.yaml',
        '/etc/puppetlabs/puppet/openvox-client.yaml'
      ].freeze

      UNIX_PUPPET_CONF_PATHS = [
        '/etc/puppetlabs/puppet/puppet.conf',
        '/etc/puppet/puppet.conf'
      ].freeze

      UNIX_PUPPET_SSL_DIRS = [
        '/etc/puppetlabs/puppet/ssl',
        '/etc/puppet/ssl',
        '/var/lib/puppet/ssl'
      ].freeze

      UNIX_CLASSIFICATION_KEY_PATHS = [
        '/etc/openvox-webui/classification_key'
      ].freeze

      def windows?
        !(RbConfig::CONFIG['host_os'] =~ /mswin|mingw|cygwin/i).nil?
      end

      # %PROGRAMDATA% with forward slashes so it can be joined with literal paths.
      def program_data
        root = ENV['PROGRAMDATA'] || ENV['ProgramData'] || 'C:/ProgramData'
        root.tr('\\', '/').chomp('/')
      end

      # Roots to search on Windows. openvox_webui::client defaults config_dir to
      # a literal C:/ProgramData, so check that too in case %PROGRAMDATA% has
      # been relocated.
      def windows_roots
        [program_data, 'C:/ProgramData'].uniq
      end

      def config_paths
        return UNIX_CONFIG_PATHS unless windows?

        windows_roots.flat_map do |root|
          [
            "#{root}/PuppetLabs/facter/openvox-client.yaml",
            "#{root}/PuppetLabs/puppet/etc/openvox-client.yaml",
            "#{root}/OpenVox-WebUI/client.yaml"
          ]
        end
      end

      def puppet_conf_paths
        return ["#{program_data}/PuppetLabs/puppet/etc/puppet.conf"] if windows?

        UNIX_PUPPET_CONF_PATHS
      end

      def puppet_ssl_dirs
        return ["#{program_data}/PuppetLabs/puppet/etc/ssl"] if windows?

        UNIX_PUPPET_SSL_DIRS
      end

      # Where an auto-generated classification key is read from and written to.
      # The first entry is the write target; the legacy Unix location stays first
      # so existing agents keep reusing the key they already generated.
      def classification_key_paths
        return UNIX_CLASSIFICATION_KEY_PATHS unless windows?

        windows_roots.map { |root| "#{root}/PuppetLabs/facter/openvox-classification-key" }
      end
    end
  end
end
