import { useState } from 'react';
import { AlertTriangle, Loader2 } from 'lucide-react';
import {
  usePluginFactCatalogs,
  usePluginFactPreview,
  useUpdatePluginFactSettings,
} from '../../hooks/usePlugins';
import type { PluginFactCatalog, PluginFactInfo, PluginFactSettingsRequest } from '../../types';
import { ErrorBox, Modal, StateBadge } from './pluginUi';
import {
  dangerBtn,
  getApiErrorMessage,
  inputClass,
  primaryBtn,
  secondaryBtn,
} from './pluginHelpers';
import {
  EXPORT_CAPABILITY_LABELS,
  canToggle,
  compactJson,
  exportBlockedReason,
  exportConfirmMatches,
  rulesBlockedReason,
} from './factHelpers';

const formatTime = (iso: string | null) => (iso ? new Date(iso).toLocaleString() : 'never');

function RulesConfirmDialog({
  catalog,
  fact,
  busy,
  error,
  onConfirm,
  onClose,
}: {
  catalog: PluginFactCatalog;
  fact: PluginFactInfo;
  busy: boolean;
  error: string | null;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <Modal title="Use fact in rules" onClose={onClose}>
      <div className="space-y-4 text-sm text-gray-700 dark:text-gray-200">
        <p>
          Allow classification rules to use <span className="font-mono">{fact.name}</span> from{' '}
          {catalog.plugin_name}?
        </p>
        <ul className="list-disc pl-5 space-y-1">
          <li>It can change node classification: the environment and the classes of nodes.</li>
          <li>
            A value older than its maximum age ({fact.max_stale_s} s) counts as missing, so rules
            that depend on it stop matching when the plugin stops reporting.
          </li>
          <li>The change is recorded in the audit log.</li>
        </ul>
        {error && <ErrorBox message={error} />}
        <div className="flex justify-end gap-3">
          <button type="button" className={secondaryBtn} onClick={onClose}>
            Cancel
          </button>
          <button type="button" className={primaryBtn} disabled={busy} onClick={onConfirm}>
            Allow in rules
          </button>
        </div>
      </div>
    </Modal>
  );
}

