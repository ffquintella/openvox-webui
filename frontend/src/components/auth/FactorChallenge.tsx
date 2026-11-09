import { useId, useMemo, useRef, useState } from 'react';
import { CircleAlert } from 'lucide-react';
import { t } from '../../i18n';
import { api, type AuthFactorDescriptor, type SessionResponse } from '../../services/api';
import type { WebAuthnEnvironment } from '../../services/webauthn';
import WebAuthnCeremony from './WebAuthnCeremony';
import { authErrorMessage, isCodeFactor, isWebAuthnFactor } from './authErrors';

export interface FactorChallengeProps {
  challengeId: string;
  factors: AuthFactorDescriptor[];
  /** Called with the new session once a factor verifies. */
  onSession: (session: SessionResponse) => void | Promise<void>;
  onCancel: () => void;
  cancelLabel: string;
  environment?: WebAuthnEnvironment;
}

function requireSession(session: SessionResponse | undefined): SessionResponse {
  if (!session?.access_token) {
    throw new Error('The server did not return a session');
  }
  return session;
}

/** Second factor step shared by the login screen and the step-up dialog. */
export default function FactorChallenge({
  challengeId,
  factors,
  onSession,
  onCancel,
  cancelLabel,
  environment,
}: FactorChallengeProps) {
  const usable = useMemo(
    () => factors.filter((f) => isWebAuthnFactor(f) || isCodeFactor(f)),
    [factors]
  );
  const [selectedId, setSelectedId] = useState(() => usable[0]?.id ?? '');
  const [code, setCode] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  // Factor last started on the server: verifying another one needs a new begin.
  const begunRef = useRef<string | null>(null);
  const sessionRef = useRef<SessionResponse | null>(null);
  const selectId = useId();
  const codeId = useId();

  const factor = usable.find((f) => f.id === selectedId) ?? usable[0];

  const finish = async (session: SessionResponse) => {
    try {
      await onSession(session);
    } catch {
      setError(t('auth.error.generic'));
    }
  };

  const submitCode = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!factor || !code.trim() || submitting) {
      return;
    }
    setSubmitting(true);
    setError(null);
    let session: SessionResponse;
    try {
      if (begunRef.current !== null && begunRef.current !== factor.id) {
        await api.beginChallenge(challengeId, factor.id);
        begunRef.current = factor.id;
      }
      session = requireSession(
        await api.verifyChallengeResponse(challengeId, factor.id, { code: code.trim() })
      );
    } catch (err) {
      setError(authErrorMessage(err, 'auth.error.code_rejected'));
      setSubmitting(false);
      return;
    }
    await finish(session);
    setSubmitting(false);
  };

  if (!factor) {
    return (
      <div className="space-y-6">
        <p role="alert" className="text-sm text-danger-700">
          {t('mfa.no_usable_factor')}
        </p>
        <button
          type="button"
          onClick={onCancel}
          className="w-full text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400"
        >
          {cancelLabel}
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {usable.length > 1 && (
        <div>
          <label
            htmlFor={selectId}
            className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2"
          >
            {t('mfa.choose_factor')}
          </label>
          <select
            id={selectId}
            value={factor.id}
            onChange={(e) => {
              setSelectedId(e.target.value);
              setError(null);
              setCode('');
            }}
            className="block w-full py-2.5 px-3 border border-gray-300 rounded-lg text-gray-900 bg-white"
          >
            {usable.map((f) => (
              <option key={f.id} value={f.id}>
                {f.label}
              </option>
            ))}
          </select>
        </div>
      )}

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

      {isWebAuthnFactor(factor) ? (
        <WebAuthnCeremony
          key={factor.id}
          mode="authenticate"
          autoFocus
          environment={environment}
          begin={async () => {
            const options = await api.beginChallenge(challengeId, factor.id);
            begunRef.current = factor.id;
            return options;
          }}
          complete={async (credential) => {
            sessionRef.current = requireSession(
              await api.verifyChallengeResponse(challengeId, factor.id, { credential })
            );
          }}
          onSuccess={() => {
            const session = sessionRef.current;
            sessionRef.current = null;
            if (session) {
              void finish(session);
            }
          }}
        />
      ) : (
        <form onSubmit={submitCode} className="space-y-6">
          <p className="text-sm text-gray-600 dark:text-gray-400">{t('mfa.code_instructions')}</p>
          <div>
            <label htmlFor={codeId} className="sr-only">
              {t('mfa.code_label')}
            </label>
            <input
              id={codeId}
              name="mfa-code"
              type="text"
              inputMode="text"
              autoComplete="one-time-code"
              autoFocus
              value={code}
              onChange={(e) => setCode(e.target.value)}
              className="block w-full px-3 py-2.5 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500 text-gray-900 bg-white"
              placeholder={factor.id === 'recovery' ? 'XXXXX-XXXXX' : '123456'}
            />
          </div>
          <button
            type="submit"
            disabled={submitting || !code.trim()}
            className="w-full flex justify-center py-2.5 px-4 border border-transparent rounded-lg shadow-sm text-sm font-medium text-white bg-primary-600 hover:bg-primary-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {submitting ? t('mfa.verifying') : t('mfa.verify')}
          </button>
        </form>
      )}

      <button
        type="button"
        onClick={onCancel}
        className="w-full text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
      >
        {cancelLabel}
      </button>
    </div>
  );
}
