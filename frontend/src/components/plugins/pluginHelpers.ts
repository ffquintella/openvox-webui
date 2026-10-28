import type { PluginSignatureStatus } from '../../types';

export function getApiErrorMessage(error: unknown, fallback = 'Request failed'): string {
  const e = error as {
    response?: { data?: { message?: unknown; error?: unknown } };
    message?: string;
  };
  const data = e?.response?.data;
  if (typeof data?.message === 'string' && data.message) return data.message;
  if (typeof data?.error === 'string' && data.error) return data.error;
  return e?.message || fallback;
}

export function getApiErrorCode(error: unknown): string | undefined {
  const code = (error as { response?: { data?: { code?: unknown } } })?.response?.data?.code;
  return typeof code === 'string' ? code : undefined;
}

const SIGNATURE_LABELS: Record<PluginSignatureStatus, string> = {
  valid: 'Valid signature from a trusted key',
  untrusted_key: 'Valid signature, but the signing key is not trusted yet',
  invalid: 'Invalid signature: the package cannot be installed',
  key_revoked: 'The signing key has been revoked',
  key_expired: 'The signing key has expired',
  key_scope: 'The signing key is not allowed to sign this plugin',
};

export function signatureLabel(status: PluginSignatureStatus): string {
  return SIGNATURE_LABELS[status];
}

export const inputClass =
  'w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-sm text-gray-900 dark:text-white';
export const primaryBtn =
  'px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed text-sm';
export const secondaryBtn =
  'px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-700';
export const dangerBtn =
  'px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:opacity-50 disabled:cursor-not-allowed text-sm';

/** Render any JSON value as plain text. */
export function toText(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  try {
    return JSON.stringify(v);
  } catch {
    return '';
  }
}

export function isSafeHttpUrl(url: string): boolean {
  try {
    const u = new URL(url);
    return u.protocol === 'http:' || u.protocol === 'https:';
  } catch {
    return false;
  }
}

/** Number of fingerprint characters the admin must retype to trust a key. */
export const TRUST_CONFIRM_LENGTH = 8;

export function expectedTrustConfirm(keyId: string): string {
  return keyId.slice(0, TRUST_CONFIRM_LENGTH).toLowerCase();
}

/**
 * Pure gate for the "trust this key and install" action: the typed text must
 * equal the first 8 characters of the signer fingerprint (hex, so compared
 * case-insensitively) and a label and password must be present.
 */
export function canConfirmUntrustedInstall(args: {
  keyId: string;
  typedConfirm: string;
  label: string;
  password: string;
}): boolean {
  return (
    args.keyId.length >= TRUST_CONFIRM_LENGTH &&
    args.typedConfirm.toLowerCase() === expectedTrustConfirm(args.keyId) &&
    args.label.trim().length > 0 &&
    args.password.length > 0
  );
}

export function parsePluginIdList(text: string): string[] {
  return text
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
}
