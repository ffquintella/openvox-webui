import { useEffect, useId, useRef, useState } from 'react';
import { CircleAlert, KeyRound, LoaderCircle } from 'lucide-react';
import { t, type MessageKey } from '../../i18n';
import {
  checkSupport,
  defaultEnvironment,
  optionsTimeoutMs,
  performAuthentication,
  performRegistration,
  WebAuthnError,
  webAuthnErrorMessageKey,
  type CredentialJSON,
  type WebAuthnEnvironment,
} from '../../services/webauthn';
import { authErrorMessage } from './authErrors';

export interface WebAuthnCeremonyProps {
  /** `authenticate` runs `navigator.credentials.get`, `register` runs `create`. */
  mode: 'authenticate' | 'register';
  /** Asks the server for the options (challenge begin or enrolment begin). */
  begin: () => Promise<unknown>;
  /** Sends the credential JSON to the server; throws when it is refused. */
  complete: (credential: CredentialJSON) => Promise<void>;
  onSuccess?: () => void;
  /** Message shown when the server refuses the response. */
  rejectedKey?: MessageKey;
  autoFocus?: boolean;
  disabled?: boolean;
  /** Browser APIs; defaults to the real ones. */
  environment?: WebAuthnEnvironment;
}

type Phase = 'idle' | 'preparing' | 'waiting' | 'verifying';

/** One WebAuthn ceremony with its status, errors, retry and cancel. */
export default function WebAuthnCeremony({
  mode,
  begin,
  complete,
  onSuccess,
  rejectedKey = 'auth.error.key_rejected',
  autoFocus = false,
  disabled = false,
  environment,
}: WebAuthnCeremonyProps) {
  const [phase, setPhase] = useState<Phase>('idle');
  const [status, setStatus] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [focusTarget, setFocusTarget] = useState<{ el: 'action' | 'cancel'; n: number } | null>(
    autoFocus ? { el: 'action', n: 0 } : null
  );
  const controllerRef = useRef<AbortController | null>(null);
  const mountedRef = useRef(true);
  const actionRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const instructionsId = useId();

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      controllerRef.current?.abort();
    };
  }, []);

  useEffect(() => {
    if (!focusTarget) {
      return;
    }
    (focusTarget.el === 'cancel' ? cancelRef : actionRef).current?.focus();
  }, [focusTarget]);

  const moveFocus = (el: 'action' | 'cancel') =>
    setFocusTarget((prev) => ({ el, n: (prev?.n ?? 0) + 1 }));

  const fail = (message: string) => {
    controllerRef.current = null;
    setPhase('idle');
    setStatus('');
    setError(message);
    moveFocus('action');
  };

  const start = async () => {
    if (phase !== 'idle' || disabled) {
      return;
    }
    const controller = new AbortController();
    controllerRef.current = controller;
    const alive = () => mountedRef.current && !controller.signal.aborted;
    setError(null);
    setPhase('preparing');
    setStatus(t('webauthn.status.preparing'));
    moveFocus('cancel');

    let options: unknown;
    try {
      options = await begin();
    } catch (err) {
      if (alive()) {
        fail(authErrorMessage(err, 'auth.error.generic'));
      }
      return;
    }
    if (!alive()) {
      return;
    }

    const timeoutMs = optionsTimeoutMs(options);
    setPhase('waiting');
    setStatus(
      timeoutMs
        ? t('webauthn.status.waiting_timeout', { seconds: Math.round(timeoutMs / 1000) })
        : t('webauthn.status.waiting')
    );

    let credential: CredentialJSON;
    try {
      const ceremony = { signal: controller.signal, environment };
      credential =
        mode === 'register'
          ? await performRegistration(options, ceremony)
          : await performAuthentication(options, ceremony);
    } catch (err) {
      if (!mountedRef.current) {
        return;
      }
      const code = err instanceof WebAuthnError ? err.code : 'unknown';
      if (code === 'aborted') {
        controllerRef.current = null;
        setPhase('idle');
        setStatus(t('webauthn.error.aborted'));
        moveFocus('action');
        return;
      }
      fail(t(webAuthnErrorMessageKey(code)));
      return;
    }
    if (!alive()) {
      return;
    }

    setPhase('verifying');
    setStatus(t('webauthn.status.verifying'));
    try {
      await complete(credential);
    } catch (err) {
      if (mountedRef.current) {
        fail(authErrorMessage(err, rejectedKey));
      }
      return;
    }
    if (!mountedRef.current) {
      return;
    }
    controllerRef.current = null;
    setPhase('idle');
    setStatus(t(mode === 'register' ? 'webauthn.status.registered' : 'webauthn.status.verified'));
    onSuccess?.();
  };

  const cancel = () => {
    const wasPreparing = phase === 'preparing';
    controllerRef.current?.abort();
    if (wasPreparing) {
      // The server request cannot be aborted; its result is ignored.
      controllerRef.current = null;
      setPhase('idle');
      setStatus(t('webauthn.error.aborted'));
      moveFocus('action');
    }
  };

  const support = checkSupport(environment ?? defaultEnvironment());
  if (!support.supported) {
    return (
      <div
        role="alert"
        className="p-3 bg-amber-50 border border-amber-200 rounded-lg flex items-start gap-2 dark:bg-amber-900/20 dark:border-amber-800"
      >
        <CircleAlert aria-hidden="true" className="w-5 h-5 text-amber-600 flex-shrink-0 mt-0.5" />
        <p className="text-sm text-amber-800 dark:text-amber-200">
          {t(webAuthnErrorMessageKey(support.reason))}
        </p>
      </div>
    );
  }

  const busy = phase !== 'idle';
  const actionText = error
    ? t('webauthn.action.retry')
    : t(mode === 'register' ? 'webauthn.action.register' : 'webauthn.action.authenticate');

  return (
    <div className="space-y-3">
      <p id={instructionsId} className="text-sm text-gray-600 dark:text-gray-400">
        {t(
          mode === 'register'
            ? 'webauthn.instructions.register'
            : 'webauthn.instructions.authenticate'
        )}
      </p>

      {error && (
        <div
          role="alert"
          className="p-3 bg-danger-50 border border-danger-200 rounded-lg flex items-start gap-2"
        >
          <CircleAlert
            aria-hidden="true"
            className="w-5 h-5 text-danger-500 flex-shrink-0 mt-0.5"
          />
          <p className="text-sm text-danger-700">{error}</p>
        </div>
      )}

      <p
        role="status"
        aria-live="polite"
        aria-atomic="true"
        className="text-sm text-gray-700 dark:text-gray-300 min-h-5 flex items-center gap-2"
      >
        {busy && <LoaderCircle aria-hidden="true" className="w-4 h-4 animate-spin" />}
        <span>{status}</span>
      </p>

      <div className="flex flex-wrap gap-2">
        {!busy && (
          <button
            ref={actionRef}
            type="button"
            onClick={() => void start()}
            disabled={disabled}
            aria-describedby={instructionsId}
            className="btn btn-primary inline-flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <KeyRound aria-hidden="true" className="w-4 h-4" />
            {actionText}
          </button>
        )}
        {(phase === 'preparing' || phase === 'waiting') && (
          <button ref={cancelRef} type="button" onClick={cancel} className="btn btn-secondary">
            {t('webauthn.action.cancel')}
          </button>
        )}
      </div>
    </div>
  );
}
