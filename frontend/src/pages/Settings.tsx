import { useState, useEffect } from 'react';
import {
  Save,
  Database,
  Shield,
  Server,
  Settings as SettingsIcon,
  Layout,
  FileCode,
  History,
  Download,
  Upload,
  CheckCircle,
  XCircle,
  AlertTriangle,
  RefreshCw,
  Clock,
  Cpu,
  HardDrive,
  Users,
  Eye,
  EyeOff,
  Monitor,
  Mail,
} from 'lucide-react';
import {
  useSettings,
  useDashboardConfig,
  useUpdateDashboardConfig,
  useRbacConfig,
  useValidateConfig,
  useImportConfig,
  useConfigHistory,
  useServerInfo,
  useSmtpSettings,
  useUpdateSmtpSettings,
} from '../hooks/useSettings';
import { api } from '../services/api';
import type {
  DashboardConfig,
  ValidateConfigResponse,
  ImportConfigResponse,
  RoleDefinition,
  PermissionDefinition,
} from '../types';

type TabId = 'general' | 'dashboard' | 'rbac' | 'import-export' | 'smtp' | 'server';

interface Tab {
  id: TabId;
  name: string;
  icon: React.ComponentType<{ className?: string }>;
}

const tabs: Tab[] = [
  { id: 'general', name: 'General', icon: SettingsIcon },
  { id: 'dashboard', name: 'Dashboard', icon: Layout },
  { id: 'rbac', name: 'RBAC', icon: Shield },
  { id: 'smtp', name: 'Email/SMTP', icon: Mail },
  { id: 'import-export', name: 'Import/Export', icon: FileCode },
  { id: 'server', name: 'Server Info', icon: Server },
];

export default function Settings() {
  const [activeTab, setActiveTab] = useState<TabId>('general');

  return (
    <div>
      <h1 className="text-2xl font-bold text-gray-900 mb-6">Settings</h1>

      {/* Tab Navigation */}
      <div className="border-b border-gray-200 mb-6">
        <nav className="-mb-px flex space-x-8">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`
                  flex items-center py-4 px-1 border-b-2 font-medium text-sm
                  ${
                    activeTab === tab.id
                      ? 'border-primary-500 text-primary-600'
                      : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
                  }
                `}
              >
                <Icon className="w-4 h-4 mr-2" />
                {tab.name}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Tab Content */}
      <div className="max-w-4xl">
        {activeTab === 'general' && <GeneralSettingsTab />}
        {activeTab === 'dashboard' && <DashboardSettingsTab />}
        {activeTab === 'rbac' && <RbacSettingsTab />}
        {activeTab === 'smtp' && <SmtpSettingsTab />}
        {activeTab === 'import-export' && <ImportExportTab />}
        {activeTab === 'server' && <ServerInfoTab />}
      </div>
    </div>
  );
}

