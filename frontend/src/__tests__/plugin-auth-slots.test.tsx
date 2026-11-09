// @vitest-environment jsdom
// The profile.security and admin.auth slots (H-6): mounted on the Profile page
// and in an Authentication tab of Settings that exists only when the server
// lists admin.auth contributions for this user.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import { api, pluginsApi } from '../services/api';
import Profile from '../pages/Profile';
import Settings from '../pages/Settings';
import type { UiContribution, UiSlots } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const contribution = (slot: string, id: string, title: string): UiContribution => ({
  plugin_id: 'mfa-fido2',
  plugin_name: 'FIDO2 Security Keys',
  slot,
  id,
  render: 'declarative',
  title,
  path: null,
  order: 100,
  label: null,
  icon: null,
  href: null,
});

function slots(entries: Record<string, UiContribution[]>): UiSlots {
  return { slots: entries, menu_ops: [] };
}

function renderPage(page: React.ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>{page}</MemoryRouter>
    </QueryClientProvider>
  );
}

function mockContribution() {
  vi.spyOn(pluginsApi, 'getContribution').mockImplementation(async (pluginId, id) => ({
    ...contribution(id === 'keys' ? 'profile.security' : 'admin.auth', id, 'Security keys'),
    plugin_id: pluginId,
    spec: { layout: 'markdown', content: `Panel ${id}` },
    routes: [],
  }));
}

describe('authentication slots', () => {
  it('mounts profile.security contributions on the Profile page', async () => {
    vi.spyOn(api, 'getMyFactors').mockResolvedValue({
      factors: [],
      available: [],
      session: { amr: ['pwd'], acr: 1 },
    });
    vi.spyOn(pluginsApi, 'getUiSlots').mockResolvedValue(
      slots({ 'profile.security': [contribution('profile.security', 'keys', 'Security keys')] })
    );
    mockContribution();
    renderPage(<Profile />);
    expect(await screen.findByTestId('plugin-slot-profile.security')).toBeInTheDocument();
    expect(await screen.findByText('Panel keys')).toBeInTheDocument();
    expect(screen.getByText('FIDO2 Security Keys')).toBeInTheDocument();
  });

  it('adds the Authentication tab only when admin.auth has contributions for the user', async () => {
    const getSlots = vi.spyOn(pluginsApi, 'getUiSlots').mockResolvedValue(slots({}));
    vi.spyOn(api, 'getSettings').mockRejectedValue(new Error('not in this test'));
    mockContribution();
    renderPage(<Settings />);
    await screen.findByRole('button', { name: 'General' });
    await vi.waitFor(() => expect(getSlots).toHaveBeenCalled());
    expect(screen.queryByRole('button', { name: 'Authentication' })).toBeNull();

    cleanup();
    getSlots.mockResolvedValue(
      slots({ 'admin.auth': [contribution('admin.auth', 'admin', 'Security keys (FIDO2)')] })
    );
    renderPage(<Settings />);
    fireEvent.click(await screen.findByRole('button', { name: 'Authentication' }));
    expect(await screen.findByTestId('plugin-slot-admin.auth')).toBeInTheDocument();
    expect(await screen.findByText('Panel admin')).toBeInTheDocument();
  });
});
