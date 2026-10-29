import type { PluginCircuitState, PluginHealth } from '../../types';

export const HEALTH_LABELS: Record<PluginHealth, string> = {
  healthy: 'Healthy',
  degraded: 'Degraded',
  circuit_open: 'Isolated',
  disabled: 'Not running',
  error: 'Error',
};

export const HEALTH_STYLES: Record<PluginHealth, string> = {
  healthy: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300',
  degraded: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300',
  circuit_open: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300',
  disabled: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300',
  error: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300',
};

export const CIRCUIT_LABELS: Record<PluginCircuitState, string> = {
  closed: 'Closed',
  half_open: 'Half open (probing)',
  open: 'Open',
};

export function formatMs(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '-';
  return ms >= 1000 ? `${(ms / 1000).toFixed(ms >= 10_000 ? 0 : 1)} s` : `${ms} ms`;
}

export function formatRate(rate: number): string {
  if (rate <= 0) return '0%';
  const pct = rate * 100;
  return pct < 1 ? '<1%' : `${Math.round(pct)}%`;
}

/** Hour label of a rollup bucket ("14:00") in the viewer's time zone. */
export function formatHour(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}