function GeneralSettingsTab() {
  const { data: settings, isLoading, error } = useSettings();

  if (isLoading) {
    return <LoadingState message="Loading settings..." />;
  }

  if (error) {
    return <ErrorState message="Failed to load settings" />;
  }

  if (!settings) {
    return <ErrorState message="No settings available" />;
  }

  return (
    <div className="space-y-6">
      {/* Server Settings */}
      <div className="card">
        <div className="flex items-center mb-4">
          <Server className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Server</h2>
        </div>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <SettingRow label="Host" value={settings.server.host} />
          <SettingRow label="Port" value={settings.server.port.toString()} />
          <SettingRow label="Workers" value={settings.server.workers.toString()} />
        </div>
      </div>

      {/* PuppetDB Settings */}
      <div className="card">
        <div className="flex items-center mb-4">
          <Database className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">PuppetDB Connection</h2>
        </div>
        {settings.puppetdb ? (
          <div className="grid grid-cols-2 gap-4 text-sm">
            <SettingRow label="URL" value={settings.puppetdb.url} />
            <SettingRow label="Timeout" value={`${settings.puppetdb.timeout_secs}s`} />
            <SettingRow
              label="SSL Verify"
              value={settings.puppetdb.ssl_verify ? 'Enabled' : 'Disabled'}
              variant={settings.puppetdb.ssl_verify ? 'success' : 'warning'}
            />
            <SettingRow
              label="SSL Configured"
              value={settings.puppetdb.ssl_configured ? 'Yes' : 'No'}
              variant={settings.puppetdb.ssl_configured ? 'success' : 'neutral'}
            />
          </div>
        ) : (
          <p className="text-sm text-gray-500">PuppetDB not configured</p>
        )}
      </div>

      {/* Puppet CA Settings */}
      {settings.puppet_ca && (
        <div className="card">
          <div className="flex items-center mb-4">
            <Shield className="w-5 h-5 text-primary-600 mr-2" />
            <h2 className="text-lg font-semibold">Puppet CA</h2>
          </div>
          <div className="grid grid-cols-2 gap-4 text-sm">
            <SettingRow label="URL" value={settings.puppet_ca.url} />
            <SettingRow label="Timeout" value={`${settings.puppet_ca.timeout_secs}s`} />
            <SettingRow
              label="SSL Verify"
              value={settings.puppet_ca.ssl_verify ? 'Enabled' : 'Disabled'}
              variant={settings.puppet_ca.ssl_verify ? 'success' : 'warning'}
            />
          </div>
        </div>
      )}

      {/* Authentication Settings */}
      <div className="card">
        <div className="flex items-center mb-4">
          <Shield className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Authentication</h2>
        </div>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <SettingRow label="Token Expiry" value={`${settings.auth.token_expiry_hours}h`} />
          <SettingRow
            label="Refresh Token Expiry"
            value={`${settings.auth.refresh_token_expiry_days}d`}
          />
          <SettingRow
            label="Min Password Length"
            value={settings.auth.password_min_length.toString()}
          />
        </div>
      </div>

      {/* Database Settings */}
      <div className="card">
        <div className="flex items-center mb-4">
          <HardDrive className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Database</h2>
        </div>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <SettingRow label="URL" value={settings.database.url_masked} mono />
          <SettingRow label="Max Connections" value={settings.database.max_connections.toString()} />
          <SettingRow label="Min Connections" value={settings.database.min_connections.toString()} />
        </div>
      </div>

      {/* Logging Settings */}
      <div className="card">
        <div className="flex items-center mb-4">
          <FileCode className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Logging</h2>
        </div>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <SettingRow label="Level" value={settings.logging.level.toUpperCase()} />
          <SettingRow label="Format" value={settings.logging.format} />
          {settings.logging.file && <SettingRow label="File" value={settings.logging.file} mono />}
        </div>
      </div>

      {/* Cache Settings */}
      <div className="card">
        <div className="flex items-center mb-4">
          <Cpu className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Cache</h2>
        </div>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <SettingRow
            label="Status"
            value={settings.cache.enabled ? 'Enabled' : 'Disabled'}
            variant={settings.cache.enabled ? 'success' : 'warning'}
          />
          <SettingRow label="Node TTL" value={`${settings.cache.node_ttl_secs}s`} />
          <SettingRow label="Fact TTL" value={`${settings.cache.fact_ttl_secs}s`} />
          <SettingRow label="Report TTL" value={`${settings.cache.report_ttl_secs}s`} />
          <SettingRow label="Max Entries" value={settings.cache.max_entries.toString()} />
        </div>
      </div>

      {/* Node Bootstrap Settings */}
      <div className="card">
        <div className="flex items-center mb-4">
          <Monitor className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Node Bootstrap</h2>
        </div>
        {settings.node_bootstrap ? (
          <div className="grid grid-cols-2 gap-4 text-sm">
            <SettingRow
              label="OpenVox Server"
              value={settings.node_bootstrap.openvox_server_url || 'Not configured'}
              variant={settings.node_bootstrap.openvox_server_url ? 'neutral' : 'warning'}
            />
            <SettingRow
              label="Agent Package"
              value={settings.node_bootstrap.agent_package_name}
            />
            {settings.node_bootstrap.repository_base_url && (
              <div className="col-span-2">
                <SettingRow
                  label="Repository URL"
                  value={settings.node_bootstrap.repository_base_url}
                  mono
                />
              </div>
            )}
          </div>
        ) : (
          <p className="text-sm text-gray-500">
            Node bootstrap not configured. Add <code className="bg-gray-100 px-1 rounded">node_bootstrap</code> to your config.yaml to enable.
          </p>
        )}
        <p className="mt-4 text-xs text-gray-500">
          These settings are used when bootstrapping new nodes. Configure them to enable the Add Node feature.
        </p>
      </div>
    </div>
  );
}

