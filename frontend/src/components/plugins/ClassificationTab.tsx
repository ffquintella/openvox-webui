import { useState } from 'react';
import { AlertTriangle, Loader2 } from 'lucide-react';
import {
  useApproveHeldMembership,
  useApproveTemplateInstance,
  useCreateTemplateInstance,
  useDeleteTemplateInstance,
  usePluginClassification,
  useRefreshGroupMembership,
  useRejectHeldMembership,
  useRunTemplateInstance,
  useSetTemplateInstanceEnabled,
} from '../../hooks/usePlugins';
import { useGroups } from '../../hooks/useGroups';
import type {
  GroupTemplateInstance,
  HeldMembership,
  PluginClassificationProvider,
  PluginClassificationTemplate,
  PluginClassificationView,
} from '../../types';
import { ErrorBox, Modal } from './pluginUi';
import {
  dangerBtn,
  getApiErrorMessage,
  inputClass,
  primaryBtn,
  secondaryBtn,
} from './pluginHelpers';
import {
  extensionCount,
  heldSummary,
  instanceState,
  parseJsonObject,
  providerNeedsAttention,
  providerStatusLabel,
} from './classificationHelpers';

const ADMIN_ONLY = 'Requires the plugins:admin permission';
const formatTime = (iso: string | null) => (iso ? new Date(iso).toLocaleString() : 'never');

function Badge({ tone, children }: { tone: 'ok' | 'warn' | 'bad' | 'info'; children: string }) {
  const tones = {
    ok: 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300',
    warn: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300',
    bad: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300',
    info: 'bg-gray-100 text-gray-700 dark:bg-gray-700 dark:text-gray-200',
  };
  return (
    <span className={`px-2 py-0.5 text-xs rounded-full font-medium ${tones[tone]}`}>
      {children}
    </span>
  );
}

