import { useState } from 'react';
import { Loader2, AlertTriangle } from 'lucide-react';
import {
  usePluginVersions,
  useRevokePluginVersion,
  useRollbackPlugin,
  useUnrevokePluginVersion,
  useUpdatePlugin,
} from '../../hooks/usePlugins';
import type { PluginVersionView } from '../../types';
import { ErrorBox, Modal } from './pluginUi';
import { dangerBtn, getApiErrorMessage, inputClass, secondaryBtn } from './pluginHelpers';
import { formatMs, formatRate } from './governanceHelpers';
import { REVOKE_REASON_MAX, canSubmitRevoke, versionActions } from './updateHelpers';

const linkBtn =
  'text-xs px-2 py-1 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50';

function Badges({ v }: { v: PluginVersionView }) {
  return (
    <span className="ml-2 inline-flex flex-wrap gap-1">
      {v.relation === 'active' && <span className="text-xs text-green-600">active</span>}
      {v.pending && <span className="text-xs text-yellow-700">pending approval</span>}
      {v.revoked && <span className="text-xs text-red-600">revoked</span>}
      {v.blocked && <span className="text-xs text-red-600">blocked by hash</span>}
    </span>
  );
}

function RollbackDialog({
  pluginId,
  target,
  onClose,
}: {
  pluginId: string;
  target: PluginVersionView;
  onClose: () => void;
}) {
  const rollback = useRollbackPlugin();
  const [error, setError] = useState<string | null>(null);
  return (
    <Modal title="Roll back plugin" onClose={onClose}>
      <div className="space-y-4 text-sm text-gray-700 dark:text-gray-300">
        <p>
          Reactivate <strong>{target.version}</strong> of{' '}
          <span className="font-mono">{pluginId}</span>?
        </p>
        <p>
          Migrations of the plugin database newer than schema {target.schema_level} are reverted
          with their <span className="font-mono">.down</span> files first. Data those files remove
          is not restored by a later update. The version you leave is skipped by automatic updates
          until you apply it by hand.
        </p>
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={dangerBtn}
            disabled={rollback.isPending}
            onClick={async () => {
              setError(null);
              try {
                await rollback.mutateAsync({ id: pluginId, version: target.version });
                onClose();
              } catch (e) {
                setError(getApiErrorMessage(e, 'Rollback failed'));
              }
            }}
          >
            {rollback.isPending ? 'Rolling back...' : 'Roll back'}
          </button>
        </div>
      </div>
    </Modal>
  );
}

function RevokeDialog({
  pluginId,
  target,
  onClose,
}: {
  pluginId: string;
  target: PluginVersionView;
  onClose: () => void;
}) {
  const revoke = useRevokePluginVersion();
  const [reason, setReason] = useState('');
  const [error, setError] = useState<string | null>(null);
  return (
    <Modal title="Revoke version" onClose={onClose}>
      <div className="space-y-4 text-sm text-gray-700 dark:text-gray-300">
        <p>
          Revoke <strong>{target.version}</strong> of <span className="font-mono">{pluginId}</span>?
          A revoked version never runs.
          {target.relation === 'active' &&
            ' It is the active version, so the plugin is disabled at once; roll back or update to run it again.'}
        </p>
        <label className="block">
          <span>Reason</span>
          <textarea
            aria-label="Revocation reason"
            className={inputClass + ' mt-1'}
            rows={3}
            maxLength={REVOKE_REASON_MAX}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
        </label>
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={dangerBtn}
            disabled={!canSubmitRevoke(reason) || revoke.isPending}
            onClick={async () => {
              setError(null);
              try {
                await revoke.mutateAsync({
                  id: pluginId,
                  version: target.version,
                  reason: reason.trim(),
                });
                onClose();
              } catch (e) {
                setError(getApiErrorMessage(e, 'Revocation failed'));
              }
            }}
          >
            Revoke
          </button>
        </div>
      </div>
    </Modal>
  );
}

