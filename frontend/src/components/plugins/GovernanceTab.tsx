import { useState } from 'react';
import { Loader2, RotateCcw } from 'lucide-react';
import clsx from 'clsx';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import {
  usePluginGovernance,
  usePluginMetrics,
  useResetPluginCircuit,
} from '../../hooks/usePlugins';
import type { PluginGovernanceEntry } from '../../types';
import { ErrorBox, Modal } from './pluginUi';
import { getApiErrorMessage, secondaryBtn } from './pluginHelpers';
import {
  CIRCUIT_LABELS,
  HEALTH_LABELS,
  HEALTH_STYLES,
  formatHour,
  formatMs,
  formatRate,
} from './governanceHelpers';

function Stat({ label, value, warn }: { label: string; value: string | number; warn?: boolean }) {
  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
      <p className="text-xs uppercase tracking-wide text-gray-500 dark:text-gray-400">{label}</p>
      <p
        className={clsx(
          'mt-1 text-2xl font-semibold',
          warn ? 'text-red-600 dark:text-red-400' : 'text-gray-900 dark:text-white'
        )}
      >
        {value}
      </p>
    </div>
  );
}

function MetricsDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const { data, isLoading, error } = usePluginMetrics(id, 24);
  const chart = (data?.series ?? []).map((p) => ({
    hour: formatHour(p.hour),
    ok: Math.max(p.calls - p.errors - p.traps - p.timeouts, 0),
    failed: p.errors + p.traps + p.timeouts,
    refused: p.quota_rejections + p.circuit_rejections,
  }));
  return (
    <Modal title={`Usage of ${id}`} onClose={onClose} wide>
      {error ? <ErrorBox message={getApiErrorMessage(error, 'Could not load metrics')} /> : null}
      {isLoading || !data ? (
        <Loader2 className="w-6 h-6 animate-spin text-blue-500" />
      ) : (
        <div className="space-y-5">
          <dl className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-sm">
            {[
              ['Calls (24 h)', data.totals.calls],
              ['Failure rate', formatRate(data.totals.error_rate)],
              ['p50 / p95', `${formatMs(data.totals.p50_ms)} / ${formatMs(data.totals.p95_ms)}`],
              ['p99 / max', `${formatMs(data.totals.p99_ms)} / ${formatMs(data.totals.max_ms)}`],
              ['Errors', data.totals.errors],
              ['Traps (fuel, memory, crash)', data.totals.traps],
              ['Timeouts', data.totals.timeouts],
              [
                'Refused (quota / breaker)',
                `${data.totals.quota_rejections} / ${data.totals.circuit_rejections}`,
              ],
            ].map(([k, v]) => (
              <div key={String(k)}>
                <dt className="text-gray-500 dark:text-gray-400">{k}</dt>
                <dd className="font-medium text-gray-900 dark:text-white">{v}</dd>
              </div>
            ))}
          </dl>
          {chart.length === 0 ? (
            <p className="text-sm text-gray-500 dark:text-gray-400">
              No handler calls in this period.
            </p>
          ) : (
            <div className="h-64" role="img" aria-label="Handler calls per hour">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={chart}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="hour" />
                  <YAxis allowDecimals={false} />
                  <Tooltip />
                  <Legend />
                  <Bar dataKey="ok" name="Succeeded" stackId="a" fill="#22c55e" />
                  <Bar dataKey="failed" name="Failed" stackId="a" fill="#ef4444" />
                  <Bar dataKey="refused" name="Refused" stackId="a" fill="#f59e0b" />
                </BarChart>
              </ResponsiveContainer>
            </div>
          )}
          {data.by_version.length > 1 && (
            <table className="min-w-full text-sm">
              <thead>
                <tr className="text-left text-gray-500 dark:text-gray-400">
                  <th className="py-1 pr-4">Version</th>
                  <th className="py-1 pr-4">Calls</th>
                  <th className="py-1 pr-4">Failure rate</th>
                  <th className="py-1">p95</th>
                </tr>
              </thead>
              <tbody>
                {data.by_version.map((v) => (
                  <tr key={v.version} className="text-gray-900 dark:text-gray-100">
                    <td className="py-1 pr-4 font-mono">{v.version}</td>
                    <td className="py-1 pr-4">{v.totals.calls}</td>
                    <td className="py-1 pr-4">{formatRate(v.totals.error_rate)}</td>
                    <td className="py-1">{formatMs(v.totals.p95_ms)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </Modal>
  );
}

function CircuitCell({ p }: { p: PluginGovernanceEntry }) {
  if (p.circuit.state === 'closed') {
    return <span className="text-gray-500 dark:text-gray-400">Closed</span>;
  }
  return (
    <div>
      <span className="font-medium text-red-600 dark:text-red-400">
        {CIRCUIT_LABELS[p.circuit.state]}
      </span>
      {p.circuit.retry_at && (
        <p className="text-xs text-gray-500 dark:text-gray-400">
          retry {new Date(p.circuit.retry_at).toLocaleTimeString()}
        </p>
      )}
      {p.circuit.last_error && (
        <p
          className="text-xs text-gray-500 dark:text-gray-400 max-w-xs truncate"
          title={p.circuit.last_error}
        >
          {p.circuit.last_error}
        </p>
      )}
    </div>
  );
}

export default function GovernanceTab({ canAdmin }: { canAdmin: boolean }) {
  const { data, isLoading, error } = usePluginGovernance();
  const reset = useResetPluginCircuit();
  const [detailId, setDetailId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const doReset = async (id: string) => {
    setActionError(null);
    try {
      await reset.mutateAsync(id);
    } catch (e) {
      setActionError(getApiErrorMessage(e, 'Could not reset the circuit breaker'));
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-32">
        <Loader2 className="w-6 h-6 animate-spin text-blue-500" />
      </div>
    );
  }
  if (error || !data) {
    return <ErrorBox message={getApiErrorMessage(error, 'Could not load plugin governance')} />;
  }

  const calls24h = data.plugins.reduce((n, p) => n + p.last_24h.calls, 0);
  const failures24h = data.plugins.reduce(
    (n, p) => n + p.last_24h.errors + p.last_24h.traps + p.last_24h.timeouts,
    0
  );
  const names = new Map(data.plugins.map((p) => [p.id, p.name]));

  return (
    <div className="space-y-4">
      {actionError && <ErrorBox message={actionError} />}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <Stat label="Plugins" value={data.plugins.length} />
        <Stat
          label="Isolated by breaker"
          value={data.open_circuits}
          warn={data.open_circuits > 0}
        />
        <Stat label="Calls (24 h)" value={calls24h} />
        <Stat
          label="Failure rate (24 h)"
          value={formatRate(calls24h ? failures24h / calls24h : 0)}
        />
      </div>

      {data.top_consumers.length > 0 && (
        <p className="text-sm text-gray-600 dark:text-gray-300">
          Top consumers (handler time, 24 h):{' '}
          {data.top_consumers.map((id) => names.get(id) ?? id).join(', ')}
        </p>
      )}

      <div className="bg-white dark:bg-gray-800 rounded-lg shadow overflow-x-auto">
        <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
          <thead className="bg-gray-50 dark:bg-gray-900/50">
            <tr className="text-left text-xs uppercase text-gray-500 dark:text-gray-400">
              <th className="px-4 py-3">Plugin</th>
              <th className="px-4 py-3">Health</th>
              <th className="px-4 py-3">Calls (1 h / 24 h)</th>
              <th className="px-4 py-3">Failures (1 h)</th>
              <th className="px-4 py-3">p95 (1 h)</th>
              <th className="px-4 py-3">Quota / min</th>
              <th className="px-4 py-3">Breaker</th>
              <th className="px-4 py-3 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200 dark:divide-gray-700 text-sm text-gray-900 dark:text-gray-100">
            {data.plugins.length === 0 && (
              <tr>
                <td colSpan={8} className="px-4 py-6 text-center text-gray-500">
                  No plugins installed.
                </td>
              </tr>
            )}
            {data.plugins.map((p) => (
              <tr key={p.id}>
                <td className="px-4 py-3">
                  <button
                    type="button"
                    className="text-left hover:underline"
                    onClick={() => setDetailId(p.id)}
                  >
                    <span className="font-medium">{p.name}</span>
                    <span className="block text-xs font-mono text-gray-500">
                      {p.id} {p.version ?? ''}
                    </span>
                  </button>
                  {p.revoked_versions.length > 0 && (
                    <span className="block text-xs text-red-600 dark:text-red-400">
                      revoked versions: {p.revoked_versions.join(', ')}
                    </span>
                  )}
                </td>
                <td className="px-4 py-3">
                  <span
                    className={clsx(
                      'px-2 py-0.5 text-xs rounded-full font-medium',
                      HEALTH_STYLES[p.health]
                    )}
                  >
                    {HEALTH_LABELS[p.health]}
                  </span>
                </td>
                <td className="px-4 py-3">
                  {p.last_hour.calls} / {p.last_24h.calls}
                </td>
                <td className="px-4 py-3">
                  {p.last_hour.errors + p.last_hour.traps + p.last_hour.timeouts}{' '}
                  <span className="text-gray-500">({formatRate(p.last_hour.error_rate)})</span>
                </td>
                <td className="px-4 py-3">{formatMs(p.last_hour.p95_ms)}</td>
                <td className="px-4 py-3">{p.invocations_per_minute_limit ?? 'unlimited'}</td>
                <td className="px-4 py-3">
                  <CircuitCell p={p} />
                </td>
                <td className="px-4 py-3 text-right">
                  {canAdmin && p.circuit.state !== 'closed' && (
                    <button
                      type="button"
                      className={secondaryBtn}
                      disabled={reset.isPending}
                      onClick={() => doReset(p.id)}
                    >
                      <span className="flex items-center gap-2">
                        <RotateCcw className="w-4 h-4" /> Reset
                      </span>
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="text-xs text-gray-500 dark:text-gray-400">
        Aggregates only: no event payloads or handler output are stored or shown.
      </p>

      {detailId && <MetricsDialog id={detailId} onClose={() => setDetailId(null)} />}
    </div>
  );
}
