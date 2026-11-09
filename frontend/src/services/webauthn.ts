// Browser side of the WebAuthn ceremonies used by the authentication pipeline
// (built-in `webauthn` factor and plugin factors of kind `webauthn`). The
// server sends options in the WebAuthn Level 3 JSON encoding, with or without
// a `{publicKey: ...}` wrapper. This module never logs options or credentials.

import type { MessageKey } from '../i18n';

export type WebAuthnCeremony = 'create' | 'get';

export type WebAuthnErrorCode =
  | 'unsupported'
  | 'insecure_context'
  | 'invalid_options'
  | 'cancelled'
  | 'timeout'
  | 'aborted'
  | 'already_registered'
  | 'security'
  | 'not_supported'
  | 'no_credential'
  | 'unknown';

export const WEBAUTHN_ERROR_CODES: readonly WebAuthnErrorCode[] = [
  'unsupported',
  'insecure_context',
  'invalid_options',
  'cancelled',
  'timeout',
  'aborted',
  'already_registered',
  'security',
  'not_supported',
  'no_credential',
  'unknown',
];

export class WebAuthnError extends Error {
  readonly code: WebAuthnErrorCode;

  constructor(code: WebAuthnErrorCode) {
    super(`WebAuthn ceremony failed: ${code}`);
    this.name = 'WebAuthnError';
    this.code = code;
  }
}

/** Message catalog key for an error code. */
export function webAuthnErrorMessageKey(code: WebAuthnErrorCode): MessageKey {
  return `webauthn.error.${code}` as MessageKey;
}

type JsonObject = Record<string, unknown>;

/** A credential in the Level 3 JSON encoding, ready to send to the server. */
export type CredentialJSON = JsonObject;

interface PublicKeyCredentialStatics {
  parseCreationOptionsFromJSON?: (json: unknown) => PublicKeyCredentialCreationOptions;
  parseRequestOptionsFromJSON?: (json: unknown) => PublicKeyCredentialRequestOptions;
}

/** What the ceremonies need from the browser; injectable for tests. */
export interface WebAuthnEnvironment {
  PublicKeyCredential?: PublicKeyCredentialStatics;
  credentials?: Pick<CredentialsContainer, 'create' | 'get'>;
  isSecureContext?: boolean;
  now?: () => number;
}

export function defaultEnvironment(): WebAuthnEnvironment {
  const win =
    typeof window !== 'undefined'
      ? (window as unknown as { PublicKeyCredential?: PublicKeyCredentialStatics } & Window)
      : undefined;
  return {
    PublicKeyCredential: win?.PublicKeyCredential,
    credentials: typeof navigator !== 'undefined' ? navigator.credentials : undefined,
    isSecureContext: win?.isSecureContext,
    now: () => Date.now(),
  };
}

export type WebAuthnSupport =
  { supported: true } | { supported: false; reason: 'unsupported' | 'insecure_context' };

export function checkSupport(env: WebAuthnEnvironment = defaultEnvironment()): WebAuthnSupport {
  if (env.isSecureContext === false) {
    return { supported: false, reason: 'insecure_context' };
  }
  const creds = env.credentials;
  if (
    !env.PublicKeyCredential ||
    !creds ||
    typeof creds.create !== 'function' ||
    typeof creds.get !== 'function'
  ) {
    return { supported: false, reason: 'unsupported' };
  }
  return { supported: true };
}

// ------------------------------------------------------------- base64url

/** Decodes base64url (also standard base64, with or without padding). */
export function base64urlToBuffer(value: unknown): ArrayBuffer {
  if (typeof value !== 'string') {
    throw new WebAuthnError('invalid_options');
  }
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/').replace(/=+$/, '');
  if (!/^[A-Za-z0-9+/]*$/.test(normalized) || normalized.length % 4 === 1) {
    throw new WebAuthnError('invalid_options');
  }
  const binary = atob(normalized + '='.repeat((4 - (normalized.length % 4)) % 4));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}

