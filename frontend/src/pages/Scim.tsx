import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Copy, KeyRound, Loader2, RotateCw, ShieldOff, Trash2, UserCog } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import type {
  Role,
  ScimGroupRefKind,
  ScimGroupSummary,
  ScimIssuedToken,
  ScimStatus,
  ScimTokenInfo,
} from '../types';

/** Server message of an axios error, with a hint for step-up. */
function errorText(err: unknown): string {
  const data = (err as { response?: { data?: { message?: string; code?: string } } })?.response
    ?.data;
  if (data?.code === 'step_up_required') {
    return 'This change needs a second factor: sign in again with your authenticator, then retry.';
  }
  return data?.message || (err as Error)?.message || 'Request failed';
}

function when(ts?: string | null): string {
  return ts ? new Date(ts).toLocaleString() : 'never';
}

function statusBadge(status: ScimTokenInfo['status']) {
  return clsx(
    'inline-flex px-2 py-0.5 rounded-full text-xs font-medium',
    status === 'active' && 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-300',
    status === 'expired' && 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300',
    status === 'revoked' && 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300'
  );
}

function endpointUrl(status: ScimStatus): string {
  if (status.base_url) return status.endpoint;
  return `${window.location.origin}${status.endpoint}`;
}

function StatusCard({ status }: { status: ScimStatus }) {
  const sso = status.sso.provider
    ? `${status.sso.provider} (linked by ${
        status.sso.link_attribute === 'user_name' ? 'userName' : 'externalId'
      })`
    : 'none: SCIM users cannot sign in';
  return (
    <div className="card mb-6">
      <div className="flex items-center gap-2 mb-4">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Status</h2>
        <span
          className={clsx(
            'inline-flex px-2 py-0.5 rounded-full text-xs font-medium',
            status.enabled
              ? 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-300'
              : 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300'
          )}
        >
          {status.enabled ? 'enabled' : 'disabled'}
        </span>
      </div>
      {!status.enabled && (
        <p className="text-sm text-amber-700 dark:text-amber-300 mb-4">
          SCIM is turned off: every request to the endpoint answers 404. Set{' '}
          <code className="font-mono">auth.scim.enabled: true</code> in config.yaml and restart.
        </p>
      )}
      <dl className="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-1 text-sm">
        <dt className="text-gray-500">Endpoint (tenant URL)</dt>
        <dd className="font-mono text-xs break-all" data-testid="scim-endpoint">
          {endpointUrl(status)}
        </dd>
        <dt className="text-gray-500">Single sign-on of SCIM users</dt>
        <dd>{sso}</dd>
        <dt className="text-gray-500">Users</dt>
        <dd>
          {status.counts.users_active} active, {status.counts.users_deactivated} deactivated,{' '}
          {status.counts.users_deleted} deleted
        </dd>
        <dt className="text-gray-500">Groups</dt>
        <dd>{status.counts.groups}</dd>
        <dt className="text-gray-500">Tokens</dt>
        <dd>
          {status.tokens.active} active of {status.tokens.total}
        </dd>
        <dt className="text-gray-500">Last request</dt>
        <dd>{when(status.last_activity)}</dd>
        <dt className="text-gray-500">Limits</dt>
        <dd>
          {status.limits.rate_limit_per_minute} requests/min per token, bodies up to{' '}
          {Math.round(status.limits.max_body_bytes / 1024)} KiB, pages of{' '}
          {status.limits.max_page_size}
        </dd>
      </dl>
    </div>
  );
}

function IssuedToken({ issued, onDone }: { issued: ScimIssuedToken; onDone: () => void }) {
  const [copied, setCopied] = useState(false);
  return (
    <div
      role="alert"
      className="mb-4 p-3 rounded-lg bg-amber-50 text-amber-900 dark:bg-amber-900/30 dark:text-amber-100 text-sm"
    >
      <p className="font-medium mb-2">
        Token for “{issued.info.name}”. Copy it now: it is not shown again.
      </p>
      <div className="flex gap-2 items-center">
        <code className="flex-1 font-mono text-xs break-all p-2 rounded bg-white dark:bg-gray-800">
          {issued.token}
        </code>
        <button
          className="btn btn-secondary flex items-center"
          onClick={async () => {
            await navigator.clipboard?.writeText(issued.token);
            setCopied(true);
          }}
        >
          <Copy className="w-4 h-4 mr-1" /> {copied ? 'Copied' : 'Copy'}
        </button>
        <button className="btn btn-primary" onClick={onDone}>
          Done
        </button>
      </div>
    </div>
  );
}

