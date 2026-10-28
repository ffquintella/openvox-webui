import { useState } from 'react';
import { Copy, KeyRound, Loader2, Plus, Pencil, Ban, Trash2, AlertTriangle } from 'lucide-react';
import clsx from 'clsx';
import {
  useTrustedKeys,
  useAddTrustedKey,
  useUpdateTrustedKey,
  useRevokeTrustedKey,
  useRemoveTrustedKey,
} from '../../hooks/usePlugins';
import type { TrustedKeyView } from '../../types';
import { ErrorBox, Modal } from './pluginUi';
import {
  dangerBtn,
  getApiErrorMessage,
  inputClass,
  parsePluginIdList,
  primaryBtn,
  secondaryBtn,
} from './pluginHelpers';

function shortFingerprint(fp: string): string {
  return fp.length > 16 ? `${fp.slice(0, 8)}...${fp.slice(-4)}` : fp;
}

function dateToIso(date: string): string | undefined {
  if (!date) return undefined;
  return new Date(`${date}T23:59:59Z`).toISOString();
}

function isoToDate(iso: string | null): string {
  return iso ? iso.slice(0, 10) : '';
}

function KeyStateBadge({ k }: { k: TrustedKeyView }) {
  const styles: Record<TrustedKeyView['state'], string> = {
    active: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300',
    revoked: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300',
    expired: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300',
  };
  return (
    <span className={clsx('px-2 py-0.5 text-xs rounded-full font-medium', styles[k.state])}>
      {k.state}
    </span>
  );
}

function AddKeyDialog({ onClose }: { onClose: () => void }) {
  const add = useAddTrustedKey();
  const [publicKey, setPublicKey] = useState('');
  const [label, setLabel] = useState('');
  const [expires, setExpires] = useState('');
  const [scope, setScope] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);

  const ready = publicKey.trim() && label.trim() && password;

  const submit = async () => {
    setError(null);
    try {
      const ids = parsePluginIdList(scope);
      await add.mutateAsync({
        public_key: publicKey.trim(),
        label: label.trim(),
        expires_at: dateToIso(expires),
        allowed_plugin_ids: ids.length > 0 ? ids : undefined,
        password,
      });
      onClose();
    } catch (e) {
      setError(getApiErrorMessage(e, 'Could not add the key'));
    }
  };

  return (
    <Modal title="Add trusted key" onClose={onClose}>
      <div className="space-y-4">
        <label className="block text-sm">
          Public key (PEM, ssh-ed25519 line or base64)
          <textarea
            aria-label="Public key"
            className={inputClass + ' font-mono mt-1'}
            rows={4}
            value={publicKey}
            onChange={(e) => setPublicKey(e.target.value)}
          />
        </label>
        <label className="block text-sm">
          Label
          <input
            aria-label="Label"
            className={inputClass + ' mt-1'}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </label>
        <label className="block text-sm">
          Expires (optional)
          <input
            aria-label="Expiry date"
            type="date"
            className={inputClass + ' mt-1'}
            value={expires}
            onChange={(e) => setExpires(e.target.value)}
          />
        </label>
        <label className="block text-sm">
          Allowed plugin ids (optional, comma separated; empty means any plugin)
          <input
            aria-label="Allowed plugin ids"
            className={inputClass + ' mt-1'}
            value={scope}
            onChange={(e) => setScope(e.target.value)}
          />
        </label>
        <label className="block text-sm">
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
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={primaryBtn}
            disabled={!ready || add.isPending}
            onClick={submit}
          >
            {add.isPending ? 'Adding...' : 'Add key'}
          </button>
        </div>
      </div>
    </Modal>
  );
}

