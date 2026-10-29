import { useState } from 'react';
import {
  Puzzle,
  KeyRound,
  Plus,
  Loader2,
  Play,
  Pause,
  ShieldCheck,
  Trash2,
  Info,
  Activity,
  RefreshCw,
} from 'lucide-react';
import clsx from 'clsx';
import {
  usePlugins,
  usePlugin,
  useEnablePlugin,
  useDisablePlugin,
  useVerifyPlugin,
  useUninstallPlugin,
} from '../hooks/usePlugins';
import { usePermissionsStore } from '../stores/permissionsStore';
import InstallPluginDialog from '../components/plugins/InstallPluginDialog';
import TrustedKeysTab from '../components/plugins/TrustedKeysTab';
import GovernanceTab from '../components/plugins/GovernanceTab';
import UpdatesTab from '../components/plugins/UpdatesTab';
import PluginVersionsPanel from '../components/plugins/PluginVersionsPanel';
import { ErrorBox, Modal, RiskBadge, StateBadge } from '../components/plugins/pluginUi';
import {
  dangerBtn,
  getApiErrorMessage,
  primaryBtn,
  secondaryBtn,
} from '../components/plugins/pluginHelpers';
import type { PluginSummary } from '../types';

type TabType = 'installed' | 'updates' | 'governance' | 'keys';

