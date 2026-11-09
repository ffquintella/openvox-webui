// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import Login from '../pages/Login';
import { api } from '../services/api';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

/** What axios rejects with for an answer of the given status and body. */
function apiError(status: number, data: Record<string, unknown>) {
  return Object.assign(new Error(`Request failed with status code ${status}`), {
    response: { status, data },
  });
}

async function signIn(error: Error) {
  vi.spyOn(api, 'getAuthFactors').mockResolvedValue([]);
  vi.spyOn(api, 'getServerInfo').mockResolvedValue(
    {} as Awaited<ReturnType<typeof api.getServerInfo>>
  );
  vi.spyOn(api, 'login').mockRejectedValue(error);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={['/login']}>
        <Login />
      </MemoryRouter>
    </QueryClientProvider>
  );
  fireEvent.change(screen.getByLabelText(/username/i), { target: { value: 'alice' } });
  fireEvent.change(screen.getByLabelText(/^password/i), { target: { value: 'secret-pw' } });
  fireEvent.click(screen.getByRole('button', { name: /sign in/i }));
  return screen.findByRole('alert');
}

describe('Login refusals', () => {
  it('explains a policy that needs a second factor the account does not have', async () => {
    const alert = await signIn(
      apiError(403, {
        error: 'forbidden',
        code: 'mfa_enrollment_required',
        message: 'Policy requires a second factor and none is enrolled; ask an administrator',
      })
    );
    expect(alert).toHaveTextContent(
      'Your organization requires a second factor and this account has none. Ask an administrator to set one up for you.'
    );
    expect(alert).not.toHaveTextContent('status code');
  });

  it('explains that the enrolled factors cannot be used right now (fail closed)', async () => {
    const alert = await signIn(
      apiError(403, {
        error: 'forbidden',
        code: 'factor_unavailable',
        message: 'None of the second factors enrolled for this account is available',
      })
    );
    expect(alert).toHaveTextContent(
      'None of your verification methods can be used right now. Use a recovery code if you have one, or ask an administrator.'
    );
  });

  it('explains the phishing-resistant policy refusal', async () => {
    const alert = await signIn(
      apiError(403, { error: 'forbidden', code: 'phishing_resistant_factor_required' })
    );
    expect(alert).toHaveTextContent(
      'Your organization requires a security key or passkey for this account.'
    );
  });

  it('shows the server message for other refusals, never the transport error', async () => {
    const alert = await signIn(
      apiError(401, { error: 'unauthorized', message: 'Invalid username or password' })
    );
    expect(alert).toHaveTextContent('Invalid username or password');
    expect(alert).not.toHaveTextContent('status code');
  });
});