function EditKeyDialog({ keyView, onClose }: { keyView: TrustedKeyView; onClose: () => void }) {
  const update = useUpdateTrustedKey();
  const [label, setLabel] = useState(keyView.label);
  const [expires, setExpires] = useState(isoToDate(keyView.expires_at));
  const [scope, setScope] = useState((keyView.allowed_plugin_ids ?? []).join(', '));
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setError(null);
    const ids = parsePluginIdList(scope);
    try {
      await update.mutateAsync({
        keyId: keyView.key_id,
        request: {
          label: label.trim(),
          expires_at: expires ? dateToIso(expires) : null,
          allowed_plugin_ids: ids.length > 0 ? ids : null,
        },
      });
      onClose();
    } catch (e) {
      setError(getApiErrorMessage(e, 'Could not update the key'));
    }
  };

  return (
    <Modal title={`Edit key ${keyView.key_id}`} onClose={onClose}>
      <div className="space-y-4">
        <label className="block text-sm">
          Label
          <input
            aria-label="Label"
            className={inputClass + ' mt-1'}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </label>
        <label className="block text-sm">
          Expires (clear to remove the expiry)
          <input
            aria-label="Expiry date"
            type="date"
            className={inputClass + ' mt-1'}
            value={expires}
            onChange={(e) => setExpires(e.target.value)}
          />
        </label>
        <label className="block text-sm">
          Allowed plugin ids (comma separated; empty means any plugin)
          <input
            aria-label="Allowed plugin ids"
            className={inputClass + ' mt-1'}
            value={scope}
            onChange={(e) => setScope(e.target.value)}
          />
        </label>
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={primaryBtn}
            disabled={!label.trim() || update.isPending}
            onClick={submit}
          >
            Save
          </button>
        </div>
      </div>
    </Modal>
  );
}

function RevokeKeyDialog({ keyView, onClose }: { keyView: TrustedKeyView; onClose: () => void }) {
  const revoke = useRevokeTrustedKey();
  const [reason, setReason] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setError(null);
    try {
      await revoke.mutateAsync({
        keyId: keyView.key_id,
        request: { reason: reason.trim(), password },
      });
      onClose();
    } catch (e) {
      setError(getApiErrorMessage(e, 'Could not revoke the key'));
    }
  };

  return (
    <Modal title={`Revoke key ${keyView.label}`} onClose={onClose}>
      <div className="space-y-4">
        {keyView.dependent_plugins.length > 0 && (
          <p className="text-sm text-yellow-800 dark:text-yellow-200">
            These plugins are signed by this key and will fail verification:{' '}
            {keyView.dependent_plugins.join(', ')}.
          </p>
        )}
        <label className="block text-sm">
          Reason
          <input
            aria-label="Reason"
            className={inputClass + ' mt-1'}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
        </label>
        <label className="block text-sm">
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
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={dangerBtn}
            disabled={!reason.trim() || !password || revoke.isPending}
            onClick={submit}
          >
            Revoke key
          </button>
        </div>
      </div>
    </Modal>
  );
}

function RemoveKeyDialog({ keyView, onClose }: { keyView: TrustedKeyView; onClose: () => void }) {
  const remove = useRemoveTrustedKey();
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    setError(null);
    try {
      await remove.mutateAsync(keyView.key_id);
      onClose();
    } catch (e) {
      setError(getApiErrorMessage(e, 'Could not remove the key'));
    }
  };
  return (
    <Modal title="Remove trusted key" onClose={onClose}>
      <div className="space-y-4">
        <p className="text-sm text-gray-700 dark:text-gray-300">
          Remove the key <strong>{keyView.label}</strong>? Packages signed by it can no longer be
          installed.
        </p>
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button type="button" className={dangerBtn} disabled={remove.isPending} onClick={submit}>
            Remove key
          </button>
        </div>
      </div>
    </Modal>
  );
}

type Dialog = { type: 'add' } | { type: 'edit' | 'revoke' | 'remove'; key: TrustedKeyView } | null;