/** Stored versions of one plugin with update, rollback and revocation actions. */
export default function PluginVersionsPanel({
  pluginId,
  canAdmin,
}: {
  pluginId: string;
  canAdmin: boolean;
}) {
  const { data: versions = [], isLoading, error } = usePluginVersions(pluginId);
  const update = useUpdatePlugin();
  const unrevoke = useUnrevokePluginVersion();
  const [rollbackTarget, setRollbackTarget] = useState<PluginVersionView | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<PluginVersionView | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const run = async (fn: () => Promise<unknown>, fallback: string) => {
    setActionError(null);
    setNotice(null);
    try {
      await fn();
    } catch (e) {
      setActionError(getApiErrorMessage(e, fallback));
    }
  };

  if (isLoading) return <Loader2 className="w-5 h-5 animate-spin text-blue-500" />;
  if (error) return <ErrorBox message={getApiErrorMessage(error, 'Could not load versions')} />;

  return (
    <div className="space-y-3">
      {actionError && <ErrorBox message={actionError} />}
      {notice && (
        <p role="status" className="text-sm text-gray-700 dark:text-gray-300">
          {notice}
        </p>
      )}
      <div className="overflow-x-auto">
        <table className="min-w-full text-sm">
          <thead>
            <tr className="text-left text-xs uppercase text-gray-500">
              <th className="py-1 pr-4">Version</th>
              <th className="py-1 pr-4">Schema</th>
              <th className="py-1 pr-4">Usage (7 days)</th>
              <th className="py-1 pr-4">Signed by</th>
              <th className="py-1 pr-4">SHA-256</th>
              <th className="py-1">Notes</th>
              {canAdmin && <th className="py-1" />}
            </tr>
          </thead>
          <tbody>
            {versions.map((v) => {
              const actions = versionActions(v);
              return (
                <tr key={v.version} className="align-top">
                  <td className="py-1 pr-4 whitespace-nowrap">
                    <span className="font-mono">{v.version}</span>
                    <Badges v={v} />
                  </td>
                  <td className="py-1 pr-4">{v.schema_level}</td>
                  <td className="py-1 pr-4 text-xs whitespace-nowrap">
                    {v.usage.calls === 0
                      ? '-'
                      : `${v.usage.calls} calls, ${formatRate(v.usage.error_rate)} failed, p95 ${formatMs(v.usage.p95_ms)}`}
                  </td>
                  <td className="py-1 pr-4 font-mono text-xs">{v.signed_by_key_id.slice(0, 8)}</td>
                  <td className="py-1 pr-4 font-mono text-xs" title={v.package_sha256}>
                    {v.package_sha256.slice(0, 12)}...
                  </td>
                  <td className="py-1 text-xs text-gray-600 dark:text-gray-300 max-w-xs">
                    {v.revoked && v.revoked_reason && (
                      <p>
                        Revoked by {v.revoked_by ?? 'unknown'}: {v.revoked_reason}
                      </p>
                    )}
                    {v.blocked_reason && !v.revoked && <p>{v.blocked_reason}</p>}
                    {v.added_capabilities.length > 0 && (
                      <p>Needs approval of {v.added_capabilities.join(', ')}</p>
                    )}
                    {v.rollback_warning && (
                      <p className="flex items-start gap-1 text-yellow-700 dark:text-yellow-300">
                        <AlertTriangle className="w-3 h-3 mt-0.5 flex-shrink-0" />
                        {v.rollback_warning}
                      </p>
                    )}
                  </td>
                  {canAdmin && (
                    <td className="py-1 pl-2">
                      <div className="flex justify-end gap-1 whitespace-nowrap">
                        {actions.update && (
                          <button
                            type="button"
                            className={linkBtn}
                            disabled={update.isPending}
                            onClick={() =>
                              run(async () => {
                                const r = await update.mutateAsync({
                                  id: pluginId,
                                  version: v.version,
                                });
                                setNotice(
                                  r.outcome === 'pending_approval'
                                    ? `${v.version} waits for approval of ${r.added_capabilities.join(', ')}`
                                    : `Updated to ${v.version}`
                                );
                              }, 'Update failed')
                            }
                          >
                            Update to
                          </button>
                        )}
                        {actions.rollback && (
                          <button
                            type="button"
                            className={linkBtn}
                            onClick={() => setRollbackTarget(v)}
                          >
                            Roll back to
                          </button>
                        )}
                        {actions.revoke && (
                          <button
                            type="button"
                            className={linkBtn + ' text-red-600'}
                            onClick={() => setRevokeTarget(v)}
                          >
                            Revoke
                          </button>
                        )}
                        {actions.unrevoke && (
                          <button
                            type="button"
                            className={linkBtn}
                            disabled={unrevoke.isPending}
                            onClick={() =>
                              run(
                                () => unrevoke.mutateAsync({ id: pluginId, version: v.version }),
                                'Could not lift the revocation'
                              )
                            }
                          >
                            Unrevoke
                          </button>
                        )}
                      </div>
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {rollbackTarget && (
        <RollbackDialog
          pluginId={pluginId}
          target={rollbackTarget}
          onClose={() => setRollbackTarget(null)}
        />
      )}
      {revokeTarget && (
        <RevokeDialog
          pluginId={pluginId}
          target={revokeTarget}
          onClose={() => setRevokeTarget(null)}
        />
      )}
    </div>
  );
}
