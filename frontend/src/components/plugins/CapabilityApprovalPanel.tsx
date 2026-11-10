import { useEffect, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { useApprovePluginCapabilities } from '../../hooks/usePlugins';
import { ErrorBox, RiskBadge } from './pluginUi';
import { getApiErrorMessage, primaryBtn } from './pluginHelpers';
import type { PluginCapabilityApproval } from '../../types';

/**
 * Lets an administrator grant the capabilities a plugin version declares.
 * Every declared capability starts ticked unless policy blocks it; only the
 * ticked ones are sent, the rest stay denied.
 */
export default function CapabilityApprovalPanel({
  pluginId,
  approvals,
  pendingVersion,
  canAdmin,
}: {
  pluginId: string;
  approvals: PluginCapabilityApproval[];
  pendingVersion: string | null;
  canAdmin: boolean;
}) {
  const approve = useApprovePluginCapabilities();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const key = approvals.map((a) => `${a.name}:${a.granted}`).join('|');
  useEffect(() => {
    setSelected(new Set(approvals.filter((a) => !a.blocked_by_policy).map((a) => a.name)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const toggle = (name: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });

  const submit = async () => {
    setError(null);
    try {
      await approve.mutateAsync({
        id: pluginId,
        request: {
          capabilities: [...selected],
          ...(pendingVersion ? { version: pendingVersion } : {}),
        },
      });
    } catch (e) {
      setError(getApiErrorMessage(e, 'Approving capabilities failed'));
    }
  };

  const allGranted = approvals.length > 0 && approvals.every((a) => a.granted);

  return (
    <section>
      <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Capabilities</h4>
      {approvals.length === 0 ? (
        <p className="text-sm text-gray-500">None</p>
      ) : (
        <ul className="mt-2 space-y-2">
          {approvals.map((c) => (
            <li key={c.name} className="flex items-start gap-2 text-sm">
              {canAdmin && (
                <input
                  type="checkbox"
                  className="mt-1"
                  aria-label={`Approve ${c.name}`}
                  checked={selected.has(c.name)}
                  disabled={c.blocked_by_policy || approve.isPending}
                  onChange={() => toggle(c.name)}
                />
              )}
              <RiskBadge risk={c.risk} />
              <div className="flex-1">
                <span className="font-mono">{c.name}</span>
                <span
                  className={
                    c.granted
                      ? 'ml-2 text-xs text-green-700 dark:text-green-400'
                      : 'ml-2 text-xs text-gray-500'
                  }
                >
                  {c.blocked_by_policy
                    ? 'blocked by policy'
                    : c.granted
                      ? 'approved'
                      : 'not approved'}
                </span>
                <p className="text-gray-600 dark:text-gray-300">{c.description}</p>
              </div>
            </li>
          ))}
        </ul>
      )}
      {error && <ErrorBox message={error} />}
      {canAdmin && approvals.length > 0 && (
        <div className="mt-3 flex items-center gap-3">
          <button
            type="button"
            className={primaryBtn}
            disabled={approve.isPending}
            onClick={submit}
          >
            {approve.isPending ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : allGranted ? (
              'Update approval'
            ) : (
              'Approve selected'
            )}
          </button>
          {pendingVersion && (
            <span className="text-xs text-gray-500">
              Applies to pending version {pendingVersion}
            </span>
          )}
        </div>
      )}
    </section>
  );
}
