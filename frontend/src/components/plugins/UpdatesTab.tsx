import { useState } from 'react';
import { AlertTriangle, CalendarClock, Loader2, RefreshCw } from 'lucide-react';
import {
  useApplyPluginUpdates,
  usePluginUpdates,
  useSetPluginUpdatePolicy,
} from '../../hooks/usePlugins';
import type { PluginAutoUpdatePolicy, PluginUpdateInfo, PluginUpdateResult } from '../../types';
import { ErrorBox, Modal, StateBadge } from './pluginUi';
import { getApiErrorMessage, inputClass, primaryBtn, secondaryBtn } from './pluginHelpers';
import {
  AUTO_UPDATE_LABELS,
  AUTO_UPDATE_SHORT,
  OUTCOME_LABELS,
  WINDOW_MAX_LENGTH,
  describeWindow,
  isValidWindowShape,
  selectableForBatch,
} from './updateHelpers';

const POLICIES: PluginAutoUpdatePolicy[] = ['off', 'patch', 'minor', 'any'];

function PolicyDialog({ item, onClose }: { item: PluginUpdateInfo; onClose: () => void }) {
  const save = useSetPluginUpdatePolicy();
  const [policy, setPolicy] = useState<PluginAutoUpdatePolicy>(item.auto_update);
  const [cron, setCron] = useState(item.update_window ?? '');
  const [error, setError] = useState<string | null>(null);
  const windowOk = isValidWindowShape(cron);
  return (
    <Modal title={`Automatic updates of ${item.name}`} onClose={onClose}>
      <div className="space-y-4 text-sm">
        <label className="block">
          <span className="text-gray-700 dark:text-gray-200">Policy</span>
          <select
            aria-label="Automatic update policy"
            className={inputClass + ' mt-1'}
            value={policy}
            onChange={(e) => setPolicy(e.target.value as PluginAutoUpdatePolicy)}
          >
            {POLICIES.map((p) => (
              <option key={p} value={p}>
                {AUTO_UPDATE_LABELS[p]}
              </option>
            ))}
          </select>
        </label>
        <label className="block">
          <span className="text-gray-700 dark:text-gray-200">Update window (cron, UTC)</span>
          <input
            aria-label="Update window"
            className={inputClass + ' mt-1 font-mono'}
            placeholder="* 2-3 * * 6"
            maxLength={WINDOW_MAX_LENGTH}
            value={cron}
            onChange={(e) => setCron(e.target.value)}
            spellCheck={false}
          />
          <span className="mt-1 block text-xs text-gray-500">
            Five fields: minute, hour, day of month, month, day of week. Updates are applied in a
            matching minute; use ranges for a window, for example{' '}
            <span className="font-mono">* 2-3 * * 6</span> is Saturdays from 02:00 to 03:59 UTC.
            Leave empty to allow any time. An update that adds capabilities still waits for
            approval.
          </span>
        </label>
        {!windowOk && <ErrorBox message="The window must be a cron expression with five fields" />}
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={primaryBtn}
            disabled={!windowOk || save.isPending}
            onClick={async () => {
              setError(null);
              try {
                await save.mutateAsync({
                  id: item.plugin_id,
                  request: {
                    auto_update: policy,
                    update_window: cron.trim() === '' ? null : cron.trim(),
                  },
                });
                onClose();
              } catch (e) {
                setError(getApiErrorMessage(e, 'Could not save the update policy'));
              }
            }}
          >
            Save
          </button>
        </div>
      </div>
    </Modal>
  );
}

function BatchResults({ results }: { results: PluginUpdateResult[] }) {
  return (
    <div
      role="status"
      className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4"
    >
      <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Batch update</h4>
      <ul className="mt-2 space-y-1 text-sm">
        {results.map((r) => (
          <li key={r.plugin_id}>
            <span className="font-mono">{r.plugin_id}</span>:{' '}
            <span className={r.outcome === 'failed' ? 'text-red-600' : undefined}>
              {OUTCOME_LABELS[r.outcome]}
            </span>
            {r.to_version && r.outcome !== 'failed' && ` (${r.from_version} to ${r.to_version})`}
            {r.message && <span className="text-gray-500"> {r.message}</span>}
          </li>
        ))}
      </ul>
    </div>
  );
}

function AutoCell({ item }: { item: PluginUpdateInfo }) {
  return (
    <div className="text-xs">
      <div className="font-medium">{AUTO_UPDATE_SHORT[item.auto_update]}</div>
      {item.auto_update !== 'off' && (
        <>
          <div className="text-gray-500">{describeWindow(item.update_window)}</div>
          {item.next_window_at && (
            <div className="text-gray-500">
              next {new Date(item.next_window_at).toLocaleString()}
            </div>
          )}
        </>
      )}
      {item.update_hold_version && (
        <div className="text-yellow-700 dark:text-yellow-300" title={item.update_hold_reason ?? ''}>
          skips {item.update_hold_version}
        </div>
      )}
    </div>
  );
}

