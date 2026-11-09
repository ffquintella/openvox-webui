import { describe, expect, it, vi } from 'vitest';
import {
  base64urlToBuffer,
  bufferToBase64url,
  checkSupport,
  credentialToJSON,
  optionsTimeoutMs,
  parseCreationOptions,
  parseRequestOptions,
  performAuthentication,
  performRegistration,
  serializeCredential,
  toWebAuthnError,
  unwrapPublicKeyOptions,
  WEBAUTHN_ERROR_CODES,
  WebAuthnError,
  webAuthnErrorMessageKey,
  type WebAuthnEnvironment,
} from '../services/webauthn';
import { catalogs } from '../i18n';

const bytes = (...values: number[]) => new Uint8Array(values).buffer;
const view = (buf: unknown) => Array.from(new Uint8Array(buf as ArrayBuffer));

// base64url of [1,2,3] = AQID, of [251,255] = -_8, of 'user' bytes = dXNlcg
const creationJSON = {
  publicKey: {
    rp: { id: 'webui.example.com', name: 'OpenVox WebUI' },
    user: { id: 'dXNlcg', name: 'alice', displayName: 'alice' },
    challenge: 'AQID',
    pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
    timeout: 60000,
    excludeCredentials: [{ type: 'public-key', id: '-_8', transports: ['usb'] }],
    authenticatorSelection: { userVerification: 'preferred' },
    attestation: 'none',
    hints: null,
    extensions: { credProps: true },
  },
};

const requestJSON = {
  challenge: 'AQID',
  timeout: 30000,
  rpId: 'webui.example.com',
  allowCredentials: [{ type: 'public-key', id: '-_8' }],
  userVerification: 'preferred',
};

function env(overrides: Partial<WebAuthnEnvironment> = {}): WebAuthnEnvironment {
  return {
    PublicKeyCredential: {},
    credentials: { create: vi.fn(), get: vi.fn() },
    isSecureContext: true,
    now: () => 0,
    ...overrides,
  };
}

function assertion(extra: Record<string, unknown> = {}) {
  return {
    id: '-_8',
    rawId: bytes(251, 255),
    type: 'public-key',
    authenticatorAttachment: 'cross-platform',
    response: {
      clientDataJSON: bytes(1),
      authenticatorData: bytes(2),
      signature: bytes(3),
      userHandle: null,
    },
    getClientExtensionResults: () => ({}),
    ...extra,
  } as unknown as PublicKeyCredential;
}

function attestation(extra: Record<string, unknown> = {}) {
  return {
    id: '-_8',
    rawId: bytes(251, 255),
    type: 'public-key',
    authenticatorAttachment: null,
    response: {
      clientDataJSON: bytes(1),
      attestationObject: bytes(4, 5),
      getTransports: () => ['usb', 'nfc'],
      getAuthenticatorData: () => bytes(6),
      getPublicKey: () => null,
      getPublicKeyAlgorithm: () => -7,
    },
    getClientExtensionResults: () => ({
      credProps: { rk: false },
      prf: { results: { first: bytes(9) } },
    }),
    ...extra,
  } as unknown as PublicKeyCredential;
}

describe('base64url', () => {
  it('round-trips bytes without padding', () => {
    expect(bufferToBase64url(bytes(1, 2, 3))).toBe('AQID');
    expect(bufferToBase64url(bytes(251, 255))).toBe('-_8');
    expect(bufferToBase64url(new Uint8Array([0, 251, 255, 0]).subarray(1, 3))).toBe('-_8');
    expect(bufferToBase64url(new ArrayBuffer(0))).toBe('');
    expect(view(base64urlToBuffer('-_8'))).toEqual([251, 255]);
    const big = new Uint8Array(70000).map((_, i) => i % 256);
    expect(view(base64urlToBuffer(bufferToBase64url(big)))).toEqual(Array.from(big));
  });

  it('accepts standard base64 and padding', () => {
    expect(view(base64urlToBuffer('+/8='))).toEqual([251, 255]);
    expect(view(base64urlToBuffer('AQID'))).toEqual([1, 2, 3]);
  });

  it('rejects malformed input as invalid options', () => {
    for (const bad of ['a', 'AQ!D', 42, null]) {
      expect(() => base64urlToBuffer(bad)).toThrowError(WebAuthnError);
    }
  });
});

