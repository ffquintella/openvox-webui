import { t, type MessageKey } from '../../i18n';
import type { AuthFactorDescriptor } from '../../services/api';

interface ApiErrorShape {
  response?: {
    status?: number;
    data?: { code?: string; details?: { attempts_left?: unknown } };
  };
}

/**
 * Translated message for an authentication API error (challenge, step-up,
 * enrolment). `rejectedKey` is used when the factor response was refused.
 */
export function authErrorMessage(err: unknown, rejectedKey: MessageKey): string {
  const response = (err as ApiErrorShape)?.response;
  const code = response?.data?.code;
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
    default:
      break;
  }
  if (response?.status === 400) {
    return t(rejectedKey);
  }
  return t('auth.error.generic');
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
