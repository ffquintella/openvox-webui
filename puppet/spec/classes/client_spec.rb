require 'spec_helper'

describe 'openvox_webui::client' do
  let(:params) do
    {
      'api_url' => 'https://openvox.example.com:5051',
    }
  end

  context 'on a Windows agent' do
    let(:facts) do
      {
        'os'         => { 'family' => 'windows' },
        'networking' => { 'fqdn' => 'win.example.com' },
      }
    end

    it { is_expected.to compile.with_all_deps }

    it 'defaults config_dir to the Windows ProgramData location' do
      is_expected.to contain_file('C:/ProgramData/PuppetLabs/facter')
        .with_ensure('directory')
      is_expected.to contain_file('C:/ProgramData/PuppetLabs/facter/openvox-client.yaml')
        .with_ensure('file')
    end

    it 'leaves POSIX ownership and mode unmanaged on Windows' do
      is_expected.to contain_file('C:/ProgramData/PuppetLabs/facter')
        .without_owner.without_group.without_mode
      is_expected.to contain_file('C:/ProgramData/PuppetLabs/facter/openvox-client.yaml')
        .without_owner.without_group.without_mode
    end

    it 'resolves the Puppet SSL dir to the Windows default' do
      is_expected.to contain_file('C:/ProgramData/PuppetLabs/facter/openvox-client.yaml')
        .with_content(%r{C:/ProgramData/PuppetLabs/puppet/etc/ssl/certs/ca\.pem})
        .with_content(%r{C:/ProgramData/PuppetLabs/puppet/etc/ssl/certs/win\.example\.com\.pem})
    end

    context 'with an explicit Windows config_dir override' do
      let(:params) { super().merge('config_dir' => 'D:/facter') }

      it { is_expected.to compile.with_all_deps }
      it { is_expected.to contain_file('D:/facter').with_ensure('directory') }
      it { is_expected.to contain_file('D:/facter/openvox-client.yaml') }
    end
  end

  context 'on a Linux agent (RedHat)' do
    let(:facts) do
      {
        'os'         => { 'family' => 'RedHat' },
        'networking' => { 'fqdn' => 'node.example.com' },
      }
    end

    it { is_expected.to compile.with_all_deps }

    it 'defaults config_dir to the *nix location with root ownership' do
      is_expected.to contain_file('/etc/puppetlabs/facter')
        .with_ensure('directory')
        .with_owner('root')
        .with_group('root')
        .with_mode('0755')
      is_expected.to contain_file('/etc/puppetlabs/facter/openvox-client.yaml')
        .with_owner('root')
        .with_group('root')
        .with_mode('0640')
    end

    it 'resolves the Puppet SSL dir to the *nix default' do
      is_expected.to contain_file('/etc/puppetlabs/facter/openvox-client.yaml')
        .with_content(%r{/etc/puppetlabs/puppet/ssl/certs/ca\.pem})
    end
  end

  context 'on a macOS agent (Darwin)' do
    let(:facts) do
      {
        'os'         => { 'family' => 'Darwin' },
        'networking' => { 'fqdn' => 'mac.example.com' },
      }
    end

    it { is_expected.to compile.with_all_deps }

    it 'uses the wheel group on Darwin' do
      is_expected.to contain_file('/etc/puppetlabs/facter')
        .with_owner('root')
        .with_group('wheel')
    end
  end
end
