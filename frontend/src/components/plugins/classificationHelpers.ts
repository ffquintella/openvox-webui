import type {
  GroupTemplateInstance,
  HeldMembership,
  MembershipProviderStatus,
  PluginClassificationView,
} from '../../types';

/** Short label of a provider status for the plugin screen. */
export function providerStatusLabel(s: MembershipProviderStatus): string {
  if (s.held) return 'update held';
  if (s.stale) return 'stale';
  switch (s.last_status) {
    case 'ok':
      return 'ok';
    case 'failed':
      return 'failed';
    case 'held':
      return 'update held';
    default:
      return 'never refreshed';
  }
}

/** True when the status needs an administrator's attention. */
export function providerNeedsAttention(s: MembershipProviderStatus): boolean {
  return s.held || s.stale || s.last_status === 'failed';
}

/** What a held update would do, for the confirmation text. */
export function heldSummary(h: HeldMembership): string {
  const pct = Math.round(h.churn_pct * 10) / 10;
  return (
    `adds ${h.added.length} and removes ${h.removed.length} of ${h.current} members ` +
    `(${pct}%, limit ${h.limit_pct}%)`
  );
}

/** What an instance does, for the table: pending approval, paused or running. */
export function instanceState(i: GroupTemplateInstance): 'pending' | 'paused' | 'active' {
  if (!i.approved) return 'pending';
  return i.enabled ? 'active' : 'paused';
}

/** Number of extensions a plugin contributes (functions, operators, strategies, providers, templates). */
export function extensionCount(p: PluginClassificationView): number {
  return (
    p.functions.length +
    p.operators.length +
    p.merge_strategies.length +
    p.providers.length +
    p.templates.length
  );
}

/** `plugin:<plugin-id>:<id>` -> plugin id, or null for a core name. */
export function pluginOfRef(ref: string): string | null {
  if (!ref.startsWith('plugin:')) return null;
  const rest = ref.slice('plugin:'.length);
  const i = rest.indexOf(':');
  return i > 0 ? rest.slice(0, i) : null;
}

export type JsonObjectResult =
  { ok: true; value: Record<string, unknown> } | { ok: false; error: string };

/** Parses a JSON object typed in a text box; blank means `{}`. */
export function parseJsonObject(text: string): JsonObjectResult {
  const t = text.trim();
  if (t === '') return { ok: true, value: {} };
  try {
    const v: unknown = JSON.parse(t);
    if (v === null || typeof v !== 'object' || Array.isArray(v)) {
      return { ok: false, error: 'Must be a JSON object.' };
    }
    return { ok: true, value: v as Record<string, unknown> };
  } catch (e) {
    return { ok: false, error: `Invalid JSON: ${e instanceof Error ? e.message : String(e)}` };
  }
}
