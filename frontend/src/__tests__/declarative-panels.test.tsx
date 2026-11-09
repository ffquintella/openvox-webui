// @vitest-environment jsdom
// Declarative plugin panels (H-6): the mfa-fido2 example specs shared with the
// Rust validator tests, rendered against a fake API through the real client.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import { AxiosError, type InternalAxiosRequestConfig } from 'axios';
import DeclarativeRenderer from '../components/plugins/DeclarativeRenderer';
import { apiClient, isPluginRoutePath, registerStepUpHandler } from '../services/api';
import type { UiRouteAccess } from '../types';
import profileFixture from '../../../tests/fixtures/plugins/mfa-fido2-ui/profile-security.json';
import adminFixture from '../../../tests/fixtures/plugins/mfa-fido2-ui/admin-auth.json';

// The same files the Rust validator tests and BDD scenarios use.
const profileSpec: unknown = profileFixture;
const adminSpec: unknown = adminFixture;

const route = (method: string, path: string, allowed = true, step_up = false): UiRouteAccess => ({
  method,
  path,
  allowed,
  step_up,
});

const PROFILE_ROUTES = [
  route('GET', '/me/credentials'),
  route('PATCH', '/me/credentials/{credential_ref}'),
];

const ADMIN_ROUTES = [
  route('GET', '/admin/summary'),
  route('GET', '/admin/credentials'),
  route('GET', '/admin/authenticators'),
  route('POST', '/admin/credentials/{credential_ref}/suspend'),
  route('POST', '/admin/credentials/{credential_ref}/unsuspend', true, true),
  route('POST', '/admin/policy/preview'),
];

interface Call {
  method: string;
  url: string;
  params?: Record<string, string>;
  data?: unknown;
}

type Reply = { status: number; data?: unknown };
type Handler = (call: Call) => Reply;

const originalAdapter = apiClient.defaults.adapter;
let calls: Call[];

function serve(handler: Handler) {
  apiClient.defaults.adapter = async (config: InternalAxiosRequestConfig) => {
    const data = typeof config.data === 'string' ? JSON.parse(config.data) : config.data;
    const call: Call = {
      method: (config.method ?? 'get').toUpperCase(),
      url: config.url ?? '',
      params: config.params,
      data,
    };
    calls.push(call);
    const reply = handler(call);
    const response = {
      data: reply.data,
      status: reply.status,
      statusText: String(reply.status),
      headers: {},
      config,
    };
    if (reply.status >= 400) {
      throw new AxiosError('failed', String(reply.status), config, null, response);
    }
    return response;
  };
}

function renderSpec(spec: unknown, routes: UiRouteAccess[], params?: Record<string, string>) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <DeclarativeRenderer spec={spec} pluginId="mfa-fido2" routes={routes} params={params} />
    </QueryClientProvider>
  );
}

const gets = (url: string) => calls.filter((c) => c.method === 'GET' && c.url === url);

beforeEach(() => {
  calls = [];
});

afterEach(() => {
  cleanup();
  apiClient.defaults.adapter = originalAdapter;
  registerStepUpHandler(null);
  vi.restoreAllMocks();
});

// ---------------------------------------------------------------------------
// profile.security
// ---------------------------------------------------------------------------

const myCredentials = {
  below_recommended_minimum: true,
  credentials: [
    {
      credential_ref: 'k 1',
      nickname: 'Desk key',
      authenticator: { model: 'YubiKey 5 Series' },
      created_at: '2026-11-09T12:00:00Z',
      last_used_at: null,
      status: 'active',
      blocked_reason: null,
    },
    {
      credential_ref: 'k2',
      nickname: '<img src=x onerror=alert(1)>',
      authenticator: { model: '<script>alert(1)</script>' },
      created_at: 'not a date',
      last_used_at: null,
      status: 'clone_suspected',
      blocked_reason: 'sign_count_regression',
    },
  ],
};

