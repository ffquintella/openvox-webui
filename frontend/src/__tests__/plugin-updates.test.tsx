// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import type { ReactNode } from 'react';
import UpdatesTab from '../components/plugins/UpdatesTab';
import PluginVersionsPanel from '../components/plugins/PluginVersionsPanel';
import { PreflightPanel } from '../components/plugins/InstallPluginDialog';
import {
  canSubmitRevoke,
  isValidWindowShape,
  selectableForBatch,
  versionActions,
} from '../components/plugins/updateHelpers';
import { pluginsApi } from '../services/api';
import type {
  PluginPreflightResult,
  PluginUpdateInfo,
  PluginUsageTotals,
  PluginVersionView,
} from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function wrap(children: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{children}</QueryClientProvider>);
}

const usage: PluginUsageTotals = {
  calls: 0,
  errors: 0,
  traps: 0,
  timeouts: 0,
  quota_rejections: 0,
  circuit_rejections: 0,
  error_rate: 0,
  avg_ms: null,
  p50_ms: null,
  p95_ms: null,
  p99_ms: null,
  max_ms: 0,
};

const info = (over: Partial<PluginUpdateInfo>): PluginUpdateInfo => ({
  plugin_id: 'acme',
  name: 'Acme',
  state: 'enabled',
  active_version: '1.0.0',
  available_version: null,
  pending_version: null,
  added_capabilities: [],
  requires_approval: false,
  rollback_warning: null,
  auto_update: 'off',
  update_window: null,
  next_window_at: null,
  update_hold_version: null,
  update_hold_reason: null,
  ...over,
});

const version = (over: Partial<PluginVersionView>): PluginVersionView => ({
  plugin_id: 'acme',
  version: '1.0.0',
  api_version: 2,
  package_sha256: 'a'.repeat(64),
  signed_by_key_id: 'b'.repeat(64),
  signed_at: '2026-10-01T00:00:00Z',
  revoked: false,
  revoked_reason: null,
  revoked_by: null,
  revoked_at: null,
  added_at: '2026-10-01T00:00:00Z',
  relation: 'active',
  pending: false,
  blocked: false,
  schema_level: 0,
  added_capabilities: [],
  can_activate: false,
  blocked_reason: null,
  rollback_warning: null,
  usage,
  ...over,
});

describe('update helpers', () => {
  it('checks the shape of an update window', () => {
    expect(isValidWindowShape('')).toBe(true);
    expect(isValidWindowShape('* 2-3 * * 6')).toBe(true);
    expect(isValidWindowShape('0 2 * *')).toBe(false);
    expect(isValidWindowShape('nightly')).toBe(false);
  });

  it('selects only plugins with an available version and needs a revocation reason', () => {
    expect(
      selectableForBatch([info({}), info({ plugin_id: 'b', available_version: '1.1.0' })])
    ).toEqual(['b']);
    expect(canSubmitRevoke('  ')).toBe(false);
    expect(canSubmitRevoke('bad build')).toBe(true);
    expect(canSubmitRevoke('x'.repeat(501))).toBe(false);
  });

  it('offers rollback only for older versions that can be activated', () => {
    expect(versionActions(version({ relation: 'older', can_activate: true })).rollback).toBe(true);
    expect(versionActions(version({ relation: 'older', can_activate: false })).rollback).toBe(
      false
    );
    expect(versionActions(version({ relation: 'newer', can_activate: true })).update).toBe(true);
    expect(versionActions(version({ revoked: true })).unrevoke).toBe(true);
  });
});

