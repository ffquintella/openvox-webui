import type {
  PluginAutoUpdatePolicy,
  PluginUpdateInfo,
  PluginUpdateOutcome,
  PluginVersionView,
} from '../../types';

export const AUTO_UPDATE_LABELS: Record<PluginAutoUpdatePolicy, string> = {
  off: 'Off (manual only)',
  patch: 'Patch releases (same major.minor)',
  minor: 'Minor and patch releases (same major)',
  any: 'Any newer version',
};

export const AUTO_UPDATE_SHORT: Record<PluginAutoUpdatePolicy, string> = {
  off: 'Off',
  patch: 'Patch',
  minor: 'Minor',
  any: 'Any',
};

export const OUTCOME_LABELS: Record<PluginUpdateOutcome, string> = {
  updated: 'Updated',
  pending_approval: 'Waiting for capability approval',
  up_to_date: 'Already up to date',
  failed: 'Failed',
};

export const WINDOW_MAX_LENGTH = 100;
export const REVOKE_REASON_MAX = 500;

/**
 * Shape check of an update window before it is sent: empty (any time) or a
 * five-field cron expression. The server validates the fields themselves.
 */
export function isValidWindowShape(window: string): boolean {
  const w = window.trim();
  if (w === '') return true;
  return w.length <= WINDOW_MAX_LENGTH && w.split(/\s+/).length === 5;
}

export function describeWindow(window: string | null): string {
  return window ? `${window} (UTC)` : 'any time';
}

/** Plugins a batch update can be applied to: those with a version available. */
export function selectableForBatch(items: PluginUpdateInfo[]): string[] {
  return items.filter((i) => i.available_version !== null).map((i) => i.plugin_id);
}

export function canSubmitRevoke(reason: string): boolean {
  const r = reason.trim();
  return r.length > 0 && r.length <= REVOKE_REASON_MAX;
}

/** What an administrator can do with one stored version. */
export function versionActions(v: PluginVersionView): {
  update: boolean;
  rollback: boolean;
  revoke: boolean;
  unrevoke: boolean;
} {
  return {
    update: v.relation === 'newer' && v.can_activate,
    rollback: v.relation === 'older' && v.can_activate,
    revoke: !v.revoked,
    unrevoke: v.revoked,
  };
}
