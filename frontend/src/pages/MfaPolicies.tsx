import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { KeyRound, Loader2, Plus, Trash2 } from 'lucide-react';
import { api } from '../services/api';
import type { MfaPolicy } from '../types';

/** Server message of an axios error, with a hint for step-up. */
function errorText(err: unknown): string {
  const data = (err as { response?: { data?: { message?: string; code?: string } } })?.response
    ?.data;
  if (data?.code === 'step_up_required') {
    return 'This change needs a second factor: sign in again with your authenticator, then retry.';
  }
  return data?.message || (err as Error)?.message || 'Request failed';
}

function newPolicy(): MfaPolicy {
  return {
    name: '',
    enabled: true,
    priority: 100,
    conditions: { roles: [], providers: [] },
    required_acr: 2,
    require_phishing_resistant: false,
  };
}

function parseProviders(text: string): string[] {
  return text
    .split(',')
    .map((p) => p.trim())
    .filter(Boolean);
}

interface RowProps {
  policy: MfaPolicy;
  roleNames: string[];
  onChange: (p: MfaPolicy) => void;
  onRemove: () => void;
}

function PolicyRow({ policy, roleNames, onChange, onRemove }: RowProps) {
  const [providers, setProviders] = useState(policy.conditions.providers.join(', '));
  const toggleRole = (role: string) => {
    const roles = policy.conditions.roles.includes(role)
      ? policy.conditions.roles.filter((r) => r !== role)
      : [...policy.conditions.roles, role];
    onChange({ ...policy, conditions: { ...policy.conditions, roles } });
  };

  return (
    <div className="card mb-4" data-testid="policy-row">
      <div className="flex flex-wrap items-end gap-4 mb-4">
        <label className="flex-1 min-w-[200px] text-sm">
          <span className="block text-gray-500 mb-1">Name</span>
          <input
            className="input w-full"
            aria-label="Policy name"
            value={policy.name}
            onChange={(e) => onChange({ ...policy, name: e.target.value })}
          />
        </label>
        <label className="w-28 text-sm">
          <span className="block text-gray-500 mb-1">Priority</span>
          <input
            type="number"
            className="input w-full"
            aria-label="Priority"
            value={policy.priority}
            onChange={(e) => onChange({ ...policy, priority: Number(e.target.value) || 0 })}
          />
        </label>
        <label className="flex items-center gap-2 text-sm pb-2">
          <input
            type="checkbox"
            checked={policy.enabled}
            onChange={(e) => onChange({ ...policy, enabled: e.target.checked })}
          />
          Enabled
        </label>
        <button
          type="button"
          onClick={onRemove}
          className="text-red-600 hover:text-red-800 pb-2"
          aria-label="Remove policy"
        >
          <Trash2 className="w-4 h-4" />
        </button>
      </div>

      <div className="mb-4 text-sm">
        <span className="block text-gray-500 mb-1">
          Applies to roles (none selected means every role)
        </span>
        <div className="flex flex-wrap gap-x-4 gap-y-1">
          {roleNames.map((r) => (
            <label key={r} className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={policy.conditions.roles.includes(r)}
                onChange={() => toggleRole(r)}
              />
              {r}
            </label>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
        <label>
          <span className="block text-gray-500 mb-1">
            Sign-in providers (comma separated, empty means all; for example local, saml)
          </span>
          <input
            className="input w-full"
            aria-label="Providers"
            value={providers}
            onChange={(e) => {
              setProviders(e.target.value);
              onChange({
                ...policy,
                conditions: { ...policy.conditions, providers: parseProviders(e.target.value) },
              });
            }}
          />
        </label>
        <label>
          <span className="block text-gray-500 mb-1">Required factor</span>
          <select
            className="input w-full"
            aria-label="Required factor"
            value={policy.require_phishing_resistant ? 'key' : 'any'}
            onChange={(e) =>
              onChange({
                ...policy,
                required_acr: 2,
                require_phishing_resistant: e.target.value === 'key',
              })
            }
          >
            <option value="any">Any second factor (authenticator, key, recovery code)</option>
            <option value="key">Security key or passkey only</option>
          </select>
        </label>
      </div>
    </div>
  );
}

export default function MfaPolicies() {
  const qc = useQueryClient();
  const { data, isLoading, error } = useQuery({
    queryKey: ['mfa-policies'],
    queryFn: api.getMfaPolicies,
  });
  const { data: roles = [] } = useQuery({ queryKey: ['roles'], queryFn: api.getRoles });
  const [policies, setPolicies] = useState<MfaPolicy[]>([]);
  const [message, setMessage] = useState('');
  const [failure, setFailure] = useState('');

  useEffect(() => {
    if (data) setPolicies(data);
  }, [data]);

  const save = useMutation({
    mutationFn: () => api.saveMfaPolicies(policies),
    onSuccess: (saved) => {
      setFailure('');
      setMessage('Policies saved.');
      setPolicies(saved);
      qc.setQueryData(['mfa-policies'], saved);
    },
    onError: (e) => {
      setMessage('');
      setFailure(errorText(e));
    },
  });

  const roleNames = roles.map((r) => r.name);
  const update = (i: number, p: MfaPolicy) =>
    setPolicies((all) => all.map((x, idx) => (idx === i ? p : x)));
  const canSave = policies.every((p) => p.name.trim() !== '');

  return (
    <div>
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100 flex items-center gap-2">
          <KeyRound className="w-6 h-6" /> MFA Policies
        </h1>
        <p className="text-gray-500 dark:text-gray-400 mt-1">
          Decide who must present a second factor at sign-in. Without an enabled policy, sign-in
          needs only the password, even for users with a registered security key.
        </p>
      </div>

      <div
        role="note"
        className="mb-6 p-3 rounded-lg bg-amber-50 text-amber-900 dark:bg-amber-900/30 dark:text-amber-100 text-sm"
      >
        Before requiring a security key, register a backup key and generate recovery codes. A user
        with no matching factor is locked out, and recovery codes are not accepted when only a
        security key or passkey is required.
      </div>

      {isLoading && (
        <div className="flex justify-center h-32 items-center">
          <Loader2 className="w-6 h-6 animate-spin text-primary-600" />
        </div>
      )}
      {error && <p className="text-red-600">{errorText(error)}</p>}

      {data && (
        <>
          {policies.length === 0 && (
            <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">
              No policy yet: no second factor is requested at sign-in.
            </p>
          )}
          {policies.map((p, i) => (
            <PolicyRow
              key={p.id ?? `new-${i}`}
              policy={p}
              roleNames={roleNames}
              onChange={(next) => update(i, next)}
              onRemove={() => setPolicies((all) => all.filter((_, idx) => idx !== i))}
            />
          ))}

          <div className="flex items-center gap-3">
            <button
              type="button"
              className="btn btn-secondary flex items-center"
              onClick={() => setPolicies((all) => [...all, newPolicy()])}
            >
              <Plus className="w-4 h-4 mr-1" /> Add policy
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={!canSave || save.isPending}
              onClick={() => save.mutate()}
            >
              {save.isPending ? 'Saving...' : 'Save policies'}
            </button>
          </div>
          {!canSave && <p className="text-sm text-amber-700 mt-2">Every policy needs a name.</p>}
          {message && (
            <p role="status" className="text-sm text-green-700 dark:text-green-300 mt-3">
              {message}
            </p>
          )}
          {failure && (
            <p role="alert" className="text-sm text-red-600 mt-3">
              {failure}
            </p>
          )}
        </>
      )}
    </div>
  );
}
