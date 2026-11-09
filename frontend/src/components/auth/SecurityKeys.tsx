import { useEffect, useId, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { KeyRound } from 'lucide-react';
import { t } from '../../i18n';
import {
  api,
  type AuthFactorDescriptor,
  type EnrolledFactor,
  type FactorEnrollResult,
} from '../../services/api';
import type { WebAuthnEnvironment } from '../../services/webauthn';
import WebAuthnCeremony from './WebAuthnCeremony';
import { authErrorMessage, isWebAuthnFactor } from './authErrors';

const MY_FACTORS_KEY = ['myFactors'];

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString();
}

export function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
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

/** One enrolled record, with an inline rename form when `canRename` (OQ-15). */
function RecordRow({
  record,
  fallbackLabel,
  canRename,
}: {
  record: EnrolledFactor;
  fallbackLabel: string;
  canRename: boolean;
}) {
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [label, setLabel] = useState(record.label);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const inputId = useId();
  const shown = record.label || fallbackLabel;

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await api.renameMyFactor(record.id, label.trim());
      setEditing(false);
      void queryClient.invalidateQueries({ queryKey: MY_FACTORS_KEY });
    } catch (err) {
      setError(authErrorMessage(err, 'auth.factor_error.invalid_label'));
    } finally {
      setSaving(false);
    }
  };

  return (
    <li className="py-2 flex items-start gap-3">
      <KeyRound aria-hidden="true" className="w-4 h-4 mt-1 text-gray-400" />
      <div className="flex-1">
        {editing ? (
          <form
            className="flex flex-wrap items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <label htmlFor={inputId} className="sr-only">
              {t('security.rename_label')}
            </label>
            <input
              id={inputId}
              type="text"
              className="input"
              maxLength={64}
              autoComplete="off"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              aria-invalid={error ? true : undefined}
            />
            <button type="submit" className="btn btn-primary" disabled={saving}>
              {t('security.rename_save')}
            </button>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                setEditing(false);
                setLabel(record.label);
                setError(null);
              }}
            >
              {t('security.rename_cancel')}
            </button>
            {error && (
              <p role="alert" className="w-full text-sm text-danger-700">
                {error}
              </p>
            )}
          </form>
        ) : (
          <p className="text-sm font-medium text-gray-900 dark:text-gray-100">{shown}</p>
        )}
        <p className="text-xs text-gray-500 dark:text-gray-400">
          {t('security.added_on', { date: formatDate(record.created_at) })}
          {' · '}
          {record.last_used_at
            ? t('security.last_used', { date: formatDate(record.last_used_at) })
            : t('security.never_used')}
        </p>
      </div>
      {canRename && !editing && (
        <button
          type="button"
          className="text-sm text-primary-600 hover:underline"
          aria-label={t('security.rename_action', { name: shown })}
          onClick={() => setEditing(true)}
        >
          {t('security.rename')}
        </button>
      )}
    </li>
  );
}

function FactorEnrolment({
  factor,
  records,
  showLabel,
  onRecoveryCodes,
  onEnrolled,
  environment,
}: {
  factor: AuthFactorDescriptor;
  records: EnrolledFactor[];
  showLabel: boolean;
  onRecoveryCodes: (codes: string[]) => void;
  onEnrolled?: (result: FactorEnrollResult) => void;
  environment?: WebAuthnEnvironment;
}) {
  const queryClient = useQueryClient();
  const [nickname, setNickname] = useState('');
  const nicknameId = useId();
  const resultRef = useRef<FactorEnrollResult | null>(null);

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
              <RecordRow
                key={record.id}
                record={record}
                fallbackLabel={factor.label}
                canRename={!onEnrolled}
              />
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
          resultRef.current = result;
          if (!onEnrolled && result?.recovery?.codes?.length) {
            onRecoveryCodes(result.recovery.codes);
          }
        }}
        onSuccess={() => {
          setNickname('');
          if (onEnrolled) {
            // The caller takes over (an enrollment-only session has ended,
            // so nothing is fetched again with it).
            onEnrolled(resultRef.current ?? { enrolled: factor.id });
            return;
          }
          void queryClient.invalidateQueries({ queryKey: MY_FACTORS_KEY });
        }}
      />
    </div>
  );
}

/**
 * Profile security section: enrol WebAuthn factors (built-in or plugin).
 * With `onEnrolled`, a successful enrolment is handed to the caller, which
 * shows the recovery codes itself (enrollment-only sign-in, OQ-12).
 */
export default function SecurityKeys({
  environment,
  onEnrolled,
}: {
  environment?: WebAuthnEnvironment;
  onEnrolled?: (result: FactorEnrollResult) => void;
}) {
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
            onEnrolled={onEnrolled}
            environment={environment}
          />
        ))}
      </div>
    </section>
  );
}
