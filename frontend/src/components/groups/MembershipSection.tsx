import { useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import {
  useApproveHeldMembership,
  useHeldMembership,
  usePluginClassification,
  useRefreshGroupMembership,
  useRejectHeldMembership,
} from '../../hooks/usePlugins';
import { getApiErrorMessage } from '../plugins/pluginHelpers';
import { heldSummary, providerStatusLabel } from '../plugins/classificationHelpers';
import { providerKey, type BehaviorFormState } from './expressionHelpers';

interface Props {
  behavior: BehaviorFormState;
  onBehaviorChange: (next: BehaviorFormState) => void;
  /** Set when editing a saved group: shows status, refresh and held updates */
  groupId: string | null;
  canAdmin: boolean;
}

/** Group kind and membership provider of a dynamic group (M27). */
export default function MembershipSection({
  behavior,
  onBehaviorChange,
  groupId,
  canAdmin,
}: Props) {
  const overview = usePluginClassification();
  const providers = (overview.data?.plugins ?? []).flatMap((p) =>
    p.providers.map((pr) => ({ plugin: p.plugin_id, ...pr }))
  );
  const held = useHeldMembership(behavior.kind === 'dynamic' ? groupId : null);
  const refresh = useRefreshGroupMembership();
  const approve = useApproveHeldMembership();
  const reject = useRejectHeldMembership();
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<BehaviorFormState>) => onBehaviorChange({ ...behavior, ...patch });

  // Without a plugin that offers providers (and for static groups) the section stays out of the way
  if (providers.length === 0 && behavior.kind !== 'dynamic') return null;

  const selected = providers.find((p) => providerKey(p.plugin, p.id) === behavior.provider);
  const status = selected?.groups.find((g) => g.group_id === groupId);
  const busy = refresh.isPending || approve.isPending || reject.isPending;
  const run = async (fn: () => Promise<unknown>, fallback: string) => {
    setError(null);
    setMessage(null);
    try {
      await fn();
    } catch (e) {
      setError(getApiErrorMessage(e, fallback));
    }
  };

  return (
    <fieldset className="space-y-3 rounded border border-gray-200 p-3">
      <legend className="px-1 text-sm font-semibold text-gray-900">Membership</legend>
      <div>
        <label className="label" htmlFor="group-kind">
          Group kind
        </label>
        <select
          id="group-kind"
          value={behavior.kind}
          onChange={(e) => set({ kind: e.target.value === 'dynamic' ? 'dynamic' : 'static' })}
          className="input"
        >
          <option value="static">Static (rules, expression and pinned nodes)</option>
          <option value="dynamic">Dynamic (members come from a plugin provider)</option>
        </select>
      </div>
      {behavior.kind === 'dynamic' && (
        <>
          <div>
            <label className="label" htmlFor="group-provider">
              Membership provider
            </label>
            <select
              id="group-provider"
              value={behavior.provider}
              onChange={(e) => set({ provider: e.target.value })}
              className="input"
            >
              <option value="">Choose a provider</option>
              {providers.map((p) => (
                <option key={providerKey(p.plugin, p.id)} value={providerKey(p.plugin, p.id)}>
                  {p.label} ({p.plugin})
                </option>
              ))}
              {behavior.provider && !selected && (
                <option value={behavior.provider}>{behavior.provider} (not available)</option>
              )}
            </select>
            <p className="text-xs text-gray-500 mt-1">
              Rules and expressions do not apply to a dynamic group. The provider refreshes the
              members on its own schedule; large changes wait for an administrator.
            </p>
          </div>
          <div>
            <label className="label" htmlFor="group-provider-params">
              Provider parameters (JSON)
            </label>
            <textarea
              id="group-provider-params"
              rows={3}
              value={behavior.providerParams}
              onChange={(e) => set({ providerParams: e.target.value })}
              className="input font-mono"
              placeholder="{}"
            />
          </div>
          {groupId && status && (
            <p className="text-xs text-gray-600">
              {status.members} members, {providerStatusLabel(status)}
              {status.last_refresh_at
                ? `, refreshed ${new Date(status.last_refresh_at).toLocaleString()}`
                : ''}
              {status.last_error ? `: ${status.last_error}` : ''}
            </p>
          )}
          {groupId && (
            <div className="flex gap-2">
              <button
                type="button"
                className="btn btn-secondary"
                disabled={!canAdmin || busy}
                title={canAdmin ? undefined : 'Requires the plugins:admin permission'}
                onClick={() =>
                  run(async () => {
                    const o = await refresh.mutateAsync(groupId);
                    setMessage(
                      o.held
                        ? 'The update was held by the churn guard.'
                        : `Refreshed: ${o.added} added, ${o.removed} removed.`
                    );
                  }, 'Refresh failed')
                }
              >
                Refresh now
              </button>
            </div>
          )}
          {held.data && (
            <div role="alert" className="rounded border border-yellow-300 bg-yellow-50 p-2 text-xs">
              <p className="flex items-center gap-1 font-medium text-yellow-800">
                <AlertTriangle className="w-3 h-3" /> Update held by the churn guard
              </p>
              <p className="text-yellow-800">The provider {heldSummary(held.data)}.</p>
              <div className="mt-1 flex gap-2">
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={!canAdmin || busy}
                  onClick={() =>
                    run(() => approve.mutateAsync(held.data!.group_id), 'Approval failed')
                  }
                >
                  Approve
                </button>
                <button
                  type="button"
                  className="btn btn-secondary"
                  disabled={!canAdmin || busy}
                  onClick={() =>
                    run(() => reject.mutateAsync(held.data!.group_id), 'Discard failed')
                  }
                >
                  Discard
                </button>
              </div>
            </div>
          )}
          {message && <p className="text-xs text-gray-600">{message}</p>}
          {error && <p className="text-xs text-red-600">{error}</p>}
        </>
      )}
    </fieldset>
  );
}
