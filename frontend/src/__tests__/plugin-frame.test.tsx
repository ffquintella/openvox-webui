// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import PluginFrame from '../components/plugins/PluginFrame';
import PluginSlot from '../components/plugins/PluginSlot';
import { pluginsApi } from '../services/api';
import type { UiContribution, UiSlots } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const iframeC: UiContribution = {
  plugin_id: 'acme',
  plugin_name: 'ACME',
  slot: 'node.detail.tabs',
  id: 'crm',
  render: 'iframe',
  title: 'CRM',
  path: null,
  order: 1000,
  label: null,
  icon: null,
  href: null,
};

function wrap(ui: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>
  );
}

describe('PluginFrame', () => {
  it('renders a sandboxed frame without same-origin access', async () => {
    vi.spyOn(pluginsApi, 'getUiCatalog').mockResolvedValue({ version: 1, slots: [] });
    vi.spyOn(pluginsApi, 'createUiSession').mockResolvedValue({
      url: '/api/v1/plugins/ui-assets/tok/ui/app.html',
      expires_in_secs: 3600,
      api_scopes: [],
    });
    wrap(<PluginFrame contribution={iframeC} slot="node.detail.tabs" />);
    const frame = await screen.findByTestId('plugin-frame-acme-crm');
    const sandbox = frame.getAttribute('sandbox') ?? '';
    expect(sandbox.split(' ').sort()).toEqual(['allow-forms', 'allow-scripts']);
    expect(sandbox).not.toContain('allow-same-origin');
    expect(frame).toHaveAttribute('loading', 'lazy');
    expect(frame).toHaveAttribute('referrerpolicy', 'no-referrer');
    expect(frame).toHaveAttribute('src', '/api/v1/plugins/ui-assets/tok/ui/app.html');
  });

  it('shows a message when the session cannot be created', async () => {
    vi.spyOn(pluginsApi, 'getUiCatalog').mockResolvedValue({ version: 1, slots: [] });
    vi.spyOn(pluginsApi, 'createUiSession').mockRejectedValue(new Error('nope'));
    wrap(<PluginFrame contribution={iframeC} slot="node.detail.tabs" />);
    expect(await screen.findByText(/could not be loaded/i)).toBeInTheDocument();
  });
});

describe('PluginSlot', () => {
  const slots: UiSlots = {
    slots: { 'footer.global': [{ ...iframeC, slot: 'footer.global', id: 'f' }] },
    menu_ops: [],
  };

  it('renders nothing when no plugin contributes', async () => {
    vi.spyOn(pluginsApi, 'getUiSlots').mockResolvedValue({ slots: {}, menu_ops: [] });
    const { container } = wrap(<PluginSlot slot="footer.global" />);
    await waitFor(() => expect(pluginsApi.getUiSlots).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('mounts the contributions of its slot only', async () => {
    vi.spyOn(pluginsApi, 'getUiSlots').mockResolvedValue(slots);
    vi.spyOn(pluginsApi, 'getUiCatalog').mockResolvedValue({ version: 1, slots: [] });
    vi.spyOn(pluginsApi, 'createUiSession').mockResolvedValue({
      url: '/x',
      expires_in_secs: 1,
      api_scopes: [],
    });
    wrap(
      <>
        <PluginSlot slot="footer.global" variant="bare" />
        <PluginSlot slot="toolbar.global" variant="bare" />
      </>
    );
    expect(await screen.findByTestId('plugin-frame-acme-f')).toBeInTheDocument();
    expect(screen.queryByTestId('plugin-slot-toolbar.global')).not.toBeInTheDocument();
  });
});
