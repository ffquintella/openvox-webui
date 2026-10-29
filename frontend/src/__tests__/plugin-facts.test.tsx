// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import type { ReactNode } from 'react';
import FactsTab from '../components/plugins/FactsTab';
import {
  compactJson,
  exportBlockedReason,
  factSourceLabel,
  isPluginSource,
  namespaceSources,
  pluginIdFromSource,
  ruleDependsOnPlugin,
  rulesBlockedReason,
} from '../components/plugins/factHelpers';
import { pluginsApi } from '../services/api';
import type { FactPathEntry, PluginFactCatalog, PluginFactInfo } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function wrap(children: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{children}</QueryClientProvider>);
}

const fact = (over: Partial<PluginFactInfo> = {}): PluginFactInfo => ({
  name: 'acme_cmdb.owner',
  local_name: 'owner',
  type: 'string',
  description: null,
  enum: null,
  ttl_s: 3600,
  max_stale_s: 86400,
  sensitivity: 'none',
  offers_rules: true,
  offers_export: true,
  usable_in_rules: false,
  exportable: false,
  approved_by: null,
  approved_at: null,
  requires: null,
  nodes: 12,
  stale_nodes: 2,
  conflict: false,
  ...over,
});

const catalog = (over: Partial<PluginFactCatalog> = {}): PluginFactCatalog => ({
  plugin_id: 'acme-cmdb',
  plugin_name: 'Acme CMDB',
  state: 'enabled',
  active_version: '1.0.0',
  namespace: 'acme_cmdb',
  mode: 'pull',
  handler: 'collect_facts',
  ttl_s: 3600,
  max_stale_s: 86400,
  export_capability: 'approved',
  facts: [fact()],
  provider: {
    last_success_at: '2026-10-29T10:00:00Z',
    last_failure_at: null,
    last_error: null,
    failures: 0,
    rejected: 1,
  },
  puppetdb_checked: true,
  ...over,
});

const preview = (name = 'acme_cmdb.owner') => ({
  plugin_id: 'acme-cmdb',
  name,
  sensitivity: 'none' as const,
  exportable: false,
  total: 2,
  limit: 50,
  rows: [
    {
      certname: 'web01.example.com',
      value: 'team-a',
      computed_at: '2026-10-29T09:00:00Z',
      stale: false,
      bytes: 6,
    },
    {
      certname: 'db01.example.com',
      value: { team: 'b' },
      computed_at: '2026-10-01T09:00:00Z',
      stale: true,
      bytes: 12,
    },
  ],
});

describe('fact helpers', () => {
  it('reads the plugin id from a source and labels PuppetDB facts', () => {
    expect(isPluginSource('plugin:acme-cmdb')).toBe(true);
    expect(isPluginSource(undefined)).toBe(false);
    expect(pluginIdFromSource('plugin:acme-cmdb')).toBe('acme-cmdb');
    expect(pluginIdFromSource('plugin:')).toBeNull();
    expect(pluginIdFromSource(undefined)).toBeNull();
    expect(factSourceLabel(undefined)).toBe('PuppetDB');
    expect(factSourceLabel('plugin:acme-cmdb')).toBe('plugin: acme-cmdb');
  });

  it('explains why exporting is blocked', () => {
    expect(exportBlockedReason(fact(), catalog())).toBeNull();
    expect(exportBlockedReason(fact({ offers_export: false }), catalog())).toMatch(
      /does not offer/
    );
    expect(exportBlockedReason(fact(), catalog({ export_capability: 'not_approved' }))).toMatch(
      /not been approved/
    );
    expect(exportBlockedReason(fact(), catalog({ export_capability: 'not_declared' }))).toMatch(
      /does not declare/
    );
    expect(exportBlockedReason(fact(), catalog({ export_capability: 'denied_by_policy' }))).toMatch(
      /denied by policy/
    );
    expect(exportBlockedReason(fact({ conflict: true }), catalog())).toMatch(/agent value wins/);
  });

  it('explains why use in rules is blocked', () => {
    expect(rulesBlockedReason(fact())).toBeNull();
    expect(rulesBlockedReason(fact({ offers_rules: false }))).toMatch(/does not offer/);
    expect(rulesBlockedReason(fact({ conflict: true }))).toMatch(/agent value wins/);
  });

  it('maps namespaces to plugins and finds the plugin a rule depends on', () => {
    const entries: FactPathEntry[] = [
      { path: ['acme_cmdb', 'owner'], type: 'string', source: 'plugin:acme-cmdb' },
      { path: ['acme_cmdb', 'rack'], type: 'string', source: 'plugin:acme-cmdb' },
      { path: ['os', 'family'], type: 'string' },
    ];
    const map = namespaceSources(entries);
    expect([...map.entries()]).toEqual([['acme_cmdb', 'acme-cmdb']]);
    expect(namespaceSources(undefined).size).toBe(0);
    expect(ruleDependsOnPlugin('acme_cmdb.owner', map)).toBe('acme-cmdb');
    expect(ruleDependsOnPlugin('acme_cmdb', map)).toBe('acme-cmdb');
    expect(ruleDependsOnPlugin('os.family', map)).toBeNull();
    expect(ruleDependsOnPlugin('', map)).toBeNull();
  });

  it('shortens long compact JSON', () => {
    expect(compactJson({ a: 1 })).toBe('{"a":1}');
    expect(compactJson('x'.repeat(300), 20)).toHaveLength(20);
  });
});

