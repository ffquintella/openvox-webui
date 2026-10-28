import { describe, expect, it } from 'vitest';
import { Puzzle, KeyRound, Server, Settings, Users } from 'lucide-react';
import { mergeMenu, isSafePluginHref, type CoreMenuItem } from '../utils/pluginMenu';
import { PLUGIN_ICON_ALLOWLIST, resolvePluginIcon } from '../utils/pluginIcons';
import type { UiContribution, UiMenuOp } from '../types';

const core: CoreMenuItem[] = [
  { id: 'core:nodes', name: 'Nodes', href: '/nodes', icon: Server },
  { id: 'core:users', name: 'Users', href: '/users', icon: Users },
  { id: 'core:plugins', name: 'Plugins', href: '/plugins', icon: Puzzle },
  { id: 'core:settings', name: 'Settings', href: '/settings', icon: Settings },
];

function contribution(partial: Partial<UiContribution>): UiContribution {
  return {
    plugin_id: 'acme',
    plugin_name: 'Acme',
    slot: 'nav.main',
    id: 'menu',
    render: 'declarative',
    title: null,
    path: null,
    order: 150,
    label: 'Acme page',
    icon: 'KeyRound',
    href: '/p/acme/home',
    ...partial,
  };
}

const ids = (items: { id: string }[]) => items.map((i) => i.id);

describe('mergeMenu', () => {
  it('keeps core order when there are no contributions', () => {
    expect(ids(mergeMenu(core, undefined, undefined))).toEqual([
      'core:nodes',
      'core:users',
      'core:plugins',
      'core:settings',
    ]);
  });

  it('interleaves plugin items by order (core is implicitly 100, 200, ...)', () => {
    const merged = mergeMenu(core, [contribution({ order: 150 })], []);
    expect(ids(merged)).toEqual([
      'core:nodes',
      'acme:menu',
      'core:users',
      'core:plugins',
      'core:settings',
    ]);
    expect(merged[1]).toMatchObject({
      name: 'Acme page',
      href: '/p/acme/home',
      icon: KeyRound,
      source: 'plugin',
    });
  });

  it('puts a high-order plugin item last and keeps ties stable (core first)', () => {
    const merged = mergeMenu(
      core,
      [contribution({ id: 'a', order: 999 }), contribution({ id: 'b', order: 100 })],
      []
    );
    expect(ids(merged)).toEqual([
      'core:nodes',
      'acme:b',
      'core:users',
      'core:plugins',
      'core:settings',
      'acme:a',
    ]);
  });

  it('applies hide and rename ops by target id', () => {
    const ops: UiMenuOp[] = [
      { plugin_id: 'acme', target: 'core:users', op: 'hide', label: null },
      { plugin_id: 'acme', target: 'core:nodes', op: 'rename', label: 'Servers' },
    ];
    const merged = mergeMenu(core, [], ops);
    expect(ids(merged)).not.toContain('core:users');
    expect(merged.find((i) => i.id === 'core:nodes')?.name).toBe('Servers');
  });

  it('never hides core:settings or core:plugins', () => {
    const ops: UiMenuOp[] = [
      { plugin_id: 'evil', target: 'core:settings', op: 'hide', label: null },
      { plugin_id: 'evil', target: 'core:plugins', op: 'hide', label: null },
    ];
    expect(ids(mergeMenu(core, [], ops))).toEqual(
      expect.arrayContaining(['core:settings', 'core:plugins'])
    );
  });

  it('ignores rename ops without a label and items with unsafe hrefs', () => {
    const merged = mergeMenu(
      core,
      [
        contribution({ id: 'x', href: 'javascript:alert(1)' }),
        contribution({ id: 'y', href: '//evil.example.com' }),
        contribution({ id: 'z', href: '/nodes' }),
      ],
      [{ plugin_id: 'acme', target: 'core:nodes', op: 'rename', label: '  ' }]
    );
    expect(ids(merged)).toEqual(['core:nodes', 'core:users', 'core:plugins', 'core:settings']);
    expect(merged[0].name).toBe('Nodes');
  });

  it('only allows hrefs inside the plugin namespace', () => {
    expect(isSafePluginHref('/p/acme/home')).toBe(true);
    expect(isSafePluginHref('/p/acme')).toBe(true);
    expect(isSafePluginHref('/settings')).toBe(false);
    expect(isSafePluginHref('/p/../settings')).toBe(false);
    expect(isSafePluginHref(null)).toBe(false);
  });
});

describe('resolvePluginIcon', () => {
  it('resolves allowlisted names', () => {
    expect(resolvePluginIcon('KeyRound')).toBe(KeyRound);
    expect(resolvePluginIcon('Server')).toBe(Server);
    expect(Object.keys(PLUGIN_ICON_ALLOWLIST)).toContain('Plug');
  });

  it('falls back to Puzzle for unknown, missing or prototype names', () => {
    expect(resolvePluginIcon('NotAnIcon')).toBe(Puzzle);
    expect(resolvePluginIcon(null)).toBe(Puzzle);
    expect(resolvePluginIcon(undefined)).toBe(Puzzle);
    expect(resolvePluginIcon('constructor')).toBe(Puzzle);
    expect(resolvePluginIcon('__proto__')).toBe(Puzzle);
  });
});
