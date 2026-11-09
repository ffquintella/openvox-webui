import { t, type MessageKey } from '../../i18n';
import en from '../../i18n/locales/en.json';
import type { AuthFactorDescriptor } from '../../services/api';

interface ApiErrorShape {
  response?: {
    status?: number;
    data?: {
      code?: string;
      message?: unknown;
      details?: { attempts_left?: unknown; factor_error?: { kind?: unknown; code?: unknown } };
    };
  };
}

/**
 * The factor's own stable code (H-8/OQ-10), from `details.factor_error.code`
 * of a `factor_unavailable` or `factor_invalid` answer.
 */
export function factorErrorCode(err: unknown): string | undefined {
  const code = (err as ApiErrorShape)?.response?.data?.details?.factor_error?.code;
  return typeof code === 'string' && /^[a-z0-9_.-]{1,64}$/.test(code) ? code : undefined;
}

/** Host message for a factor code, when the host catalog has one. */
function hostFactorMessage(code: string | undefined): string | undefined {
  if (!code) return undefined;
  const key = `auth.factor_error.${code}`;
  return key in en ? t(key as MessageKey) : undefined;
}

/**
 * Translated message for an authentication API error (challenge, step-up,
 * enrolment). `rejectedKey` is used when the factor response was refused.
 */
export function authErrorMessage(err: unknown, rejectedKey: MessageKey): string {
  const response = (err as ApiErrorShape)?.response;
  const code = response?.data?.code;
  // Plugin message catalogs (`<plugin>.*` keys) are not served yet; host
  // codes have host messages, other codes fall back to the category.
  const specific = hostFactorMessage(factorErrorCode(err));
  if (specific) return specific;
  switch (code) {
    case 'factor_failed': {
      const left = response?.data?.details?.attempts_left;
      const base = t(rejectedKey);
      return typeof left === 'number'
        ? `${base} ${t('auth.error.attempts_left', { count: left })}`
        : base;
    }
    case 'invalid_challenge':
      return t('auth.error.invalid_challenge');
    case 'locked':
      return t('auth.error.locked');
    case 'factor_unavailable':
      return t('auth.error.factor_unavailable');
    case 'enrollment_only_session':
      return t('auth.error.enrollment_only');
    case 'phishing_resistant_factor_required':
      return t('auth.error.phishing_resistant_required');
    default:
      break;
  }
  if (response?.status === 400) {
    return t(rejectedKey);
  }
  return t('auth.error.generic');
}

/**
 * Message for a refused password sign-in: host text for the policy and
 * availability refusals (fail closed), else the server's own message, and
 * never the transport error ("Request failed with status code 403").
 */
export function loginErrorMessage(err: unknown): string {
  const response = (err as ApiErrorShape)?.response;
  switch (response?.data?.code) {
    case 'mfa_enrollment_required':
      return t('auth.error.mfa_enrollment_required');
    case 'factor_unavailable':
      return t('auth.error.no_factor_available');
    case 'phishing_resistant_factor_required':
      return t('auth.error.phishing_resistant_required');
    case 'locked':
      return t('auth.error.locked');
    default:
      break;
  }
  const message = response?.data?.message;
  if (typeof message === 'string' && message.trim()) {
    return message;
  }
  if (response) {
    return t('auth.error.generic');
  }
  return (err as Error)?.message || t('auth.error.generic');
}

export function isWebAuthnFactor(factor: Pick<AuthFactorDescriptor, 'kind'>): boolean {
  return factor.kind === 'webauthn';
}

export function isCodeFactor(factor: Pick<AuthFactorDescriptor, 'fields'>): boolean {
  return Array.isArray(factor.fields) && factor.fields.includes('code');
}

/** Descriptor guessed from a bare factor id (SSO redirect carries ids only). */
export function descriptorFromId(id: string): AuthFactorDescriptor {
  const webauthn = id === 'webauthn';
  return {
    id,
    kind: webauthn ? 'webauthn' : id,
    label: id,
    fields: webauthn ? ['credential'] : ['code'],
  };
}