describe('FactsTab', () => {
  it('shows an empty state when no plugin declares facts', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([]);
    wrap(<FactsTab canAdmin />);
    expect(await screen.findByText('No installed plugin declares facts')).toBeInTheDocument();
  });

  it('shows the plugin, its provider status and the fact flags', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([
      catalog({
        provider: {
          last_success_at: null,
          last_failure_at: '2026-10-29T10:00:00Z',
          last_error: 'upstream timeout',
          failures: 3,
          rejected: 1,
        },
        facts: [
          fact({ sensitivity: 'pii' }),
          fact({ name: 'acme_cmdb.rack', local_name: 'rack', conflict: true }),
        ],
      }),
    ]);
    wrap(<FactsTab canAdmin />);
    expect(await screen.findByText('Acme CMDB')).toBeInTheDocument();
    expect(screen.getByText(/Error: upstream timeout/)).toBeInTheDocument();
    expect(screen.getByText(/Failures: 3 \| Rejected: 1/)).toBeInTheDocument();
    expect(screen.getByText('PII')).toBeInTheDocument();
    expect(screen.getByText('hidden: agent fact wins')).toBeInTheDocument();
    expect(screen.getByText('Approved')).toBeInTheDocument();
  });

  it('disables the toggles with the reason when the manifest does not offer them', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([
      catalog({ facts: [fact({ offers_rules: false, offers_export: false })] }),
    ]);
    wrap(<FactsTab canAdmin />);
    const rules = await screen.findByLabelText('Usable in rules: acme_cmdb.owner');
    const exp = screen.getByLabelText('Exportable to agents: acme_cmdb.owner');
    expect(rules).toBeDisabled();
    expect(rules).toHaveAttribute(
      'title',
      expect.stringMatching(/does not offer this fact for rules/)
    );
    expect(exp).toBeDisabled();
    expect(exp).toHaveAttribute(
      'title',
      expect.stringMatching(/does not offer this fact for export/)
    );
  });

  it('disables export when the capability is not approved', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([
      catalog({ export_capability: 'not_approved' }),
    ]);
    wrap(<FactsTab canAdmin />);
    const exp = await screen.findByLabelText('Exportable to agents: acme_cmdb.owner');
    expect(exp).toBeDisabled();
    expect(exp).toHaveAttribute('title', expect.stringMatching(/facts:export:external/));
    expect(screen.getByLabelText('Usable in rules: acme_cmdb.owner')).toBeEnabled();
  });

  it('is read-only without the admin permission', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([catalog()]);
    wrap(<FactsTab canAdmin={false} />);
    expect(await screen.findByLabelText('Usable in rules: acme_cmdb.owner')).toBeDisabled();
    expect(screen.getByLabelText('Exportable to agents: acme_cmdb.owner')).toBeDisabled();
  });

  it('previews the values and needs the exact fact name before enabling export', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([catalog()]);
    const previewSpy = vi.spyOn(pluginsApi, 'getPluginFactPreview').mockResolvedValue(preview());
    const save = vi
      .spyOn(pluginsApi, 'updatePluginFactSettings')
      .mockResolvedValue(fact({ exportable: true }));
    wrap(<FactsTab canAdmin />);
    fireEvent.click(await screen.findByLabelText('Exportable to agents: acme_cmdb.owner'));

    const dialog = await screen.findByRole('dialog', { name: 'Export fact to agents' });
    expect(save).not.toHaveBeenCalled();
    expect(await within(dialog).findByText('web01.example.com')).toBeInTheDocument();
    expect(previewSpy).toHaveBeenCalledWith('acme-cmdb', 'acme_cmdb.owner');
    expect(within(dialog).getByText('{"team":"b"}')).toBeInTheDocument();
    expect(within(dialog).getByText(/High risk/)).toBeInTheDocument();
    expect(within(dialog).queryByText(/personal data \(PII\)/)).toBeNull();

    const confirm = within(dialog).getByRole('button', { name: 'Export to agents' });
    expect(confirm).toBeDisabled();
    const input = within(dialog).getByLabelText('Confirm fact name');
    fireEvent.change(input, { target: { value: 'owner' } });
    expect(confirm).toBeDisabled();
    fireEvent.change(input, { target: { value: 'acme_cmdb.owner' } });
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith('acme-cmdb', 'acme_cmdb.owner', {
        exportable: true,
        confirm: 'acme_cmdb.owner',
      })
    );
  });

  it('adds a PII warning to the export dialog and shows API errors', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([
      catalog({ facts: [fact({ sensitivity: 'pii' })] }),
    ]);
    vi.spyOn(pluginsApi, 'getPluginFactPreview').mockResolvedValue({
      ...preview(),
      sensitivity: 'pii',
    });
    vi.spyOn(pluginsApi, 'updatePluginFactSettings').mockRejectedValue({
      response: { data: { message: 'Capability facts:export:external is not approved' } },
    });
    wrap(<FactsTab canAdmin />);
    fireEvent.click(await screen.findByLabelText('Exportable to agents: acme_cmdb.owner'));
    const dialog = await screen.findByRole('dialog', { name: 'Export fact to agents' });
    expect(within(dialog).getByText(/personal data \(PII\)/)).toBeInTheDocument();
    await within(dialog).findByText('web01.example.com');
    fireEvent.change(within(dialog).getByLabelText('Confirm fact name'), {
      target: { value: 'acme_cmdb.owner' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Export to agents' }));
    expect(await within(dialog).findByText(/is not approved/)).toBeInTheDocument();
  });

  it('asks for confirmation before allowing a fact in rules', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([catalog()]);
    const save = vi
      .spyOn(pluginsApi, 'updatePluginFactSettings')
      .mockResolvedValue(fact({ usable_in_rules: true }));
    wrap(<FactsTab canAdmin />);
    fireEvent.click(await screen.findByLabelText('Usable in rules: acme_cmdb.owner'));
    const dialog = await screen.findByRole('dialog', { name: 'Use fact in rules' });
    expect(save).not.toHaveBeenCalled();
    expect(within(dialog).getByText(/environment and the classes/)).toBeInTheDocument();
    expect(within(dialog).getByText(/counts as missing/)).toBeInTheDocument();
    expect(within(dialog).getByText(/audit log/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Allow in rules' }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith('acme-cmdb', 'acme_cmdb.owner', { usable_in_rules: true })
    );
  });

  it('disables a setting directly, without a confirmation', async () => {
    vi.spyOn(pluginsApi, 'getPluginFactCatalogs').mockResolvedValue([
      catalog({
        // The capability was revoked later: the setting can still be switched off.
        export_capability: 'not_approved',
        facts: [fact({ exportable: true, usable_in_rules: true })],
      }),
    ]);
    const save = vi
      .spyOn(pluginsApi, 'updatePluginFactSettings')
      .mockResolvedValue(fact({ exportable: false }));
    wrap(<FactsTab canAdmin />);
    const exp = await screen.findByLabelText('Exportable to agents: acme_cmdb.owner');
    expect(exp).toBeEnabled();
    fireEvent.click(exp);
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith('acme-cmdb', 'acme_cmdb.owner', { exportable: false })
    );
    expect(screen.queryByRole('dialog')).toBeNull();

    fireEvent.click(screen.getByLabelText('Usable in rules: acme_cmdb.owner'));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith('acme-cmdb', 'acme_cmdb.owner', { usable_in_rules: false })
    );
  });
});
