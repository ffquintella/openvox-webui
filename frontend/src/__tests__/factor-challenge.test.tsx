// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import FactorChallenge from '../components/auth/FactorChallenge';
import StepUpDialog from '../components/auth/StepUpDialog';
import {
  api,
  requestStepUp,
  type AuthFactorDescriptor,
  type SessionResponse,
} from '../services/api';
import { useAuthStore } from '../stores/authStore';
import { domError, fakeAssertion, installWebAuthn, requestOptions } from './webauthn-ui-helpers';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const pluginKey: AuthFactorDescriptor = {
  id: 'mfa-fido2:security-key',
  kind: 'webauthn',
  label: 'Security key',
  fields: ['credential'],
};
const totp: AuthFactorDescriptor = {
  id: 'totp',
  kind: 'otp',
  label: 'Authenticator app',
  fields: ['code'],
};

const session: SessionResponse = {
  access_token: 'new-access',
  refresh_token: 'new-refresh',
  token_type: 'Bearer',
  expires_in: 3600,
  user: { id: 'u1', username: 'alice', email: 'alice@example.com', role: 'admin' },
};

describe('FactorChallenge', () => {
  it('runs a plugin WebAuthn factor and sends {credential} to verify', async () => {
    const browser = installWebAuthn();
    browser.get.mockResolvedValue(fakeAssertion());
    const begin = vi.spyOn(api, 'beginChallenge').mockResolvedValue(requestOptions);
    const verify = vi.spyOn(api, 'verifyChallengeResponse').mockResolvedValue(session);
    const onSession = vi.fn();

    render(
      <FactorChallenge
        challengeId="ch-1"
        factors={[pluginKey]}
        onSession={onSession}
        onCancel={vi.fn()}
        cancelLabel="Back to sign in"
      />
    );
    const action = screen.getByRole('button', { name: /use security key/i });
    expect(action).toHaveFocus();
    fireEvent.click(action);

    await waitFor(() => expect(onSession).toHaveBeenCalledWith(session));
    expect(begin).toHaveBeenCalledWith('ch-1', 'mfa-fido2:security-key');
    expect(verify).toHaveBeenCalledWith('ch-1', 'mfa-fido2:security-key', {
      credential: expect.objectContaining({ id: '-_8', rawId: '-_8', type: 'public-key' }),
    });
  });

  it('begins the code factor again after a WebAuthn attempt made it active', async () => {
    const browser = installWebAuthn();
    browser.get.mockRejectedValue(domError('NotAllowedError'));
    const begin = vi.spyOn(api, 'beginChallenge').mockResolvedValue(requestOptions);
    const verify = vi.spyOn(api, 'verifyChallengeResponse').mockResolvedValue(session);
    const onSession = vi.fn();

    render(
      <FactorChallenge
        challengeId="ch-2"
        factors={[pluginKey, totp]}
        onSession={onSession}
        onCancel={vi.fn()}
        cancelLabel="Back to sign in"
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /use security key/i }));
    await screen.findByRole('alert');

    fireEvent.change(screen.getByLabelText('Verification method'), { target: { value: 'totp' } });
    fireEvent.change(screen.getByLabelText('Verification code'), { target: { value: ' 123456 ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Verify' }));

    await waitFor(() => expect(onSession).toHaveBeenCalledWith(session));
    expect(begin).toHaveBeenLastCalledWith('ch-2', 'totp');
    expect(verify).toHaveBeenCalledWith('ch-2', 'totp', { code: '123456' });
  });

  it('shows a refused code with the attempts left', async () => {
    vi.spyOn(api, 'verifyChallengeResponse').mockRejectedValue({
      response: { status: 401, data: { code: 'factor_failed', details: { attempts_left: 4 } } },
    });
    render(
      <FactorChallenge
        challengeId="ch-3"
        factors={[totp]}
        onSession={vi.fn()}
        onCancel={vi.fn()}
        cancelLabel="Back to sign in"
      />
    );
    fireEvent.change(screen.getByLabelText('Verification code'), { target: { value: '000000' } });
    fireEvent.click(screen.getByRole('button', { name: 'Verify' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The code was not accepted. Check it and try again. Attempts left: 4.'
    );
  });
});

describe('StepUpDialog', () => {
  it('opens on request, verifies a security key and stores the new session', async () => {
    const browser = installWebAuthn();
    browser.get.mockResolvedValue(fakeAssertion());
    vi.spyOn(api, 'stepUp').mockResolvedValue({
      status: 'mfa_required',
      challenge_id: 'su-1',
      expires_in: 300,
      required_acr: 2,
      factors: [
        {
          id: 'webauthn',
          kind: 'webauthn',
          label: 'Security key or passkey',
          fields: ['credential'],
        },
      ],
    });
    vi.spyOn(api, 'beginChallenge').mockResolvedValue(requestOptions);
    const verify = vi.spyOn(api, 'verifyChallengeResponse').mockResolvedValue(session);
    const login = vi.fn();
    useAuthStore.setState({
      user: {
        id: 'u1',
        username: 'alice',
        email: 'alice@example.com',
        role: 'admin',
        auth_provider: 'local',
      },
      login,
    });

    render(<StepUpDialog />);
    let result: Promise<boolean> = Promise.resolve(false);
    act(() => {
      result = requestStepUp();
    });

    const dialog = await screen.findByRole('dialog', { name: "Confirm it's you" });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    fireEvent.click(await screen.findByRole('button', { name: /use security key/i }));

    await expect(result).resolves.toBe(true);
    expect(verify).toHaveBeenCalledWith('su-1', 'webauthn', { credential: expect.any(Object) });
    expect(login).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'u1', auth_provider: 'local' }),
      'new-access',
      'new-refresh'
    );
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('resolves false when cancelled with Escape', async () => {
    vi.spyOn(api, 'stepUp').mockReturnValue(new Promise(() => undefined));
    render(<StepUpDialog />);
    let result: Promise<boolean> = Promise.resolve(true);
    act(() => {
      result = requestStepUp();
    });
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveFocus();
    fireEvent.keyDown(dialog, { key: 'Escape' });
    await expect(result).resolves.toBe(false);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('shows why the step-up could not start', async () => {
    vi.spyOn(api, 'stepUp').mockRejectedValue({
      response: { status: 503, data: { code: 'factor_unavailable' } },
    });
    render(<StepUpDialog />);
    act(() => {
      void requestStepUp();
    });
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This verification method is unavailable'
    );
  });
});
