// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import TrustedModule from '../components/plugins/TrustedModule';
import ModuleTrustPanel from '../components/plugins/ModuleTrustPanel';
import PluginSlot from '../components/plugins/PluginSlot';
import type { TrustedModuleProps } from '../components/plugins/moduleLoader';
import { pluginsApi } from '../services/api';
import type { PluginDetail, UiContribution } from '../types';

// PluginSlot uses the default loader; keep it away from the network.
vi.mock('../components/plugins/moduleLoader', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../components/plugins/moduleLoader')>();
  return {
    ...actual,
    loadTrustedModule: vi.fn(async () => (p: TrustedModuleProps) => (
      <p>slot module for {p.context.slot}</p>
    )),
  };
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const SRI = 'sha384-' + 'A'.repeat(64);
const URL_ = `/api/v1/plugins/ui-modules/${'a'.repeat(64)}/ui/remoteEntry.js`;

const moduleC: UiContribution = {
  plugin_id: 'acme',
  plugin_name: 'ACME',
  slot: 'node.detail.tabs',
  id: 'console',
  render: 'module',
  title: 'Console',
  path: null,
  order: 1000,
  label: null,
  icon: null,
  href: null,
  integrity: SRI,
  expose: './Page',
};

const session = {
  url: URL_,
  integrity: SRI,
  expose: './Page',
  version: '1.0.0',
  expires_in_secs: 3600,
};

function wrap(ui: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>
  );
}

describe('TrustedModule', () => {
  it('loads the module on demand and renders it with the slot context', async () => {
    vi.spyOn(pluginsApi, 'createModuleSession').mockResolvedValue(session);
    const loader = vi.fn(async () => (p: TrustedModuleProps) => (
      <p>
        {p.context.plugin_id}/{p.context.contribution_id} on {p.context.params.certname}
      </p>
    ));
    wrap(
      <TrustedModule
        contribution={moduleC}
        slot="node.detail.tabs"
        params={{ certname: 'web01' }}
        loader={loader}
      />
    );
    expect(await screen.findByText('acme/console on web01')).toBeInTheDocument();
    expect(pluginsApi.createModuleSession).toHaveBeenCalledWith('acme', 'console');
    expect(loader).toHaveBeenCalledWith(
      expect.objectContaining({ url: URL_, integrity: SRI, expose: './Page' })
    );
  });

  it('contains a module that throws in an error boundary', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(pluginsApi, 'createModuleSession').mockResolvedValue(session);
    const Broken = () => {
      throw new Error('boom');
    };
    wrap(
      <>
        <TrustedModule contribution={moduleC} slot="page" loader={async () => Broken} />
        <p>host still here</p>
      </>
    );
    expect(await screen.findByText(/could not be loaded/i)).toBeInTheDocument();
    expect(screen.getByText('host still here')).toBeInTheDocument();
  });

  it('shows a message when the module fails to load (integrity mismatch)', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(pluginsApi, 'createModuleSession').mockResolvedValue(session);
    const loader = vi.fn(() => Promise.reject(new Error('integrity mismatch')));
    wrap(<TrustedModule contribution={moduleC} slot="page" loader={loader} />);
    expect(await screen.findByText(/could not be loaded/i)).toBeInTheDocument();
  });

  it('does not load when the session SRI differs from the listed one or is missing', async () => {
    const loader = vi.fn();
    vi.spyOn(pluginsApi, 'createModuleSession').mockResolvedValue({
      ...session,
      integrity: 'sha384-' + 'B'.repeat(64),
    });
    wrap(<TrustedModule contribution={moduleC} slot="page" loader={loader} />);
    expect(await screen.findByText(/could not be loaded/i)).toBeInTheDocument();
    cleanup();
    vi.spyOn(pluginsApi, 'createModuleSession').mockResolvedValue({ ...session, integrity: '' });
    wrap(
      <TrustedModule contribution={{ ...moduleC, integrity: null }} slot="page" loader={loader} />
    );
    expect(await screen.findByText(/could not be loaded/i)).toBeInTheDocument();
    expect(loader).not.toHaveBeenCalled();
  });

  it('shows a message when no session is issued (not trusted or turned off)', async () => {
    vi.spyOn(pluginsApi, 'createModuleSession').mockRejectedValue(new Error('404'));
    const loader = vi.fn();
    wrap(<TrustedModule contribution={moduleC} slot="page" loader={loader} />);
    expect(await screen.findByText(/could not be loaded/i)).toBeInTheDocument();
    expect(loader).not.toHaveBeenCalled();
  });
});

