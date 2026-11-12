// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import MfaPolicies from '../pages/MfaPolicies';
import { api } from '../services/api';
import type { Role } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const roles = [
  { id: 'r1', name: 'super_admin', display_name: 'Super', is_system: true },
  { id: 'r2', name: 'operator', display_name: 'Operator', is_system: true },
] as unknown as Role[];

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MfaPolicies />
    </QueryClientProvider>
  );
}

describe('MfaPolicies page', () => {
  it('explains that no policy means no second factor', async () => {
    vi.spyOn(api, 'getMfaPolicies').mockResolvedValue([]);
    vi.spyOn(api, 'getRoles').mockResolvedValue(roles);
    renderPage();
    expect(await screen.findByText(/no second factor is requested/i)).toBeInTheDocument();
  });

  it('creates a policy and saves it', async () => {
    vi.spyOn(api, 'getMfaPolicies').mockResolvedValue([]);
    vi.spyOn(api, 'getRoles').mockResolvedValue(roles);
    const save = vi.spyOn(api, 'saveMfaPolicies').mockImplementation(async (p) => p);
    renderPage();

    fireEvent.click(await screen.findByText('Add policy'));
    fireEvent.change(screen.getByLabelText('Policy name'), { target: { value: 'admins' } });
    fireEvent.click(await screen.findByLabelText('super_admin'));
    fireEvent.change(screen.getByLabelText('Providers'), { target: { value: 'local, saml' } });
    fireEvent.change(screen.getByLabelText('Required factor'), { target: { value: 'key' } });
    fireEvent.click(screen.getByText('Save policies'));

    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0][0]).toEqual([
      {
        name: 'admins',
        enabled: true,
        priority: 100,
        conditions: { roles: ['super_admin'], providers: ['local', 'saml'] },
        required_acr: 2,
        require_phishing_resistant: true,
      },
    ]);
    expect(await screen.findByText('Policies saved.')).toBeInTheDocument();
  });

  it('blocks saving a policy without a name', async () => {
    vi.spyOn(api, 'getMfaPolicies').mockResolvedValue([]);
    vi.spyOn(api, 'getRoles').mockResolvedValue(roles);
    renderPage();
    fireEvent.click(await screen.findByText('Add policy'));
    expect(screen.getByText('Save policies')).toBeDisabled();
  });

  it('shows the step-up hint', async () => {
    vi.spyOn(api, 'getMfaPolicies').mockResolvedValue([]);
    vi.spyOn(api, 'getRoles').mockResolvedValue(roles);
    vi.spyOn(api, 'saveMfaPolicies').mockRejectedValue({
      response: { data: { code: 'step_up_required' } },
    });
    renderPage();
    fireEvent.click(await screen.findByText('Add policy'));
    fireEvent.change(screen.getByLabelText('Policy name'), { target: { value: 'x' } });
    fireEvent.click(screen.getByText('Save policies'));
    expect(await screen.findByRole('alert')).toHaveTextContent(/second factor/i);
  });
});