function TokensCard({ maxLifetime }: { maxLifetime?: number | null }) {
  const queryClient = useQueryClient();
  const { data: tokens = [], isLoading } = useQuery({
    queryKey: ['scim-tokens'],
    queryFn: api.getScimTokens,
  });
  const [name, setName] = useState('');
  const [days, setDays] = useState(maxLifetime ? String(maxLifetime) : '');
  const [issued, setIssued] = useState<ScimIssuedToken | null>(null);
  const [error, setError] = useState('');
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['scim-tokens'] });
    queryClient.invalidateQueries({ queryKey: ['scim-status'] });
  };
  const expires = days ? Number(days) : undefined;

  const create = useMutation({
    mutationFn: () => api.createScimToken({ name, expires_in_days: expires }),
    onSuccess: (t) => {
      setIssued(t);
      setName('');
      setError('');
      refresh();
    },
    onError: (e) => setError(errorText(e)),
  });
  const rotate = useMutation({
    mutationFn: (id: string) => api.rotateScimToken(id, { expires_in_days: expires }),
    onSuccess: (t) => {
      setIssued(t);
      setError('');
      refresh();
    },
    onError: (e) => setError(errorText(e)),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => api.revokeScimToken(id),
    onSuccess: () => {
      setError('');
      refresh();
    },
    onError: (e) => setError(errorText(e)),
  });

  return (
    <div className="card mb-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">Tokens</h2>
      <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">
        The identity provider sends a token as <code className="font-mono">Bearer</code> credential.
        Only a hash is stored. Rotating replaces the secret at once; for a change without downtime,
        issue a second token, update the provider, then revoke the first.
      </p>
      {issued && <IssuedToken issued={issued} onDone={() => setIssued(null)} />}
      {isLoading ? (
        <Loader2 className="w-4 h-4 animate-spin text-gray-400" />
      ) : tokens.length === 0 ? (
        <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">No token yet.</p>
      ) : (
        <table className="min-w-full text-sm mb-4">
          <thead>
            <tr className="text-left text-xs uppercase text-gray-500 dark:text-gray-400">
              <th className="py-1 pr-3">Name</th>
              <th className="py-1 pr-3">Ends with</th>
              <th className="py-1 pr-3">Status</th>
              <th className="py-1 pr-3">Expires</th>
              <th className="py-1 pr-3">Last used</th>
              <th className="py-1" />
            </tr>
          </thead>
          <tbody>
            {tokens.map((t) => (
              <tr key={t.id} className="border-t border-gray-100 dark:border-gray-700">
                <td className="py-1 pr-3 font-medium">{t.name}</td>
                <td className="py-1 pr-3 font-mono text-xs">…{t.token_hint}</td>
                <td className="py-1 pr-3">
                  <span className={statusBadge(t.status)}>{t.status}</span>
                </td>
                <td className="py-1 pr-3">{t.expires_at ? when(t.expires_at) : 'no expiry'}</td>
                <td className="py-1 pr-3">{when(t.last_used_at)}</td>
                <td className="py-1 text-right whitespace-nowrap">
                  {t.status !== 'revoked' && (
                    <>
                      <button
                        className="text-primary-600 hover:text-primary-800 mr-3"
                        title="Rotate"
                        aria-label={`Rotate token ${t.name}`}
                        onClick={() => {
                          if (
                            window.confirm(`Rotate "${t.name}"? The current secret stops working.`)
                          )
                            rotate.mutate(t.id);
                        }}
                      >
                        <RotateCw className="w-4 h-4" />
                      </button>
                      <button
                        className="text-red-600 hover:text-red-800"
                        title="Revoke"
                        aria-label={`Revoke token ${t.name}`}
                        onClick={() => {
                          if (window.confirm(`Revoke "${t.name}"? Provisioning with it stops.`))
                            revoke.mutate(t.id);
                        }}
                      >
                        <ShieldOff className="w-4 h-4" />
                      </button>
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="flex flex-wrap gap-2">
        <input
          className="input flex-1 min-w-[12rem]"
          placeholder="Token name (for example: Entra ID)"
          aria-label="Token name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <input
          className="input w-40"
          type="number"
          min={1}
          max={maxLifetime ?? 3650}
          placeholder={maxLifetime ? `days (max ${maxLifetime})` : 'days (optional)'}
          aria-label="Expires in days"
          value={days}
          onChange={(e) => setDays(e.target.value)}
        />
        <button
          className="btn btn-primary flex items-center"
          disabled={!name.trim() || create.isPending}
          onClick={() => create.mutate()}
        >
          <KeyRound className="w-4 h-4 mr-1" /> Issue token
        </button>
      </div>
      {error && <p className="mt-2 text-sm text-red-600">{error}</p>}
    </div>
  );
}

const KIND_LABELS: Record<ScimGroupRefKind, string> = {
  display_name: 'Display name',
  external_id: 'External id',
  scim_group: 'SCIM group id',
};

function MappingsCard({ roles, groups }: { roles: Role[]; groups: ScimGroupSummary[] }) {
  const queryClient = useQueryClient();
  const { data: mappings = [] } = useQuery({
    queryKey: ['scim-mappings'],
    queryFn: api.getScimRoleMappings,
  });
  const [groupRef, setGroupRef] = useState('');
  const [kind, setKind] = useState<ScimGroupRefKind>('display_name');
  const [roleId, setRoleId] = useState('');
  const [error, setError] = useState('');
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['scim-mappings'] });
    queryClient.invalidateQueries({ queryKey: ['scim-groups'] });
  };
  const create = useMutation({
    mutationFn: () =>
      api.createScimRoleMapping({ group_ref: groupRef, group_ref_kind: kind, role_id: roleId }),
    onSuccess: () => {
      setGroupRef('');
      setError('');
      refresh();
    },
    onError: (e) => setError(errorText(e)),
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.deleteScimRoleMapping(id),
    onSuccess: refresh,
    onError: (e) => setError(errorText(e)),
  });
  const grantable = roles.filter((r) => r.name !== 'super_admin' && r.name !== 'superadmin');

  return (
    <div className="card mb-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
        Group to role mapping
      </h2>
      <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">
        Members of a pushed group receive the role as soon as the provider changes the membership.
        Roles that are not mapped are never granted, super_admin cannot be mapped, and roles
        assigned by hand are kept.
      </p>
      {mappings.length > 0 ? (
        <table className="min-w-full text-sm mb-4">
          <thead>
            <tr className="text-left text-xs uppercase text-gray-500 dark:text-gray-400">
              <th className="py-1 pr-3">Group</th>
              <th className="py-1 pr-3">Match by</th>
              <th className="py-1 pr-3">Role</th>
              <th className="py-1" />
            </tr>
          </thead>
          <tbody>
            {mappings.map((m) => (
              <tr key={m.id} className="border-t border-gray-100 dark:border-gray-700">
                <td className="py-1 pr-3 break-all">{m.group_label || m.group_ref}</td>
                <td className="py-1 pr-3 text-xs">
                  {KIND_LABELS[m.group_ref_kind] ?? m.group_ref_kind}
                </td>
                <td className="py-1 pr-3">{m.role_name}</td>
                <td className="py-1 text-right">
                  <button
                    onClick={() => remove.mutate(m.id)}
                    className="text-red-600 hover:text-red-800"
                    title="Remove mapping"
                    aria-label={`Remove mapping of ${m.group_label || m.group_ref}`}
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">No group is mapped yet.</p>
      )}
      <div className="flex flex-wrap gap-2">
        <input
          className="input flex-1 min-w-[14rem]"
          placeholder="Group display name, external id or SCIM id"
          aria-label="Group"
          list="scim-group-names"
          value={groupRef}
          onChange={(e) => setGroupRef(e.target.value)}
        />
        <datalist id="scim-group-names">
          {groups.map((g) => (
            <option key={g.id} value={g.display_name} />
          ))}
        </datalist>
        <select
          className="input w-40"
          aria-label="Match by"
          value={kind}
          onChange={(e) => setKind(e.target.value as ScimGroupRefKind)}
        >
          {(Object.keys(KIND_LABELS) as ScimGroupRefKind[]).map((k) => (
            <option key={k} value={k}>
              {KIND_LABELS[k]}
            </option>
          ))}
        </select>
        <select
          className="input w-48"
          aria-label="Role"
          value={roleId}
          onChange={(e) => setRoleId(e.target.value)}
        >
          <option value="">Role...</option>
          {grantable.map((r) => (
            <option key={r.id} value={r.id}>
              {r.display_name}
            </option>
          ))}
        </select>
        <button
          className="btn btn-primary"
          disabled={!groupRef.trim() || !roleId || create.isPending}
          onClick={() => create.mutate()}
        >
          Add mapping
        </button>
      </div>
      {error && <p className="mt-2 text-sm text-red-600">{error}</p>}
    </div>
  );
}

function GroupsCard({ groups }: { groups: ScimGroupSummary[] }) {
  return (
    <div className="card mb-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">Pushed groups</h2>
      {groups.length === 0 ? (
        <p className="text-sm text-gray-500 dark:text-gray-400">
          The identity provider has not pushed any group.
        </p>
      ) : (
        <table className="min-w-full text-sm">
          <thead>
            <tr className="text-left text-xs uppercase text-gray-500 dark:text-gray-400">
              <th className="py-1 pr-3">Display name</th>
              <th className="py-1 pr-3">External id</th>
              <th className="py-1 pr-3">Members</th>
              <th className="py-1 pr-3">Roles</th>
            </tr>
          </thead>
          <tbody>
            {groups.map((g) => (
              <tr key={g.id} className="border-t border-gray-100 dark:border-gray-700">
                <td className="py-1 pr-3 font-medium">{g.display_name}</td>
                <td className="py-1 pr-3 text-xs break-all">{g.external_id ?? '—'}</td>
                <td className="py-1 pr-3">{g.members}</td>
                <td className="py-1 pr-3">{g.roles.length ? g.roles.join(', ') : 'none'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function RecentActivity({ status }: { status: ScimStatus }) {
  if (status.recent.length === 0) return null;
  return (
    <div className="card mb-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
        Recent activity
      </h2>
      <ul className="text-sm divide-y divide-gray-100 dark:divide-gray-700">
        {status.recent.map((r) => (
          <li key={r.id} className="py-1 flex gap-3">
            <span className="text-gray-500 whitespace-nowrap">{when(r.created_at)}</span>
            <span className="font-mono text-xs">{r.action}</span>
            <span className="break-all">{r.resource_id}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export default function Scim() {
  const {
    data: status,
    isLoading,
    error,
  } = useQuery({ queryKey: ['scim-status'], queryFn: api.getScimStatus });
  const { data: roles = [] } = useQuery({ queryKey: ['roles'], queryFn: api.getRoles });
  const { data: groups = [] } = useQuery({ queryKey: ['scim-groups'], queryFn: api.getScimGroups });

  return (
    <div>
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100 flex items-center gap-2">
          <UserCog className="w-6 h-6" /> SCIM Provisioning
        </h1>
        <p className="text-gray-500 dark:text-gray-400 mt-1">
          Users and groups pushed by an identity provider (SCIM 2.0), their tokens and role mapping
        </p>
      </div>
      {isLoading && (
        <div className="flex justify-center h-32 items-center">
          <Loader2 className="w-6 h-6 animate-spin text-primary-600" />
        </div>
      )}
      {error && <p className="text-red-600">{errorText(error)}</p>}
      {status && (
        <>
          <StatusCard status={status} />
          <TokensCard maxLifetime={status.limits.max_token_lifetime_days} />
          <MappingsCard roles={roles} groups={groups} />
          <GroupsCard groups={groups} />
          <RecentActivity status={status} />
        </>
      )}
    </div>
  );
}
