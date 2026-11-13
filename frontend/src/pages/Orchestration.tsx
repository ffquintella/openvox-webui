import { useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Loader2, Terminal } from 'lucide-react';
import { useBoltStatus } from '../hooks/useOrchestration';
import { useAuthStore } from '../stores/authStore';
import { usePermissionsStore } from '../stores/permissionsStore';
import { errorText } from '../utils/orchestration';
import RunPanel from '../components/orchestration/RunPanel';
import JobsPanel from '../components/orchestration/JobsPanel';
import AllowlistPanel from '../components/orchestration/AllowlistPanel';

type TabId = 'run' | 'jobs' | 'allowlist';

export default function Orchestration() {
  const [params, setParams] = useSearchParams();
  const user = useAuthStore((s) => s.user);
  const hasPermission = usePermissionsStore((s) => s.hasPermission);
  const canCreate = hasPermission('orchestration', 'create');
  const canApprove = hasPermission('orchestration', 'approve');
  const isAdmin = hasPermission('orchestration', 'admin');
  const { data: status, isLoading, error } = useBoltStatus();

  const tabs = useMemo(() => {
    const list: Array<{ id: TabId; label: string }> = [];
    if (canCreate) list.push({ id: 'run', label: 'Run' });
    list.push({ id: 'jobs', label: 'Jobs' });
    if (isAdmin) list.push({ id: 'allowlist', label: 'Allowlist' });
    return list;
  }, [canCreate, isAdmin]);

  const requested = params.get('tab') as TabId | null;
  const active: TabId = tabs.some((t) => t.id === requested) ? (requested as TabId) : tabs[0].id;
  const actor = { userId: user?.id, canApprove, canCreate };

  const select = (tab: TabId, extra: Record<string, string> = {}) =>
    setParams({ tab, ...extra }, { replace: true });

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white flex items-center gap-2">
          <Terminal className="w-6 h-6" /> Orchestration
        </h1>
        <p className="text-sm text-gray-500 mt-1">
          Run allowlisted OpenBolt tasks and plans against nodes, with approval and an audit trail.
        </p>
      </div>

      {isLoading ? (
        <Loader2 className="w-5 h-5 animate-spin" />
      ) : error ? (
        <div role="alert" className="p-3 rounded bg-red-50 text-red-700 text-sm">
          {errorText(error)}
        </div>
      ) : !status?.enabled ? (
        <div className="card text-center py-12" data-testid="orchestration-disabled">
          <Terminal className="w-12 h-12 mx-auto mb-3 text-gray-300" />
          <p className="text-gray-700 dark:text-gray-200 text-lg">Orchestration is disabled</p>
          <p className="text-sm text-gray-500 mt-1">
            An administrator can turn it on with <code>orchestration.bolt.enabled</code> in the
            server configuration.
          </p>
        </div>
      ) : (
        <>
          {status.state === 'degraded' && (
            <div role="alert" className="mb-4 p-3 rounded bg-yellow-50 text-yellow-800 text-sm">
              OpenBolt is not working correctly{status.error ? `: ${status.error}` : '.'} Runs may
              fail.
            </div>
          )}
          {(status.warnings ?? []).map((w) => (
            <div key={w} className="mb-2 p-3 rounded bg-yellow-50 text-yellow-800 text-sm">
              {w}
            </div>
          ))}

          <div className="border-b border-gray-200 dark:border-gray-700 mb-6">
            <nav className="flex gap-4" role="tablist">
              {tabs.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  role="tab"
                  aria-selected={active === t.id}
                  onClick={() => select(t.id)}
                  className={`py-3 px-1 border-b-2 font-medium text-sm transition-colors ${
                    active === t.id
                      ? 'border-primary-500 text-primary-600'
                      : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
                  }`}
                >
                  {t.label}
                </button>
              ))}
            </nav>
          </div>

          {active === 'run' && (
            <RunPanel
              status={status}
              canRunCommands={isAdmin}
              initialNode={params.get('node')}
              onCreated={(job) => select('jobs', { job: job.id })}
            />
          )}
          {active === 'jobs' && <JobsPanel actor={actor} initialJobId={params.get('job')} />}
          {active === 'allowlist' && isAdmin && <AllowlistPanel />}
        </>
      )}
    </div>
  );
}
