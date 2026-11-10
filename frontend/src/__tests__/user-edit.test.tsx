// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import Users from '../pages/Users';
import { api, pluginsApi } from '../services/api';
import type { UserResponse } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const local: UserResponse = {
  id: 'u1',
  username: 'alice',
  email: 'alice@example.com',
  role: 'viewer',
  auth_provider: 'local',
  roles: [],
  created_at: '2026-01-01T00:00:00Z',
};
const sso: UserResponse = { ...local, id: 'u2', username: 'bob', auth_provider: 'saml' };
const managed: UserResponse = { ...local, id: 'u3', username: 'carol', directory_id: 'ad' };

function renderPage(users: UserResponse[]) {
  vi.spyOn(api, 'getUsers').mockResolvedValue(users);
  vi.spyOn(api, 'getRoles').mockResolvedValue([]);
  vi.spyOn(pluginsApi, 'getUiSlots').mockResolvedValue({ slots: {}, menu_ops: [] });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Users />
    </QueryClientProvider>
  );
}

async function open(name: string) {
  fireEvent.click(await screen.findByText(name));
}

describe('editing a user', () => {
  it('sends only the changed details', async () => {
    renderPage([local]);
    const update = vi.spyOn(api, 'updateUser').mockResolvedValue({ ...local, username: 'alice2' });
    await open('alice');
    fireEvent.click(await screen.findByRole('button', { name: /edit/i }));
    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'alice2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(update).toHaveBeenCalledWith('u1', { username: 'alice2' }));
  });

  it('forces a password change with a temporary password', async () => {
    renderPage([local]);
    const update = vi.spyOn(api, 'updateUser').mockResolvedValue(local);
    await open('alice');
    fireEvent.click(await screen.findByRole('button', { name: /edit/i }));
    fireEvent.change(screen.getByLabelText('Temporary password'), {
      target: { value: 'temporary-pass-1' },
    });
    expect(screen.getByLabelText(/require a password change/i)).toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(update).toHaveBeenCalledWith('u1', {
        password: 'temporary-pass-1',
        force_password_change: true,
      })
    );
  });

  it('shows the server error and keeps the form open', async () => {
    renderPage([local]);
    vi.spyOn(api, 'updateUser').mockRejectedValue({
      response: { data: { message: 'Email already exists' } },
    });
    await open('alice');
    fireEvent.click(await screen.findByRole('button', { name: /edit/i }));
    fireEvent.change(screen.getByLabelText('E-mail'), { target: { value: 'x@example.com' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Email already exists');
  });

  it('offers no password options for SSO-only users', async () => {
    renderPage([sso]);
    await open('bob');
    fireEvent.click(await screen.findByRole('button', { name: /edit/i }));
    expect(screen.queryByLabelText('Temporary password')).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/require a password change/i)).not.toBeInTheDocument();
  });

  it('keeps directory-managed users read-only', async () => {
    renderPage([managed]);
    await open('carol');
    await screen.findByText(/managed by the directory/i);
    expect(screen.queryByRole('button', { name: /edit/i })).not.toBeInTheDocument();
  });
});
