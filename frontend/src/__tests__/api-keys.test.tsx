// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import ApiKeys from '../pages/ApiKeys';
import { api } from '../services/api';
import type { ApiKey } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const existing: ApiKey = {
  id: 'k1',
  organization_id: 'o',
  user_id: 'u',
  name: 'ci',
  role_ids: [],
  created_at: '2026-01-01T00:00:00Z',
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <ApiKeys />
    </QueryClientProvider>
  );
}

describe('ApiKeys page', () => {
  it('lists keys', async () => {
    vi.spyOn(api, 'getApiKeys').mockResolvedValue([existing]);
    renderPage();
    expect(await screen.findByText('ci')).toBeInTheDocument();
  });

  it('shows the secret once after creating', async () => {
    vi.spyOn(api, 'getApiKeys').mockResolvedValue([]);
    const create = vi
      .spyOn(api, 'createApiKey')
      .mockResolvedValue({ ...existing, name: 'new', key: 'ovk_secret' });
    renderPage();
    fireEvent.change(await screen.findByLabelText('Key name'), { target: { value: 'new' } });
    fireEvent.click(screen.getByRole('button', { name: /create key/i }));
    expect(await screen.findByText('ovk_secret')).toBeInTheDocument();
    expect(create).toHaveBeenCalledWith({ name: 'new', expires_at: undefined });
  });

  it('revokes after confirmation', async () => {
    vi.spyOn(api, 'getApiKeys').mockResolvedValue([existing]);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const revoke = vi.spyOn(api, 'revokeApiKey').mockResolvedValue();
    renderPage();
    fireEvent.click(await screen.findByLabelText('Revoke ci'));
    await waitFor(() => expect(revoke.mock.calls[0][0]).toBe('k1'));
  });
});
