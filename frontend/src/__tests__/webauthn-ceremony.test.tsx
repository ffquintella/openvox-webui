// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import WebAuthnCeremony from '../components/auth/WebAuthnCeremony';
import {
  creationOptions,
  deferred,
  domError,
  fakeAssertion,
  fakeAttestation,
  installWebAuthn,
  requestOptions,
} from './webauthn-ui-helpers';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('WebAuthnCeremony (authenticate)', () => {
  it('runs begin, navigator.credentials.get and complete, then announces success', async () => {
    const browser = installWebAuthn();
    browser.get.mockResolvedValue(fakeAssertion());
    const begin = vi.fn().mockResolvedValue(requestOptions);
    const complete = vi.fn().mockResolvedValue(undefined);
    const onSuccess = vi.fn();

    render(
      <WebAuthnCeremony
        mode="authenticate"
        begin={begin}
        complete={complete}
        onSuccess={onSuccess}
      />
    );
    const status = screen.getByRole('status');
    expect(status).toHaveAttribute('aria-live', 'polite');

    fireEvent.click(screen.getByRole('button', { name: /use security key/i }));

    await waitFor(() => expect(onSuccess).toHaveBeenCalledTimes(1));
    expect(begin).toHaveBeenCalledTimes(1);
    const options = browser.get.mock.calls[0][0];
    expect(Array.from(new Uint8Array(options.publicKey.challenge))).toEqual([1, 2, 3]);
    expect(options.signal).toBeInstanceOf(AbortSignal);
    expect(complete).toHaveBeenCalledWith(
      expect.objectContaining({
        id: '-_8',
        rawId: '-_8',
        type: 'public-key',
        response: { clientDataJSON: 'AQ', authenticatorData: 'Ag', signature: 'Aw' },
      })
    );
    expect(status).toHaveTextContent('Security key verified.');
  });

  it('states the timeout while waiting and lets the user cancel with the keyboard', async () => {
    const browser = installWebAuthn();
    browser.get.mockImplementation(
      ({ signal }: { signal: AbortSignal }) =>
        new Promise((_, reject) => {
          signal.addEventListener('abort', () => reject(domError('AbortError')));
        })
    );
    const complete = vi.fn();
    render(
      <WebAuthnCeremony
        mode="authenticate"
        begin={() => Promise.resolve(requestOptions)}
        complete={complete}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /use security key/i }));
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent('expires in 30 seconds')
    );
    const cancel = screen.getByRole('button', { name: 'Cancel' });
    expect(cancel).toHaveFocus();

    fireEvent.click(cancel);
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent('Security key request cancelled.')
    );
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /use security key/i })).toHaveFocus();
    expect(complete).not.toHaveBeenCalled();
  });

  it('shows NotAllowedError as cancelled or timed out and focuses retry', async () => {
    const browser = installWebAuthn();
    browser.get.mockRejectedValue(domError('NotAllowedError'));
    render(
      <WebAuthnCeremony
        mode="authenticate"
        begin={() => Promise.resolve(requestOptions)}
        complete={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /use security key/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The request was cancelled or timed out'
    );
    expect(screen.getByRole('button', { name: /try again/i })).toHaveFocus();
  });

  it('maps SecurityError to an administrator message', async () => {
    const browser = installWebAuthn();
    browser.get.mockRejectedValue(domError('SecurityError'));
    render(
      <WebAuthnCeremony
        mode="authenticate"
        begin={() => Promise.resolve(requestOptions)}
        complete={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /use security key/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Contact your administrator');
  });

  it('shows the server refusal with the attempts left', async () => {
    const browser = installWebAuthn();
    browser.get.mockResolvedValue(fakeAssertion());
    const refused = {
      response: { status: 401, data: { code: 'factor_failed', details: { attempts_left: 2 } } },
    };
    render(
      <WebAuthnCeremony
        mode="authenticate"
        begin={() => Promise.resolve(requestOptions)}
        complete={() => Promise.reject(refused)}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /use security key/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The security key response was not accepted. Try again. Attempts left: 2.'
    );
  });

  it('ignores a slow begin that was cancelled', async () => {
    const browser = installWebAuthn();
    const pending = deferred<unknown>();
    render(
      <WebAuthnCeremony mode="authenticate" begin={() => pending.promise} complete={vi.fn()} />
    );
    fireEvent.click(screen.getByRole('button', { name: /use security key/i }));
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    pending.resolve(requestOptions);
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent('Security key request cancelled.')
    );
    expect(browser.get).not.toHaveBeenCalled();
  });
});

describe('WebAuthnCeremony (register)', () => {
  it('passes decoded creation options and reports an already registered key', async () => {
    const browser = installWebAuthn();
    browser.create.mockRejectedValue(domError('InvalidStateError'));
    render(
      <WebAuthnCeremony
        mode="register"
        begin={() => Promise.resolve(creationOptions)}
        complete={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /add security key/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This security key is already registered to your account.'
    );
    const options = browser.create.mock.calls[0][0];
    expect(Array.from(new Uint8Array(options.publicKey.user.id))).toEqual([117, 115, 101, 114]);
  });

  it('sends the attestation JSON to complete', async () => {
    const browser = installWebAuthn();
    browser.create.mockResolvedValue(fakeAttestation());
    const complete = vi.fn().mockResolvedValue(undefined);
    render(
      <WebAuthnCeremony
        mode="register"
        begin={() => Promise.resolve(creationOptions)}
        complete={complete}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /add security key/i }));
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent('Security key added.')
    );
    expect(complete).toHaveBeenCalledWith(
      expect.objectContaining({
        rawId: '-_8',
        clientExtensionResults: { credProps: { rk: true } },
        response: { clientDataJSON: 'AQ', attestationObject: 'BAU', transports: ['usb'] },
      })
    );
  });
});

describe('WebAuthnCeremony (browser support)', () => {
  it('explains an unsupported browser and offers no button', () => {
    installWebAuthn({ supported: false });
    render(<WebAuthnCeremony mode="authenticate" begin={vi.fn()} complete={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent('This browser cannot use security keys');
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('explains an insecure context', () => {
    installWebAuthn({ secure: false });
    render(<WebAuthnCeremony mode="register" begin={vi.fn()} complete={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent('secure (HTTPS) connection');
  });
});
