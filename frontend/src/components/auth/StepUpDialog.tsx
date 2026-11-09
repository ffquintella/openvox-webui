import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { CircleAlert, ShieldCheck } from 'lucide-react';
import { t } from '../../i18n';
import {
  api,
  registerStepUpHandler,
  type MfaChallenge,
  type SessionResponse,
} from '../../services/api';
import { useAuthStore } from '../../stores/authStore';
import type { User } from '../../types';
import type { WebAuthnEnvironment } from '../../services/webauthn';
import FactorChallenge from './FactorChallenge';
import { authErrorMessage } from './authErrors';

interface Pending {
  promise: Promise<boolean>;
  resolve: (ok: boolean) => void;
}

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

/**
 * Step-up (M29): when an API call answers `401 step_up_required`, the API
 * client asks this dialog for a second factor; on success the new session
 * replaces the old one (revoked by the server) and the call is repeated.
 */
export default function StepUpDialog({ environment }: { environment?: WebAuthnEnvironment }) {
  const [open, setOpen] = useState(false);
  const [offer, setOffer] = useState<MfaChallenge | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const pendingRef = useRef<Pending | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const titleId = useId();
  const descriptionId = useId();

  const finish = useCallback((ok: boolean) => {
    const pending = pendingRef.current;
    pendingRef.current = null;
    setOpen(false);
    setOffer(null);
    setStartError(null);
    const back = returnFocusRef.current;
    returnFocusRef.current = null;
    back?.focus?.();
    pending?.resolve(ok);
  }, []);

  useEffect(() => {
    const handler = (): Promise<boolean> => {
      if (pendingRef.current) {
        return pendingRef.current.promise;
      }
      let resolve: (ok: boolean) => void = () => undefined;
      const promise = new Promise<boolean>((r) => {
        resolve = r;
      });
      const pending = { promise, resolve };
      pendingRef.current = pending;
      returnFocusRef.current =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setOffer(null);
      setStartError(null);
      setOpen(true);
      api
        .stepUp()
        .then((o) => {
          if (pendingRef.current === pending) {
            setOffer(o);
          }
        })
        .catch((err) => {
          if (pendingRef.current === pending) {
            setStartError(authErrorMessage(err, 'stepup.start_failed'));
          }
        });
      return promise;
    };
    registerStepUpHandler(handler);
    return () => {
      registerStepUpHandler(null);
      pendingRef.current?.resolve(false);
      pendingRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (open) {
      dialogRef.current?.focus();
    }
  }, [open]);

  const handleSession = (session: SessionResponse) => {
    const { user, login } = useAuthStore.getState();
    const next: User = {
      ...(user ?? {}),
      id: session.user.id,
      username: session.user.username,
      email: session.user.email,
      role: session.user.role as User['role'],
    };
    login(next, session.access_token, session.refresh_token);
    finish(true);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      finish(false);
      return;
    }
    if (e.key !== 'Tab' || !dialogRef.current) {
      return;
    }
    const items = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (
      e.shiftKey &&
      (document.activeElement === first || document.activeElement === dialogRef.current)
    ) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-[60] p-4">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="bg-white dark:bg-gray-800 rounded-xl shadow-xl w-full max-w-md p-6 space-y-4 focus:outline-none"
      >
        <div className="flex items-center gap-3">
          <ShieldCheck aria-hidden="true" className="w-6 h-6 text-primary-600" />
          <h2 id={titleId} className="text-lg font-semibold text-gray-900 dark:text-white">
            {t('stepup.title')}
          </h2>
        </div>
        <p id={descriptionId} className="text-sm text-gray-600 dark:text-gray-400">
          {t('stepup.description')}
        </p>

        {startError ? (
          <div className="space-y-4">
            <div
              role="alert"
              className="p-3 bg-danger-50 border border-danger-200 rounded-lg flex items-start gap-2"
            >
              <CircleAlert
                aria-hidden="true"
                className="w-5 h-5 text-danger-500 flex-shrink-0 mt-0.5"
              />
              <p className="text-sm text-danger-700">{startError}</p>
            </div>
            <button type="button" onClick={() => finish(false)} className="btn btn-secondary">
              {t('stepup.cancel')}
            </button>
          </div>
        ) : offer ? (
          <FactorChallenge
            challengeId={offer.challenge_id}
            factors={offer.factors}
            onSession={handleSession}
            onCancel={() => finish(false)}
            cancelLabel={t('stepup.cancel')}
            environment={environment}
          />
        ) : (
          <div className="space-y-4">
            <p
              role="status"
              aria-live="polite"
              className="text-sm text-gray-700 dark:text-gray-300"
            >
              {t('stepup.preparing')}
            </p>
            <button type="button" onClick={() => finish(false)} className="btn btn-secondary">
              {t('stepup.cancel')}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
