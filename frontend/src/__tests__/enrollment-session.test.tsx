// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { AxiosAdapter } from 'axios';
import '@testing-library/jest-dom/vitest';
import EnrollmentSession from '../components/auth/EnrollmentSession';
import { authErrorMessage } from '../components/auth/authErrors';
import {
  api,
  apiClient,
  isEnrollmentSession,
  setEnrollmentSessionToken,
  type MyFactors,
} from '../services/api';
import { creationOptions, fakeAttestation, installWebAuthn } from './webauthn-ui-helpers';

const originalAdapter = apiClient.defaults.adapter;

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  apiClient.defaults.adapter = originalAdapter;
  setEnrollmentSessionToken(null);
  localStorage.clear();
});

const factors: MyFactors = {
  factors: [],
  available: [
    { id: 'totp', kind: 'otp', label: 'Authenticator app', fields: ['code'] },
    { id: 'webauthn', kind: 'webauthn', label: 'Security key or passkey', fields: ['credential'] },
  ],
  session: { amr: ['pwd'], acr: 1, scope: 'enrollment' },
};

function renderSession(onDone = vi.fn()) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <EnrollmentSession username="dana" expiresIn={890} onDone={onDone} />
    </QueryClientProvider>
  );
  return onDone;
}

describe('EnrollmentSession', () => {
  it('adds a security key, shows the recovery codes once and ends the session', async () => {
    const browser = installWebAuthn();
    browser.create.mockResolvedValue(fakeAttestation());
    const list = vi.spyOn(api, 'getMyFactors').mockResolvedValue(factors);
    vi.spyOn(api, 'beginFactorEnroll').mockResolvedValue(creationOptions);
    vi.spyOn(api, 'completeFactorEnroll').mockResolvedValue({
      enrolled: 'webauthn',
      recovery: { codes: ['AAAAA-BBBBB'], shown_once: true },
      reauthenticate: true,
    });
    const onDone = renderSession();

    expect(
      await screen.findByRole('heading', { name: 'Add a verification method' })
    ).toBeInTheDocument();
    expect(screen.getByText(/Signed in as dana\..*ends in 15 minutes/)).toBeInTheDocument();
    const addKey = await screen.findByRole('button', { name: /add security key/i });
    const callsBefore = list.mock.calls.length;
    fireEvent.click(addKey);

    const heading = await screen.findByRole('heading', { name: 'Save your recovery codes' });
    expect(heading).toHaveFocus();
    expect(screen.getByText('AAAAA-BBBBB')).toBeInTheDocument();
    // The ended session is never used again.
    expect(list).toHaveBeenCalledTimes(callsBefore);
    expect(onDone).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'I have saved these codes' }));
    expect(onDone).toHaveBeenCalledWith('enrolled');
  });

  it('explains when no method can be added here and signs out', async () => {
    installWebAuthn();
    vi.spyOn(api, 'getMyFactors').mockResolvedValue({
      ...factors,
      available: [factors.available[0]],
    });
    const logout = vi.spyOn(api, 'logout').mockResolvedValue();
    const onDone = renderSession();

    expect(await screen.findByRole('alert')).toHaveTextContent('No verification method');
    fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
    await waitFor(() => expect(onDone).toHaveBeenCalledWith('signed_out'));
    expect(logout).toHaveBeenCalledTimes(1);
  });
});

describe('enrollment-only session in the API client', () => {
  it('recognises the login answer', () => {
    expect(
      isEnrollmentSession({
        status: 'enrollment_required',
        session_scope: 'enrollment',
        access_token: 'x',
        token_type: 'Bearer',
        expires_in: 900,
        user: { id: '1', username: 'dana', email: 'd@x', role: 'viewer' },
      })
    ).toBe(true);
    expect(
      isEnrollmentSession({
        status: 'mfa_required',
        challenge_id: 'c',
        expires_in: 300,
        required_acr: 2,
        factors: [],
      })
    ).toBe(false);
  });

  it('sends the in-memory token and never stores it', async () => {
    const seen: string[] = [];
    const adapter: AxiosAdapter = async (config) => {
      seen.push(String(config.headers.Authorization));
      return { data: factors, status: 200, statusText: 'OK', headers: {}, config };
    };
    apiClient.defaults.adapter = adapter;

    setEnrollmentSessionToken('enrollment-token');
    await api.getMyFactors();
    expect(localStorage.getItem('auth_token')).toBeNull();

    setEnrollmentSessionToken(null);
    localStorage.setItem('auth_token', 'stored-token');
    await api.getMyFactors();
    expect(seen).toEqual(['Bearer enrollment-token', 'Bearer stored-token']);
  });

  it('translates the enrollment_only_session refusal', () => {
    const err = { response: { status: 403, data: { code: 'enrollment_only_session' } } };
    expect(authErrorMessage(err, 'auth.error.key_rejected')).toBe(
      'This sign-in can only add a verification method. Add one, then sign in again.'
    );
  });

  it('translates the phishing-resistant policy refusal (OQ-6)', () => {
    const err = {
      response: { status: 403, data: { code: 'phishing_resistant_factor_required' } },
    };
    expect(authErrorMessage(err, 'auth.error.key_rejected')).toBe(
      'Your organization requires a security key or passkey for this account. Add one, or ask your administrator.'
    );
  });
});
