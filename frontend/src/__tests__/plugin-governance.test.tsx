// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import GovernanceTab from '../components/plugins/GovernanceTab';
import { formatMs, formatRate } from '../components/plugins/governanceHelpers';
import { pluginsApi } from '../services/api';
import type { PluginGovernanceOverview, PluginUsageTotals } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const totals = (over: Partial<PluginUsageTotals> = {}): PluginUsageTotals => ({
  calls: 0,
  errors: 0,
  traps: 0,
  timeouts: 0,
  quota_rejections: 0,
  circuit_rejections: 0,
  error_rate: 0,
  avg_ms: null,
  p50_ms: null,
  p95_ms: null,
  p99_ms: null,
  max_ms: 0,
  ...over,
});

const overview: PluginGovernanceOverview = {
  open_circuits: 1,
  top_consumers: ['flaky'],
  generated_at: '2026-10-30T10:00:00Z',
  plugins: [
    {
      id: 'flaky',
      name: 'Flaky',
      version: '1.0.0',
      state: 'error',
      state_reason: 'circuit_open',
      health: 'circuit_open',
      circuit: { state: 'open', trips: 2, retry_at: null, last_error: 'boom' },
      invocations_per_minute_limit: 60,
      last_hour: totals({ calls: 10, errors: 5, error_rate: 0.5, p95_ms: 1500 }),
      last_24h: totals({ calls: 40, errors: 8, error_rate: 0.2 }),
      revoked_versions: [],
    },
    {
      id: 'calm',
      name: 'Calm',
      version: '2.0.0',
      state: 'enabled',
      state_reason: null,
      health: 'healthy',
      circuit: { state: 'closed', trips: 0, retry_at: null, last_error: null },
      invocations_per_minute_limit: null,
      last_hour: totals({ calls: 3 }),
      last_24h: totals({ calls: 9 }),
      revoked_versions: [],
    },
  ],
};

function renderTab(canAdmin: boolean) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <GovernanceTab canAdmin={canAdmin} />
    </QueryClientProvider>
  );
}

describe('formatting', () => {
  it('formats durations and rates', () => {
    expect(formatMs(null)).toBe('-');
    expect(formatMs(250)).toBe('250 ms');
    expect(formatMs(1500)).toBe('1.5 s');
    expect(formatRate(0)).toBe('0%');
    expect(formatRate(0.004)).toBe('<1%');
    expect(formatRate(0.5)).toBe('50%');
  });
});

describe('GovernanceTab', () => {
  it('shows health, breaker state and lets an administrator reset a tripped plugin', async () => {
    vi.spyOn(pluginsApi, 'governance').mockResolvedValue(overview);
    const reset = vi
      .spyOn(pluginsApi, 'resetCircuit')
      .mockResolvedValue({ state: 'closed', trips: 0, retry_at: null, last_error: null });
    renderTab(true);
    expect(await screen.findByText('Isolated')).toBeInTheDocument();
    expect(screen.getByText('Healthy')).toBeInTheDocument();
    expect(screen.getByText('boom')).toBeInTheDocument();
    expect(screen.getByText('unlimited')).toBeInTheDocument();
    const buttons = screen.getAllByRole('button', { name: /reset/i });
    expect(buttons).toHaveLength(1);
    fireEvent.click(buttons[0]);
    await waitFor(() => expect(reset).toHaveBeenCalledWith('flaky'));
  });

  it('hides the reset action without the admin permission', async () => {
    vi.spyOn(pluginsApi, 'governance').mockResolvedValue(overview);
    renderTab(false);
    await screen.findByText('Isolated');
    expect(screen.queryByRole('button', { name: /reset/i })).not.toBeInTheDocument();
  });
});