describe('option parsing', () => {
  it('accepts the {publicKey} wrapper and the bare form, dropping nulls', () => {
    expect(unwrapPublicKeyOptions(creationJSON)).not.toHaveProperty('hints');
    expect(unwrapPublicKeyOptions(requestJSON).rpId).toBe('webui.example.com');
    expect(() => unwrapPublicKeyOptions({ publicKey: {} })).toThrowError(WebAuthnError);
    expect(() => unwrapPublicKeyOptions('nope')).toThrowError(WebAuthnError);
    expect(optionsTimeoutMs(creationJSON)).toBe(60000);
    expect(optionsTimeoutMs({ challenge: 'AQID' })).toBeUndefined();
  });

  it('decodes creation options manually without the Level 3 helper', () => {
    const options = parseCreationOptions(creationJSON, env());
    expect(view(options.challenge)).toEqual([1, 2, 3]);
    expect(view(options.user.id)).toEqual(Array.from(new TextEncoder().encode('user')));
    expect(view(options.excludeCredentials![0].id)).toEqual([251, 255]);
    expect(options.excludeCredentials![0].transports).toEqual(['usb']);
    expect(options.rp.id).toBe('webui.example.com');
    expect(options.extensions).toEqual({ credProps: true });
    expect(options).not.toHaveProperty('hints');
  });

  it('decodes request options manually without the Level 3 helper', () => {
    const options = parseRequestOptions({ publicKey: requestJSON }, env());
    expect(view(options.challenge)).toEqual([1, 2, 3]);
    expect(view(options.allowCredentials![0].id)).toEqual([251, 255]);
    expect(options.userVerification).toBe('preferred');
  });

  it('uses the Level 3 helpers with the inner publicKey when available', () => {
    const parsedCreate = { challenge: new ArrayBuffer(1) };
    const parsedGet = { challenge: new ArrayBuffer(2) };
    const statics = {
      parseCreationOptionsFromJSON: vi.fn(() => parsedCreate),
      parseRequestOptionsFromJSON: vi.fn(() => parsedGet),
    };
    const e = env({ PublicKeyCredential: statics as never });
    expect(parseCreationOptions(creationJSON, e)).toBe(parsedCreate);
    expect(statics.parseCreationOptionsFromJSON).toHaveBeenCalledWith(
      expect.objectContaining({ challenge: 'AQID', user: creationJSON.publicKey.user })
    );
    expect(parseRequestOptions({ publicKey: requestJSON }, e)).toBe(parsedGet);
    expect(statics.parseRequestOptionsFromJSON).toHaveBeenCalledWith(requestJSON);
  });

  it('falls back to the manual decoder when a helper throws', () => {
    const statics = {
      parseRequestOptionsFromJSON: vi.fn(() => {
        throw new TypeError('strict');
      }),
    };
    const options = parseRequestOptions(
      requestJSON,
      env({ PublicKeyCredential: statics as never })
    );
    expect(view(options.challenge)).toEqual([1, 2, 3]);
  });

  it('rejects creation options without a user', () => {
    const { user: _user, ...rest } = creationJSON.publicKey;
    expect(() => parseCreationOptions(rest, env())).toThrowError(WebAuthnError);
  });
});

describe('credential serialization', () => {
  it('serializes an assertion manually and omits a null userHandle', () => {
    expect(serializeCredential(assertion())).toEqual({
      id: '-_8',
      rawId: '-_8',
      type: 'public-key',
      authenticatorAttachment: 'cross-platform',
      clientExtensionResults: {},
      response: { clientDataJSON: 'AQ', authenticatorData: 'Ag', signature: 'Aw' },
    });
    const withHandle = assertion({
      response: {
        clientDataJSON: bytes(1),
        authenticatorData: bytes(2),
        signature: bytes(3),
        userHandle: bytes(1, 2, 3),
      },
    });
    expect((serializeCredential(withHandle).response as Record<string, unknown>).userHandle).toBe(
      'AQID'
    );
  });

  it('serializes an attestation manually with transports and extension results', () => {
    const json = serializeCredential(attestation());
    expect(json).toEqual({
      id: '-_8',
      rawId: '-_8',
      type: 'public-key',
      clientExtensionResults: { credProps: { rk: false }, prf: { results: { first: 'CQ' } } },
      response: {
        clientDataJSON: 'AQ',
        attestationObject: 'BAU',
        transports: ['usb', 'nfc'],
        authenticatorData: 'Bg',
        publicKeyAlgorithm: -7,
      },
    });
  });

  it('tolerates missing Level 2 response helpers', () => {
    const minimal = attestation({
      response: { clientDataJSON: bytes(1), attestationObject: bytes(4) },
      getClientExtensionResults: undefined,
    });
    const json = serializeCredential(minimal);
    expect(json.response).toEqual({
      clientDataJSON: 'AQ',
      attestationObject: 'BA',
      transports: [],
    });
    expect(json.clientExtensionResults).toEqual({});
  });

  it('prefers the native toJSON and falls back when it is unusable', () => {
    const native = { id: 'x', rawId: 'x', type: 'public-key', response: { clientDataJSON: 'AQ' } };
    expect(credentialToJSON(assertion({ toJSON: () => native }))).toBe(native);
    const broken = assertion({
      toJSON: () => {
        throw new Error('boom');
      },
    });
    expect(credentialToJSON(broken).rawId).toBe('-_8');
    expect(credentialToJSON(assertion({ toJSON: () => ({}) })).rawId).toBe('-_8');
  });
});