function DashboardSettingsTab() {
  const { data: config, isLoading, error } = useDashboardConfig();
  const updateConfig = useUpdateDashboardConfig();
  const [editedConfig, setEditedConfig] = useState<Partial<DashboardConfig>>({});
  const [hasChanges, setHasChanges] = useState(false);

  useEffect(() => {
    if (config) {
      setEditedConfig(config);
    }
  }, [config]);

  const handleChange = <K extends keyof DashboardConfig>(key: K, value: DashboardConfig[K]) => {
    setEditedConfig((prev) => ({ ...prev, [key]: value }));
    setHasChanges(true);
  };

  const handleSave = async () => {
    try {
      await updateConfig.mutateAsync(editedConfig);
      setHasChanges(false);
    } catch {
      // Error handled by mutation
    }
  };

  if (isLoading) {
    return <LoadingState message="Loading dashboard settings..." />;
  }

  if (error) {
    return <ErrorState message="Failed to load dashboard settings" />;
  }

  if (!config) {
    return <ErrorState message="No dashboard configuration available" />;
  }

  return (
    <div className="space-y-6">
      <div className="card">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center">
            <Layout className="w-5 h-5 text-primary-600 mr-2" />
            <h2 className="text-lg font-semibold">Dashboard Preferences</h2>
          </div>
          {hasChanges && (
            <span className="text-sm text-amber-600 flex items-center">
              <AlertTriangle className="w-4 h-4 mr-1" />
              Unsaved changes
            </span>
          )}
        </div>

        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="label">Default Time Range</label>
              <select
                value={editedConfig.default_time_range || '24h'}
                onChange={(e) => handleChange('default_time_range', e.target.value)}
                className="input"
              >
                <option value="1h">Last 1 hour</option>
                <option value="6h">Last 6 hours</option>
                <option value="12h">Last 12 hours</option>
                <option value="24h">Last 24 hours</option>
                <option value="7d">Last 7 days</option>
                <option value="30d">Last 30 days</option>
              </select>
            </div>

            <div>
              <label className="label">Refresh Interval (seconds)</label>
              <input
                type="number"
                value={editedConfig.refresh_interval_secs || 60}
                onChange={(e) => handleChange('refresh_interval_secs', parseInt(e.target.value))}
                className="input"
                min={10}
                max={3600}
              />
            </div>

            <div>
              <label className="label">Nodes Per Page</label>
              <input
                type="number"
                value={editedConfig.nodes_per_page || 25}
                onChange={(e) => handleChange('nodes_per_page', parseInt(e.target.value))}
                className="input"
                min={10}
                max={100}
              />
            </div>

            <div>
              <label className="label">Reports Per Page</label>
              <input
                type="number"
                value={editedConfig.reports_per_page || 25}
                onChange={(e) => handleChange('reports_per_page', parseInt(e.target.value))}
                className="input"
                min={10}
                max={100}
              />
            </div>

            <div>
              <label className="label">Theme</label>
              <select
                value={editedConfig.theme || 'light'}
                onChange={(e) => handleChange('theme', e.target.value)}
                className="input"
              >
                <option value="light">Light</option>
                <option value="dark">Dark</option>
                <option value="system">System</option>
              </select>
            </div>

            <div>
              <label className="label">Inactive Threshold (hours)</label>
              <input
                type="number"
                value={editedConfig.inactive_threshold_hours || 24}
                onChange={(e) => handleChange('inactive_threshold_hours', parseInt(e.target.value))}
                className="input"
                min={1}
                max={168}
              />
            </div>
          </div>

          <div className="flex items-center">
            <input
              type="checkbox"
              id="showInactiveNodes"
              checked={editedConfig.show_inactive_nodes ?? true}
              onChange={(e) => handleChange('show_inactive_nodes', e.target.checked)}
              className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
            />
            <label htmlFor="showInactiveNodes" className="ml-2 text-sm text-gray-700">
              Show inactive nodes on dashboard
            </label>
          </div>
        </div>

        <div className="mt-6 flex justify-end">
          <button
            onClick={handleSave}
            disabled={!hasChanges || updateConfig.isPending}
            className="btn btn-primary flex items-center disabled:opacity-50"
          >
            {updateConfig.isPending ? (
              <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Save className="w-4 h-4 mr-2" />
            )}
            Save Changes
          </button>
        </div>
      </div>

      {/* Widgets Configuration */}
      <div className="card">
        <div className="flex items-center mb-4">
          <Layout className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Dashboard Widgets</h2>
        </div>

        <div className="space-y-2">
          {(editedConfig.widgets || []).map((widget, index) => (
            <div
              key={widget.id || index}
              className="flex items-center justify-between p-3 bg-gray-50 rounded-lg"
            >
              <div className="flex items-center">
                {widget.enabled ? (
                  <Eye className="w-4 h-4 text-green-500 mr-2" />
                ) : (
                  <EyeOff className="w-4 h-4 text-gray-400 mr-2" />
                )}
                <span className="font-medium">{widget.title || widget.type}</span>
                <span className="ml-2 text-xs text-gray-500">({widget.type})</span>
              </div>
              <div className="flex items-center space-x-2">
                <button
                  onClick={() => {
                    const newWidgets = [...(editedConfig.widgets || [])];
                    newWidgets[index] = { ...widget, enabled: !widget.enabled };
                    handleChange('widgets', newWidgets);
                  }}
                  className="text-sm text-primary-600 hover:text-primary-800"
                >
                  {widget.enabled ? 'Disable' : 'Enable'}
                </button>
              </div>
            </div>
          ))}
          {(!editedConfig.widgets || editedConfig.widgets.length === 0) && (
            <p className="text-sm text-gray-500">No widgets configured</p>
          )}
        </div>
      </div>
    </div>
  );
}