describe('UpdatesTab', () => {
  const items = [
    info({ plugin_id: 'acme', name: 'Acme', available_version: '1.1.0' }),
    info({
      plugin_id: 'beta',
      name: 'Beta',
      available_version: '2.0.0',
      requires_approval: true,
      added_capabilities: ['kv:rw'],
      rollback_warning: 'Rolling back from 2.0.0 to 1.0.0 will be blocked',
    }),
    info({ plugin_id: 'calm', name: 'Calm', auto_update: 'patch', update_window: '* 2 * * *' }),
  ];

  it('applies the selected updates in one batch and shows the outcomes', async () => {
    vi.spyOn(pluginsApi, 'updates').mockResolvedValue(items);
    const apply = vi.spyOn(pluginsApi, 'applyUpdates').mockResolvedValue([
      {
        plugin_id: 'acme',
        outcome: 'updated',
        from_version: '1.0.0',
        to_version: '1.1.0',
        added_capabilities: [],
        code: null,
        message: null,
      },
      {
        plugin_id: 'beta',
        outcome: 'pending_approval',
        from_version: '1.0.0',
        to_version: '2.0.0',
        added_capabilities: ['kv:rw'],
        code: null,
        message: null,
      },
    ]);
    wrap(<UpdatesTab canAdmin />);
    expect(await screen.findByText(/will be blocked/)).toBeInTheDocument();
    expect(screen.getByText(/Adds kv:rw/)).toBeInTheDocument();
    expect(screen.getByLabelText('Select Calm')).toBeDisabled();

    const button = screen.getByRole('button', { name: /Update selected \(0\)/ });
    expect(button).toBeDisabled();
    fireEvent.click(screen.getByLabelText('Select Acme'));
    fireEvent.click(screen.getByLabelText('Select Beta'));
    fireEvent.click(screen.getByRole('button', { name: /Update selected \(2\)/ }));
    await waitFor(() => expect(apply).toHaveBeenCalledWith(['acme', 'beta']));
    expect(await screen.findByText('Waiting for capability approval')).toBeInTheDocument();
  });

  it('validates the window before saving the automatic update policy', async () => {
    vi.spyOn(pluginsApi, 'updates').mockResolvedValue(items);
    const save = vi
      .spyOn(pluginsApi, 'setUpdatePolicy')
      .mockResolvedValue({} as Awaited<ReturnType<typeof pluginsApi.setUpdatePolicy>>);
    wrap(<UpdatesTab canAdmin />);
    fireEvent.click(await screen.findByLabelText('Automatic updates of Calm'));
    const input = screen.getByLabelText('Update window');
    fireEvent.change(input, { target: { value: 'nightly' } });
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    fireEvent.change(input, { target: { value: '* 2-3 * * 6' } });
    fireEvent.change(screen.getByLabelText('Automatic update policy'), {
      target: { value: 'minor' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith('calm', {
        auto_update: 'minor',
        update_window: '* 2-3 * * 6',
      })
    );
  });

  it('is read-only without the admin permission', async () => {
    vi.spyOn(pluginsApi, 'updates').mockResolvedValue(items);
    wrap(<UpdatesTab canAdmin={false} />);
    await screen.findByText('Acme');
    expect(screen.queryByRole('button', { name: /Update selected/ })).toBeNull();
    expect(screen.queryByLabelText('Select Acme')).toBeNull();
  });
});

describe('PluginVersionsPanel', () => {
  const versions = [
    version({ version: '1.2.0', relation: 'active', schema_level: 3 }),
    version({
      version: '1.1.0',
      relation: 'older',
      can_activate: false,
      blocked_reason: 'Rollback is blocked: migration(s) 0003 have no .down file',
    }),
    version({ version: '1.0.0', relation: 'older', can_activate: true, schema_level: 1 }),
  ];

  it('explains a blocked rollback and rolls back to a version that allows it', async () => {
    vi.spyOn(pluginsApi, 'versions').mockResolvedValue(versions);
    const rollback = vi
      .spyOn(pluginsApi, 'rollback')
      .mockResolvedValue({} as Awaited<ReturnType<typeof pluginsApi.rollback>>);
    wrap(<PluginVersionsPanel pluginId="acme" canAdmin />);
    expect(await screen.findByText(/0003 have no .down file/)).toBeInTheDocument();
    const buttons = screen.getAllByRole('button', { name: 'Roll back to' });
    expect(buttons).toHaveLength(1);
    fireEvent.click(buttons[0]);
    fireEvent.click(screen.getByRole('button', { name: 'Roll back' }));
    await waitFor(() => expect(rollback).toHaveBeenCalledWith('acme', '1.0.0'));
  });

  it('asks for a reason before revoking a version', async () => {
    vi.spyOn(pluginsApi, 'versions').mockResolvedValue(versions);
    const revoke = vi
      .spyOn(pluginsApi, 'revokeVersion')
      .mockResolvedValue({} as Awaited<ReturnType<typeof pluginsApi.revokeVersion>>);
    wrap(<PluginVersionsPanel pluginId="acme" canAdmin />);
    await screen.findByText('1.2.0');
    fireEvent.click(screen.getAllByRole('button', { name: 'Revoke' })[0]);
    const revokeButtons = screen.getAllByRole('button', { name: 'Revoke' });
    const confirm = revokeButtons[revokeButtons.length - 1];
    expect(confirm).toBeDisabled();
    expect(screen.getByText(/the plugin is disabled at once/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Revocation reason'), {
      target: { value: 'supply chain incident' },
    });
    fireEvent.click(confirm);
    await waitFor(() =>
      expect(revoke).toHaveBeenCalledWith('acme', '1.2.0', 'supply chain incident')
    );
  });

  it('shows no actions without the admin permission', async () => {
    vi.spyOn(pluginsApi, 'versions').mockResolvedValue(versions);
    wrap(<PluginVersionsPanel pluginId="acme" canAdmin={false} />);
    await screen.findByText('1.2.0');
    expect(screen.queryByRole('button', { name: 'Revoke' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Roll back to' })).toBeNull();
  });
});

describe('PreflightPanel for an update', () => {
  const result: PluginPreflightResult = {
    manifest: {
      id: 'acme',
      name: 'Acme',
      version: '1.2.0',
      publisher: 'Acme Inc',
      api_version: 2,
      license: null,
      runtime_kind: 'wasm',
      min_webui_version: null,
    },
    package_sha256: 'f'.repeat(64),
    signature: {
      status: 'valid',
      key_id: 'c'.repeat(64),
      key_label: 'Acme',
      key_origin: 'admin',
      signed_at: '2026-10-01T00:00:00Z',
      message: null,
    },
    capabilities: [],
    declared_pii_fields: [],
    installed_version: '1.1.0',
    issues: [],
    can_install: true,
    added_capabilities: ['kv:rw'],
    rollback_warning: 'Rolling back from 1.2.0 to 1.1.0 will be blocked: migration(s) 0003',
  };

  it('warns about a blocked rollback before updating and can store the version only', () => {
    const onInstall = vi.fn();
    render(
      <PreflightPanel
        result={result}
        canTrustKeys={false}
        installing={false}
        installError={null}
        onInstall={onInstall}
        onCancel={() => undefined}
      />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('will be blocked');
    expect(screen.getByText(/This version adds kv:rw/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox'));
    fireEvent.click(screen.getByRole('button', { name: 'Store version' }));
    expect(onInstall).toHaveBeenCalledWith({ stage: true });
  });
});
