import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  BookUser,
  CheckCircle,
  Loader2,
  Plug,
  RefreshCw,
  Search,
  Trash2,
  XCircle,
} from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import type {
  DirectoryGroupEntry,
  DirectoryInfo,
  DirectorySyncRun,
  DirectoryTestResult,
  Role,
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

function runBadge(status: DirectorySyncRun['status']) {
  return clsx(
    'inline-flex px-2 py-0.5 rounded-full text-xs font-medium',
    status === 'ok' && 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-300',
    status === 'partial' && 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300',
    status === 'failed' && 'bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-300',
    status === 'running' && 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300'
  );
}

function SyncHistory({ directoryId }: { directoryId: string }) {
  const { data: runs = [], isLoading } = useQuery({
    queryKey: ['directory-sync-runs', directoryId],
    queryFn: () => api.getDirectorySyncRuns(directoryId),
    refetchInterval: 10000,
  });
  if (isLoading) return <Loader2 className="w-4 h-4 animate-spin text-gray-400" />;
  if (runs.length === 0) {
    return <p className="text-sm text-gray-500 dark:text-gray-400">No synchronization yet.</p>;
  }
  return (
    <table className="min-w-full text-sm">
      <thead>
        <tr className="text-left text-xs uppercase text-gray-500 dark:text-gray-400">
          <th className="py-1 pr-3">Started</th>
          <th className="py-1 pr-3">Trigger</th>
          <th className="py-1 pr-3">Status</th>
          <th className="py-1 pr-3">Created</th>
          <th className="py-1 pr-3">Updated</th>
          <th className="py-1 pr-3">Deactivated</th>
          <th className="py-1 pr-3">Reactivated</th>
          <th className="py-1 pr-3">Errors</th>
        </tr>
      </thead>
      <tbody>
        {runs.map((r) => (
          <tr key={r.id} className="border-t border-gray-100 dark:border-gray-700">
            <td className="py-1 pr-3 whitespace-nowrap">
              {new Date(r.started_at).toLocaleString()}
            </td>
            <td className="py-1 pr-3">{r.trigger_kind}</td>
            <td className="py-1 pr-3">
              <span className={runBadge(r.status)} title={r.error ?? undefined}>
                {r.status}
              </span>
            </td>
            <td className="py-1 pr-3">{r.created}</td>
            <td className="py-1 pr-3">{r.updated}</td>
            <td className="py-1 pr-3">{r.deactivated}</td>
            <td className="py-1 pr-3">{r.reactivated}</td>
            <td className="py-1 pr-3">{r.errors}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function RoleMappings({ directory, roles }: { directory: DirectoryInfo; roles: Role[] }) {
  const queryClient = useQueryClient();
  const key = ['directory-mappings', directory.id];
  const { data: mappings = [] } = useQuery({
    queryKey: key,
    queryFn: () => api.getDirectoryRoleMappings(directory.id),
  });
  const [query, setQuery] = useState('');
  const [groups, setGroups] = useState<DirectoryGroupEntry[]>([]);
  const [groupRef, setGroupRef] = useState('');
  const [groupLabel, setGroupLabel] = useState('');
  const [roleId, setRoleId] = useState('');
  const [error, setError] = useState('');

  const search = useMutation({
    mutationFn: () => api.searchDirectoryGroups(directory.id, query),
    onSuccess: (g) => {
      setGroups(g);
      setError('');
    },
    onError: (e) => setError(errorText(e)),
  });
  const create = useMutation({
    mutationFn: () =>
      api.createDirectoryRoleMapping(directory.id, {
        group_ref: groupRef,
        role_id: roleId,
        group_label: groupLabel || undefined,
      }),
    onSuccess: () => {
      setGroupRef('');
      setGroupLabel('');
      setGroups([]);
      setError('');
      queryClient.invalidateQueries({ queryKey: key });
      queryClient.invalidateQueries({ queryKey: ['directories'] });
    },
    onError: (e) => setError(errorText(e)),
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.deleteDirectoryRoleMapping(directory.id, id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: key });
      queryClient.invalidateQueries({ queryKey: ['directories'] });
    },
    onError: (e) => setError(errorText(e)),
  });
  const grantable = roles.filter((r) => r.name !== 'super_admin' && r.name !== 'superadmin');

  return (
    <div>
      <p className="text-sm text-gray-500 dark:text-gray-400 mb-3">
        Members of a group (nested groups included on Active Directory) receive the role at every
        login and synchronization. Roles that are not mapped are never granted, and roles assigned
        by hand are kept.
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
                <td className="py-1 pr-3">
                  <div className="font-medium">{m.group_label || m.group_ref}</div>
                  {m.group_label && (
                    <div className="text-xs text-gray-500 break-all">{m.group_ref}</div>
                  )}
                </td>
                <td className="py-1 pr-3 uppercase text-xs">{m.group_ref_kind}</td>
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

      <div className="space-y-2">
        {directory.modes.lookup && (
          <div className="flex gap-2">
            <input
              className="input flex-1"
              placeholder="Search groups by name"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <button
              className="btn btn-secondary flex items-center"
              onClick={() => search.mutate()}
              disabled={search.isPending}
            >
              <Search className="w-4 h-4 mr-1" /> Search
            </button>
          </div>
        )}
        {groups.length > 0 && (
          <ul className="max-h-40 overflow-y-auto border rounded border-gray-200 dark:border-gray-700">
            {groups.map((g) => (
              <li key={g.dn}>
                <button
                  className={clsx(
                    'w-full text-left px-3 py-1 text-sm hover:bg-gray-50 dark:hover:bg-gray-700',
                    groupRef === g.dn && 'bg-primary-50 dark:bg-primary-900/30'
                  )}
                  onClick={() => {
                    setGroupRef(g.dn);
                    setGroupLabel(g.name);
                  }}
                >
                  <span className="font-medium">{g.name}</span>
                  <span className="block text-xs text-gray-500 break-all">{g.dn}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="flex flex-wrap gap-2">
          <input
            className="input flex-1 min-w-[16rem]"
            placeholder="Group DN, SID (S-1-5-...) or objectGUID"
            value={groupRef}
            onChange={(e) => setGroupRef(e.target.value)}
          />
          <select className="input w-48" value={roleId} onChange={(e) => setRoleId(e.target.value)}>
            <option value="">Role...</option>
            {grantable.map((r) => (
              <option key={r.id} value={r.id}>
                {r.display_name}
              </option>
            ))}
          </select>
          <button
            className="btn btn-primary"
            disabled={!groupRef || !roleId || create.isPending}
            onClick={() => create.mutate()}
          >
            Add mapping
          </button>
        </div>
        {error && <p className="text-sm text-red-600">{error}</p>}
      </div>
    </div>
  );
}

function DirectoryCard({ directory, roles }: { directory: DirectoryInfo; roles: Role[] }) {
  const queryClient = useQueryClient();
  const [test, setTest] = useState<DirectoryTestResult | null>(null);
  const [message, setMessage] = useState('');

  const testMutation = useMutation({
    mutationFn: () => api.testDirectory(directory.id),
    onSuccess: setTest,
    onError: (e) => setTest({ ok: false, message: errorText(e) }),
  });
  const syncMutation = useMutation({
    mutationFn: () => api.syncDirectory(directory.id),
    onSuccess: () => {
      setMessage('Synchronization started; the history updates when it finishes.');
      setTimeout(() => {
        queryClient.invalidateQueries({ queryKey: ['directory-sync-runs', directory.id] });
        queryClient.invalidateQueries({ queryKey: ['directories'] });
      }, 2000);
    },
    onError: (e) => setMessage(errorText(e)),
  });

  return (
    <div className="card mb-6">
      <div className="flex items-start justify-between mb-4">
        <div>
          <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
            {directory.label}
            <span className="ml-2 text-sm font-normal text-gray-500">({directory.id})</span>
          </h2>
          <p className="text-sm text-gray-500 dark:text-gray-400">
            {directory.kind === 'active_directory' ? 'Active Directory' : 'LDAP'} ·{' '}
            {directory.enabled ? 'enabled' : 'disabled'} · {directory.servers.join(', ')}
          </p>
        </div>
        <div className="flex gap-2">
          <button
            className="btn btn-secondary flex items-center"
            onClick={() => testMutation.mutate()}
            disabled={testMutation.isPending}
          >
            {testMutation.isPending ? (
              <Loader2 className="w-4 h-4 mr-1 animate-spin" />
            ) : (
              <Plug className="w-4 h-4 mr-1" />
            )}
            Test connection
          </button>
          <button
            className="btn btn-primary flex items-center"
            onClick={() => syncMutation.mutate()}
            disabled={!directory.enabled || directory.sync_running || syncMutation.isPending}
          >
            <RefreshCw className={clsx('w-4 h-4 mr-1', directory.sync_running && 'animate-spin')} />
            Sync now
          </button>
        </div>
      </div>

      {test && (
        <div
          className={clsx(
            'mb-4 p-3 rounded-lg flex items-start gap-2 text-sm',
            test.ok
              ? 'bg-green-50 text-green-800 dark:bg-green-900/30 dark:text-green-200'
              : 'bg-red-50 text-red-800 dark:bg-red-900/30 dark:text-red-200'
          )}
        >
          {test.ok ? (
            <CheckCircle className="w-4 h-4 mt-0.5" />
          ) : (
            <XCircle className="w-4 h-4 mt-0.5" />
          )}
          <span>
            {test.ok
              ? `Connected to ${test.server} in ${test.elapsed_ms} ms; users ${
                  test.users_found ? 'found' : 'not found'
                }, groups ${test.groups_found ? 'found' : 'not found'}.`
              : `${test.stage ? `${test.stage}: ` : ''}${test.message ?? test.error}`}
          </span>
        </div>
      )}
      {message && <p className="mb-4 text-sm text-gray-600 dark:text-gray-300">{message}</p>}

      <dl className="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-1 text-sm mb-6">
        <dt className="text-gray-500">Base DN</dt>
        <dd className="break-all">{directory.base_dn}</dd>
        <dt className="text-gray-500">Service account</dt>
        <dd className="break-all">{directory.bind_dn}</dd>
        <dt className="text-gray-500">User filter</dt>
        <dd className="break-all font-mono text-xs">{directory.user_filter}</dd>
        <dt className="text-gray-500">Accepted realms</dt>
        <dd>{directory.domains.length ? directory.domains.join(', ') : 'none'}</dd>
        <dt className="text-gray-500">Modes</dt>
        <dd>
          {[
            directory.modes.lookup && 'lookup',
            directory.modes.jit && 'just-in-time provisioning',
            directory.modes.sync.enabled &&
              `sync every ${Math.round(directory.modes.sync.interval_secs / 60)} min`,
          ]
            .filter(Boolean)
            .join(', ') || 'login only'}
        </dd>
        <dt className="text-gray-500">Nested groups</dt>
        <dd>{directory.groups.nested ? 'yes' : 'no'}</dd>
      </dl>

      <h3 className="font-semibold text-gray-900 dark:text-gray-100 mb-2">Group to role mapping</h3>
      <RoleMappings directory={directory} roles={roles} />

      <h3 className="font-semibold text-gray-900 dark:text-gray-100 mt-6 mb-2">
        Synchronization history
      </h3>
      <SyncHistory directoryId={directory.id} />
    </div>
  );
}

export default function Directories() {
  const {
    data: directories = [],
    isLoading,
    error,
  } = useQuery({
    queryKey: ['directories'],
    queryFn: api.getDirectories,
  });
  const { data: roles = [] } = useQuery({ queryKey: ['roles'], queryFn: api.getRoles });

  return (
    <div>
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100">Directories</h1>
        <p className="text-gray-500 dark:text-gray-400 mt-1">
          LDAP and Active Directory sign-in, provisioning and group-to-role mapping
        </p>
      </div>
      {isLoading && (
        <div className="flex justify-center h-32 items-center">
          <Loader2 className="w-6 h-6 animate-spin text-primary-600" />
        </div>
      )}
      {error && <p className="text-red-600">{errorText(error)}</p>}
      {!isLoading && !error && directories.length === 0 && (
        <div className="card text-center py-12 text-gray-500 dark:text-gray-400">
          <BookUser className="w-12 h-12 mx-auto mb-4 text-gray-300" />
          No directory is configured. Directories are defined under{' '}
          <code className="font-mono">auth.directories</code> in config.yaml.
        </div>
      )}
      {directories.map((d) => (
        <DirectoryCard key={d.id} directory={d} roles={roles} />
      ))}
    </div>
  );
}