describe('PluginSlot with a trusted module', () => {
  it('mounts module contributions listed by the server', async () => {
    vi.spyOn(pluginsApi, 'getUiSlots').mockResolvedValue({
      slots: { 'footer.global': [{ ...moduleC, slot: 'footer.global' }] },
      menu_ops: [],
    });
    vi.spyOn(pluginsApi, 'createModuleSession').mockResolvedValue(session);
    wrap(<PluginSlot slot="footer.global" variant="bare" />);
    expect(await screen.findByText('slot module for footer.global')).toBeInTheDocument();
    expect(screen.getByTestId('plugin-module-acme-console')).toBeInTheDocument();
  });
});

describe('ModuleTrustPanel', () => {
  const base = {
    id: 'acme',
    name: 'ACME',
    active_version: '1.0.0',
  } as unknown as PluginDetail;
  const entry = { id: 'console', slot: 'page', src: 'ui/remoteEntry.js', integrity: SRI };

  it('explains that trusted modules are turned off', () => {
    wrap(
      <ModuleTrustPanel
        plugin={{
          ...base,
          module_trust: {
            allowed_by_config: false,
            contributions: [entry],
            trusted: null,
            refusal: null,
            served: false,
          },
        }}
        canTrust
        canAdmin
      />
    );
    expect(screen.getByTestId('module-trust-status')).toHaveTextContent(
      'plugins.allow_trusted_modules'
    );
    expect(screen.queryByRole('button', { name: /trust version/i })).not.toBeInTheDocument();
  });

  it('trusts the active version with the password and withdraws it', async () => {
    const trusted = {
      plugin_id: 'acme',
      version: '1.0.0',
      package_sha256: 'x',
      modules: [entry],
      trusted_by: 'admin',
      trusted_at: '2026-10-30T00:00:00Z',
    };
    const trustSpy = vi.spyOn(pluginsApi, 'trustModules').mockResolvedValue(base);
    const view = {
      allowed_by_config: true,
      contributions: [entry],
      trusted: null,
      refusal: null,
      served: false,
    };
    const { rerender } = wrap(
      <ModuleTrustPanel plugin={{ ...base, module_trust: view }} canTrust canAdmin />
    );
    const button = screen.getByRole('button', { name: 'Trust version 1.0.0' });
    expect(button).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Your password'), { target: { value: 'pw' } });
    fireEvent.click(button);
    await waitFor(() =>
      expect(trustSpy).toHaveBeenCalledWith('acme', { password: 'pw', version: '1.0.0' })
    );

    const untrustSpy = vi.spyOn(pluginsApi, 'untrustModules').mockResolvedValue(base);
    const qc = new QueryClient();
    rerender(
      <QueryClientProvider client={qc}>
        <MemoryRouter>
          <ModuleTrustPanel
            plugin={{ ...base, module_trust: { ...view, trusted, served: true } }}
            canTrust={false}
            canAdmin
          />
        </MemoryRouter>
      </QueryClientProvider>
    );
    expect(screen.getByTestId('module-trust-status')).toHaveTextContent(
      'Version 1.0.0 trusted by admin'
    );
    fireEvent.click(screen.getByRole('button', { name: 'Withdraw trust' }));
    await waitFor(() => expect(untrustSpy).toHaveBeenCalledWith('acme'));
  });

  it('never offers trust for an authentication plugin', () => {
    wrap(
      <ModuleTrustPanel
        plugin={{
          ...base,
          module_trust: {
            allowed_by_config: true,
            contributions: [],
            trusted: null,
            refusal: 'Authentication and user-provider plugins can never run as trusted modules',
            served: false,
          },
        }}
        canTrust
        canAdmin
      />
    );
    expect(screen.getByTestId('module-trust-status')).toHaveTextContent(/can never run/);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
