import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Copy, KeyRound, Loader2, Plus, Trash2 } from 'lucide-react';
import { api } from '../services/api';
import type { CreatedApiKey } from '../types';

function errorText(err: unknown): string {
  const data = (err as { response?: { data?: { message?: string } } })?.response?.data;
  return data?.message || (err as Error)?.message || 'Request failed';
}

function fmt(value?: string | null, empty = 'Never'): string {
  return value ? new Date(value).toLocaleString() : empty;
}

export default function ApiKeys() {
  const queryClient = useQueryClient();
  const [name, setName] = useState('');
  const [expiry, setExpiry] = useState('');
  const [created, setCreated] = useState<CreatedApiKey | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { data: keys = [], isLoading } = useQuery({
    queryKey: ['api-keys'],
    queryFn: api.getApiKeys,
  });

  const createMutation = useMutation({
    mutationFn: () =>
      api.createApiKey({
        name: name.trim(),
        expires_at: expiry ? new Date(`${expiry}T23:59:59`).toISOString() : undefined,
      }),
    onSuccess: (key) => {
      setCreated(key);
      setCopied(false);
      setError(null);
      setName('');
      setExpiry('');
      queryClient.invalidateQueries({ queryKey: ['api-keys'] });
    },
    onError: (err) => setError(errorText(err)),
  });

  const revokeMutation = useMutation({
    mutationFn: (id: string) => api.revokeApiKey(id),
    onSuccess: () => {
      setError(null);
      queryClient.invalidateQueries({ queryKey: ['api-keys'] });
    },
    onError: (err) => setError(errorText(err)),
  });

  const copy = async () => {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(created.key);
      setCopied(true);
    } catch {
      setError('Could not copy; select the key and copy it manually.');
    }
  };

  const revoke = (id: string, keyName: string) => {
    if (window.confirm(`Revoke API key "${keyName}"? Clients using it will stop working.`)) {
      revokeMutation.mutate(id);
    }
  };

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white flex items-center gap-2">
          <KeyRound className="w-6 h-6" /> API Keys
        </h1>
        <p className="text-sm text-gray-500 mt-1">
          Keys let scripts and integrations call the API with the header <code>X-API-Key</code>. A
          key acts with your roles.
        </p>
      </div>

      {error && (
        <div role="alert" className="mb-4 p-3 rounded bg-red-50 text-red-700 text-sm">
          {error}
        </div>
      )}

      {created && (
        <div className="card mb-6 border border-yellow-400" data-testid="created-key">
          <p className="text-sm font-medium mb-2">
            Copy the key for &ldquo;{created.name}&rdquo; now. It will not be shown again.
          </p>
          <div className="flex items-center gap-2">
            <code className="flex-1 break-all p-2 rounded bg-gray-100 dark:bg-gray-800 text-sm">
              {created.key}
            </code>
            <button type="button" className="btn btn-secondary" onClick={copy}>
              {copied ? <Check className="w-4 h-4" /> : <Copy className="w-4 h-4" />}
              <span className="ml-1">{copied ? 'Copied' : 'Copy'}</span>
            </button>
            <button type="button" className="btn btn-secondary" onClick={() => setCreated(null)}>
              Dismiss
            </button>
          </div>
        </div>
      )}

      <form
        className="card mb-6 flex flex-wrap items-end gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) createMutation.mutate();
        }}
      >
        <label className="flex-1 min-w-[200px] text-sm">
          <span className="block text-gray-500 mb-1">Name</span>
          <input
            className="input w-full"
            aria-label="Key name"
            placeholder="e.g. CI pipeline"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label className="text-sm">
          <span className="block text-gray-500 mb-1">Expires (optional)</span>
          <input
            type="date"
            className="input"
            aria-label="Expiry date"
            value={expiry}
            onChange={(e) => setExpiry(e.target.value)}
          />
        </label>
        <button
          type="submit"
          className="btn btn-primary"
          disabled={!name.trim() || createMutation.isPending}
        >
          {createMutation.isPending ? (
            <Loader2 className="w-4 h-4 animate-spin" />
          ) : (
            <Plus className="w-4 h-4" />
          )}
          <span className="ml-1">Create key</span>
        </button>
      </form>

      <div className="card overflow-x-auto">
        {isLoading ? (
          <Loader2 className="w-5 h-5 animate-spin" />
        ) : keys.length === 0 ? (
          <p className="text-sm text-gray-500">You have no API keys.</p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-gray-500">
                <th className="py-2">Name</th>
                <th>Created</th>
                <th>Last used</th>
                <th>Expires</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {keys.map((k) => (
                <tr key={k.id} className="border-t border-gray-200 dark:border-gray-700">
                  <td className="py-2 font-medium">{k.name}</td>
                  <td>{fmt(k.created_at)}</td>
                  <td>{fmt(k.last_used_at)}</td>
                  <td>{fmt(k.expires_at, 'No expiry')}</td>
                  <td className="text-right">
                    <button
                      type="button"
                      className="text-red-600 hover:text-red-800"
                      aria-label={`Revoke ${k.name}`}
                      onClick={() => revoke(k.id, k.name)}
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
