import { useState } from 'react';
import { Save, Database, Shield, Server } from 'lucide-react';

export default function Settings() {
  const [settings, setSettings] = useState({
    puppetdbUrl: 'http://localhost:8081',
    puppetdbTimeout: 30,
    sslVerify: true,
    jwtExpiry: 24,
  });

  const handleSave = () => {
    // TODO: Implement settings save
    console.log('Saving settings:', settings);
  };

  return (
    <div>
      <h1 className="text-2xl font-bold text-gray-900 mb-8">Settings</h1>

      <div className="max-w-2xl space-y-6">
        {/* PuppetDB Settings */}
        <div className="card">
          <div className="flex items-center mb-4">
            <Database className="w-5 h-5 text-primary-600 mr-2" />
            <h2 className="text-lg font-semibold">PuppetDB Connection</h2>
          </div>

          <div className="space-y-4">
            <div>
              <label className="label">PuppetDB URL</label>
              <input
                type="url"
                value={settings.puppetdbUrl}
                onChange={(e) =>
                  setSettings({ ...settings, puppetdbUrl: e.target.value })
                }
                className="input"
                placeholder="http://puppetdb:8081"
              />
            </div>

            <div>
              <label className="label">Timeout (seconds)</label>
              <input
                type="number"
                value={settings.puppetdbTimeout}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    puppetdbTimeout: parseInt(e.target.value),
                  })
                }
                className="input"
                min={1}
                max={300}
              />
            </div>

            <div className="flex items-center">
              <input
                type="checkbox"
                id="sslVerify"
                checked={settings.sslVerify}
                onChange={(e) =>
                  setSettings({ ...settings, sslVerify: e.target.checked })
                }
                className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
              />
              <label htmlFor="sslVerify" className="ml-2 text-sm text-gray-700">
                Verify SSL certificates
              </label>
            </div>
          </div>
        </div>

        {/* Authentication Settings */}
        <div className="card">
          <div className="flex items-center mb-4">
            <Shield className="w-5 h-5 text-primary-600 mr-2" />
            <h2 className="text-lg font-semibold">Authentication</h2>
          </div>

          <div className="space-y-4">
            <div>
              <label className="label">Token Expiry (hours)</label>
              <input
                type="number"
                value={settings.jwtExpiry}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    jwtExpiry: parseInt(e.target.value),
                  })
                }
                className="input"
                min={1}
                max={720}
              />
            </div>
          </div>
        </div>

        {/* Server Info */}
        <div className="card">
          <div className="flex items-center mb-4">
            <Server className="w-5 h-5 text-primary-600 mr-2" />
            <h2 className="text-lg font-semibold">Server Information</h2>
          </div>

          <div className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-gray-500">Version</span>
              <span className="font-medium">0.1.0</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">API Endpoint</span>
              <span className="font-medium">/api/v1</span>
            </div>
          </div>
        </div>

        {/* Save Button */}
        <div className="flex justify-end">
          <button onClick={handleSave} className="btn btn-primary flex items-center">
            <Save className="w-4 h-4 mr-2" />
            Save Settings
          </button>
        </div>
      </div>
    </div>
  );
}
