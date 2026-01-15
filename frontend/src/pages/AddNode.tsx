import { useState, useEffect } from 'react';
import { Link } from 'react-router-dom';
import { ArrowLeft, Copy, Check, Terminal, Server, Package, AlertTriangle, ExternalLink } from 'lucide-react';
import { api } from '../services/api';
import type { BootstrapConfigResponse } from '../types';

export default function AddNode() {
  const [config, setConfig] = useState<BootstrapConfigResponse | null>(null);
  const [copied, setCopied] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchConfig = async () => {
      try {
        const data = await api.getBootstrapConfig();
        setConfig(data);
      } catch (err) {
        setError('Failed to load bootstrap configuration');
        console.error('Bootstrap config error:', err);
      } finally {
        setLoading(false);
      }
    };
    fetchConfig();
  }, []);

  // Build the curl command using the current window location
  const curlCommand = `curl -sSL ${window.location.origin}/api/v1/bootstrap/script | sudo bash`;

  const handleCopy = () => {
    navigator.clipboard.writeText(curlCommand);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <AlertTriangle className="w-12 h-12 mx-auto mb-4 text-red-400" />
          <h2 className="text-lg font-semibold text-gray-900 mb-2">Error</h2>
          <p className="text-gray-500">{error}</p>
        </div>
      </div>
    );
  }

  const isConfigured = config?.puppet_server_url;

  return (
    <div>
      {/* Header */}
      <div className="flex items-center gap-4 mb-8">
        <Link
          to="/nodes"
          className="p-2 rounded-lg hover:bg-gray-100 transition-colors"
        >
          <ArrowLeft className="w-5 h-5 text-gray-600" />
        </Link>
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Add Node</h1>
          <p className="text-gray-500">Bootstrap a new node to join your Puppet infrastructure</p>
        </div>
      </div>

      {/* Warning if not configured */}
      {!isConfigured && (
        <div className="card bg-amber-50 border-amber-200 mb-6">
          <div className="flex items-start gap-3">
            <AlertTriangle className="w-5 h-5 text-amber-600 mt-0.5 flex-shrink-0" />
            <div>
              <h3 className="font-medium text-amber-800">Configuration Required</h3>
              <p className="text-amber-700 text-sm mt-1">
                The Puppet Server URL has not been configured. Please configure it in{' '}
                <Link to="/settings" className="underline font-medium">Settings</Link>{' '}
                before bootstrapping nodes.
              </p>
              <p className="text-amber-700 text-sm mt-2">
                Add the following to your <code className="bg-amber-100 px-1 rounded">config.yaml</code>:
              </p>
              <pre className="bg-amber-100 text-amber-900 p-2 rounded mt-2 text-xs overflow-x-auto">
{`node_bootstrap:
  puppet_server_url: "puppet.example.com"
  repository_base_url: "https://yum.example.com/openvox"
  agent_package_name: "openvox-agent"`}
              </pre>
            </div>
          </div>
        </div>
      )}

      {/* Configuration Summary */}
      <div className="card mb-6">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <Server className="w-5 h-5 text-primary-600" />
          Configuration
        </h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
          <div className="flex justify-between py-2 border-b border-gray-100">
            <span className="text-gray-500">Puppet Server</span>
            <span className="font-medium text-gray-900">
              {config?.puppet_server_url || <span className="text-amber-600">Not configured</span>}
            </span>
          </div>
          <div className="flex justify-between py-2 border-b border-gray-100">
            <span className="text-gray-500">Package</span>
            <span className="font-medium text-gray-900">{config?.agent_package_name || 'openvox-agent'}</span>
          </div>
          {config?.repository_base_url && (
            <div className="flex justify-between py-2 border-b border-gray-100 md:col-span-2">
              <span className="text-gray-500">Repository URL</span>
              <span className="font-mono text-xs text-gray-900 truncate ml-4">{config.repository_base_url}</span>
            </div>
          )}
        </div>
      </div>

      {/* Bootstrap Command */}
      <div className="card mb-6">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <Terminal className="w-5 h-5 text-primary-600" />
          Bootstrap Command
        </h2>
        <p className="text-gray-600 mb-4">
          Run this command on the new node to install and configure the Puppet agent:
        </p>
        <div className="relative">
          <pre className="bg-gray-900 text-gray-100 p-4 rounded-lg overflow-x-auto font-mono text-sm">
            {curlCommand}
          </pre>
          <button
            onClick={handleCopy}
            disabled={!isConfigured}
            className="absolute top-2 right-2 p-2 rounded-lg bg-gray-800 hover:bg-gray-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            title={isConfigured ? "Copy to clipboard" : "Configure Puppet Server URL first"}
          >
            {copied ? (
              <Check className="w-4 h-4 text-green-400" />
            ) : (
              <Copy className="w-4 h-4 text-gray-400" />
            )}
          </button>
        </div>
        {!isConfigured && (
          <p className="text-sm text-amber-600 mt-2">
            Configure the Puppet Server URL before using this command.
          </p>
        )}
      </div>

      {/* Advanced Options */}
      <div className="card mb-6">
        <h2 className="text-lg font-semibold mb-4">Advanced Options</h2>
        <div className="space-y-4">
          <div>
            <h3 className="font-medium text-gray-700 mb-2">Non-Interactive Mode</h3>
            <p className="text-gray-600 text-sm mb-2">
              For automated deployments, use the non-interactive flag:
            </p>
            <pre className="bg-gray-100 text-gray-800 p-3 rounded-lg font-mono text-sm overflow-x-auto">
              {`${curlCommand.replace('| sudo bash', '| sudo bash -s -- --non-interactive')}`}
            </pre>
          </div>
          <div>
            <h3 className="font-medium text-gray-700 mb-2">Dry Run Mode</h3>
            <p className="text-gray-600 text-sm mb-2">
              To see what the script will do without making changes:
            </p>
            <pre className="bg-gray-100 text-gray-800 p-3 rounded-lg font-mono text-sm overflow-x-auto">
              {`${curlCommand.replace('| sudo bash', '| sudo bash -s -- --dry-run')}`}
            </pre>
          </div>
        </div>
      </div>

      {/* Instructions */}
      <div className="card">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <Package className="w-5 h-5 text-primary-600" />
          What the Script Does
        </h2>
        <ol className="list-decimal list-inside space-y-2 text-gray-600">
          <li>Detects the operating system (RHEL/CentOS, Debian/Ubuntu, etc.)</li>
          <li>Configures the package repository for OpenVox/Puppet packages</li>
          <li>Installs the <code className="bg-gray-100 px-1 rounded">{config?.agent_package_name || 'openvox-agent'}</code> package</li>
          <li>Configures <code className="bg-gray-100 px-1 rounded">puppet.conf</code> with the Puppet Server URL</li>
          <li>Enables the Puppet agent service</li>
          <li>Runs the Puppet agent to submit a certificate signing request</li>
        </ol>

        <div className="mt-6 p-4 bg-blue-50 rounded-lg">
          <p className="text-sm text-blue-800">
            <strong>Next Step:</strong> After running the bootstrap script, you will need to sign
            the node's certificate. You can do this from the{' '}
            <Link to="/ca" className="underline font-medium inline-flex items-center gap-1">
              CA Management <ExternalLink className="w-3 h-3" />
            </Link>{' '}
            page.
          </p>
        </div>

        <div className="mt-4 p-4 bg-gray-50 rounded-lg">
          <h3 className="font-medium text-gray-700 mb-2">Supported Operating Systems</h3>
          <ul className="grid grid-cols-2 gap-2 text-sm text-gray-600">
            <li>RHEL 7, 8, 9</li>
            <li>CentOS 7, 8, 9</li>
            <li>Rocky Linux 8, 9</li>
            <li>AlmaLinux 8, 9</li>
            <li>Ubuntu 18.04, 20.04, 22.04</li>
            <li>Debian 10, 11, 12</li>
          </ul>
        </div>
      </div>
    </div>
  );
}
