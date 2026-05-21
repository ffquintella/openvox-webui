import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import { format } from 'date-fns';
import { api } from '../../services/api';

interface TimeSeriesMetricsProps {
  title?: string;
}

type TimeRange = '24h' | '7d' | '30d';

interface DataPoint {
  timestamp: string;
  label: string;
  changed: number;
  unchanged: number;
  failed: number;
  total: number;
}

const COLORS = {
  changed: '#22c55e',
  unchanged: '#3b82f6',
  failed: '#ef4444',
};

// Hours requested from the backend for each range. The hourly summary
// endpoint returns one row per UTC hour; we aggregate to days on the
// client for the 7d and 30d views.
const HOURS_FOR_RANGE: Record<TimeRange, number> = {
  '24h': 24,
  '7d': 24 * 7,
  '30d': 24 * 30,
};

function CustomTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: Array<{ name: string; value: number; color: string }>;
  label?: string;
}) {
  if (!active || !payload || !payload.length) return null;

  return (
    <div className="bg-white shadow-lg rounded-lg p-3 border border-gray-200">
      <p className="font-semibold text-gray-900 mb-2">{label}</p>
      {payload.map((entry) => (
        <div key={entry.name} className="flex items-center justify-between gap-4 text-sm">
          <div className="flex items-center gap-2">
            <div
              className="w-3 h-3 rounded-sm"
              style={{ backgroundColor: entry.color }}
            />
            <span className="text-gray-600 capitalize">{entry.name}</span>
          </div>
          <span className="font-medium text-gray-900">{entry.value}</span>
        </div>
      ))}
      <div className="mt-2 pt-2 border-t border-gray-100 flex items-center justify-between text-sm">
        <span className="text-gray-600">Total</span>
        <span className="font-medium text-gray-900">
          {payload.reduce((sum, p) => sum + p.value, 0)}
        </span>
      </div>
    </div>
  );
}

