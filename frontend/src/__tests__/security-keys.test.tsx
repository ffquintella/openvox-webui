// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import SecurityKeys from '../components/auth/SecurityKeys';
import { api, type MyFactors } from '../services/api';
import { creationOptions, domError, fakeAttestation, installWebAuthn } from './webauthn-ui-helpers';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const myFactors: MyFactors = {
  factors: [
    {
      id: 'rec-1',
      kind: 'webauthn',
      label: 'Office key',
      verified: true,
      created_at: '2026-11-01T10:00:00Z',
      last_used_at: null,
    },
  ],
  available: [
    { id: 'totp', kind: 'otp', label: 'Authenticator app', fields: ['code'] },
    { id: 'webauthn', kind: 'webauthn', label: 'Security key or passkey', fields: ['credential'] },
  ],
  session: { amr: ['pwd'], acr: 1 },
};

function renderKeys() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <SecurityKeys />
    </QueryClientProvider>
  );
}

describe('SecurityKeys', () => {
  it('lists registered keys and hides itself without a WebAuthn factor', async () => {
    installWebAuthn();
    vi.spyOn(api, 'getMyFactors').mockResolvedValue(myFactors);
    renderKeys();
    expect(
      await screen.findByRole('heading', { name: 'Security keys and passkeys' })
    ).toBeInTheDocument();
    expect(screen.getByText('Office key')).toBeInTheDocument();
    expect(screen.getByText(/Never used/)).toBeInTheDocument();

    cleanup();
    vi.spyOn(api, 'getMyFactors').mockResolvedValue({
      ...myFactors,
      available: [myFactors.available[0]],
    });
    const { container } = renderKeys();
    await waitFor(() => expect(api.getMyFactors).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('enrols a key with its nickname and shows the recovery codes once', async () => {
    const browser = installWebAuthn();
    browser.create.mockResolvedValue(fakeAttestation());
    vi.spyOn(api, 'getMyFactors').mockResolvedValue({ ...myFactors, factors: [] });
    const begin = vi.spyOn(api, 'beginFactorEnroll').mockResolvedValue(creationOptions);
    const complete = vi.spyOn(api, 'completeFactorEnroll').mockResolvedValue({
      enrolled: 'webauthn',
      recovery: { codes: ['AAAAA-BBBBB', 'CCCCC-DDDDD'], shown_once: true },
    });
    renderKeys();

    fireEvent.change(await screen.findByLabelText('Nickname (optional)'), {
      target: { value: '  Blue key ' },
    });
    fireEvent.click(screen.getByRole('button', { name: /add security key/i }));

    const heading = await screen.findByRole('heading', { name: 'Save your recovery codes' });
    expect(heading).toHaveFocus();
    expect(screen.getByText('AAAAA-BBBBB')).toBeInTheDocument();
    expect(begin).toHaveBeenCalledWith('webauthn');
    expect(complete).toHaveBeenCalledWith('webauthn', {
      credential: expect.objectContaining({
        rawId: '-_8',
        response: expect.objectContaining({ attestationObject: 'BAU' }),
      }),
      label: 'Blue key',
    });
    expect(screen.getByRole('status')).toHaveTextContent('Security key added.');

    fireEvent.click(screen.getByRole('button', { name: 'I have saved these codes' }));
    expect(screen.queryByText('AAAAA-BBBBB')).not.toBeInTheDocument();
  });

  it('renames a registered key and shows a refused name (OQ-15)', async () => {
    installWebAuthn();
    vi.spyOn(api, 'getMyFactors').mockResolvedValue(myFactors);
    const rename = vi
      .spyOn(api, 'renameMyFactor')
      .mockRejectedValueOnce({
        response: {
          status: 400,
          data: {
            code: 'factor_invalid',
            details: { factor_error: { kind: 'invalid', code: 'invalid_label' } },
          },
        },
      })
      .mockResolvedValueOnce({ id: 'rec-1', label: 'Desk key' });
    renderKeys();

    fireEvent.click(await screen.findByRole('button', { name: 'Rename Office key' }));
    const input = screen.getByLabelText('New name');
    fireEvent.change(input, { target: { value: ' ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The name is not valid');

    fireEvent.change(input, { target: { value: ' Desk key ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(rename).toHaveBeenLastCalledWith('rec-1', 'Desk key'));
    await waitFor(() => expect(screen.queryByLabelText('New name')).not.toBeInTheDocument());
  });

  it('reports a key that is already registered', async () => {
    const browser = installWebAuthn();
    browser.create.mockRejectedValue(domError('InvalidStateError'));
    vi.spyOn(api, 'getMyFactors').mockResolvedValue(myFactors);
    vi.spyOn(api, 'beginFactorEnroll').mockResolvedValue(creationOptions);
    const complete = vi.spyOn(api, 'completeFactorEnroll');
    renderKeys();

    fireEvent.click(await screen.findByRole('button', { name: /add security key/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent('already registered');
    expect(complete).not.toHaveBeenCalled();
  });
});