/** Encodes bytes as base64url without padding. */
export function bufferToBase64url(data: ArrayBuffer | ArrayBufferView): string {
  const bytes = ArrayBuffer.isView(data)
    ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
    : new Uint8Array(data);
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

// --------------------------------------------------------------- options

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isBinary(value: unknown): value is ArrayBuffer | ArrayBufferView {
  // Tag check instead of instanceof: buffers may come from another realm.
  return (
    ArrayBuffer.isView(value) || Object.prototype.toString.call(value) === '[object ArrayBuffer]'
  );
}

// JSON `null` would become 0 or "null" through WebIDL conversion; drop it.
function withoutNulls(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.filter((v) => v !== null && v !== undefined).map(withoutNulls);
  }
  if (isObject(value)) {
    const out: JsonObject = {};
    for (const [key, v] of Object.entries(value)) {
      if (v !== null && v !== undefined) {
        out[key] = withoutNulls(v);
      }
    }
    return out;
  }
  return value;
}

/** The `publicKey` member of server options, accepting the bare form too. */
export function unwrapPublicKeyOptions(raw: unknown): JsonObject {
  const inner = isObject(raw) && isObject(raw.publicKey) ? raw.publicKey : raw;
  if (!isObject(inner) || typeof inner.challenge !== 'string') {
    throw new WebAuthnError('invalid_options');
  }
  return withoutNulls(inner) as JsonObject;
}

/** Ceremony timeout announced by the server options, in milliseconds. */
export function optionsTimeoutMs(raw: unknown): number | undefined {
  try {
    const timeout = unwrapPublicKeyOptions(raw).timeout;
    return typeof timeout === 'number' && timeout > 0 ? timeout : undefined;
  } catch {
    return undefined;
  }
}

function decodeDescriptors(list: unknown): PublicKeyCredentialDescriptor[] {
  if (!Array.isArray(list)) {
    throw new WebAuthnError('invalid_options');
  }
  return list.map((item) => {
    if (!isObject(item)) {
      throw new WebAuthnError('invalid_options');
    }
    const descriptor: JsonObject = { ...item, type: item.type ?? 'public-key' };
    descriptor.id = base64urlToBuffer(item.id);
    return descriptor as unknown as PublicKeyCredentialDescriptor;
  });
}

/** Manual Level 3 decoder, used when the browser lacks the JSON helpers. */
export function decodeCreationOptions(json: JsonObject): PublicKeyCredentialCreationOptions {
  const { user, rp, pubKeyCredParams } = json;
  if (!isObject(user) || !isObject(rp) || !Array.isArray(pubKeyCredParams)) {
    throw new WebAuthnError('invalid_options');
  }
  const out: JsonObject = {
    ...json,
    challenge: base64urlToBuffer(json.challenge),
    user: { ...user, id: base64urlToBuffer(user.id) },
  };
  if (json.excludeCredentials !== undefined) {
    out.excludeCredentials = decodeDescriptors(json.excludeCredentials);
  }
  return out as unknown as PublicKeyCredentialCreationOptions;
}

/** Manual Level 3 decoder, used when the browser lacks the JSON helpers. */
export function decodeRequestOptions(json: JsonObject): PublicKeyCredentialRequestOptions {
  const out: JsonObject = { ...json, challenge: base64urlToBuffer(json.challenge) };
  if (json.allowCredentials !== undefined) {
    out.allowCredentials = decodeDescriptors(json.allowCredentials);
  }
  return out as unknown as PublicKeyCredentialRequestOptions;
}

function parseWith<T>(
  helper: ((json: unknown) => T) | undefined,
  statics: PublicKeyCredentialStatics | undefined,
  json: JsonObject,
  manual: (json: JsonObject) => T
): T {
  if (typeof helper === 'function') {
    try {
      return helper.call(statics, json);
    } catch {
      // Some browsers validate stricter than the spec; the manual decoder
      // below accepts what the server sends.
    }
  }
  return manual(json);
}