export default function TimeSeriesMetrics({
  title = 'Report Metrics Over Time',
}: TimeSeriesMetricsProps) {
  const [timeRange, setTimeRange] = useState<TimeRange>('7d');
  const [showStacked, setShowStacked] = useState(true);

  // Hourly summary is small (24 / 168 / 720 rows max) and is the
  // pre-aggregated source of truth populated by the backend scheduler.
  const { data: hourly = [], isLoading } = useQuery({
    queryKey: ['reports', 'hourly-summary', timeRange],
    queryFn: () => api.getReportHourlySummary(HOURS_FOR_RANGE[timeRange]),
  });

  const chartData = useMemo<DataPoint[]>(() => {
    if (timeRange === '24h') {
      // One row per hour, label as local HH:mm so the user reads it in
      // their own timezone even though the bucket is UTC.
      return hourly.map((row) => {
        const t = new Date(row.hour);
        return {
          timestamp: row.hour,
          label: format(t, 'HH:mm'),
          changed: row.changed,
          unchanged: row.unchanged,
          failed: row.failed,
          total: row.total,
        };
      });
    }

    // 7d / 30d: collapse hourly rows into UTC day buckets so the X axis
    // matches what the user thinks of as "a day". We key off the UTC date
    // portion of the row's hour timestamp.
    const byDay = new Map<string, DataPoint>();
    for (const row of hourly) {
      const dayKey = row.hour.slice(0, 10); // YYYY-MM-DD
      const existing = byDay.get(dayKey);
      if (existing) {
        existing.changed += row.changed;
        existing.unchanged += row.unchanged;
        existing.failed += row.failed;
        existing.total += row.total;
      } else {
        byDay.set(dayKey, {
          timestamp: `${dayKey}T00:00:00Z`,
          label: '',
          changed: row.changed,
          unchanged: row.unchanged,
          failed: row.failed,
          total: row.total,
        });
      }
    }
    const formatStr = timeRange === '7d' ? 'EEE' : 'MMM d';
    return Array.from(byDay.values())
      .sort((a, b) => a.timestamp.localeCompare(b.timestamp))
      .map((d) => ({ ...d, label: format(new Date(d.timestamp), formatStr) }));
  }, [hourly, timeRange]);

  const totals = useMemo(() => {
    return chartData.reduce(
      (acc, d) => ({
        changed: acc.changed + d.changed,
        unchanged: acc.unchanged + d.unchanged,
        failed: acc.failed + d.failed,
        total: acc.total + d.total,
      }),
      { changed: 0, unchanged: 0, failed: 0, total: 0 }
    );
  }, [chartData]);

  return (
    <div className="w-full">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold text-gray-900">{title}</h3>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-gray-600">
            <input
              type="checkbox"
              checked={showStacked}
              onChange={(e) => setShowStacked(e.target.checked)}
              className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
            />
            Stacked
          </label>

          <div className="flex rounded-lg border border-gray-300 overflow-hidden">
            {(['24h', '7d', '30d'] as TimeRange[]).map((range) => (
              <button
                key={range}
                onClick={() => setTimeRange(range)}
                className={`px-3 py-1 text-sm ${
                  timeRange === range
                    ? 'bg-primary-500 text-white'
                    : 'bg-white text-gray-600 hover:bg-gray-50'
                }`}
              >
                {range}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-4 gap-4 mb-4">
        <div className="text-center">
          <p className="text-2xl font-bold text-gray-900">{totals.total}</p>
          <p className="text-xs text-gray-500">Total Reports</p>
        </div>
        <div className="text-center">
          <p className="text-2xl font-bold text-success-600">{totals.changed}</p>
          <p className="text-xs text-gray-500">Changed</p>
        </div>
        <div className="text-center">
          <p className="text-2xl font-bold text-primary-600">{totals.unchanged}</p>
          <p className="text-xs text-gray-500">Unchanged</p>
        </div>
        <div className="text-center">
          <p className="text-2xl font-bold text-danger-600">{totals.failed}</p>
          <p className="text-xs text-gray-500">Failed</p>
        </div>
      </div>

      <div className="h-64">
        {isLoading ? (
          <div className="h-full flex items-center justify-center text-sm text-gray-500">
            Loading…
          </div>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={chartData} margin={{ top: 10, right: 30, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="colorChanged" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor={COLORS.changed} stopOpacity={0.8} />
                  <stop offset="95%" stopColor={COLORS.changed} stopOpacity={0.1} />
                </linearGradient>
                <linearGradient id="colorUnchanged" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor={COLORS.unchanged} stopOpacity={0.8} />
                  <stop offset="95%" stopColor={COLORS.unchanged} stopOpacity={0.1} />
                </linearGradient>
                <linearGradient id="colorFailed" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor={COLORS.failed} stopOpacity={0.8} />
                  <stop offset="95%" stopColor={COLORS.failed} stopOpacity={0.1} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="label" tick={{ fontSize: 12 }} tickLine={false} />
              <YAxis tick={{ fontSize: 12 }} tickLine={false} />
              <Tooltip content={<CustomTooltip />} />
              <Legend />
              <Area
                type="linear"
                dataKey="failed"
                stackId={showStacked ? '1' : undefined}
                stroke={COLORS.failed}
                fill="url(#colorFailed)"
                name="Failed"
              />
              <Area
                type="linear"
                dataKey="changed"
                stackId={showStacked ? '1' : undefined}
                stroke={COLORS.changed}
                fill="url(#colorChanged)"
                name="Changed"
              />
              <Area
                type="linear"
                dataKey="unchanged"
                stackId={showStacked ? '1' : undefined}
                stroke={COLORS.unchanged}
                fill="url(#colorUnchanged)"
                name="Unchanged"
              />
            </AreaChart>
          </ResponsiveContainer>
        )}
      </div>

      {totals.total > 0 && (
        <div className="mt-4 p-3 bg-gray-50 rounded-lg text-sm text-gray-600">
          <p>
            <span className="font-medium">Success rate:</span>{' '}
            {(((totals.changed + totals.unchanged) / totals.total) * 100).toFixed(1)}%
            {totals.failed > 0 && (
              <span className="text-danger-600 ml-2">({totals.failed} failures)</span>
            )}
          </p>
          <p>
            <span className="font-medium">
              Average reports per {timeRange === '24h' ? 'hour' : 'day'}:
            </span>{' '}
            {(totals.total / Math.max(chartData.length, 1)).toFixed(1)}
          </p>
        </div>
      )}
    </div>
  );
}
