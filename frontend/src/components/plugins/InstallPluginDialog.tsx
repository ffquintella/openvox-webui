import { useState } from 'react';
import { Loader2, Upload, ShieldAlert } from 'lucide-react';
import { usePreflightPlugin, useInstallPlugin } from '../../hooks/usePlugins';
import type { PluginPreflightResult } from '../../types';
import { ErrorBox, Modal, RiskBadge } from './pluginUi';
import {
  TRUST_CONFIRM_LENGTH,
  canConfirmUntrustedInstall,
  getApiErrorMessage,
  inputClass,
  primaryBtn,
  secondaryBtn,
  signatureLabel,
} from './pluginHelpers';

interface PreflightPanelProps {
  result: PluginPreflightResult;
  canTrustKeys: boolean;
  installing: boolean;
  installError: string | null;
  onInstall: (options?: {
    trustKeyConfirm: string;
    trustKeyLabel: string;
    reauthPassword: string;
  }) => void;
  onCancel: () => void;
}

export function PreflightPanel({
  result,
  canTrustKeys,
  installing,
  installError,
  onInstall,
  onCancel,
}: PreflightPanelProps) {
  const [typedConfirm, setTypedConfirm] = useState('');
  const [label, setLabel] = useState('');
  const [password, setPassword] = useState('');

  const { manifest, signature } = result;
  const untrusted = signature.status === 'untrusted_key';
  const signatureOk = signature.status === 'valid';
  const blocked = !result.can_install || (!signatureOk && !untrusted);

  let disabledReason: string | null = null;
  if (!signatureOk && !untrusted) {
    disabledReason = signature.message ?? signatureLabel(signature.status);
  } else if (!result.can_install) {
    disabledReason = result.issues[0] ?? 'This package cannot be installed.';
  }

  const trustReady =
    untrusted &&
    canTrustKeys &&
    canConfirmUntrustedInstall({
      keyId: signature.key_id,
      typedConfirm,
      label,
      password,
    });

  return (
    <div className="space-y-5">
      <section>
        <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Package</h4>
        <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
          <dt className="text-gray-500">Name</dt>
          <dd>{manifest.name}</dd>
          <dt className="text-gray-500">Id</dt>
          <dd className="font-mono">{manifest.id}</dd>
          <dt className="text-gray-500">Version</dt>
          <dd>
            {manifest.version}
            {result.installed_version ? ` (installed: ${result.installed_version})` : ''}
          </dd>
          <dt className="text-gray-500">Publisher</dt>
          <dd>{manifest.publisher}</dd>
          <dt className="text-gray-500">Runtime</dt>
          <dd>{manifest.runtime_kind}</dd>
          <dt className="text-gray-500">Plugin API</dt>
          <dd>{manifest.api_version}</dd>
          <dt className="text-gray-500">License</dt>
          <dd>{manifest.license ?? 'not declared'}</dd>
          <dt className="text-gray-500">SHA-256</dt>
          <dd className="font-mono text-xs break-all">{result.package_sha256}</dd>
        </dl>
      </section>

      <section>
        <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Signature</h4>
        <p
          data-testid="signature-status"
          className={
            signatureOk
              ? 'mt-1 text-sm text-green-700 dark:text-green-300'
              : 'mt-1 text-sm text-red-700 dark:text-red-300'
          }
        >
          {signatureLabel(signature.status)}
        </p>
        {signature.message && <p className="text-xs text-gray-500">{signature.message}</p>}
        <p className="mt-1 text-xs text-gray-500">
          Signer fingerprint:{' '}
          <span data-testid="signer-fingerprint" className="font-mono break-all">
            {signature.key_id}
          </span>
          {signature.key_label ? ` (${signature.key_label})` : ''}
        </p>
      </section>

      <section>
        <h4 className="text-sm font-semibold text-gray-900 dark:text-white">
          Capabilities requested
        </h4>
        {result.capabilities.length === 0 ? (
          <p className="mt-1 text-sm text-gray-500">None</p>
        ) : (
          <ul className="mt-2 space-y-2">
            {result.capabilities.map((c) => (
              <li key={c.name} className="flex items-start gap-2 text-sm">
                <RiskBadge risk={c.risk} />
                <div>
                  <span className="font-mono">{c.name}</span>
                  <p className="text-gray-600 dark:text-gray-300">{c.description}</p>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      {result.declared_pii_fields.length > 0 && (
        <section>
          <h4 className="text-sm font-semibold text-gray-900 dark:text-white">
            Declared personal data fields
          </h4>
          <p className="mt-1 text-sm font-mono">{result.declared_pii_fields.join(', ')}</p>
        </section>
      )}

      {result.issues.length > 0 && (
        <section>
          <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Issues</h4>
          <ul className="mt-1 list-disc pl-5 text-sm text-red-700 dark:text-red-300">
            {result.issues.map((issue, i) => (
              <li key={i}>{issue}</li>
            ))}
          </ul>
        </section>
      )}

      {untrusted && !blocked && (
        <section className="rounded-lg border border-yellow-300 bg-yellow-50 dark:bg-yellow-900/20 p-4 space-y-3">
          <div className="flex items-start gap-2 text-sm text-yellow-800 dark:text-yellow-200">
            <ShieldAlert className="w-4 h-4 mt-0.5 flex-shrink-0" />
            <span>
              This package is signed by a key that is not trusted. Compare the fingerprint above
              with the value you received from the publisher over another channel before continuing.
            </span>
          </div>
          {canTrustKeys ? (
            <>
              <label className="block text-sm">
                <span className="text-gray-700 dark:text-gray-200">
                  Type the first {TRUST_CONFIRM_LENGTH} characters of the fingerprint
                </span>
                <input
                  aria-label="Fingerprint confirmation"
                  className={inputClass + ' font-mono mt-1'}
                  value={typedConfirm}
                  onChange={(e) => setTypedConfirm(e.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
              <label className="block text-sm">
                <span className="text-gray-700 dark:text-gray-200">Label for this key</span>
                <input
                  aria-label="Key label"
                  className={inputClass + ' mt-1'}
                  value={label}
                  onChange={(e) => setLabel(e.target.value)}
                />
              </label>
              <label className="block text-sm">
                <span className="text-gray-700 dark:text-gray-200">Your password</span>
                <input
                  aria-label="Your password"
                  type="password"
                  className={inputClass + ' mt-1'}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  autoComplete="current-password"
                />
              </label>
            </>
          ) : (
            <p className="text-sm text-yellow-800 dark:text-yellow-200">
              You are not allowed to trust new signing keys. Ask a key manager to trust this key
              first, then upload the package again.
            </p>
          )}
        </section>
      )}

      {disabledReason && <ErrorBox message={disabledReason} />}
      {installError && <ErrorBox message={installError} />}

      <div className="flex justify-end gap-3">
        <button type="button" className={secondaryBtn} onClick={onCancel}>
          Cancel
        </button>
        {signatureOk && (
          <button
            type="button"
            className={primaryBtn}
            disabled={blocked || installing}
            onClick={() => onInstall()}
          >
            {installing ? 'Installing...' : 'Install'}
          </button>
        )}
        {untrusted && (
          <button
            type="button"
            className={primaryBtn}
            disabled={blocked || !trustReady || installing}
            onClick={() =>
              onInstall({
                trustKeyConfirm: typedConfirm.toLowerCase(),
                trustKeyLabel: label.trim(),
                reauthPassword: password,
              })
            }
          >
            {installing ? 'Installing...' : 'Trust key and install'}
          </button>
        )}
        {!signatureOk && !untrusted && (
          <button type="button" className={primaryBtn} disabled>
            Install
          </button>
        )}
      </div>
    </div>
  );
}

export default function InstallPluginDialog({
  canTrustKeys,
  onClose,
}: {
  canTrustKeys: boolean;
  onClose: () => void;
}) {
  const [file, setFile] = useState<File | null>(null);
  const [result, setResult] = useState<PluginPreflightResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const preflight = usePreflightPlugin();
  const install = useInstallPlugin();

  const handleFile = async (picked: File | undefined) => {
    setError(null);
    setResult(null);
    if (!picked) return;
    setFile(picked);
    try {
      setResult(await preflight.mutateAsync(picked));
    } catch (e) {
      setError(getApiErrorMessage(e, 'Could not read this package'));
    }
  };

  const handleInstall = async (options?: {
    trustKeyConfirm: string;
    trustKeyLabel: string;
    reauthPassword: string;
  }) => {
    if (!file) return;
    setError(null);
    try {
      await install.mutateAsync({ pkg: file, options });
      onClose();
    } catch (e) {
      setError(getApiErrorMessage(e, 'Install failed'));
    }
  };

  return (
    <Modal title="Install plugin" onClose={onClose} wide>
      <div className="space-y-5">
        <label className="flex items-center gap-3 text-sm">
          <Upload className="w-4 h-4 text-gray-500" />
          <input
            type="file"
            accept=".ovxp"
            aria-label="Plugin package"
            onChange={(e) => handleFile(e.target.files?.[0])}
          />
        </label>
        {preflight.isPending && (
          <div className="flex items-center gap-2 text-sm text-gray-500">
            <Loader2 className="w-4 h-4 animate-spin" /> Checking package...
          </div>
        )}
        {error && !result && <ErrorBox message={error} />}
        {result && (
          <PreflightPanel
            key={result.package_sha256}
            result={result}
            canTrustKeys={canTrustKeys}
            installing={install.isPending}
            installError={error}
            onInstall={handleInstall}
            onCancel={onClose}
          />
        )}
      </div>
    </Modal>
  );
}