export function parseCreationOptions(
  raw: unknown,
  env: WebAuthnEnvironment = defaultEnvironment()
): PublicKeyCredentialCreationOptions {
  const json = unwrapPublicKeyOptions(raw);
  const statics = env.PublicKeyCredential;
  return parseWith(statics?.parseCreationOptionsFromJSON, statics, json, decodeCreationOptions);
}

export function parseRequestOptions(
  raw: unknown,
  env: WebAuthnEnvironment = defaultEnvironment()
): PublicKeyCredentialRequestOptions {
  const json = unwrapPublicKeyOptions(raw);
  const statics = env.PublicKeyCredential;
  return parseWith(statics?.parseRequestOptionsFromJSON, statics, json, decodeRequestOptions);
}

// ------------------------------------------------------------ credentials

function encodeBinaryDeep(value: unknown): unknown {
  if (isBinary(value)) {
    return bufferToBase64url(value);
  }
  if (Array.isArray(value)) {
    return value.map(encodeBinaryDeep);
  }
  if (isObject(value)) {
    const out: JsonObject = {};
    for (const [key, v] of Object.entries(value)) {
      if (v !== undefined) {
        out[key] = encodeBinaryDeep(v);
      }
    }
    return out;
  }
  return value;
}

function attempt<T>(fn: () => T): T | undefined {
  try {
    return fn();
  } catch {
    return undefined;
  }
}

type AnyResponse = AuthenticatorResponse &
  Partial<{
    attestationObject: ArrayBuffer;
    getTransports: () => string[];
    getAuthenticatorData: () => ArrayBuffer;
    getPublicKey: () => ArrayBuffer | null;
    getPublicKeyAlgorithm: () => number;
    authenticatorData: ArrayBuffer;
    signature: ArrayBuffer;
    userHandle: ArrayBuffer | null;
  }>;

/** Manual Level 3 serializer, used when the browser lacks `toJSON()`. */
export function serializeCredential(credential: PublicKeyCredential): CredentialJSON {
  const r = credential.response as AnyResponse;
  const response: JsonObject = { clientDataJSON: bufferToBase64url(r.clientDataJSON) };
  if (r.attestationObject) {
    response.attestationObject = bufferToBase64url(r.attestationObject);
    const transports =
      typeof r.getTransports === 'function' ? attempt(() => r.getTransports!()) : [];
    response.transports = Array.isArray(transports) ? transports : [];
    const authData =
      typeof r.getAuthenticatorData === 'function'
        ? attempt(() => r.getAuthenticatorData!())
        : null;
    if (authData) {
      response.authenticatorData = bufferToBase64url(authData);
    }
    const publicKey =
      typeof r.getPublicKey === 'function' ? attempt(() => r.getPublicKey!()) : null;
    if (publicKey) {
      response.publicKey = bufferToBase64url(publicKey);
    }
    const alg =
      typeof r.getPublicKeyAlgorithm === 'function'
        ? attempt(() => r.getPublicKeyAlgorithm!())
        : undefined;
    if (typeof alg === 'number') {
      response.publicKeyAlgorithm = alg;
    }
  } else {
    if (!r.authenticatorData || !r.signature) {
      throw new WebAuthnError('no_credential');
    }
    response.authenticatorData = bufferToBase64url(r.authenticatorData);
    response.signature = bufferToBase64url(r.signature);
    if (r.userHandle) {
      response.userHandle = bufferToBase64url(r.userHandle);
    }
  }
  const extensions =
    typeof credential.getClientExtensionResults === 'function'
      ? attempt(() => credential.getClientExtensionResults())
      : undefined;
  const out: CredentialJSON = {
    id: credential.id,
    rawId: bufferToBase64url(credential.rawId),
    type: credential.type,
    response,
    clientExtensionResults: encodeBinaryDeep(extensions ?? {}),
  };
  if (credential.authenticatorAttachment) {
    out.authenticatorAttachment = credential.authenticatorAttachment;
  }
  return out;
}