function RbacSettingsTab() {
  const { data: rbacConfig, isLoading, error } = useRbacConfig();

  if (isLoading) {
    return <LoadingState message="Loading RBAC settings..." />;
  }

  if (error) {
    return <ErrorState message="Failed to load RBAC settings" />;
  }

  if (!rbacConfig) {
    return <ErrorState message="No RBAC configuration available" />;
  }

  return (
    <div className="space-y-6">
      <div className="card">
        <div className="flex items-center mb-4">
          <Shield className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">RBAC Configuration</h2>
        </div>

        <div className="grid grid-cols-2 gap-4 text-sm">
          <SettingRow label="Default Role" value={rbacConfig.default_role} />
          <SettingRow label="Session Timeout" value={`${rbacConfig.session_timeout_minutes} min`} />
          <SettingRow label="Max Failed Logins" value={rbacConfig.max_failed_logins.toString()} />
          <SettingRow
            label="Lockout Duration"
            value={`${rbacConfig.lockout_duration_minutes} min`}
          />
        </div>
      </div>

      <div className="card">
        <div className="flex items-center mb-4">
          <Users className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Configured Roles</h2>
        </div>

        <div className="space-y-4">
          {rbacConfig.roles.map((role: RoleDefinition) => (
            <div key={role.name} className="border rounded-lg p-4">
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center">
                  <h3 className="font-medium">{role.display_name || role.name}</h3>
                  {role.is_system && (
                    <span className="ml-2 px-2 py-0.5 text-xs bg-gray-100 text-gray-600 rounded">
                      System
                    </span>
                  )}
                </div>
              </div>
              {role.description && <p className="text-sm text-gray-500 mb-3">{role.description}</p>}
              <div className="flex flex-wrap gap-2">
                {role.permissions.map((perm: PermissionDefinition, idx: number) => (
                  <span
                    key={idx}
                    className="px-2 py-1 text-xs bg-primary-50 text-primary-700 rounded"
                  >
                    {perm.resource}:{perm.action}
                    {perm.scope !== 'all' && ` (${perm.scope})`}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function ImportExportTab() {
  const [yamlContent, setYamlContent] = useState('');
  const [validationResult, setValidationResult] = useState<ValidateConfigResponse | null>(null);
  const [importResult, setImportResult] = useState<ImportConfigResponse | null>(null);
  const [isExporting, setIsExporting] = useState(false);
  const validateConfig = useValidateConfig();
  const importConfig = useImportConfig();
  const { data: history, isLoading: historyLoading } = useConfigHistory();

  const handleExport = async () => {
    setIsExporting(true);
    try {
      const result = await api.exportConfig();
      setYamlContent(result.content);
      setValidationResult(null);
      setImportResult(null);
    } catch (err) {
      console.error('Export failed:', err);
    } finally {
      setIsExporting(false);
    }
  };

  const handleValidate = async () => {
    if (!yamlContent.trim()) return;
    try {
      const result = await validateConfig.mutateAsync(yamlContent);
      setValidationResult(result);
      setImportResult(null);
    } catch {
      // Error handled by mutation
    }
  };

  const handleImport = async (dryRun: boolean) => {
    if (!yamlContent.trim()) return;
    try {
      const result = await importConfig.mutateAsync({ content: yamlContent, dryRun });
      setImportResult(result);
      if (!dryRun && result.success) {
        setYamlContent('');
        setValidationResult(null);
      }
    } catch {
      // Error handled by mutation
    }
  };

  const handleDownload = () => {
    const blob = new Blob([yamlContent], { type: 'text/yaml' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `openvox-config-${new Date().toISOString().split('T')[0]}.yaml`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onload = (event) => {
        setYamlContent(event.target?.result as string);
        setValidationResult(null);
        setImportResult(null);
      };
      reader.readAsText(file);
    }
  };

  return (
    <div className="space-y-6">
      {/* Export/Import Actions */}
      <div className="card">
        <div className="flex items-center mb-4">
          <FileCode className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Configuration Import/Export</h2>
        </div>

        <div className="flex flex-wrap gap-3 mb-4">
          <button
            onClick={handleExport}
            disabled={isExporting}
            className="btn btn-secondary flex items-center"
          >
            {isExporting ? (
              <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Download className="w-4 h-4 mr-2" />
            )}
            Export Current Config
          </button>

          <label className="btn btn-secondary flex items-center cursor-pointer">
            <Upload className="w-4 h-4 mr-2" />
            Upload YAML File
            <input
              type="file"
              accept=".yaml,.yml"
              onChange={handleFileUpload}
              className="hidden"
            />
          </label>

          {yamlContent && (
            <button onClick={handleDownload} className="btn btn-secondary flex items-center">
              <Download className="w-4 h-4 mr-2" />
              Download as File
            </button>
          )}
        </div>

        {/* YAML Editor */}
        <div className="mb-4">
          <label className="label">YAML Configuration</label>
          <textarea
            value={yamlContent}
            onChange={(e) => {
              setYamlContent(e.target.value);
              setValidationResult(null);
              setImportResult(null);
            }}
            className="w-full h-96 font-mono text-sm p-4 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
            placeholder="# Paste your YAML configuration here or click 'Export Current Config' to load existing configuration"
            spellCheck={false}
          />
        </div>

        {/* Validation/Import Actions */}
        <div className="flex flex-wrap gap-3 mb-4">
          <button
            onClick={handleValidate}
            disabled={!yamlContent.trim() || validateConfig.isPending}
            className="btn btn-secondary flex items-center disabled:opacity-50"
          >
            {validateConfig.isPending ? (
              <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <CheckCircle className="w-4 h-4 mr-2" />
            )}
            Validate
          </button>

          <button
            onClick={() => handleImport(true)}
            disabled={!yamlContent.trim() || importConfig.isPending}
            className="btn btn-secondary flex items-center disabled:opacity-50"
          >
            {importConfig.isPending ? (
              <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Eye className="w-4 h-4 mr-2" />
            )}
            Dry Run
          </button>

          <button
            onClick={() => handleImport(false)}
            disabled={!yamlContent.trim() || importConfig.isPending}
            className="btn btn-primary flex items-center disabled:opacity-50"
          >
            {importConfig.isPending ? (
              <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Upload className="w-4 h-4 mr-2" />
            )}
            Import Configuration
          </button>
        </div>

        {/* Validation Result */}
        {validationResult && (
          <div
            className={`p-4 rounded-lg ${
              validationResult.valid
                ? 'bg-green-50 border border-green-200'
                : 'bg-red-50 border border-red-200'
            }`}
          >
            <div className="flex items-center mb-2">
              {validationResult.valid ? (
                <CheckCircle className="w-5 h-5 text-green-600 mr-2" />
              ) : (
                <XCircle className="w-5 h-5 text-red-600 mr-2" />
              )}
              <span
                className={`font-medium ${
                  validationResult.valid ? 'text-green-800' : 'text-red-800'
                }`}
              >
                {validationResult.valid ? 'Configuration is valid' : 'Validation failed'}
              </span>
            </div>

            {validationResult.errors.length > 0 && (
              <ul className="mt-2 space-y-1">
                {validationResult.errors.map((err, idx) => (
                  <li key={idx} className="text-sm text-red-700">
                    <span className="font-mono">{err.path}</span>: {err.message}
                    {err.line && <span className="text-gray-500"> (line {err.line})</span>}
                  </li>
                ))}
              </ul>
            )}

            {validationResult.warnings.length > 0 && (
              <div className="mt-3">
                <div className="flex items-center text-amber-700 mb-1">
                  <AlertTriangle className="w-4 h-4 mr-1" />
                  <span className="text-sm font-medium">Warnings</span>
                </div>
                <ul className="space-y-1">
                  {validationResult.warnings.map((warn, idx) => (
                    <li key={idx} className="text-sm text-amber-700">
                      {warn}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}

        {/* Import Result */}
        {importResult && (
          <div
            className={`p-4 rounded-lg ${
              importResult.success
                ? 'bg-green-50 border border-green-200'
                : 'bg-red-50 border border-red-200'
            }`}
          >
            <div className="flex items-center mb-2">
              {importResult.success ? (
                <CheckCircle className="w-5 h-5 text-green-600 mr-2" />
              ) : (
                <XCircle className="w-5 h-5 text-red-600 mr-2" />
              )}
              <span
                className={`font-medium ${
                  importResult.success ? 'text-green-800' : 'text-red-800'
                }`}
              >
                {importResult.dry_run ? 'Dry run: ' : ''}
                {importResult.message}
              </span>
            </div>

            {importResult.validation_errors.length > 0 && (
              <ul className="mt-2 space-y-1">
                {importResult.validation_errors.map((err, idx) => (
                  <li key={idx} className="text-sm text-red-700">
                    {err}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </div>

      {/* Configuration History */}
      <div className="card">
        <div className="flex items-center mb-4">
          <History className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Configuration History</h2>
        </div>

        {historyLoading ? (
          <LoadingState message="Loading history..." />
        ) : history && history.length > 0 ? (
          <div className="space-y-2">
            {history.map((entry) => (
              <div key={entry.id} className="flex items-center justify-between p-3 bg-gray-50 rounded-lg">
                <div className="flex items-center">
                  <Clock className="w-4 h-4 text-gray-400 mr-2" />
                  <div>
                    <span className="font-medium">{entry.action}</span>
                    <span className="text-gray-500 mx-2">by</span>
                    <span className="text-gray-700">{entry.user}</span>
                  </div>
                </div>
                <div className="text-right">
                  <div className="text-sm text-gray-500">
                    {new Date(entry.timestamp).toLocaleString()}
                  </div>
                  <div className="text-xs text-gray-400">{entry.changes_summary}</div>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm text-gray-500">No configuration changes recorded yet</p>
        )}
      </div>
    </div>
  );
}

function ServerInfoTab() {
  const { data: serverInfo, isLoading, error, refetch } = useServerInfo();

  if (isLoading) {
    return <LoadingState message="Loading server info..." />;
  }

  if (error) {
    return <ErrorState message="Failed to load server info" />;
  }

  if (!serverInfo) {
    return <ErrorState message="No server information available" />;
  }

  const formatUptime = (secs: number) => {
    const days = Math.floor(secs / 86400);
    const hours = Math.floor((secs % 86400) / 3600);
    const minutes = Math.floor((secs % 3600) / 60);

    const parts = [];
    if (days > 0) parts.push(`${days}d`);
    if (hours > 0) parts.push(`${hours}h`);
    if (minutes > 0) parts.push(`${minutes}m`);
    if (parts.length === 0) parts.push(`${secs}s`);

    return parts.join(' ');
  };

  return (
    <div className="space-y-6">
      <div className="card">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center">
            <Server className="w-5 h-5 text-primary-600 mr-2" />
            <h2 className="text-lg font-semibold">Server Information</h2>
          </div>
          <button onClick={() => refetch()} className="btn btn-secondary btn-sm flex items-center">
            <RefreshCw className="w-4 h-4 mr-1" />
            Refresh
          </button>
        </div>

        <div className="grid grid-cols-2 gap-4 text-sm">
          <SettingRow label="Version" value={serverInfo.version} />
          <SettingRow label="Rust Version" value={serverInfo.rust_version} />
          {serverInfo.build_timestamp && (
            <SettingRow
              label="Build Time"
              value={new Date(serverInfo.build_timestamp).toLocaleString()}
            />
          )}
          {serverInfo.git_commit && (
            <SettingRow label="Git Commit" value={serverInfo.git_commit.substring(0, 8)} mono />
          )}
          <SettingRow label="Uptime" value={formatUptime(serverInfo.uptime_secs)} />
          {serverInfo.config_file_path && (
            <SettingRow label="Config File" value={serverInfo.config_file_path} mono />
          )}
        </div>
      </div>

      <div className="card">
        <div className="flex items-center mb-4">
          <CheckCircle className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">Enabled Features</h2>
        </div>

        <div className="flex flex-wrap gap-2">
          {serverInfo.features.map((feature: string) => (
            <span
              key={feature}
              className="px-3 py-1 bg-green-50 text-green-700 rounded-full text-sm"
            >
              {feature}
            </span>
          ))}
          {serverInfo.features.length === 0 && (
            <p className="text-sm text-gray-500">No additional features enabled</p>
          )}
        </div>
      </div>

      {/* SAML SSO Status */}
      <div className="card">
        <div className="flex items-center mb-4">
          <Shield className="w-5 h-5 text-primary-600 mr-2" />
          <h2 className="text-lg font-semibold">SAML Single Sign-On</h2>
        </div>

        {serverInfo.saml.enabled ? (
          <div className="space-y-4">
            <div className="flex items-center">
              {serverInfo.saml.configured ? (
                <>
                  <CheckCircle className="w-5 h-5 text-green-500 mr-2" />
                  <span className="text-green-700 font-medium">SAML SSO is enabled and configured</span>
                </>
              ) : (
                <>
                  <AlertTriangle className="w-5 h-5 text-amber-500 mr-2" />
                  <span className="text-amber-700 font-medium">SAML SSO is enabled but not fully configured</span>
                </>
              )}
            </div>

            <div className="grid grid-cols-2 gap-4 text-sm">
              {serverInfo.saml.sp_entity_id && (
                <SettingRow label="SP Entity ID" value={serverInfo.saml.sp_entity_id} mono />
              )}
              {serverInfo.saml.idp_entity_id && (
                <SettingRow label="IdP Entity ID" value={serverInfo.saml.idp_entity_id} mono />
              )}
              {serverInfo.saml.login_url && (
                <SettingRow label="Login URL" value={serverInfo.saml.login_url} mono />
              )}
            </div>

            {serverInfo.saml.configured && (
              <div className="pt-4 border-t border-gray-100">
                <a
                  href="/api/v1/auth/saml/metadata"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="btn btn-secondary btn-sm flex items-center inline-flex"
                >
                  <Download className="w-4 h-4 mr-2" />
                  Download SP Metadata
                </a>
                <p className="text-xs text-gray-500 mt-2">
                  Use this metadata XML to configure your Identity Provider.
                </p>
              </div>
            )}
          </div>
        ) : (
          <div className="flex items-center text-gray-500">
            <XCircle className="w-5 h-5 mr-2" />
            <span>SAML SSO is not enabled</span>
          </div>
        )}
      </div>
    </div>
  );
}

// Helper Components

interface SettingRowProps {
  label: string;
  value: string;
  variant?: 'success' | 'warning' | 'neutral';
  mono?: boolean;
}

function SettingRow({ label, value, variant, mono }: SettingRowProps) {
  const variantClasses = {
    success: 'text-green-600',
    warning: 'text-amber-600',
    neutral: 'text-gray-700',
  };

  return (
    <div className="flex justify-between py-2 border-b border-gray-100">
      <span className="text-gray-500">{label}</span>
      <span
        className={`font-medium ${variant ? variantClasses[variant] : 'text-gray-900'} ${
          mono ? 'font-mono text-xs' : ''
        }`}
      >
        {value}
      </span>
    </div>
  );
}

function LoadingState({ message }: { message: string }) {
  return (
    <div className="flex items-center justify-center py-12">
      <RefreshCw className="w-6 h-6 text-primary-600 animate-spin mr-3" />
      <span className="text-gray-500">{message}</span>
    </div>
  );
}

function ErrorState({ message }: { message: string }) {
  return (
    <div className="flex items-center justify-center py-12">
      <XCircle className="w-6 h-6 text-red-500 mr-3" />
      <span className="text-red-600">{message}</span>
    </div>
  );
}

function SmtpSettingsTab() {
  const { data: smtp, isLoading, error } = useSmtpSettings();
  const updateMutation = useUpdateSmtpSettings();
  const [showPassword, setShowPassword] = useState(false);

  const [formData, setFormData] = useState({
    host: '',
    port: 587,
    username: '',
    password: '',
    from_address: '',
    use_tls: true,
  });

  useEffect(() => {
    if (smtp) {
      setFormData({
        host: smtp.host,
        port: smtp.port,
        username: smtp.username || '',
        password: smtp.password || '',
        from_address: smtp.from_address,
        use_tls: smtp.use_tls,
      });
    }
  }, [smtp]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    updateMutation.mutate({
      host: formData.host,
      port: formData.port,
      username: formData.username || undefined,
      password: formData.password || undefined,
      from_address: formData.from_address,
      use_tls: formData.use_tls,
    });
  };

  if (isLoading) {
    return <LoadingState message="Loading SMTP settings..." />;
  }

  if (error) {
    return <ErrorState message="Failed to load SMTP settings" />;
  }

  return (
    <div className="space-y-6">
      <div className="card">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center">
            <Mail className="w-5 h-5 text-primary-600 mr-2" />
            <h2 className="text-lg font-semibold">SMTP Server Configuration</h2>
          </div>
          {smtp?.configured && (
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-green-100 text-green-800">
              <CheckCircle className="w-3 h-3 mr-1" />
              Configured
            </span>
          )}
        </div>

        <p className="text-sm text-gray-600 dark:text-gray-400 mb-6">
          Configure the SMTP server for sending email notifications from alert rules.
        </p>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="col-span-2 sm:col-span-1">
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                SMTP Host
              </label>
              <input
                type="text"
                value={formData.host}
                onChange={(e) => setFormData({ ...formData, host: e.target.value })}
                className="w-full px-3 py-2 border border-gray-300 rounded-md dark:border-gray-600 dark:bg-gray-700"
                placeholder="smtp.example.com"
                required
              />
            </div>

            <div className="col-span-2 sm:col-span-1">
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                SMTP Port
              </label>
              <input
                type="number"
                value={formData.port}
                onChange={(e) => setFormData({ ...formData, port: parseInt(e.target.value) })}
                className="w-full px-3 py-2 border border-gray-300 rounded-md dark:border-gray-600 dark:bg-gray-700"
                placeholder="587"
                min="1"
                max="65535"
                required
              />
            </div>

            <div className="col-span-2">
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                From Address
              </label>
              <input
                type="email"
                value={formData.from_address}
                onChange={(e) => setFormData({ ...formData, from_address: e.target.value })}
                className="w-full px-3 py-2 border border-gray-300 rounded-md dark:border-gray-600 dark:bg-gray-700"
                placeholder="noreply@example.com"
                required
              />
            </div>

            <div className="col-span-2 sm:col-span-1">
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                Username (optional)
              </label>
              <input
                type="text"
                value={formData.username}
                onChange={(e) => setFormData({ ...formData, username: e.target.value })}
                className="w-full px-3 py-2 border border-gray-300 rounded-md dark:border-gray-600 dark:bg-gray-700"
                placeholder="smtp-user"
              />
            </div>

            <div className="col-span-2 sm:col-span-1">
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                Password (optional)
              </label>
              <div className="relative">
                <input
                  type={showPassword ? 'text' : 'password'}
                  value={formData.password}
                  onChange={(e) => setFormData({ ...formData, password: e.target.value })}
                  className="w-full px-3 py-2 border border-gray-300 rounded-md dark:border-gray-600 dark:bg-gray-700 pr-10"
                  placeholder="••••••••"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword(!showPassword)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-500 hover:text-gray-700"
                >
                  {showPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                </button>
              </div>
            </div>

            <div className="col-span-2">
              <label className="flex items-center space-x-2">
                <input
                  type="checkbox"
                  checked={formData.use_tls}
                  onChange={(e) => setFormData({ ...formData, use_tls: e.target.checked })}
                  className="rounded border-gray-300"
                />
                <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
                  Use TLS encryption
                </span>
              </label>
              <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                Recommended for secure connections (port 587)
              </p>
            </div>
          </div>

          <div className="flex justify-end space-x-3 pt-4 border-t border-gray-200 dark:border-gray-700">
            <button
              type="submit"
              disabled={updateMutation.isPending}
              className="inline-flex items-center px-4 py-2 border border-transparent rounded-md shadow-sm text-sm font-medium text-white bg-primary-600 hover:bg-primary-700 disabled:opacity-50"
            >
              {updateMutation.isPending ? (
                <>
                  <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
                  Saving...
                </>
              ) : (
                <>
                  <Save className="w-4 h-4 mr-2" />
                  Save SMTP Settings
                </>
              )}
            </button>
          </div>

          {updateMutation.isSuccess && (
            <div className="mt-4 p-3 bg-green-50 border border-green-200 rounded-md flex items-center text-green-800">
              <CheckCircle className="w-4 h-4 mr-2" />
              <span className="text-sm">SMTP settings saved successfully</span>
            </div>
          )}

          {updateMutation.isError && (
            <div className="mt-4 p-3 bg-red-50 border border-red-200 rounded-md flex items-center text-red-800">
              <XCircle className="w-4 h-4 mr-2" />
              <span className="text-sm">Failed to save SMTP settings</span>
            </div>
          )}
        </form>
      </div>
    </div>
  );
}
