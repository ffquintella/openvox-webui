import type { LucideIcon } from 'lucide-react';
import type { UiContribution, UiMenuOp } from '../types';
import { resolvePluginIcon } from './pluginIcons';

/** Built-in items that plugins can never hide, so an admin cannot be locked out. */
export const PROTECTED_MENU_IDS: ReadonlySet<string> = new Set(['core:settings', 'core:plugins']);

export interface CoreMenuItem {
  id: string;
  name: string;
  href: string;
  icon: LucideIcon;
  /** Defaults to 100, 200, ... following the array position. */
  order?: number;
}

export interface MenuEntry {
  id: string;
  name: string;
  href: string;
  icon: LucideIcon;
  order: number;
  source: 'core' | 'plugin';
  pluginId?: string;
}

/** Plugin menu links must stay inside the plugin page namespace. */
export function isSafePluginHref(href: string | null | undefined): href is string {
  return !!href && /^\/p\/[^/\s]+(\/|$)/.test(href) && !href.includes('//') && !href.includes('..');
}

/**
 * Merge built-in items with plugin menu contributions and apply menu ops.
 * Pure: no React, no network. Ordering is by `order` (core items implicitly
 * 100, 200, ...), ties keep core first and then input order.
 */
export function mergeMenu(
  core: CoreMenuItem[],
  contributions: UiContribution[] | undefined,
  ops: UiMenuOp[] | undefined
): MenuEntry[] {
  const entries: MenuEntry[] = core.map((item, index) => ({
    id: item.id,
    name: item.name,
    href: item.href,
    icon: item.icon,
    order: item.order ?? (index + 1) * 100,
    source: 'core',
  }));

  for (const c of contributions ?? []) {
    if (!isSafePluginHref(c.href)) continue;
    const id = `${c.plugin_id}:${c.id}`;
    if (entries.some((e) => e.id === id)) continue;
    entries.push({
      id,
      name: c.label ?? c.title ?? c.id,
      href: c.href,
      icon: resolvePluginIcon(c.icon),
      order: c.order,
      source: 'plugin',
      pluginId: c.plugin_id,
    });
  }

  const hidden = new Set<string>();
  const renamed = new Map<string, string>();
  for (const op of ops ?? []) {
    if (op.op === 'hide' && !PROTECTED_MENU_IDS.has(op.target)) {
      hidden.add(op.target);
    } else if (op.op === 'rename' && op.label && op.label.trim()) {
      renamed.set(op.target, op.label.trim());
    }
  }

  return entries
    .filter((e) => !hidden.has(e.id))
    .map((e) => (renamed.has(e.id) ? { ...e, name: renamed.get(e.id) as string } : e))
    .map((e, index) => ({ e, index }))
    .sort((a, b) => a.e.order - b.e.order || a.index - b.index)
    .map(({ e }) => e);
}
