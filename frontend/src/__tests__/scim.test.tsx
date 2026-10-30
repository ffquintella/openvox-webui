// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import Scim from '../pages/Scim';
import { api } from '../services/api';
import type { Role, ScimRoleMapping, ScimStatus, ScimTokenInfo } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const status: ScimStatus = {
  enabled: true,
  endpoint: 'https://webui.example.com/api/v1/scim/v2',
  base_url: 'https://webui.example.com',
  sso: { provider: 'saml', link_attribute: 'user_name' },
  limits: {
    rate_limit_per_minute: 600,
    max_auth_failures_per_minute: 10,
    max_body_bytes: 1048576,
    max_page_size: 200,
    max_token_lifetime_days: null,
  },
  counts: { users_active: 4, users_deactivated: 1, users_deleted: 2, groups: 1 },
  tokens: { active: 1, total: 2 },
  last_activity: '2026-10-30T10:00:00Z',
  recent: [
    {
      id: 'a1',
      action: 'scim.user_created',
      resource_id: 'alice@example.com',
      created_at: '2026-10-30T10:00:00Z',
    },
  ],
};

const token: ScimTokenInfo = {
  id: 'abc123',
  name: 'Entra ID',
  token_hint: 'Xy9z',
  created_at: '2026-10-30T09:00:00Z',
  status: 'active',
};

const mapping: ScimRoleMapping = {
  id: 'm1',
  directory_id: 'scim',
  group_ref: 'WebUI Operators',
  group_ref_kind: 'display_name',
  role_id: 'r-op',
  role_name: 'operator',
  created_at: '2026-10-30T10:00:00Z',
};

const roles = [
  { id: 'r-op', name: 'operator', display_name: 'Operator', is_system: true },
  { id: 'r-super', name: 'super_admin', display_name: 'Super Administrator', is_system: true },
] as unknown as Role[];

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Scim />
    </QueryClientProvider>
  );
}

function mockApi(overrides: Partial<ScimStatus> = {}) {
  vi.spyOn(api, 'getScimStatus').mockResolvedValue({ ...status, ...overrides });
  vi.spyOn(api, 'getScimTokens').mockResolvedValue([
    token,
    { ...token, id: 'old', name: 'Okta', token_hint: 'aaaa', status: 'revoked' },
  ]);
  vi.spyOn(api, 'getRoles').mockResolvedValue(roles);
  vi.spyOn(api, 'getScimGroups').mockResolvedValue([
    {
      id: 'g1',
      display_name: 'WebUI Operators',
      external_id: 'g-ops',
      members: 3,
      updated_at: '2026-10-30T10:00:00Z',
      roles: ['operator'],
    },
  ]);
  vi.spyOn(api, 'getScimRoleMappings').mockResolvedValue([mapping]);
}

describe('SCIM provisioning page', () => {
  it('shows the status, tokens, mappings and groups, and never offers super_admin', async () => {
    mockApi();
    renderPage();
    expect(await screen.findByTestId('scim-endpoint')).toHaveTextContent(
      'https://webui.example.com/api/v1/scim/v2'
    );
    expect(screen.getByText('4 active, 1 deactivated, 2 deleted')).toBeInTheDocument();
    expect(await screen.findByText('Entra ID')).toBeInTheDocument();
    expect(screen.getByText('…Xy9z')).toBeInTheDocument();
    // A revoked token cannot be rotated or revoked again.
    expect(screen.getByLabelText('Rotate token Entra ID')).toBeInTheDocument();
    expect(screen.queryByLabelText('Rotate token Okta')).not.toBeInTheDocument();
    expect(await screen.findAllByText('WebUI Operators')).not.toHaveLength(0);
    expect(screen.getByText('scim.user_created')).toBeInTheDocument();
    const roleSelect = screen.getByLabelText('Role');
    expect(roleSelect).toHaveTextContent('Operator');
    expect(roleSelect).not.toHaveTextContent('Super Administrator');
  });

  it('explains how to turn SCIM on when it is disabled', async () => {
    mockApi({ enabled: false });
    renderPage();
    expect(await screen.findByText('disabled')).toBeInTheDocument();
    expect(screen.getByText('auth.scim.enabled: true')).toBeInTheDocument();
  });

  it('shows a new token once and hides it when done', async () => {
    mockApi();
    const create = vi.spyOn(api, 'createScimToken').mockResolvedValue({
      token: 'ovscim_0123456789abcdef0123456789abcdef_secretvalue',
      info: { ...token, id: 'new', name: 'Okta prod' },
    });
    renderPage();
    fireEvent.change(await screen.findByLabelText('Token name'), {
      target: { value: 'Okta prod' },
    });
    fireEvent.change(screen.getByLabelText('Expires in days'), { target: { value: '90' } });
    fireEvent.click(screen.getByText('Issue token'));
    await waitFor(() =>
      expect(create).toHaveBeenCalledWith({ name: 'Okta prod', expires_in_days: 90 })
    );
    expect(
      await screen.findByText('ovscim_0123456789abcdef0123456789abcdef_secretvalue')
    ).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent('it is not shown again');
    fireEvent.click(screen.getByText('Done'));
    expect(
      screen.queryByText('ovscim_0123456789abcdef0123456789abcdef_secretvalue')
    ).not.toBeInTheDocument();
  });

  it('revokes a token after confirmation and adds a mapping', async () => {
    mockApi();
    const revoke = vi.spyOn(api, 'revokeScimToken').mockResolvedValue();
    const createMapping = vi.spyOn(api, 'createScimRoleMapping').mockResolvedValue(mapping);
    vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
    renderPage();
    const revokeButton = await screen.findByLabelText('Revoke token Entra ID');
    fireEvent.click(revokeButton);
    expect(revoke).not.toHaveBeenCalled();
    fireEvent.click(revokeButton);
    await waitFor(() => expect(revoke).toHaveBeenCalledWith('abc123'));

    fireEvent.change(screen.getByLabelText('Group'), { target: { value: 'Platform Team' } });
    fireEvent.change(screen.getByLabelText('Match by'), { target: { value: 'external_id' } });
    fireEvent.change(screen.getByLabelText('Role'), { target: { value: 'r-op' } });
    fireEvent.click(screen.getByText('Add mapping'));
    await waitFor(() =>
      expect(createMapping).toHaveBeenCalledWith({
        group_ref: 'Platform Team',
        group_ref_kind: 'external_id',
        role_id: 'r-op',
      })
    );
  });

  it('turns a step-up refusal into a hint', async () => {
    mockApi();
    vi.spyOn(api, 'createScimToken').mockRejectedValue({
      response: { data: { code: 'step_up_required', message: 'step up' } },
    });
    renderPage();
    fireEvent.change(await screen.findByLabelText('Token name'), { target: { value: 'x' } });
    fireEvent.click(screen.getByText('Issue token'));
    expect(await screen.findByText(/needs a second factor/)).toBeInTheDocument();
  });
});