/** Level 3 JSON for a credential: native `toJSON()` when it works, else manual. */
export function credentialToJSON(credential: PublicKeyCredential): CredentialJSON {
  const native = (credential as unknown as { toJSON?: () => unknown }).toJSON;
  if (typeof native === 'function') {
    const json = attempt(() => native.call(credential));
    if (isObject(json) && typeof json.id === 'string' && isObject(json.response)) {
      return json;
    }
  }
  return serializeCredential(credential);
}

// ----------------------------------------------------------------- errors

const TIMEOUT_SLACK_MS = 1000;

/** Maps a browser exception to a stable error code. */
export function toWebAuthnError(
  err: unknown,
  ceremony: WebAuthnCeremony,
  timing?: { elapsedMs: number; timeoutMs?: number }
): WebAuthnError {
  if (err instanceof WebAuthnError) {
    return err;
  }
  const name =
    isObject(err) || err instanceof Error ? String((err as { name?: unknown }).name) : '';
  switch (name) {
    case 'NotAllowedError': {
      const timedOut =
        !!timing?.timeoutMs && timing.elapsedMs >= timing.timeoutMs - TIMEOUT_SLACK_MS;
      return new WebAuthnError(timedOut ? 'timeout' : 'cancelled');
    }
    case 'AbortError':
      return new WebAuthnError('aborted');
    case 'InvalidStateError':
      return new WebAuthnError(ceremony === 'create' ? 'already_registered' : 'unknown');
    case 'SecurityError':
      return new WebAuthnError('security');
    case 'NotSupportedError':
    case 'ConstraintError':
      return new WebAuthnError('not_supported');
    case 'TypeError':
      return new WebAuthnError('invalid_options');
    default:
      return new WebAuthnError('unknown');
  }
}

// -------------------------------------------------------------- ceremonies

export interface CeremonyOptions {
  signal?: AbortSignal;
  environment?: WebAuthnEnvironment;
}

async function runCeremony(
  ceremony: WebAuthnCeremony,
  raw: unknown,
  { signal, environment = defaultEnvironment() }: CeremonyOptions
): Promise<CredentialJSON> {
  const support = checkSupport(environment);
  if (!support.supported) {
    throw new WebAuthnError(support.reason);
  }
  const credentials = environment.credentials!;
  const now = environment.now ?? (() => Date.now());
  let result: Credential | null;
  let timeoutMs: number | undefined;
  const started = now();
  try {
    if (ceremony === 'create') {
      const publicKey = parseCreationOptions(raw, environment);
      timeoutMs = publicKey.timeout;
      result = await credentials.create({ publicKey, signal });
    } else {
      const publicKey = parseRequestOptions(raw, environment);
      timeoutMs = publicKey.timeout;
      result = await credentials.get({ publicKey, signal });
    }
  } catch (err) {
    if (signal?.aborted) {
      throw new WebAuthnError('aborted');
    }
    throw toWebAuthnError(err, ceremony, { elapsedMs: now() - started, timeoutMs });
  }
  if (!result || result.type !== 'public-key') {
    throw new WebAuthnError('no_credential');
  }
  return credentialToJSON(result as PublicKeyCredential);
}

/** Runs `navigator.credentials.create` and returns the credential as JSON. */
export function performRegistration(
  options: unknown,
  ceremony: CeremonyOptions = {}
): Promise<CredentialJSON> {
  return runCeremony('create', options, ceremony);
}

/** Runs `navigator.credentials.get` and returns the assertion as JSON. */
export function performAuthentication(
  options: unknown,
  ceremony: CeremonyOptions = {}
): Promise<CredentialJSON> {
  return runCeremony('get', options, ceremony);
}
