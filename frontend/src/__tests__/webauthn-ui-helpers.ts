// Browser WebAuthn stubs for jsdom component tests (no real authenticator).
import { vi } from 'vitest';

export const buf = (...values: number[]) => new Uint8Array(values).buffer;

export const requestOptions = {
  publicKey: {
    challenge: 'AQID',
    timeout: 30000,
    rpId: 'localhost',
    allowCredentials: [{ type: 'public-key', id: '-_8' }],
    userVerification: 'preferred',
  },
};

export const creationOptions = {
  publicKey: {
    rp: { id: 'localhost', name: 'OpenVox WebUI' },
    user: { id: 'dXNlcg', name: 'alice', displayName: 'alice' },
    challenge: 'AQID',
    pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
    timeout: 60000,
  },
};

export function fakeAssertion() {
  return {
    id: '-_8',
    rawId: buf(251, 255),
    type: 'public-key',
    authenticatorAttachment: 'cross-platform',
    response: {
      clientDataJSON: buf(1),
      authenticatorData: buf(2),
      signature: buf(3),
      userHandle: null,
    },
    getClientExtensionResults: () => ({}),
  };
}

export function fakeAttestation() {
  return {
    id: '-_8',
    rawId: buf(251, 255),
    type: 'public-key',
    authenticatorAttachment: 'cross-platform',
    response: {
      clientDataJSON: buf(1),
      attestationObject: buf(4, 5),
      getTransports: () => ['usb'],
    },
    getClientExtensionResults: () => ({ credProps: { rk: true } }),
  };
}

export const domError = (name: string) => Object.assign(new Error(name), { name });

export interface BrowserStub {
  create: ReturnType<typeof vi.fn>;
  get: ReturnType<typeof vi.fn>;
}

/** Installs PublicKeyCredential and navigator.credentials in jsdom. */
export function installWebAuthn({ secure = true, supported = true } = {}): BrowserStub {
  const stub: BrowserStub = { create: vi.fn(), get: vi.fn() };
  Object.defineProperty(window, 'isSecureContext', { value: secure, configurable: true });
  if (supported) {
    vi.stubGlobal('PublicKeyCredential', function PublicKeyCredential() {});
    Object.defineProperty(navigator, 'credentials', { value: stub, configurable: true });
  } else {
    vi.stubGlobal('PublicKeyCredential', undefined);
    Object.defineProperty(navigator, 'credentials', { value: undefined, configurable: true });
  }
  return stub;
}

/** A promise whose settlement the test controls. */
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