function HeldBanner({
  held,
  groupName,
  canAdmin,
}: {
  held: HeldMembership;
  groupName: string;
  canAdmin: boolean;
}) {
  const approve = useApproveHeldMembership();
  const reject = useRejectHeldMembership();
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const busy = approve.isPending || reject.isPending;
  return (
    <div
      role="alert"
      className="mt-2 rounded border border-yellow-300 bg-yellow-50 p-3 text-sm dark:border-yellow-700 dark:bg-yellow-900/20"
    >
      <p className="flex items-center gap-2 font-medium text-yellow-800 dark:text-yellow-200">
        <AlertTriangle className="w-4 h-4" /> Update held by the churn guard for {groupName}
      </p>
      <p className="mt-1 text-yellow-800 dark:text-yellow-200">
        The provider {heldSummary(held)}. The group keeps its current members until you decide.
      </p>
      {error && <ErrorBox message={error} />}
      <div className="mt-2 flex gap-2">
        <button
          type="button"
          className={primaryBtn}
          disabled={!canAdmin || busy}
          title={canAdmin ? undefined : ADMIN_ONLY}
          onClick={() => setConfirming(true)}
        >
          Review and approve
        </button>
        <button
          type="button"
          className={secondaryBtn}
          disabled={!canAdmin || busy}
          title={canAdmin ? undefined : ADMIN_ONLY}
          onClick={async () => {
            setError(null);
            try {
              await reject.mutateAsync(held.group_id);
            } catch (e) {
              setError(getApiErrorMessage(e, 'Could not discard the update'));
            }
          }}
        >
          Discard
        </button>
      </div>
      {confirming && (
        <Modal title="Approve membership update" onClose={() => setConfirming(false)}>
          <div className="space-y-3 text-sm text-gray-700 dark:text-gray-200">
            <p>
              Apply this update to <span className="font-medium">{groupName}</span>? It{' '}
              {heldSummary(held)}. Nodes that enter or leave get a different classification at their
              next run.
            </p>
            <MemberList label="Entering" names={held.added} />
            <MemberList label="Leaving" names={held.removed} />
            <div className="flex justify-end gap-2">
              <button type="button" className={secondaryBtn} onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button
                type="button"
                className={primaryBtn}
                disabled={busy}
                onClick={async () => {
                  setError(null);
                  try {
                    await approve.mutateAsync(held.group_id);
                    setConfirming(false);
                  } catch (e) {
                    setError(getApiErrorMessage(e, 'Could not apply the update'));
                    setConfirming(false);
                  }
                }}
              >
                Approve update
              </button>
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}

function MemberList({ label, names }: { label: string; names: string[] }) {
  if (names.length === 0) return null;
  const shown = names.slice(0, 20);
  return (
    <div>
      <div className="text-xs font-medium uppercase text-gray-500">
        {label} ({names.length})
      </div>
      <ul className="mt-1 max-h-32 overflow-y-auto font-mono text-xs">
        {shown.map((n) => (
          <li key={n}>{n}</li>
        ))}
        {names.length > shown.length && <li>... and {names.length - shown.length} more</li>}
      </ul>
    </div>
  );
}

function ProvidersTable({
  providers,
  held,
  groupNames,
  canAdmin,
}: {
  providers: PluginClassificationProvider[];
  held: HeldMembership[];
  groupNames: Record<string, string>;
  canAdmin: boolean;
}) {
  const refresh = useRefreshGroupMembership();
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  if (providers.length === 0) return null;
  return (
    <div className="px-4 py-3">
      <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Membership providers</h4>
      {error && <ErrorBox message={error} />}
      {message && <p className="mt-1 text-xs text-gray-600 dark:text-gray-300">{message}</p>}
      <div className="mt-2 space-y-3">
        {providers.map((p) => (
          <div key={p.id} className="text-sm">
            <div className="font-medium">
              {p.label} <span className="font-mono text-xs text-gray-500">{p.id}</span>
            </div>
            <div className="text-xs text-gray-600 dark:text-gray-300">
              Refresh {p.refresh?.cron ?? 'on demand'} | stale after {p.max_stale_s ?? 3600}s (
              {p.on_stale ?? 'keep'}) | churn limit {p.churn_limit_pct ?? 20}%
            </div>
            {p.groups.length === 0 ? (
              <p className="text-xs text-gray-500">
                No dynamic group uses this provider yet. Create a group of kind dynamic to use it.
              </p>
            ) : (
              <table className="mt-1 min-w-full text-xs">
                <tbody>
                  {p.groups.map((g) => {
                    const h = held.find((x) => x.group_id === g.group_id);
                    return (
                      <tr key={g.group_id} className="align-top">
                        <td className="py-1 pr-3">{groupNames[g.group_id] ?? g.group_id}</td>
                        <td className="py-1 pr-3">
                          <Badge tone={providerNeedsAttention(g) ? 'warn' : 'ok'}>
                            {providerStatusLabel(g)}
                          </Badge>
                        </td>
                        <td className="py-1 pr-3">{g.members} members</td>
                        <td className="py-1 pr-3">refreshed {formatTime(g.last_refresh_at)}</td>
                        <td className="py-1 pr-3 text-red-600 dark:text-red-400">{g.last_error}</td>
                        <td className="py-1">
                          <button
                            type="button"
                            className={secondaryBtn}
                            disabled={!canAdmin || refresh.isPending}
                            title={canAdmin ? undefined : ADMIN_ONLY}
                            onClick={async () => {
                              setError(null);
                              setMessage(null);
                              try {
                                const o = await refresh.mutateAsync(g.group_id);
                                setMessage(
                                  o.held
                                    ? 'The update was held by the churn guard.'
                                    : `Refreshed: ${o.added} added, ${o.removed} removed.`
                                );
                              } catch (e) {
                                setError(getApiErrorMessage(e, 'Refresh failed'));
                              }
                            }}
                          >
                            Refresh now
                          </button>
                          {h && (
                            <HeldBanner
                              held={h}
                              groupName={groupNames[g.group_id] ?? g.group_id}
                              canAdmin={canAdmin}
                            />
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function NewInstanceDialog({
  pluginId,
  template,
  onClose,
}: {
  pluginId: string;
  template: PluginClassificationTemplate;
  onClose: () => void;
}) {
  const { data: groups = [] } = useGroups();
  const create = useCreateTemplateInstance();
  const [name, setName] = useState('');
  const [parent, setParent] = useState('');
  const [classes, setClasses] = useState('');
  const [variables, setVariables] = useState('');
  const [error, setError] = useState<string | null>(null);
  const classesResult = parseJsonObject(classes);
  const variablesResult = parseJsonObject(variables);
  const problem =
    name.trim() === ''
      ? 'Enter a name.'
      : !classesResult.ok
        ? `Classes: ${classesResult.error}`
        : !variablesResult.ok
          ? `Variables: ${variablesResult.error}`
          : null;
  return (
    <Modal title={`Use template ${template.label ?? template.id}`} onClose={onClose}>
      <form
        className="space-y-3 text-sm"
        onSubmit={async (e) => {
          e.preventDefault();
          if (problem || !classesResult.ok || !variablesResult.ok) return;
          setError(null);
          try {
            await create.mutateAsync({
              pluginId,
              templateId: template.id,
              request: {
                name: name.trim(),
                parent_id: parent || null,
                classes: classesResult.value,
                variables: variablesResult.value,
              },
            });
            onClose();
          } catch (err) {
            setError(getApiErrorMessage(err, 'Could not create the instance'));
          }
        }}
      >
        <p className="text-gray-600 dark:text-gray-300">
          One group is generated for each value of{' '}
          <span className="font-mono">{template.group_by}</span> (at most{' '}
          {template.max_groups ?? 100}). Use <span className="font-mono">{'{{key}}'}</span> in
          classes and variables for the value.
          {template.requires_approval !== false &&
            ' An administrator must approve it before any group is created.'}
        </p>
        <label className="block">
          <span className="text-xs font-medium">Name</span>
          <input className={inputClass} value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="block">
          <span className="text-xs font-medium">Parent group</span>
          <select className={inputClass} value={parent} onChange={(e) => setParent(e.target.value)}>
            <option value="">(none, top level)</option>
            {groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </label>
        <label className="block">
          <span className="text-xs font-medium">Classes (JSON)</span>
          <textarea
            className={`${inputClass} font-mono`}
            rows={3}
            value={classes}
            onChange={(e) => setClasses(e.target.value)}
            placeholder='{"profile::datacenter": {"site": "{{key}}"}}'
          />
        </label>
        <label className="block">
          <span className="text-xs font-medium">Variables (JSON)</span>
          <textarea
            className={`${inputClass} font-mono`}
            rows={2}
            value={variables}
            onChange={(e) => setVariables(e.target.value)}
            placeholder='{"datacenter": "{{key}}"}'
          />
        </label>
        {(error || (problem && name !== '')) && <ErrorBox message={error ?? problem ?? ''} />}
        <div className="flex justify-end gap-2">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className={primaryBtn} disabled={!!problem || create.isPending}>
            Create
          </button>
        </div>
      </form>
    </Modal>
  );
}

function InstanceRow({
  instance,
  canAdmin,
  parentName,
}: {
  instance: GroupTemplateInstance;
  canAdmin: boolean;
  parentName: string | null;
}) {
  const approve = useApproveTemplateInstance();
  const run = useRunTemplateInstance();
  const toggle = useSetTemplateInstanceEnabled();
  const del = useDeleteTemplateInstance();
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const state = instanceState(instance);
  const busy = approve.isPending || run.isPending || toggle.isPending || del.isPending;
  const guard = async (fn: () => Promise<unknown>, fallback: string) => {
    setError(null);
    setResult(null);
    try {
      await fn();
    } catch (e) {
      setError(getApiErrorMessage(e, fallback));
    }
  };
  return (
    <tr className="align-top">
      <td className="py-2 pr-3">
        <div className="font-medium">{instance.name}</div>
        <div className="text-xs text-gray-500">under {parentName ?? 'top level'}</div>
      </td>
      <td className="py-2 pr-3">
        <Badge tone={state === 'active' ? 'ok' : state === 'pending' ? 'warn' : 'info'}>
          {state === 'pending' ? 'waiting for approval' : state}
        </Badge>
      </td>
      <td className="py-2 pr-3">{instance.generated_groups} groups</td>
      <td className="py-2 pr-3 text-xs">
        {instance.last_run_at
          ? `${formatTime(instance.last_run_at)}: ${instance.last_status ?? ''}`
          : 'not run yet'}
      </td>
      <td className="py-2">
        <div className="flex flex-wrap gap-2">
          {state === 'pending' && (
            <button
              type="button"
              className={primaryBtn}
              disabled={!canAdmin || busy}
              title={canAdmin ? undefined : ADMIN_ONLY}
              onClick={() => guard(() => approve.mutateAsync(instance.id), 'Approval failed')}
            >
              Approve
            </button>
          )}
          {state !== 'pending' && (
            <button
              type="button"
              className={secondaryBtn}
              disabled={!canAdmin || busy || state === 'paused'}
              title={canAdmin ? undefined : ADMIN_ONLY}
              onClick={() =>
                guard(async () => {
                  const r = await run.mutateAsync(instance.id);
                  setResult(
                    `${r.created} created, ${r.updated} updated, ${r.removed} removed` +
                      (r.truncated ? ' (limited by max groups)' : '')
                  );
                }, 'Run failed')
              }
            >
              Run now
            </button>
          )}
          {state !== 'pending' && (
            <button
              type="button"
              className={secondaryBtn}
              disabled={!canAdmin || busy}
              title={canAdmin ? undefined : ADMIN_ONLY}
              onClick={() =>
                guard(
                  () => toggle.mutateAsync({ id: instance.id, enabled: !instance.enabled }),
                  'Could not change the instance'
                )
              }
            >
              {instance.enabled ? 'Pause' : 'Resume'}
            </button>
          )}
          <button
            type="button"
            className={dangerBtn}
            disabled={!canAdmin || busy}
            title={canAdmin ? undefined : ADMIN_ONLY}
            onClick={() => setConfirmDelete(true)}
          >
            Delete
          </button>
        </div>
        {result && <p className="mt-1 text-xs text-gray-600 dark:text-gray-300">{result}</p>}
        {error && <ErrorBox message={error} />}
        {confirmDelete && (
          <Modal title="Delete template instance" onClose={() => setConfirmDelete(false)}>
            <div className="space-y-3 text-sm">
              <p>
                Delete <span className="font-medium">{instance.name}</span> and the{' '}
                {instance.generated_groups} groups it generated? Nodes in those groups lose their
                classes and variables.
              </p>
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  className={secondaryBtn}
                  onClick={() => setConfirmDelete(false)}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className={dangerBtn}
                  onClick={async () => {
                    await guard(() => del.mutateAsync(instance.id), 'Delete failed');
                    setConfirmDelete(false);
                  }}
                >
                  Delete
                </button>
              </div>
            </div>
          </Modal>
        )}
      </td>
    </tr>
  );
}

function TemplatesTable({
  plugin,
  canAdmin,
  groupNames,
}: {
  plugin: PluginClassificationView;
  canAdmin: boolean;
  groupNames: Record<string, string>;
}) {
  const [creating, setCreating] = useState<PluginClassificationTemplate | null>(null);
  if (plugin.templates.length === 0) return null;
  return (
    <div className="px-4 py-3">
      <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Group templates</h4>
      <div className="mt-2 space-y-3">
        {plugin.templates.map((t) => (
          <div key={t.id} className="text-sm">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <span className="font-medium">{t.label ?? t.id}</span>{' '}
                <span className="font-mono text-xs text-gray-500">{t.group_by}</span>
                <span className="text-xs text-gray-500"> | max {t.max_groups ?? 100} groups</span>
              </div>
              <button
                type="button"
                className={secondaryBtn}
                disabled={!canAdmin}
                title={canAdmin ? undefined : ADMIN_ONLY}
                onClick={() => setCreating(t)}
              >
                Use template
              </button>
            </div>
            {t.instances.length > 0 && (
              <table className="mt-1 min-w-full text-sm">
                <tbody>
                  {t.instances.map((i) => (
                    <InstanceRow
                      key={i.id}
                      instance={i}
                      canAdmin={canAdmin}
                      parentName={i.parent_id ? (groupNames[i.parent_id] ?? i.parent_id) : null}
                    />
                  ))}
                </tbody>
              </table>
            )}
          </div>
        ))}
      </div>
      {creating && (
        <NewInstanceDialog
          pluginId={plugin.plugin_id}
          template={creating}
          onClose={() => setCreating(null)}
        />
      )}
    </div>
  );
}

function Vocabulary({ plugin }: { plugin: PluginClassificationView }) {
  const rows: { kind: string; id: string; detail: string }[] = [
    ...plugin.functions.map((f) => ({
      kind: 'function',
      id: `plugin:${plugin.plugin_id}:${f.id}`,
      detail: `(${f.args.join(', ')}) -> ${f.returns}`,
    })),
    ...plugin.operators.map((o) => ({
      kind: 'operator',
      id: `plugin:${plugin.plugin_id}:${o.id}`,
      detail: `${o.arg_types.join(' x ')} -> bool`,
    })),
    ...plugin.merge_strategies.map((m) => ({
      kind: 'merge strategy',
      id: `plugin:${plugin.plugin_id}:${m.id}`,
      detail: m.description ?? '',
    })),
  ];
  if (rows.length === 0) return null;
  return (
    <div className="px-4 py-3">
      <h4 className="text-sm font-semibold text-gray-900 dark:text-white">
        Functions, operators and merge strategies
      </h4>
      <ul className="mt-1 space-y-1 text-xs">
        {rows.map((r) => (
          <li key={`${r.kind}-${r.id}`}>
            <span className="text-gray-500">{r.kind}</span>{' '}
            <span className="font-mono">{r.id}</span>{' '}
            <span className="text-gray-600 dark:text-gray-300">{r.detail}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** What plugins add to classification: vocabulary, membership providers and group templates (M27). */
export default function ClassificationTab({ canAdmin }: { canAdmin: boolean }) {
  const { data, isLoading, error } = usePluginClassification();
  const { data: groups = [] } = useGroups();
  const groupNames = Object.fromEntries(groups.map((g) => [g.id, g.name]));

  if (isLoading) {
    return (
      <p className="flex items-center gap-2 text-sm text-gray-500">
        <Loader2 className="w-4 h-4 animate-spin" /> Loading classification extensions...
      </p>
    );
  }
  if (error || !data) {
    return (
      <ErrorBox message={getApiErrorMessage(error, 'Could not load classification extensions')} />
    );
  }
  const unavailable = Object.entries(data.unavailable);
  return (
    <div className="space-y-4">
      {unavailable.length > 0 && (
        <div
          role="alert"
          className="rounded border border-yellow-300 bg-yellow-50 p-3 text-sm dark:border-yellow-700 dark:bg-yellow-900/20"
        >
          <p className="flex items-center gap-2 font-medium text-yellow-800 dark:text-yellow-200">
            <AlertTriangle className="w-4 h-4" /> Groups that need a plugin which is not enabled
          </p>
          <ul className="mt-1 list-disc pl-5 text-yellow-800 dark:text-yellow-200">
            {unavailable.map(([group, reasons]) => (
              <li key={group}>
                <span className="font-medium">{group}</span>: {reasons.join('; ')}
              </li>
            ))}
          </ul>
          <p className="mt-1 text-xs text-yellow-800 dark:text-yellow-200">
            Their plugin rules never match until the plugin is enabled again.
          </p>
        </div>
      )}
      {data.plugins.length === 0 ? (
        <p className="text-sm text-gray-500">
          No enabled plugin extends classification. A plugin can add functions, operators, dynamic
          groups, group templates and merge strategies.
        </p>
      ) : (
        data.plugins.map((p) => (
          <section
            key={p.plugin_id}
            aria-label={`Classification of ${p.plugin_id}`}
            className="bg-white dark:bg-gray-800 rounded-lg shadow divide-y divide-gray-200 dark:divide-gray-700"
          >
            <div className="px-4 py-3">
              <h3 className="font-semibold text-gray-900 dark:text-white">
                {p.plugin_id} <span className="text-xs font-normal text-gray-500">{p.version}</span>
              </h3>
              <p className="text-xs text-gray-500">{extensionCount(p)} extensions</p>
            </div>
            <Vocabulary plugin={p} />
            <ProvidersTable
              providers={p.providers}
              held={data.held}
              groupNames={groupNames}
              canAdmin={canAdmin}
            />
            <TemplatesTable plugin={p} canAdmin={canAdmin} groupNames={groupNames} />
          </section>
        ))
      )}
    </div>
  );
}