/** Available updates, batch apply and the automatic update policy per plugin. */
export default function UpdatesTab({ canAdmin }: { canAdmin: boolean }) {
  const { data: items = [], isLoading, error } = usePluginUpdates();
  const apply = useApplyPluginUpdates();
  const [selected, setSelected] = useState<string[]>([]);
  const [results, setResults] = useState<PluginUpdateResult[] | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [policyFor, setPolicyFor] = useState<PluginUpdateInfo | null>(null);

  const selectable = selectableForBatch(items);
  const chosen = selected.filter((id) => selectable.includes(id));
  const toggle = (id: string) =>
    setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));

  const runBatch = async () => {
    setActionError(null);
    setResults(null);
    try {
      setResults(await apply.mutateAsync(chosen));
      setSelected([]);
    } catch (e) {
      setActionError(getApiErrorMessage(e, 'Batch update failed'));
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-32">
        <Loader2 className="w-6 h-6 animate-spin text-blue-500" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {error ? <ErrorBox message={getApiErrorMessage(error, 'Could not load updates')} /> : null}
      {actionError && <ErrorBox message={actionError} />}
      {results && <BatchResults results={results} />}
      <div className="flex items-center justify-between">
        <p className="text-sm text-gray-500 dark:text-gray-400">
          Upload a newer package with "Store only" to stage it, then apply it here, in a batch or in
          the plugin's update window.
        </p>
        {canAdmin && (
          <button
            type="button"
            className={primaryBtn}
            disabled={chosen.length === 0 || apply.isPending}
            onClick={runBatch}
          >
            <span className="flex items-center gap-2">
              <RefreshCw className="w-4 h-4" />
              {apply.isPending ? 'Updating...' : `Update selected (${chosen.length})`}
            </span>
          </button>
        )}
      </div>
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow overflow-x-auto">
        <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
          <thead className="bg-gray-50 dark:bg-gray-900/40">
            <tr>
              {(canAdmin ? [''] : [])
                .concat(['Plugin', 'Active', 'Available', 'Notes', 'Automatic updates'])
                .concat(canAdmin ? [''] : [])
                .map((h, i) => (
                  <th
                    key={`${h}-${i}`}
                    className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase"
                  >
                    {h}
                  </th>
                ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
            {items.length === 0 && (
              <tr>
                <td colSpan={7} className="px-4 py-6 text-sm text-center text-gray-500">
                  No plugins installed
                </td>
              </tr>
            )}
            {items.map((item) => (
              <tr key={item.plugin_id} className="align-top">
                {canAdmin && (
                  <td className="px-4 py-3">
                    <input
                      type="checkbox"
                      aria-label={`Select ${item.name}`}
                      disabled={!selectable.includes(item.plugin_id)}
                      checked={chosen.includes(item.plugin_id)}
                      onChange={() => toggle(item.plugin_id)}
                    />
                  </td>
                )}
                <td className="px-4 py-3 text-sm">
                  <div className="font-medium">{item.name}</div>
                  <div className="font-mono text-xs text-gray-500">{item.plugin_id}</div>
                  <div className="mt-1">
                    <StateBadge state={item.state} />
                  </div>
                </td>
                <td className="px-4 py-3 text-sm font-mono">{item.active_version ?? '-'}</td>
                <td className="px-4 py-3 text-sm font-mono">
                  {item.available_version ?? <span className="text-gray-400">none</span>}
                  {item.pending_version && (
                    <div className="text-xs text-yellow-700 dark:text-yellow-300 font-sans">
                      {item.pending_version} waits for approval
                    </div>
                  )}
                </td>
                <td className="px-4 py-3 text-xs text-gray-600 dark:text-gray-300 max-w-sm">
                  {item.requires_approval && (
                    <p>Adds {item.added_capabilities.join(', ')}: needs approval before it runs</p>
                  )}
                  {item.rollback_warning && (
                    <p className="flex items-start gap-1 text-yellow-700 dark:text-yellow-300">
                      <AlertTriangle className="w-3 h-3 mt-0.5 flex-shrink-0" />
                      {item.rollback_warning}
                    </p>
                  )}
                </td>
                <td className="px-4 py-3">
                  <AutoCell item={item} />
                </td>
                {canAdmin && (
                  <td className="px-4 py-3">
                    <button
                      type="button"
                      title="Automatic updates"
                      aria-label={`Automatic updates of ${item.name}`}
                      className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
                      onClick={() => setPolicyFor(item)}
                    >
                      <CalendarClock className="w-4 h-4" />
                    </button>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {policyFor && <PolicyDialog item={policyFor} onClose={() => setPolicyFor(null)} />}
    </div>
  );
}
