// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { PreflightPanel } from '../components/plugins/InstallPluginDialog';
import { canConfirmUntrustedInstall } from '../components/plugins/pluginHelpers';
import type { PluginPreflightResult, PluginSignatureStatus } from '../types';

afterEach(cleanup);

const KEY_ID = 'a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90';

function preflight(
  status: PluginSignatureStatus,
  overrides: Partial<PluginPreflightResult> = {}
): PluginPreflightResult {
  return {
    manifest: {
      id: 'acme',
      name: 'Acme',
      version: '1.0.0',
      publisher: 'Acme Inc',
      api_version: 2,
      license: 'Apache-2.0',
      runtime_kind: 'wasm',
      min_webui_version: null,
    },
    package_sha256: 'f'.repeat(64),
    signature: {
      status,
      key_id: KEY_ID,
      key_label: null,
      key_origin: null,
      signed_at: '2026-10-01T00:00:00Z',
      message: null,
    },
    capabilities: [
      { name: 'http:egress', scope: null, risk: 'high', description: 'Calls external hosts' },
    ],
    declared_pii_fields: ['email'],
    installed_version: null,
    issues: [],
    can_install: true,
    added_capabilities: [],
    rollback_warning: null,
    ...overrides,
  };
}

function renderPanel(result: PluginPreflightResult, canTrustKeys = true) {
  const onInstall = vi.fn();
  render(
    <PreflightPanel
      result={result}
      canTrustKeys={canTrustKeys}
      installing={false}
      installError={null}
      onInstall={onInstall}
      onCancel={() => undefined}
    />
  );
  return onInstall;
}

describe('canConfirmUntrustedInstall', () => {
  const base = { keyId: KEY_ID, typedConfirm: 'a1b2c3d4', label: 'Acme', password: 'pw' };

  it('requires the exact first 8 characters', () => {
    expect(canConfirmUntrustedInstall(base)).toBe(true);
    expect(canConfirmUntrustedInstall({ ...base, typedConfirm: 'a1b2c3d' })).toBe(false);
    expect(canConfirmUntrustedInstall({ ...base, typedConfirm: 'a1b2c3d5' })).toBe(false);
    expect(canConfirmUntrustedInstall({ ...base, typedConfirm: 'a1b2c3d4e' })).toBe(false);
    expect(canConfirmUntrustedInstall({ ...base, typedConfirm: '' })).toBe(false);
  });

  it('requires a label and a password', () => {
    expect(canConfirmUntrustedInstall({ ...base, label: '  ' })).toBe(false);
    expect(canConfirmUntrustedInstall({ ...base, password: '' })).toBe(false);
  });
});

describe('PreflightPanel for an untrusted key', () => {
  it('keeps the trust button disabled until the exact 8 characters, label and password are given', () => {
    const onInstall = renderPanel(preflight('untrusted_key'));
    const button = screen.getByRole('button', { name: 'Trust key and install' });
    expect(button).toBeDisabled();
    expect(screen.getByTestId('signer-fingerprint')).toHaveTextContent(KEY_ID);

    fireEvent.change(screen.getByLabelText('Key label'), { target: { value: 'Acme signing' } });
    fireEvent.change(screen.getByLabelText('Your password'), { target: { value: 'secret' } });

    fireEvent.change(screen.getByLabelText('Fingerprint confirmation'), {
      target: { value: 'a1b2c3d' },
    });
    expect(button).toBeDisabled();

    fireEvent.change(screen.getByLabelText('Fingerprint confirmation'), {
      target: { value: 'a1b2c3d5' },
    });
    expect(button).toBeDisabled();

    fireEvent.change(screen.getByLabelText('Fingerprint confirmation'), {
      target: { value: 'a1b2c3d4' },
    });
    expect(button).toBeEnabled();

    fireEvent.click(button);
    expect(onInstall).toHaveBeenCalledWith({
      trustKeyConfirm: 'a1b2c3d4',
      trustKeyLabel: 'Acme signing',
      reauthPassword: 'secret',
    });
  });

  it('explains that a key manager must trust the key when the user cannot', () => {
    renderPanel(preflight('untrusted_key'), false);
    expect(screen.getByText(/Ask a key manager to trust this key/)).toBeInTheDocument();
    expect(screen.queryByLabelText('Fingerprint confirmation')).toBeNull();
    expect(screen.getByRole('button', { name: 'Trust key and install' })).toBeDisabled();
  });
});

describe('PreflightPanel for other signature states', () => {
  it('offers a plain install only for a valid signature', () => {
    const onInstall = renderPanel(preflight('valid'));
    const install = screen.getByRole('button', { name: 'Install' });
    expect(install).toBeEnabled();
    expect(screen.queryByRole('button', { name: 'Trust key and install' })).toBeNull();
    fireEvent.click(install);
    expect(onInstall).toHaveBeenCalledWith();
  });

  it.each(['invalid', 'key_revoked', 'key_expired', 'key_scope'] as const)(
    'disables install for %s with the reason shown',
    (status) => {
      renderPanel(preflight(status, { can_install: false }));
      expect(screen.getByRole('button', { name: 'Install' })).toBeDisabled();
      expect(screen.queryByRole('button', { name: 'Trust key and install' })).toBeNull();
      expect(screen.getAllByRole('alert').length).toBeGreaterThan(0);
    }
  );

  it('disables install when can_install is false even if the signature is valid', () => {
    renderPanel(preflight('valid', { can_install: false, issues: ['Newer version installed'] }));
    expect(screen.getByRole('button', { name: 'Install' })).toBeDisabled();
    expect(screen.getAllByText('Newer version installed').length).toBeGreaterThan(0);
  });

  it('shows capabilities with risk and declared personal data fields', () => {
    renderPanel(preflight('valid'));
    expect(screen.getByText('high')).toBeInTheDocument();
    expect(screen.getByText('Calls external hosts')).toBeInTheDocument();
    expect(screen.getByText('email')).toBeInTheDocument();
  });
});