export default function TrustedKeysTab({ canManage }: { canManage: boolean }) {
  const { data: keys = [], isLoading, error } = useTrustedKeys();
  const [dialog, setDialog] = useState<Dialog>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const copy = async (fp: string) => {
    try {
      await navigator.clipboard.writeText(fp);
      setCopied(fp);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      // Clipboard may be unavailable; the full fingerprint is also in the tooltip.
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
      <div className="flex items-center justify-between">
        <p className="text-sm text-gray-500 dark:text-gray-400">
          Only packages signed by one of these keys can be installed.
        </p>
        {canManage && (
          <button type="button" className={primaryBtn} onClick={() => setDialog({ type: 'add' })}>
            <span className="flex items-center gap-2">
              <Plus className="w-4 h-4" /> Add key
            </span>
          </button>
        )}
      </div>
      {error ? <ErrorBox message={getApiErrorMessage(error, 'Could not load keys')} /> : null}
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow overflow-x-auto">
        <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
          <thead className="bg-gray-50 dark:bg-gray-900/40">
            <tr>
              {['Label', 'Fingerprint', 'Origin', 'Scope', 'Expires', 'State', 'Used by', ''].map(
                (h) => (
                  <th
                    key={h}
                    className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase"
                  >
                    {h}
                  </th>
                )
              )}
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
            {keys.length === 0 && (
              <tr>
                <td colSpan={8} className="px-4 py-6 text-sm text-center text-gray-500">
                  No trusted keys
                </td>
              </tr>
            )}
            {keys.map((k) => {
              const cannotRemove = k.origin === 'builtin' || k.dependent_plugins.length > 0;
              return (
                <tr key={k.key_id}>
                  <td className="px-4 py-3 text-sm">
                    <div className="flex items-center gap-2">
                      <KeyRound className="w-4 h-4 text-gray-400" />
                      {k.label}
                    </div>
                    {k.revoked_reason && (
                      <p className="text-xs text-gray-500">Revoked: {k.revoked_reason}</p>
                    )}
                  </td>
                  <td className="px-4 py-3 text-sm">
                    <button
                      type="button"
                      title={k.fingerprint}
                      onClick={() => copy(k.fingerprint)}
                      className="flex items-center gap-1 font-mono text-xs hover:text-blue-600"
                    >
                      {shortFingerprint(k.fingerprint)}
                      <Copy className="w-3 h-3" />
                      {copied === k.fingerprint && <span className="text-green-600">copied</span>}
                    </button>
                  </td>
                  <td className="px-4 py-3 text-sm">
                    <span className="px-2 py-0.5 text-xs rounded-full bg-gray-100 dark:bg-gray-700">
                      {k.origin}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-xs">
                    {k.allowed_plugin_ids ? k.allowed_plugin_ids.join(', ') : 'any plugin'}
                  </td>
                  <td className="px-4 py-3 text-sm">
                    {k.expires_at ? isoToDate(k.expires_at) : 'never'}
                    {k.expires_soon && (
                      <span className="ml-2 inline-flex items-center gap-1 text-xs text-yellow-700 dark:text-yellow-300">
                        <AlertTriangle className="w-3 h-3" /> expires soon
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3">
                    <KeyStateBadge k={k} />
                  </td>
                  <td className="px-4 py-3 text-xs">
                    {k.dependent_plugins.length > 0 ? k.dependent_plugins.join(', ') : '-'}
                  </td>
                  <td className="px-4 py-3">
                    {canManage && (
                      <div className="flex justify-end gap-1">
                        <button
                          type="button"
                          title="Edit"
                          aria-label={`Edit ${k.label}`}
                          className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
                          onClick={() => setDialog({ type: 'edit', key: k })}
                        >
                          <Pencil className="w-4 h-4" />
                        </button>
                        <button
                          type="button"
                          title="Revoke"
                          aria-label={`Revoke ${k.label}`}
                          disabled={k.state === 'revoked'}
                          className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded disabled:opacity-40"
                          onClick={() => setDialog({ type: 'revoke', key: k })}
                        >
                          <Ban className="w-4 h-4" />
                        </button>
                        <button
                          type="button"
                          title={
                            k.origin === 'builtin'
                              ? 'Built-in keys cannot be removed'
                              : k.dependent_plugins.length > 0
                                ? 'Plugins depend on this key'
                                : 'Remove'
                          }
                          aria-label={`Remove ${k.label}`}
                          disabled={cannotRemove}
                          className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded disabled:opacity-40"
                          onClick={() => setDialog({ type: 'remove', key: k })}
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {dialog?.type === 'add' && <AddKeyDialog onClose={() => setDialog(null)} />}
      {dialog?.type === 'edit' && (
        <EditKeyDialog keyView={dialog.key} onClose={() => setDialog(null)} />
      )}
      {dialog?.type === 'revoke' && (
        <RevokeKeyDialog keyView={dialog.key} onClose={() => setDialog(null)} />
      )}
      {dialog?.type === 'remove' && (
        <RemoveKeyDialog keyView={dialog.key} onClose={() => setDialog(null)} />
      )}
    </div>
  );
}
