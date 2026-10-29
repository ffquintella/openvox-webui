import type {
  FactPathEntry,
  PluginFactCatalog,
  PluginFactExportCapability,
  PluginFactInfo,
} from '../../types';

const PLUGIN_SOURCE_PREFIX = 'plugin:';

export function isPluginSource(source: string | null | undefined): source is string {
  return typeof source === 'string' && source.startsWith(PLUGIN_SOURCE_PREFIX);
}

/** Plugin id of a "plugin:<id>" source, or null for PuppetDB facts. */
export function pluginIdFromSource(source: string | null | undefined): string | null {
  if (!isPluginSource(source)) return null;
  const id = source.slice(PLUGIN_SOURCE_PREFIX.length);
  return id === '' ? null : id;
}

/** Human label of a fact source; a missing source means PuppetDB. */
export function factSourceLabel(source: string | null | undefined): string {
  const id = pluginIdFromSource(source);
  return id ? `plugin: ${id}` : 'PuppetDB';
}

export const EXPORT_CAPABILITY_LABELS: Record<PluginFactExportCapability, string> = {
  approved: 'Approved',
  not_approved: 'Not approved',
  not_declared: 'Not declared',
  denied_by_policy: 'Denied by policy',
};

const CONFLICT_REASON =
  'This fact collides with a fact reported by agents: the agent value wins and the plugin value is hidden';

/** Why "Exportable to agents" cannot be enabled, or null when it can. */
export function exportBlockedReason(
  fact: PluginFactInfo,
  catalog: Pick<PluginFactCatalog, 'export_capability'>
): string | null {
  if (!fact.offers_export) return 'The plugin manifest does not offer this fact for export';
  switch (catalog.export_capability) {
    case 'not_declared':
      return 'The plugin does not declare the facts:export:external capability';
    case 'not_approved':
      return 'The capability facts:export:external has not been approved for this plugin';
    case 'denied_by_policy':
      return 'Exporting facts to agents is denied by policy';
    default:
      break;
  }
  if (fact.conflict) return CONFLICT_REASON;
  return null;
}

/** Why "Usable in rules" cannot be enabled, or null when it can. */
export function rulesBlockedReason(fact: PluginFactInfo): string | null {
  if (!fact.offers_rules) return 'The plugin manifest does not offer this fact for rules';
  if (fact.conflict) return CONFLICT_REASON;
  return null;
}

/**
 * Whether a blocked setting may still be turned off. A setting that is already
 * on can always be disabled, whatever the reason that blocks enabling it.
 */
export function canToggle(enabled: boolean, blockedReason: string | null): boolean {
  return enabled || blockedReason === null;
}

/** Namespace (first path segment) to plugin id, from the plugin entries of /facts/paths. */
export function namespaceSources(entries: FactPathEntry[] | undefined | null): Map<string, string> {
  const map = new Map<string, string>();
  for (const entry of entries ?? []) {
    const id = pluginIdFromSource(entry.source);
    const ns = entry.path?.[0];
    if (id && ns && !map.has(ns)) map.set(ns, id);
  }
  return map;
}

/** Plugin id a rule's fact path depends on (its namespace is plugin provided), or null. */
export function ruleDependsOnPlugin(
  factPath: string | null | undefined,
  map: Map<string, string>
): string | null {
  if (!factPath) return null;
  const ns = factPath.split(/[.[]/)[0]?.trim();
  if (!ns) return null;
  return map.get(ns) ?? null;
}

/** Compact JSON for a preview cell, cut to a readable length. */
export function compactJson(value: unknown, max = 120): string {
  let text: string;
  try {
    text = JSON.stringify(value) ?? 'null';
  } catch {
    text = String(value);
  }
  return text.length > max ? `${text.slice(0, max - 3)}...` : text;
}

/** The export confirmation must be the exact full fact name. */
export function exportConfirmMatches(typed: string, factName: string): boolean {
  return typed === factName;
}
