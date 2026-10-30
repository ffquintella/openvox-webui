import { useState } from 'react';
import { ShieldAlert } from 'lucide-react';
import { useTrustPluginModules, useUntrustPluginModules } from '../../hooks/usePlugins';
import { ErrorBox } from './pluginUi';
import { dangerBtn, getApiErrorMessage, inputClass, primaryBtn } from './pluginHelpers';
import type { PluginDetail } from '../../types';

/**
 * Trusted module pages of the active version (M31): what the plugin would
 * run in the host page without a sandbox, and the approval that allows it.
 */
export default function ModuleTrustPanel({
  plugin,
  canTrust,
  canAdmin,
}: {
  plugin: PluginDetail;
  canTrust: boolean;
  canAdmin: boolean;
}) {
  const view = plugin.module_trust;
  const trust = useTrustPluginModules();
  const untrust = useUntrustPluginModules();
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  if (!view) return null;

  const onTrust = async () => {
    setError(null);
    try {
      await trust.mutateAsync({
        id: plugin.id,
        request: { password, version: plugin.active_version ?? undefined },
      });
      setPassword('');
    } catch (e) {
      setError(getApiErrorMessage(e, 'The module pages could not be trusted'));
    }
  };
  const onUntrust = async () => {
    setError(null);
    try {
      await untrust.mutateAsync(plugin.id);
    } catch (e) {
      setError(getApiErrorMessage(e, 'The approval could not be withdrawn'));
    }
  };

  const canApprove =
    view.allowed_by_config && !view.refusal && !view.trusted && view.contributions.length > 0;

  return (
    <section data-testid="module-trust-panel">
      <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Trusted module pages</h4>
      <div className="mt-2 rounded-md border border-amber-300 bg-amber-50 dark:bg-amber-900/20 p-3 text-sm text-amber-900 dark:text-amber-200 flex gap-2">
        <ShieldAlert className="w-4 h-4 mt-0.5 shrink-0" />
        <span>
          Module pages run inside this application with the signed-in user&apos;s access, without a
          sandbox. Trust only code you have reviewed; a new version needs a new approval.
        </span>
      </div>
      <ul className="mt-2 space-y-1 text-sm">
        {view.contributions.map((c) => (
          <li key={c.id}>
            <span className="font-mono">{c.id}</span> in <span className="font-mono">{c.slot}</span>{' '}
            (<span className="font-mono">{c.src}</span>)
            <div className="font-mono text-xs text-gray-500 break-all">{c.integrity}</div>
          </li>
        ))}
      </ul>
      <p
        className="mt-2 text-sm text-gray-700 dark:text-gray-300"
        data-testid="module-trust-status"
      >
        {view.refusal
          ? view.refusal
          : !view.allowed_by_config
            ? 'Turned off: set plugins.allow_trusted_modules to true to allow trusted module pages.'
            : view.trusted
              ? `Version ${view.trusted.version} trusted by ${view.trusted.trusted_by}${view.served ? '' : ' (not served while the plugin is not enabled)'}.`
              : 'Not trusted: the module pages are not served.'}
      </p>
      {canApprove && canTrust && (
        <div className="mt-2 flex items-end gap-2">
          <label className="block text-sm flex-1">
            Your password
            <input
              aria-label="Your password"
              type="password"
              className={inputClass + ' mt-1'}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="current-password"
            />
          </label>
          <button
            type="button"
            className={primaryBtn}
            disabled={!password || trust.isPending}
            onClick={onTrust}
          >
            Trust version {plugin.active_version}
          </button>
        </div>
      )}
      {view.trusted && (canAdmin || canTrust) && (
        <button
          type="button"
          className={dangerBtn + ' mt-2'}
          disabled={untrust.isPending}
          onClick={onUntrust}
        >
          Withdraw trust
        </button>
      )}
      {error && (
        <div className="mt-2">
          <ErrorBox message={error} />
        </div>
      )}
    </section>
  );
}