describe('declarative table bound to a plugin route', () => {
  it('renders an accessible table from the route data, with text-only values', async () => {
    serve(() => ({ status: 200, data: myCredentials }));
    const { container } = renderSpec(profileSpec, PROFILE_ROUTES, { user: 'alice' });

    expect(await screen.findByText('Desk key')).toBeInTheDocument();
    const table = screen.getByRole('table', { name: 'Registered security keys' });
    const headers = within(table)
      .getAllByRole('columnheader')
      .map((h) => h.textContent);
    expect(headers).toEqual([
      'Nickname',
      'Model',
      'Added',
      'Last used',
      'Status',
      'Reason',
      'Actions',
    ]);
    expect(within(table).getByText('Active')).toBeInTheDocument();
    expect(within(table).getByText('Possible clone')).toBeInTheDocument();
    expect(within(table).getAllByText('Never')).toHaveLength(2);
    expect(within(table).getByText('not a date')).toBeInTheDocument();

    // The bound notice and the table share one request to the plugin route.
    expect(screen.getByRole('note')).toHaveTextContent(/fewer security keys/);
    expect(gets('/plugins/mfa-fido2/me/credentials')).toHaveLength(1);

    // Plugin data is text, never markup.
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('script')).toBeNull();
    expect(screen.getByText('<img src=x onerror=alert(1)>')).toBeInTheDocument();
  });

  it('shows the spec empty text and hides a notice whose condition is false', async () => {
    serve(() => ({ status: 200, data: { below_recommended_minimum: false, credentials: [] } }));
    renderSpec(profileSpec, PROFILE_ROUTES);
    expect(await screen.findByText('You have no security keys yet.')).toBeInTheDocument();
    expect(screen.queryByText(/fewer security keys/)).toBeNull();
  });

  it('shows a generic error when the plugin route fails', async () => {
    serve(() => ({
      status: 503,
      data: { error: 'service_unavailable', code: 'plugin_unavailable', message: 'secret detail' },
    }));
    renderSpec(profileSpec, PROFILE_ROUTES);
    const alerts = await screen.findAllByRole('alert');
    expect(alerts[0]).toHaveTextContent('The plugin could not handle the request');
    expect(screen.queryByText(/secret detail/)).toBeNull();
  });

  it('never calls a route the server did not allow for this user', async () => {
    serve(() => ({ status: 200, data: myCredentials }));
    renderSpec(profileSpec, [route('GET', '/me/credentials', false)]);
    expect(
      await screen.findByText('You do not have access to this information.')
    ).toBeInTheDocument();
    expect(calls).toHaveLength(0);
  });

  it('renames a key through the labelled dialog with the encoded route parameter', async () => {
    serve((c) =>
      c.method === 'PATCH'
        ? { status: 200, data: { credential_ref: 'k 1', nickname: 'Desk' } }
        : { status: 200, data: myCredentials }
    );
    renderSpec(profileSpec, PROFILE_ROUTES);
    await screen.findByText('Desk key');
    fireEvent.click(screen.getAllByRole('button', { name: 'Rename' })[0]);

    const dialog = screen.getByRole('dialog', { name: 'Rename' });
    const input = within(dialog).getByLabelText(/Nickname/);
    expect(input).toHaveValue('Desk key');
    expect(input).toHaveFocus();
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rename' }));
    expect(within(dialog).getByText('Nickname is required.')).toBeInTheDocument();
    expect(calls.filter((c) => c.method === 'PATCH')).toHaveLength(0);

    fireEvent.change(input, { target: { value: 'Desk' } });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rename' }));
    expect(await screen.findByText('The key was renamed.')).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).toBeNull();
    const patch = calls.find((c) => c.method === 'PATCH');
    expect(patch?.url).toBe('/plugins/mfa-fido2/me/credentials/k%201');
    expect(patch?.data).toEqual({ nickname: 'Desk' });
    // The data is fetched again after a change.
    await waitFor(() => expect(gets('/plugins/mfa-fido2/me/credentials')).toHaveLength(2));
  });

  it('hides actions whose route the user may not call', async () => {
    serve(() => ({ status: 200, data: myCredentials }));
    renderSpec(profileSpec, [
      route('GET', '/me/credentials'),
      route('PATCH', '/me/credentials/{credential_ref}', false),
    ]);
    await screen.findByText('Desk key');
    expect(screen.queryByRole('button', { name: 'Rename' })).toBeNull();
    expect(screen.queryByRole('columnheader', { name: 'Actions' })).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// admin.auth
// ---------------------------------------------------------------------------

const summary = {
  config: { valid: false, errors: [], warnings: [] },
  totals: { credentials: 1234, users: 7 },
  blocked_by_policy: { count: 2, by_reason: {} },
  by_model: [{ aaguid: 'a-1', vendor: 'Yubico', model: 'YubiKey 5 Series', count: 3 }],
  rp_id_mismatch: 0,
  users_below_recommended_minimum: 1,
};

const inventory = (cursor?: string) => ({
  items: [
    {
      credential_ref: cursor ? 'c3' : 'c1',
      username: cursor ? 'carol' : 'alice',
      nickname: 'Desk',
      authenticator: { model: 'YubiKey 5 Series' },
      attestation_trust: 'verified',
      status: 'active',
      last_used_at: null,
    },
    {
      credential_ref: 'c2',
      username: 'bob',
      nickname: null,
      authenticator: { model: 'Titan' },
      attestation_trust: 'none',
      status: 'suspended',
      last_used_at: '2026-11-01T10:00:00Z',
    },
  ],
  next_cursor: cursor ? null : 'page-2',
});

function adminServer(overrides: Partial<Record<string, (c: Call) => Reply>> = {}): Handler {
  return (c) => {
    const custom = overrides[`${c.method} ${c.url}`];
    if (custom) return custom(c);
    if (c.url === '/plugins/mfa-fido2/admin/summary') return { status: 200, data: summary };
    if (c.url === '/plugins/mfa-fido2/admin/credentials') {
      return { status: 200, data: inventory(c.params?.cursor) };
    }
    if (c.url === '/plugins/mfa-fido2/admin/authenticators') {
      return { status: 200, data: { items: [] } };
    }
    return { status: 200, data: {} };
  };
}

async function renderAdmin(routes = ADMIN_ROUTES) {
  const view = renderSpec(adminSpec, routes);
  await screen.findByText('alice');
  return view;
}

describe('declarative administration panel', () => {
  it('shows bound stats, a conditional notice and row actions by condition', async () => {
    serve(adminServer());
    await renderAdmin();
    expect(screen.getByText((1234).toLocaleString())).toBeInTheDocument();
    expect(screen.getByText('Configuration errors')).toBeInTheDocument();
    expect(gets('/plugins/mfa-fido2/admin/summary')).toHaveLength(1);

    const inventoryTable = screen.getByRole('table', { name: 'Inventory' });
    const rows = within(inventoryTable).getAllByRole('row');
    expect(within(rows[1]).getByRole('button', { name: 'Suspend' })).toBeInTheDocument();
    expect(within(rows[1]).queryByRole('button', { name: 'Unsuspend' })).toBeNull();
    expect(within(rows[2]).getByRole('button', { name: 'Unsuspend' })).toBeInTheDocument();
    expect(within(rows[2]).queryByRole('button', { name: 'Suspend' })).toBeNull();
    const byModel = screen.getByRole('table', { name: 'Keys by model' });
    expect(within(byModel).getByText('Yubico')).toBeInTheDocument();
    expect(screen.getByRole('table', { name: 'Authenticator catalog' })).toHaveTextContent(
      'No rows'
    );
  });

  it('confirms a destructive action and requires its reason before calling the route', async () => {
    serve(
      adminServer({
        'POST /plugins/mfa-fido2/admin/credentials/c1/suspend': () => ({
          status: 200,
          data: { status: 'suspended' },
        }),
      })
    );
    await renderAdmin();
    fireEvent.click(screen.getByRole('button', { name: 'Suspend' }));
    const dialog = screen.getByRole('dialog', { name: 'Suspend this security key?' });
    expect(dialog).toHaveAccessibleDescription(/stops working for sign-in/);
    fireEvent.click(within(dialog).getByRole('button', { name: 'Suspend key' }));
    expect(within(dialog).getByText('Reason is required.')).toBeInTheDocument();
    expect(calls.filter((c) => c.method === 'POST')).toHaveLength(0);

    fireEvent.change(within(dialog).getByLabelText(/Reason/), { target: { value: 'Lost' } });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Suspend key' }));
    expect(await screen.findByText('The key was suspended.')).toBeInTheDocument();
    const post = calls.find((c) => c.method === 'POST');
    expect(post?.url).toBe('/plugins/mfa-fido2/admin/credentials/c1/suspend');
    expect(post?.data).toEqual({ reason: 'Lost' });
  });

  it('closes the dialog on Escape without calling the route', async () => {
    serve(adminServer());
    await renderAdmin();
    fireEvent.click(screen.getByRole('button', { name: 'Suspend' }));
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(calls.filter((c) => c.method === 'POST')).toHaveLength(0);
  });

  it('runs a step-up route through the host step-up handler and repeats the call', async () => {
    let attempts = 0;
    serve(
      adminServer({
        'POST /plugins/mfa-fido2/admin/credentials/c2/unsuspend': () => {
          attempts += 1;
          return attempts === 1
            ? { status: 401, data: { error: 'unauthorized', code: 'step_up_required' } }
            : { status: 200, data: { status: 'active' } };
        },
      })
    );
    const stepUp = vi.fn(async () => true);
    registerStepUpHandler(stepUp);
    await renderAdmin();
    fireEvent.click(screen.getByRole('button', { name: 'Unsuspend' }));
    const dialog = screen.getByRole('dialog', { name: 'Unsuspend' });
    expect(
      within(dialog).getByText('You may be asked to verify your identity with a second factor.')
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Unsuspend key' }));
    expect(await screen.findByText('The key was unsuspended.')).toBeInTheDocument();
    expect(stepUp).toHaveBeenCalledTimes(1);
    expect(attempts).toBe(2);
  });

  it('reports a cancelled step-up and keeps the dialog open', async () => {
    serve(
      adminServer({
        'POST /plugins/mfa-fido2/admin/credentials/c2/unsuspend': () => ({
          status: 401,
          data: { error: 'unauthorized', code: 'step_up_required' },
        }),
      })
    );
    registerStepUpHandler(async () => false);
    await renderAdmin();
    fireEvent.click(screen.getByRole('button', { name: 'Unsuspend' }));
    const dialog = screen.getByRole('dialog', { name: 'Unsuspend' });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Unsuspend key' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      /needs a second verification/
    );
  });

  it('applies filters and pages with the cursor as query parameters', async () => {
    serve(adminServer());
    await renderAdmin();
    const filters = screen.getByRole('form', { name: 'Filters' });
    fireEvent.change(within(filters).getByLabelText('User'), { target: { value: 'alice' } });
    fireEvent.click(within(filters).getByRole('button', { name: 'Apply filters' }));
    await waitFor(() =>
      expect(
        gets('/plugins/mfa-fido2/admin/credentials').some(
          (c) => c.params?.username === 'alice' && c.params?.limit === '50'
        )
      ).toBe(true)
    );
    await screen.findByText('alice');
    fireEvent.click(screen.getByRole('button', { name: 'Next page' }));
    expect(await screen.findByText('carol')).toBeInTheDocument();
    const pages = gets('/plugins/mfa-fido2/admin/credentials');
    const last = pages[pages.length - 1];
    expect(last?.params).toEqual({ limit: '50', username: 'alice', cursor: 'page-2' });
    expect(screen.getByRole('button', { name: 'Previous page' })).toBeEnabled();
  });

  it('validates a JSON form field and shows the route result', async () => {
    serve(
      adminServer({
        'POST /plugins/mfa-fido2/admin/policy/preview': () => ({
          status: 200,
          data: {
            valid: false,
            errors: [{ path: 'rp.origins', code: 'required', message: 'at least one origin' }],
            warnings: [],
            impact: { credentials_blocked: 4, users_without_usable_credential: 1 },
          },
        }),
      })
    );
    await renderAdmin();
    const field = screen.getByLabelText(/Proposed configuration/);
    fireEvent.change(field, { target: { value: '{bad' } });
    fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    expect(
      screen.getByText('Proposed configuration (JSON) must be valid JSON.')
    ).toBeInTheDocument();
    expect(calls.filter((c) => c.method === 'POST')).toHaveLength(0);

    fireEvent.change(field, { target: { value: '{"rp": {"id": "example.org"}}' } });
    fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    const result = await screen.findByRole('region', { name: 'Result' });
    expect(within(result).getByText('rp.origins: at least one origin')).toBeInTheDocument();
    expect(within(result).getByText('None')).toBeInTheDocument();
    expect(calls.find((c) => c.method === 'POST')?.data).toEqual({
      config: { rp: { id: 'example.org' } },
    });
  });

  it('shows the plugin message of a refused action as text', async () => {
    serve(
      adminServer({
        'POST /plugins/mfa-fido2/admin/credentials/c1/suspend': () => ({
          status: 409,
          data: { error: { code: 'already_suspended', message: '<b>Already</b> suspended' } },
        }),
      })
    );
    const { container } = await renderAdmin();
    fireEvent.click(screen.getByRole('button', { name: 'Suspend' }));
    const dialog = screen.getByRole('dialog');
    fireEvent.change(within(dialog).getByLabelText(/Reason/), { target: { value: 'x' } });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Suspend key' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('<b>Already</b> suspended');
    expect(container.querySelector('b')).toBeNull();
  });

  it('offers no write actions or forms to a user without the write routes', async () => {
    serve(adminServer());
    await renderAdmin([
      route('GET', '/admin/summary'),
      route('GET', '/admin/credentials'),
      route('GET', '/admin/authenticators'),
      route('POST', '/admin/credentials/{credential_ref}/suspend', false),
      route('POST', '/admin/credentials/{credential_ref}/unsuspend', false, true),
      route('POST', '/admin/policy/preview', false),
    ]);
    expect(screen.queryByRole('button', { name: 'Suspend' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Preview' })).toBeNull();
  });
});

describe('destructive calls', () => {
  it('always ask for confirmation, even when a spec declares none', async () => {
    serve((c) =>
      c.method === 'DELETE'
        ? { status: 204 }
        : { status: 200, data: { items: [{ id: 'a1', name: 'First' }] } }
    );
    renderSpec(
      {
        layout: 'table',
        source: { route: '/items', path: 'items' },
        columns: [{ key: 'name', label: 'Name' }],
        row_actions: [{ id: 'delete', label: 'Delete', method: 'DELETE', route: '/items/{id}' }],
      },
      [route('GET', '/items'), route('DELETE', '/items/{id}')]
    );
    await screen.findByText('First');
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete' });
    expect(calls.filter((c) => c.method === 'DELETE')).toHaveLength(0);
    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));
    expect(await screen.findByText('Done.')).toBeInTheDocument();
    const del = calls.find((c) => c.method === 'DELETE');
    expect(del?.url).toBe('/plugins/mfa-fido2/items/a1');
    expect(del?.data).toBeUndefined();
  });
});

describe('plugin route path guard', () => {
  it('accepts filled routes and refuses traversal, schemes and encoded separators', () => {
    expect(isPluginRoutePath('/me/credentials/k%201')).toBe(true);
    for (const bad of [
      '',
      '/',
      '//evil.example/x',
      '/me/../admin',
      '/me/./x',
      '/me/a%2Fb',
      '/me/a%5cb',
      '/me?x=1',
      'https://evil.example',
      '/me/a b',
    ]) {
      expect(isPluginRoutePath(bad), bad).toBe(false);
    }
  });
});
