require 'puppetlabs_spec_helper/module_spec_helper'
require 'rspec-puppet-facts'

include RspecPuppetFacts

default_facts = {
  puppetversion: Puppet.version,
  facterversion: Facter.version,
}

RSpec.configure do |c|
  c.default_facts = default_facts
  c.before :each do
    # Ensure deterministic behaviour regardless of the host running the tests.
    Puppet.settings[:strict] = :warning
  end
end
