import { useId, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { t } from '../../i18n';
import { api, type FactorEnrollResult } from '../../services/api';
import type { WebAuthnEnvironment } from '../../services/webauthn';
import SecurityKeys, { RecoveryCodes } from './SecurityKeys';
import { isWebAuthnFactor } from './authErrors';

export type EnrollmentEnd = 'enrolled' | 'signed_out';

/**
 * Enrollment-only sign-in (OQ-12): during a grace window the server only lets
 * this session list and enroll second factors. The caller has put the session
 * token on the API client; the first enrolled factor ends the session, so the
 * user then signs in again through the normal challenge.
 */
export default function EnrollmentSession({
  username,
  expiresIn,
  onDone,
  environment,
}: {
  username: string;
  /** Seconds the enrollment-only session lasts. */
  expiresIn: number;
  onDone: (end: EnrollmentEnd) => void;
  environment?: WebAuthnEnvironment;
}) {
  const headingId = useId();
  const [codes, setCodes] = useState<string[] | null>(null);
  const [signingOut, setSigningOut] = useState(false);
  const { data, isError } = useQuery({
    queryKey: ['myFactors'],
    queryFn: api.getMyFactors,
    refetchOnWindowFocus: false,
  });

  const handleEnrolled = (result: FactorEnrollResult) => {
    const recovery = result.recovery?.codes ?? [];
    if (recovery.length > 0) {
      setCodes(recovery);
    } else {
      onDone('enrolled');
    }
  };

  const signOut = async () => {
    setSigningOut(true);
    try {
      await api.logout();
    } catch {
      // The session ends with its window anyway.
    }
    onDone('signed_out');
  };

  if (codes) {
    return <RecoveryCodes codes={codes} onDone={() => onDone('enrolled')} />;
  }

  const minutes = Math.max(1, Math.ceil(expiresIn / 60));
  const canEnroll = (data?.available ?? []).some(isWebAuthnFactor);

  return (
    <section aria-labelledby={headingId} className="space-y-4">
      <div>
        <h2 id={headingId} className="text-lg font-semibold text-gray-900">
          {t('enrollment.title')}
        </h2>
        <p className="text-sm text-gray-600 mt-1">
          {t('enrollment.description', { username, minutes })}
        </p>
      </div>
      {isError && (
        <p role="alert" className="text-sm text-danger-700">
          {t('security.load_failed')}
        </p>
      )}
      {data && !canEnroll && (
        <p role="alert" className="text-sm text-danger-700">
          {t('enrollment.no_method')}
        </p>
      )}
      {canEnroll && <SecurityKeys environment={environment} onEnrolled={handleEnrolled} />}
      <button
        type="button"
        onClick={signOut}
        disabled={signingOut}
        className="w-full py-2.5 px-4 border border-gray-300 rounded-lg text-sm font-medium text-gray-700 bg-white hover:bg-gray-50 disabled:opacity-50"
      >
        {t('enrollment.sign_out')}
      </button>
    </section>
  );
}
