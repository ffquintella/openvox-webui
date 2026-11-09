import { useEffect, useId, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { KeyRound } from 'lucide-react';
import { t } from '../../i18n';
import { api, type AuthFactorDescriptor, type EnrolledFactor } from '../../services/api';
import type { WebAuthnEnvironment } from '../../services/webauthn';
import WebAuthnCeremony from './WebAuthnCeremony';
import { isWebAuthnFactor } from './authErrors';

const MY_FACTORS_KEY = ['myFactors'];

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString();
}

function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const headingId = useId();

  useEffect(() => {
    headingRef.current?.focus();
  }, []);

  return (
    <div
      role="region"
      aria-labelledby={headingId}
      className="mb-6 p-4 rounded-lg border border-amber-300 bg-amber-50 dark:bg-amber-900/20 dark:border-amber-700"
    >
      <h3
        id={headingId}
        ref={headingRef}
        tabIndex={-1}
        className="font-semibold text-gray-900 dark:text-gray-100 focus:outline-none"
      >
        {t('security.recovery_title')}
      </h3>
      <p className="text-sm text-gray-700 dark:text-gray-300 mt-1">
        {t('security.recovery_description')}
      </p>
      <ul className="grid grid-cols-2 gap-2 font-mono text-sm my-3 text-gray-900 dark:text-gray-100">
        {codes.map((code) => (
          <li key={code}>{code}</li>
        ))}
      </ul>
      <button type="button" onClick={onDone} className="btn btn-primary">
        {t('security.recovery_done')}
      </button>
    </div>
  );
}

function FactorEnrolment({
  factor,
  records,
  showLabel,
  onRecoveryCodes,
  environment,
}: {
  factor: AuthFactorDescriptor;
  records: EnrolledFactor[];
  showLabel: boolean;
  onRecoveryCodes: (codes: string[]) => void;
  environment?: WebAuthnEnvironment;
}) {
  const queryClient = useQueryClient();
  const [nickname, setNickname] = useState('');
  const nicknameId = useId();

  return (
    <div className="space-y-4">
      {showLabel && (
        <h3 className="font-medium text-gray-900 dark:text-gray-100">{factor.label}</h3>
      )}
      <div>
        <h4 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
          {t('security.enrolled_heading')}
        </h4>
        {records.length === 0 ? (
          <p className="text-sm text-gray-500 dark:text-gray-400">{t('security.enrolled_none')}</p>
        ) : (
          <ul className="divide-y divide-gray-200 dark:divide-gray-700">
            {records.map((record) => (
              <li key={record.id} className="py-2 flex items-start gap-3">
                <KeyRound aria-hidden="true" className="w-4 h-4 mt-1 text-gray-400" />
                <div>
                  <p className="text-sm font-medium text-gray-900 dark:text-gray-100">
                    {record.label || factor.label}
                  </p>
                  <p className="text-xs text-gray-500 dark:text-gray-400">
                    {t('security.added_on', { date: formatDate(record.created_at) })}
                    {' · '}
                    {record.last_used_at
                      ? t('security.last_used', { date: formatDate(record.last_used_at) })
                      : t('security.never_used')}
                  </p>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div>
        <label
          htmlFor={nicknameId}
          className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
        >
          {t('security.nickname_label')}
        </label>
        <input
          id={nicknameId}
          type="text"
          className="input"
          maxLength={64}
          autoComplete="off"
          value={nickname}
          onChange={(e) => setNickname(e.target.value)}
          placeholder={t('security.nickname_placeholder')}
        />
      </div>
      <WebAuthnCeremony
        mode="register"
        environment={environment}
        begin={() => api.beginFactorEnroll(factor.id)}
        complete={async (credential) => {
          const label = nickname.trim();
          const result = await api.completeFactorEnroll(
            factor.id,
            label ? { credential, label } : { credential }
          );
          if (result?.recovery?.codes?.length) {
            onRecoveryCodes(result.recovery.codes);
          }
        }}
        onSuccess={() => {
          setNickname('');
          void queryClient.invalidateQueries({ queryKey: MY_FACTORS_KEY });
        }}
      />
    </div>
  );
}

/** Profile security section: enrol WebAuthn factors (built-in or plugin). */
export default function SecurityKeys({ environment }: { environment?: WebAuthnEnvironment }) {
  const headingId = useId();
  const [recoveryCodes, setRecoveryCodes] = useState<string[] | null>(null);
  const { data, isLoading, isError } = useQuery({
    queryKey: MY_FACTORS_KEY,
    queryFn: api.getMyFactors,
  });

  if (isLoading) {
    return null;
  }
  if (isError || !data) {
    return (
      <section className="card mb-6" aria-labelledby={headingId}>
        <h2 id={headingId} className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
          {t('security.heading')}
        </h2>
        <p role="alert" className="text-sm text-danger-700">
          {t('security.load_failed')}
        </p>
      </section>
    );
  }

  const factors = (data.available ?? []).filter(isWebAuthnFactor);
  if (factors.length === 0) {
    return null;
  }

  return (
    <section className="card mb-6" aria-labelledby={headingId}>
      <h2 id={headingId} className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-1">
        {t('security.heading')}
      </h2>
      <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">{t('security.description')}</p>
      {recoveryCodes && (
        <RecoveryCodes codes={recoveryCodes} onDone={() => setRecoveryCodes(null)} />
      )}
      <div className="space-y-8">
        {factors.map((factor) => (
          <FactorEnrolment
            key={factor.id}
            factor={factor}
            records={(data.factors ?? []).filter((r) => r.kind === factor.id)}
            showLabel={factors.length > 1}
            onRecoveryCodes={setRecoveryCodes}
            environment={environment}
          />
        ))}
      </div>
    </section>
  );
}
