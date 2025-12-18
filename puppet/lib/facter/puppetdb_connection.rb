# frozen_string_literal: true

# Custom fact to discover PuppetDB connection details
Facter.add(:puppetdb_connection) do
  confine kernel: :Linux
  setcode do
    result = {}

    # Try to read puppet.conf
    puppet_conf = '/etc/puppetlabs/puppet/puppet.conf'
    if File.exist?(puppet_conf)
      config = Puppet::Util::IniConfig::File.new.read(puppet_conf)
      server = config[:main][:server] || config[:agent][:server] || 'puppet'
      result[:server] = server
      result[:url] = "https://#{server}:8081"
    end

    # Try to find SSL certificates
    ssl_dir = '/etc/puppetlabs/puppet/ssl'
    if File.directory?(ssl_dir)
      certname = Facter.value(:clientcert) || Facter.value(:fqdn)
      
      cert_file = File.join(ssl_dir, 'certs', "#{certname}.pem")
      key_file = File.join(ssl_dir, 'private_keys', "#{certname}.pem")
      ca_file = File.join(ssl_dir, 'certs', 'ca.pem')

      result[:ssl_cert] = cert_file if File.exist?(cert_file)
      result[:ssl_key] = key_file if File.exist?(key_file)
      result[:ssl_ca] = ca_file if File.exist?(ca_file)
    end

    result.empty? ? nil : result
  end
end
