// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import JobsPanel from '../components/orchestration/JobsPanel';
import ParameterForm from '../components/orchestration/ParameterForm';
import { api } from '../services/api';
import { initialParamValues, parameterFields } from '../utils/orchestration';
import type { BoltJob } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const job = (over: Partial<BoltJob>): BoltJob => ({
  id: 'j1',
  kind: 'task',
  name: 'package',
  parameters: {},
  target_count: 2,
  noop: false,
  status: 'pending_approval',
  requires_approval: true,
  requested_by: 'alice',
  requested_by_id: 'u1',
  created_at: '2026-11-01T10:00:00Z',
  ...over,
});

function renderPanel(actor: { userId: string; canApprove: boolean; canCreate: boolean }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <JobsPanel actor={actor} />
    </QueryClientProvider>
  );
}

describe('JobsPanel', () => {
  beforeEach(() => {
    vi.spyOn(api, 'getBoltJobs').mockResolvedValue([
      job({ id: 'j1', name: 'mine', requested_by_id: 'u1' }),
      job({ id: 'j2', name: 'theirs', requested_by: 'bob', requested_by_id: 'u2' }),
      job({ id: 'j3', name: 'done', status: 'succeeded', requested_by_id: 'u2' }),
    ]);
  });

  it('lists jobs with status badges', async () => {
    const { container } = renderPanel({ userId: 'u1', canApprove: true, canCreate: true });
    expect(await screen.findByText('mine')).toBeInTheDocument();
    expect(screen.getByText('theirs')).toBeInTheDocument();
    expect(container.querySelectorAll('[data-status="pending_approval"]')).toHaveLength(2);
    expect(container.querySelector('[data-status="succeeded"]')).toHaveTextContent('Succeeded');
  });

  it("disables approve and reject on the requester's own job only", async () => {
    renderPanel({ userId: 'u1', canApprove: true, canCreate: true });
    await screen.findByText('mine');
    expect(screen.getByLabelText('Approve mine')).toBeDisabled();
    expect(screen.getByLabelText('Reject mine')).toBeDisabled();
    expect(screen.getByLabelText('Approve theirs')).toBeEnabled();
    expect(screen.getByLabelText('Reject theirs')).toBeEnabled();
    expect(screen.queryByLabelText('Approve done')).not.toBeInTheDocument();
  });

  it('hides approval actions without the approve permission', async () => {
    renderPanel({ userId: 'u1', canApprove: false, canCreate: true });
    await screen.findByText('mine');
    expect(screen.queryByLabelText('Approve theirs')).not.toBeInTheDocument();
  });

  it("approves another user's job", async () => {
    const approve = vi.spyOn(api, 'approveBoltJob').mockResolvedValue(job({ id: 'j2' }));
    renderPanel({ userId: 'u1', canApprove: true, canCreate: true });
    fireEvent.click(await screen.findByLabelText('Approve theirs'));
    await waitFor(() => expect(approve.mock.calls[0][0]).toBe('j2'));
  });

  it('filters by status through the API', async () => {
    renderPanel({ userId: 'u1', canApprove: true, canCreate: true });
    await screen.findByText('mine');
    fireEvent.change(screen.getByLabelText('Filter by status'), { target: { value: 'failed' } });
    await waitFor(() =>
      expect(api.getBoltJobs).toHaveBeenLastCalledWith({ status: 'failed', limit: 100 })
    );
  });

  it('shows target output as plain text, not markup', async () => {
    vi.spyOn(api, 'getBoltJob').mockResolvedValue({
      ...job({ id: 'j3', name: 'done', status: 'succeeded', requested_by_id: 'u2' }),
      targets: [
        {
          certname: 'web1',
          status: 'succeeded',
          exit_code: 0,
          output: '<img src=x onerror=alert(1)>',
        },
      ],
    });
    const { container } = renderPanel({ userId: 'u1', canApprove: true, canCreate: true });
    fireEvent.click(await screen.findByText('done'));
    expect(await screen.findByText('<img src=x onerror=alert(1)>')).toBeInTheDocument();
    expect(container.ownerDocument.querySelector('img')).toBeNull();
  });
});

describe('ParameterForm', () => {
  it('renders inputs by type and uses a password input for sensitive values', () => {
    const fields = parameterFields({
      user: { type: 'String', description: 'Account' },
      secret_value: { type: 'String', sensitive: true },
      count: { type: 'Optional[Integer]' },
      mode: { type: 'Enum[a, b]' },
      flag: { type: 'Boolean' },
      list: { type: 'Array[String]', optional: true },
    });
    render(
      <ParameterForm
        fields={fields}
        values={initialParamValues(fields)}
        errors={{ user: 'Required' }}
        onChange={() => {}}
      />
    );
    expect(screen.getByLabelText(/^secret_value/)).toHaveAttribute('type', 'password');
    expect(screen.getByLabelText(/^count/)).toHaveAttribute('type', 'number');
    expect(screen.getByLabelText(/^user/)).toHaveAttribute('type', 'text');
    expect(screen.getByLabelText(/^mode/).tagName).toBe('SELECT');
    expect(screen.getByLabelText('flag')).toHaveAttribute('type', 'checkbox');
    expect(screen.getByLabelText(/^list/).tagName).toBe('TEXTAREA');
    expect(screen.getByText('Required')).toBeInTheDocument();
  });
});
