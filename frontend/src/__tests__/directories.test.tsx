// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import Directories from '../pages/Directories';
import { api } from '../services/api';
import type { DirectoryInfo, DirectoryRoleMapping, Role } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const directory: DirectoryInfo = {
  id: 'corp',
  label: 'Corporate account',
  kind: 'active_directory',
  enabled: true,
  servers: ['ldaps://dc1.example.com'],
  start_tls: false,
  ca_file: null,
  bind_dn: 'CN=svc,DC=example,DC=com',
  base_dn: 'DC=example,DC=com',
  user_filter: '(sAMAccountName={u})',
  domains: ['EXAMPLE'],
  attributes: {
    username: 'sAMAccountName',
    email: 'mail',
    display_name: 'displayName',
    id: ['objectGUID'],
  },
  groups: { base_dn: 'DC=example,DC=com', filter: '(objectClass=group)', nested: true },
  modes: { lookup: true, jit: true, sync: { enabled: true, interval_secs: 3600 } },
  deny_without_role: true,
  sync_running: false,
  last_sync: null,
  mappings: 1,
};

const mapping: DirectoryRoleMapping = {
  id: 'm1',
  directory_id: 'corp',
  group_ref: 'cn=ops,dc=example,dc=com',
  group_ref_kind: 'dn',
  group_label: 'Ops',
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
      <Directories />
    </QueryClientProvider>
  );
}

describe('Directories page', () => {
  it('shows directories, mappings and history, and never offers super_admin', async () => {
    vi.spyOn(api, 'getDirectories').mockResolvedValue([directory]);
    vi.spyOn(api, 'getRoles').mockResolvedValue(roles);
    vi.spyOn(api, 'getDirectoryRoleMappings').mockResolvedValue([mapping]);
    vi.spyOn(api, 'getDirectorySyncRuns').mockResolvedValue([
      {
        id: 'run1',
        directory_id: 'corp',
        trigger_kind: 'schedule',
        status: 'ok',
        started_at: '2026-10-30T10:00:00Z',
        created: 2,
        updated: 1,
        deactivated: 1,
        reactivated: 0,
        errors: 0,
      },
    ]);
    renderPage();
    expect(await screen.findByText('Corporate account')).toBeInTheDocument();
    expect(await screen.findByText('Ops')).toBeInTheDocument();
    expect(await screen.findByText('schedule')).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: 'Super Administrator' })).toBeNull();
    expect(screen.getByRole('option', { name: 'Operator' })).toBeInTheDocument();
  });

  it('tests the connection and explains a step-up refusal', async () => {
    vi.spyOn(api, 'getDirectories').mockResolvedValue([directory]);
    vi.spyOn(api, 'getRoles').mockResolvedValue(roles);
    vi.spyOn(api, 'getDirectoryRoleMappings').mockResolvedValue([mapping]);
    vi.spyOn(api, 'getDirectorySyncRuns').mockResolvedValue([]);
    vi.spyOn(api, 'testDirectory').mockResolvedValue({
      ok: true,
      server: 'ldaps://dc1.example.com',
      elapsed_ms: 12,
      users_found: true,
      groups_found: true,
    });
    const remove = vi.spyOn(api, 'deleteDirectoryRoleMapping').mockRejectedValue({
      response: { data: { code: 'step_up_required', message: 'needs level 2' } },
    });
    renderPage();
    fireEvent.click(await screen.findByText('Test connection'));
    expect(await screen.findByText(/Connected to ldaps:\/\/dc1.example.com/)).toBeInTheDocument();
    fireEvent.click(await screen.findByLabelText('Remove mapping of Ops'));
    await waitFor(() => expect(remove).toHaveBeenCalledWith('corp', 'm1'));
    expect(await screen.findByText(/needs a second factor/)).toBeInTheDocument();
  });

  it('explains how to configure a directory when there is none', async () => {
    vi.spyOn(api, 'getDirectories').mockResolvedValue([]);
    vi.spyOn(api, 'getRoles').mockResolvedValue([]);
    renderPage();
    expect(await screen.findByText(/No directory is configured/)).toBeInTheDocument();
  });
});