function PluginDetailDialog({
  id,
  canAdmin,
  onClose,
}: {
  id: string;
  canAdmin: boolean;
  onClose: () => void;
}) {
  const { data, isLoading } = usePlugin(id);
  return (
    <Modal title="Plugin details" onClose={onClose} wide>
      {isLoading || !data ? (
        <Loader2 className="w-6 h-6 animate-spin text-blue-500" />
      ) : (
        <div className="space-y-5">
          <div>
            <h4 className="text-base font-semibold text-gray-900 dark:text-white">{data.name}</h4>
            <p className="text-sm font-mono text-gray-500">{data.id}</p>
            {data.state_reason && (
              <p className="mt-1 text-sm text-gray-600 dark:text-gray-300">{data.state_reason}</p>
            )}
          </div>
          <section>
            <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Capabilities</h4>
            {data.capabilities.length === 0 ? (
              <p className="text-sm text-gray-500">None</p>
            ) : (
              <ul className="mt-2 space-y-2">
                {data.capabilities.map((c) => (
                  <li key={c.name} className="flex items-start gap-2 text-sm">
                    <RiskBadge risk={c.risk} />
                    <div>
                      <span className="font-mono">{c.name}</span>
                      <p className="text-gray-600 dark:text-gray-300">{c.description}</p>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>
          <section>
            <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Versions</h4>
            <div className="mt-2">
              <PluginVersionsPanel pluginId={data.id} canAdmin={canAdmin} />
            </div>
          </section>
        </div>
      )}
    </Modal>
  );
}

export default function Plugins() {
  const hasPermission = usePermissionsStore((s) => s.hasPermission);
  const canAdmin = hasPermission('plugins', 'admin');
  const canManageKeys = hasPermission('plugin_trust', 'admin');

  const [activeTab, setActiveTab] = useState<TabType>('installed');
  const [showInstall, setShowInstall] = useState(false);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [confirmUninstall, setConfirmUninstall] = useState<PluginSummary | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const { data: plugins = [], isLoading, error } = usePlugins();
  const enable = useEnablePlugin();
  const disable = useDisablePlugin();
  const verify = useVerifyPlugin();
  const uninstall = useUninstallPlugin();

  const run = async (fn: () => Promise<unknown>, fallback: string) => {
    setActionError(null);
    try {
      await fn();
    } catch (e) {
      setActionError(getApiErrorMessage(e, fallback));
    }
  };

  const tabs = [
    { id: 'installed' as const, name: 'Installed', icon: Puzzle, badge: plugins.length },
    {
      id: 'updates' as const,
      name: 'Updates',
      icon: RefreshCw,
      badge: plugins.filter((p) => p.available_version).length || undefined,
    },
    { id: 'governance' as const, name: 'Governance', icon: Activity, badge: undefined },
    { id: 'keys' as const, name: 'Trusted keys', icon: KeyRound, badge: undefined },
  ];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-gray-900 dark:text-white">Plugins</h1>
          <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
            Install, enable and verify signed plugins
          </p>
        </div>
        {activeTab === 'installed' && canAdmin && (
          <button type="button" className={primaryBtn} onClick={() => setShowInstall(true)}>
            <span className="flex items-center gap-2">
              <Plus className="w-4 h-4" /> Install plugin
            </span>
          </button>
        )}
      </div>

      <div className="border-b border-gray-200 dark:border-gray-700">
        <nav className="flex space-x-8">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={clsx(
                  'flex items-center gap-2 py-4 px-1 border-b-2 font-medium text-sm transition-colors',
                  activeTab === tab.id
                    ? 'border-blue-500 text-blue-600 dark:text-blue-400'
                    : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300 dark:text-gray-400'
                )}
              >
                <Icon className="w-4 h-4" />
                {tab.name}
                {tab.badge !== undefined && (
                  <span className="ml-2 px-2 py-0.5 text-xs rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                    {tab.badge}
                  </span>
                )}
              </button>
            );
          })}
        </nav>
      </div>

      {activeTab === 'installed' && (
        <div className="space-y-4">
          {actionError && <ErrorBox message={actionError} />}
          {error ? (
            <ErrorBox message={getApiErrorMessage(error, 'Could not load plugins')} />
          ) : null}
          {isLoading ? (
            <div className="flex items-center justify-center h-32">
              <Loader2 className="w-6 h-6 animate-spin text-blue-500" />
            </div>
          ) : (
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow overflow-x-auto">
              <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                <thead className="bg-gray-50 dark:bg-gray-900/40">
                  <tr>
                    {['Name', 'Version', 'Publisher', 'Runtime', 'State', 'Signer', ''].map((h) => (
                      <th
                        key={h}
                        className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase"
                      >
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
                  {plugins.length === 0 && (
                    <tr>
                      <td colSpan={7} className="px-4 py-6 text-sm text-center text-gray-500">
                        No plugins installed
                      </td>
                    </tr>
                  )}
                  {plugins.map((p) => (
                    <tr key={p.id}>
                      <td className="px-4 py-3 text-sm">
                        <button
                          type="button"
                          className="text-left hover:text-blue-600"
                          onClick={() => setDetailId(p.id)}
                        >
                          <div className="font-medium">{p.name}</div>
                          <div className="font-mono text-xs text-gray-500">{p.id}</div>
                        </button>
                      </td>
                      <td className="px-4 py-3 text-sm">
                        {p.active_version ?? '-'}
                        {p.available_version && (
                          <div className="text-xs text-blue-600 dark:text-blue-400">
                            {p.available_version} available
                          </div>
                        )}
                        {p.pending_version && (
                          <div className="text-xs text-yellow-700 dark:text-yellow-300">
                            {p.pending_version} waits for approval
                          </div>
                        )}
                      </td>
                      <td className="px-4 py-3 text-sm">{p.publisher}</td>
                      <td className="px-4 py-3 text-sm">{p.runtime_kind ?? '-'}</td>
                      <td className="px-4 py-3 text-sm">
                        <StateBadge state={p.state} reason={p.state_reason} />
                        {p.state_reason && (
                          <p className="mt-1 text-xs text-gray-500 max-w-xs">{p.state_reason}</p>
                        )}
                      </td>
                      <td className="px-4 py-3 text-xs">
                        {p.signer_label ?? '-'}
                        {p.signer_key_id && (
                          <div className="font-mono text-gray-500">
                            {p.signer_key_id.slice(0, 8)}
                          </div>
                        )}
                      </td>
                      <td className="px-4 py-3">
                        {canAdmin && (
                          <div className="flex justify-end gap-1">
                            {p.state === 'enabled' ? (
                              <button
                                type="button"
                                title="Disable"
                                aria-label={`Disable ${p.name}`}
                                className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
                                disabled={disable.isPending}
                                onClick={() =>
                                  run(() => disable.mutateAsync(p.id), 'Disable failed')
                                }
                              >
                                <Pause className="w-4 h-4" />
                              </button>
                            ) : (
                              <button
                                type="button"
                                title="Enable"
                                aria-label={`Enable ${p.name}`}
                                className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
                                disabled={enable.isPending}
                                onClick={() => run(() => enable.mutateAsync(p.id), 'Enable failed')}
                              >
                                <Play className="w-4 h-4" />
                              </button>
                            )}
                            <button
                              type="button"
                              title="Verify signature"
                              aria-label={`Verify ${p.name}`}
                              className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
                              disabled={verify.isPending}
                              onClick={() => run(() => verify.mutateAsync(p.id), 'Verify failed')}
                            >
                              <ShieldCheck className="w-4 h-4" />
                            </button>
                            <button
                              type="button"
                              title="Uninstall"
                              aria-label={`Uninstall ${p.name}`}
                              className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-red-600"
                              onClick={() => setConfirmUninstall(p)}
                            >
                              <Trash2 className="w-4 h-4" />
                            </button>
                          </div>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {!canAdmin && (
            <p className="flex items-center gap-2 text-xs text-gray-500">
              <Info className="w-3 h-3" /> You can view plugins but not change them.
            </p>
          )}
        </div>
      )}

      {activeTab === 'updates' && <UpdatesTab canAdmin={canAdmin} />}

      {activeTab === 'governance' && <GovernanceTab canAdmin={canAdmin} />}

      {activeTab === 'keys' && <TrustedKeysTab canManage={canManageKeys} />}

      {showInstall && (
        <InstallPluginDialog canTrustKeys={canManageKeys} onClose={() => setShowInstall(false)} />
      )}
      {detailId && (
        <PluginDetailDialog id={detailId} canAdmin={canAdmin} onClose={() => setDetailId(null)} />
      )}
      {confirmUninstall && (
        <Modal title="Uninstall plugin" onClose={() => setConfirmUninstall(null)}>
          <div className="space-y-4">
            <p className="text-sm text-gray-700 dark:text-gray-300">
              Uninstall <strong>{confirmUninstall.name}</strong>? Its pages and widgets will
              disappear from the interface.
            </p>
            <div className="flex justify-end gap-3">
              <button
                type="button"
                className={secondaryBtn}
                onClick={() => setConfirmUninstall(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className={dangerBtn}
                disabled={uninstall.isPending}
                onClick={async () => {
                  const id = confirmUninstall.id;
                  setConfirmUninstall(null);
                  await run(() => uninstall.mutateAsync(id), 'Uninstall failed');
                }}
              >
                Uninstall
              </button>
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}
