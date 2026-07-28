# frozen_string_literal: true

# Minimal Facter stand-in for the unit tests. Lives on the load path as
# 'facter' so that both this suite and the `require 'facter'` inside
# OpenVoxInventory.collect_inventory resolve to it.
module Facter
  module_function

  def add(_name, &block)
    return unless block

    shim = Object.new
    def shim.confine
      true
    end

    def shim.setcode
      true
    end

    shim.instance_eval(&block)
  end

  def warn(_message); end

  def debug(_message); end

  def value(_name)
    nil
  end
end