function ExportDialog({
  catalog,
  fact,
  busy,
  error,
  onConfirm,
  onClose,
}: {
  catalog: PluginFactCatalog;
  fact: PluginFactInfo;
  busy: boolean;
  error: string | null;
  onConfirm: (confirm: string) => void;
  onClose: () => void;
}) {
  const preview = usePluginFactPreview(catalog.plugin_id, fact.name);
  const [typed, setTyped] = useState('');
  const matches = exportConfirmMatches(typed, fact.name);
  const pii = fact.sensitivity === 'pii';
  return (
    <Modal title="Export fact to agents" onClose={onClose} wide>
      <div className="space-y-4 text-sm text-gray-700 dark:text-gray-200">
        <div
          role="alert"
          className="flex items-start gap-2 rounded-md border border-orange-300 bg-orange-50 dark:bg-orange-900/20 dark:border-orange-700 p-3 text-orange-800 dark:text-orange-200"
        >
          <AlertTriangle className="w-4 h-4 mt-0.5 flex-shrink-0" />
          <p>
            High risk: the values of <span className="font-mono">{fact.name}</span> will be sent to
            Puppet agents as external facts and are readable on every node that receives them.
          </p>
        </div>
        {pii && (
          <div
            role="alert"
            className="flex items-start gap-2 rounded-md border border-red-300 bg-red-50 dark:bg-red-900/20 dark:border-red-700 p-3 text-red-800 dark:text-red-200"
          >
            <AlertTriangle className="w-4 h-4 mt-0.5 flex-shrink-0" />
            <p>
              This fact is marked as containing personal data (PII). Exporting it copies personal
              data onto the agents.
            </p>
          </div>
        )}
        <section>
          <h4 className="font-semibold text-gray-900 dark:text-white">
            Values that would be exported
          </h4>
          {preview.isLoading && <Loader2 className="mt-2 w-5 h-5 animate-spin text-blue-500" />}
          {preview.error ? (
            <div className="mt-2">
              <ErrorBox message={getApiErrorMessage(preview.error, 'Could not load the preview')} />
            </div>
          ) : null}
          {preview.data && (
            <>
              <p className="mt-1 text-xs text-gray-500">
                Showing {preview.data.rows.length} of {preview.data.total} nodes.
              </p>
              <div className="mt-2 max-h-64 overflow-auto border border-gray-200 dark:border-gray-700 rounded">
                <table className="min-w-full text-xs">
                  <thead className="bg-gray-50 dark:bg-gray-900/40 text-left uppercase text-gray-500">
                    <tr>
                      <th className="px-3 py-2">Node</th>
                      <th className="px-3 py-2">Value</th>
                      <th className="px-3 py-2">Computed</th>
                      <th className="px-3 py-2">Stale</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
                    {preview.data.rows.length === 0 && (
                      <tr>
                        <td colSpan={4} className="px-3 py-3 text-center text-gray-500">
                          No node has a value yet
                        </td>
                      </tr>
                    )}
                    {preview.data.rows.map((row) => (
                      <tr key={row.certname}>
                        <td className="px-3 py-2 font-mono">{row.certname}</td>
                        <td className="px-3 py-2 font-mono break-all">{compactJson(row.value)}</td>
                        <td className="px-3 py-2">{formatTime(row.computed_at)}</td>
                        <td className="px-3 py-2">{row.stale ? 'stale' : 'no'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </section>
        <label className="block">
          <span>
            Type the full fact name <span className="font-mono font-semibold">{fact.name}</span> to
            confirm
          </span>
          <input
            aria-label="Confirm fact name"
            className={inputClass + ' mt-1 font-mono'}
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            spellCheck={false}
            autoComplete="off"
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
            disabled={!matches || !preview.data || busy}
            onClick={() => onConfirm(fact.name)}
          >
            Export to agents
          </button>
        </div>
      </div>
    </Modal>
  );
}

function ProviderStatus({ catalog }: { catalog: PluginFactCatalog }) {
  const p = catalog.provider;
  return (
    <div className="text-xs text-gray-600 dark:text-gray-300 space-y-0.5">
      <div>Last success: {formatTime(p.last_success_at)}</div>
      <div>Last failure: {formatTime(p.last_failure_at)}</div>
      {p.last_error && <div className="text-red-600 dark:text-red-400">Error: {p.last_error}</div>}
      <div>
        Failures: {p.failures} | Rejected: {p.rejected}
      </div>
    </div>
  );
}

const ADMIN_ONLY = 'Requires the plugins:admin permission';

function CatalogCard({
  catalog,
  canAdmin,
  busy,
  onRulesToggle,
  onExportToggle,
}: {
  catalog: PluginFactCatalog;
  canAdmin: boolean;
  busy: boolean;
  onRulesToggle: (fact: PluginFactInfo, next: boolean) => void;
  onExportToggle: (fact: PluginFactInfo, next: boolean) => void;
}) {
  return (
    <section
      aria-label={`Facts of ${catalog.plugin_name}`}
      className="bg-white dark:bg-gray-800 rounded-lg shadow"
    >
      <div className="flex flex-wrap items-start justify-between gap-4 px-4 py-4 border-b border-gray-200 dark:border-gray-700">
        <div className="text-sm">
          <div className="flex items-center gap-2">
            <h3 className="font-semibold text-gray-900 dark:text-white">{catalog.plugin_name}</h3>
            <StateBadge state={catalog.state} />
          </div>
          <div className="font-mono text-xs text-gray-500">{catalog.plugin_id}</div>
          <div className="mt-1 text-xs text-gray-600 dark:text-gray-300">
            Namespace <span className="font-mono">{catalog.namespace}</span> | Mode {catalog.mode}
            {catalog.handler && (
              <>
                {' '}
                (handler <span className="font-mono">{catalog.handler}</span>)
              </>
            )}
          </div>
          <div className="text-xs text-gray-600 dark:text-gray-300">
            Export capability:{' '}
            <span className="font-medium">
              {EXPORT_CAPABILITY_LABELS[catalog.export_capability]}
            </span>
          </div>
        </div>
        <ProviderStatus catalog={catalog} />
      </div>
      {!catalog.puppetdb_checked && (
        <p className="px-4 pt-3 text-xs text-yellow-700 dark:text-yellow-300">
          PuppetDB is unavailable, so conflicts with agent facts could not be checked.
        </p>
      )}
      <div className="overflow-x-auto">
        <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
          <thead className="bg-gray-50 dark:bg-gray-900/40">
            <tr>
              {['Fact', 'Type', 'Nodes', 'Stale', 'Usable in rules', 'Exportable to agents'].map(
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
            {catalog.facts.map((fact) => {
              const rulesReason = rulesBlockedReason(fact);
              const exportReason = exportBlockedReason(fact, catalog);
              const rulesBlocked = !canToggle(fact.usable_in_rules, rulesReason);
              const exportBlocked = !canToggle(fact.exportable, exportReason);
              return (
                <tr key={fact.name} className="align-top">
                  <td className="px-4 py-3 text-sm">
                    <div className="font-mono">{fact.name}</div>
                    {fact.description && (
                      <div className="text-xs text-gray-500">{fact.description}</div>
                    )}
                    <div className="mt-1 flex flex-wrap gap-1">
                      {fact.sensitivity === 'pii' && (
                        <span className="px-2 py-0.5 text-xs rounded-full font-medium bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300">
                          PII
                        </span>
                      )}
                      {fact.conflict && (
                        <span
                          className="px-2 py-0.5 text-xs rounded-full font-medium bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300"
                          title="This fact is also reported by agents: PuppetDB wins and the plugin value is hidden"
                        >
                          hidden: agent fact wins
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-3 text-sm font-mono">{fact.type}</td>
                  <td className="px-4 py-3 text-sm">{fact.nodes}</td>
                  <td className="px-4 py-3 text-sm">{fact.stale_nodes}</td>
                  <td className="px-4 py-3">
                    <input
                      type="checkbox"
                      className="h-4 w-4"
                      aria-label={`Usable in rules: ${fact.name}`}
                      title={
                        rulesBlocked
                          ? (rulesReason ?? undefined)
                          : !canAdmin
                            ? ADMIN_ONLY
                            : undefined
                      }
                      checked={fact.usable_in_rules}
                      disabled={!canAdmin || busy || rulesBlocked}
                      onChange={(e) => onRulesToggle(fact, e.target.checked)}
                    />
                  </td>
                  <td className="px-4 py-3">
                    <input
                      type="checkbox"
                      className="h-4 w-4"
                      aria-label={`Exportable to agents: ${fact.name}`}
                      title={
                        exportBlocked
                          ? (exportReason ?? undefined)
                          : !canAdmin
                            ? ADMIN_ONLY
                            : undefined
                      }
                      checked={fact.exportable}
                      disabled={!canAdmin || busy || exportBlocked}
                      onChange={(e) => onExportToggle(fact, e.target.checked)}
                    />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

type Pending = { kind: 'rules' | 'export'; catalog: PluginFactCatalog; fact: PluginFactInfo };

/** Facts declared by plugins, with the administrator settings for rules and agent export. */
export default function FactsTab({ canAdmin }: { canAdmin: boolean }) {
  const { data: catalogs = [], isLoading, error } = usePluginFactCatalogs();
  const update = useUpdatePluginFactSettings();
  const [pending, setPending] = useState<Pending | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [dialogError, setDialogError] = useState<string | null>(null);

  const save = async (
    target: { catalog: PluginFactCatalog; fact: PluginFactInfo },
    request: PluginFactSettingsRequest,
    inDialog: boolean
  ) => {
    setActionError(null);
    setDialogError(null);
    try {
      await update.mutateAsync({
        id: target.catalog.plugin_id,
        name: target.fact.name,
        request,
      });
      setPending(null);
    } catch (e) {
      const message = getApiErrorMessage(e, 'Could not save the fact settings');
      if (inDialog) setDialogError(message);
      else setActionError(message);
    }
  };

  const close = () => {
    setPending(null);
    setDialogError(null);
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
      {error ? (
        <ErrorBox message={getApiErrorMessage(error, 'Could not load plugin facts')} />
      ) : null}
      {actionError && <ErrorBox message={actionError} />}
      <p className="text-sm text-gray-500 dark:text-gray-400">
        Facts reported by plugins are merged into the node facts. They are not used in rules and are
        not sent to agents until an administrator enables it here, fact by fact.
      </p>
      {!error && catalogs.length === 0 && (
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow px-4 py-6 text-sm text-center text-gray-500">
          No installed plugin declares facts
        </div>
      )}
      {catalogs.map((catalog) => (
        <CatalogCard
          key={catalog.plugin_id}
          catalog={catalog}
          canAdmin={canAdmin}
          busy={update.isPending}
          onRulesToggle={(fact, next) =>
            next
              ? setPending({ kind: 'rules', catalog, fact })
              : void save({ catalog, fact }, { usable_in_rules: false }, false)
          }
          onExportToggle={(fact, next) =>
            next
              ? setPending({ kind: 'export', catalog, fact })
              : void save({ catalog, fact }, { exportable: false }, false)
          }
        />
      ))}
      {pending?.kind === 'rules' && (
        <RulesConfirmDialog
          catalog={pending.catalog}
          fact={pending.fact}
          busy={update.isPending}
          error={dialogError}
          onClose={close}
          onConfirm={() => void save(pending, { usable_in_rules: true }, true)}
        />
      )}
      {pending?.kind === 'export' && (
        <ExportDialog
          catalog={pending.catalog}
          fact={pending.fact}
          busy={update.isPending}
          error={dialogError}
          onClose={close}
          onConfirm={(confirm) => void save(pending, { exportable: true, confirm }, true)}
        />
      )}
    </div>
  );
}