describe('error mapping', () => {
  const dom = (name: string) => Object.assign(new Error(name), { name });

  it('maps browser exceptions to stable codes', () => {
    expect(toWebAuthnError(dom('NotAllowedError'), 'get').code).toBe('cancelled');
    expect(toWebAuthnError(dom('AbortError'), 'get').code).toBe('aborted');
    expect(toWebAuthnError(dom('InvalidStateError'), 'create').code).toBe('already_registered');
    expect(toWebAuthnError(dom('InvalidStateError'), 'get').code).toBe('unknown');
    expect(toWebAuthnError(dom('SecurityError'), 'create').code).toBe('security');
    expect(toWebAuthnError(dom('NotSupportedError'), 'create').code).toBe('not_supported');
    expect(toWebAuthnError(dom('ConstraintError'), 'create').code).toBe('not_supported');
    expect(toWebAuthnError(new TypeError('bad'), 'get').code).toBe('invalid_options');
    expect(toWebAuthnError('weird', 'get').code).toBe('unknown');
  });

  it('reports a NotAllowedError after the timeout as a timeout', () => {
    expect(
      toWebAuthnError(dom('NotAllowedError'), 'get', { elapsedMs: 60000, timeoutMs: 60000 }).code
    ).toBe('timeout');
    expect(
      toWebAuthnError(dom('NotAllowedError'), 'get', { elapsedMs: 5000, timeoutMs: 60000 }).code
    ).toBe('cancelled');
  });

  it('has an English message for every error code', () => {
    for (const code of WEBAUTHN_ERROR_CODES) {
      expect(catalogs.en[webAuthnErrorMessageKey(code)]).toBeTruthy();
    }
  });
});

describe('support detection', () => {
  it('detects insecure contexts and missing APIs', () => {
    expect(checkSupport(env())).toEqual({ supported: true });
    expect(checkSupport(env({ isSecureContext: false }))).toEqual({
      supported: false,
      reason: 'insecure_context',
    });
    expect(checkSupport(env({ PublicKeyCredential: undefined }))).toEqual({
      supported: false,
      reason: 'unsupported',
    });
    expect(checkSupport(env({ credentials: undefined })).supported).toBe(false);
  });
});

describe('ceremonies', () => {
  it('runs navigator.credentials.get with decoded options and the signal', async () => {
    const e = env();
    vi.mocked(e.credentials!.get).mockResolvedValue(assertion());
    const controller = new AbortController();
    const json = await performAuthentication(
      { publicKey: requestJSON },
      { signal: controller.signal, environment: e }
    );
    const arg = vi.mocked(e.credentials!.get).mock.calls[0][0]!;
    expect(arg.signal).toBe(controller.signal);
    expect(view(arg.publicKey!.challenge)).toEqual([1, 2, 3]);
    expect(json.rawId).toBe('-_8');
  });

  it('runs navigator.credentials.create and serializes the attestation', async () => {
    const e = env();
    vi.mocked(e.credentials!.create).mockResolvedValue(attestation());
    const json = await performRegistration(creationJSON, { environment: e });
    expect((json.response as Record<string, unknown>).attestationObject).toBe('BAU');
  });

  it('turns failures into WebAuthnError codes', async () => {
    let clock = 0;
    const e = env({ now: () => clock });
    vi.mocked(e.credentials!.get).mockImplementation(async () => {
      clock = 30000;
      throw Object.assign(new Error('x'), { name: 'NotAllowedError' });
    });
    await expect(performAuthentication(requestJSON, { environment: e })).rejects.toMatchObject({
      code: 'timeout',
    });

    vi.mocked(e.credentials!.get).mockResolvedValue(null);
    await expect(performAuthentication(requestJSON, { environment: e })).rejects.toMatchObject({
      code: 'no_credential',
    });

    await expect(
      performAuthentication(requestJSON, { environment: env({ isSecureContext: false }) })
    ).rejects.toMatchObject({ code: 'insecure_context' });

    await expect(performAuthentication({ nope: 1 }, { environment: e })).rejects.toMatchObject({
      code: 'invalid_options',
    });
  });

  it('reports an aborted signal as aborted whatever the browser threw', async () => {
    const e = env();
    const controller = new AbortController();
    vi.mocked(e.credentials!.create).mockImplementation(async () => {
      controller.abort();
      throw new Error('anything');
    });
    await expect(
      performRegistration(creationJSON, { signal: controller.signal, environment: e })
    ).rejects.toMatchObject({ code: 'aborted' });
  });
});
