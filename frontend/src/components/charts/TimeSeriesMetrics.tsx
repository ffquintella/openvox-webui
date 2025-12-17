import { useMemo, useState } from 'react';
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
import { format, subDays, subHours, startOfDay, startOfHour, eachDayOfInterval, eachHourOfInterval } from 'date-fns';
import type { Report } from '../../types';

interface TimeSeriesMetricsProps {
  reports: Report[];
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
  reports,
  title = 'Report Metrics Over Time',
}: TimeSeriesMetricsProps) {
  const [timeRange, setTimeRange] = useState<TimeRange>('7d');
  const [showStacked, setShowStacked] = useState(true);

  const chartData = useMemo(() => {
    const now = new Date();
    let intervals: Date[];
    let formatStr: string;

    switch (timeRange) {
      case '24h':
        intervals = eachHourOfInterval({
          start: subHours(now, 24),
          end: now,
        });
        formatStr = 'HH:mm';
        break;
      case '7d':
        intervals = eachDayOfInterval({
          start: subDays(now, 7),
          end: now,
        });
        formatStr = 'EEE';
        break;
      case '30d':
        intervals = eachDayOfInterval({
          start: subDays(now, 30),
          end: now,
        });
        formatStr = 'MMM d';
        break;
      default:
        intervals = [];
        formatStr = '';
    }

    const data: DataPoint[] = intervals.map((interval) => {
      const intervalStart = timeRange === '24h' ? startOfHour(interval) : startOfDay(interval);
      const intervalEnd = timeRange === '24h'
        ? startOfHour(new Date(interval.getTime() + 60 * 60 * 1000))
        : startOfDay(new Date(interval.getTime() + 24 * 60 * 60 * 1000));

      const intervalReports = reports.filter((r) => {
        if (!r.start_time) return false;
        const reportTime = new Date(r.start_time);
        return reportTime >= intervalStart && reportTime < intervalEnd;
      });

      return {
        timestamp: interval.toISOString(),
        label: format(interval, formatStr),
        changed: intervalReports.filter((r) => r.status === 'changed').length,
        unchanged: intervalReports.filter((r) => r.status === 'unchanged').length,
        failed: intervalReports.filter((r) => r.status === 'failed').length,
        total: intervalReports.length,
      };
    });

    return data;
  }, [reports, timeRange]);

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
          {/* Stacked toggle */}
          <label className="flex items-center gap-2 text-sm text-gray-600">
            <input
              type="checkbox"
              checked={showStacked}
              onChange={(e) => setShowStacked(e.target.checked)}
              className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
            />
            Stacked
          </label>

          {/* Time range selector */}
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

      {/* Summary stats */}
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

      {/* Chart */}
      <div className="h-64">
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
            <XAxis
              dataKey="label"
              tick={{ fontSize: 12 }}
              tickLine={false}
            />
            <YAxis tick={{ fontSize: 12 }} tickLine={false} />
            <Tooltip content={<CustomTooltip />} />
            <Legend />
            <Area
              type="monotone"
              dataKey="failed"
              stackId={showStacked ? '1' : undefined}
              stroke={COLORS.failed}
              fill="url(#colorFailed)"
              name="Failed"
            />
            <Area
              type="monotone"
              dataKey="changed"
              stackId={showStacked ? '1' : undefined}
              stroke={COLORS.changed}
              fill="url(#colorChanged)"
              name="Changed"
            />
            <Area
              type="monotone"
              dataKey="unchanged"
              stackId={showStacked ? '1' : undefined}
              stroke={COLORS.unchanged}
              fill="url(#colorUnchanged)"
              name="Unchanged"
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>

      {/* Insights */}
      {totals.total > 0 && (
        <div className="mt-4 p-3 bg-gray-50 rounded-lg text-sm text-gray-600">
          <p>
            <span className="font-medium">Success rate:</span>{' '}
            {(((totals.changed + totals.unchanged) / totals.total) * 100).toFixed(1)}%
            {totals.failed > 0 && (
              <span className="text-danger-600 ml-2">
                ({totals.failed} failures)
              </span>
            )}
          </p>
          <p>
            <span className="font-medium">Average reports per{' '}
            {timeRange === '24h' ? 'hour' : 'day'}:</span>{' '}
            {(totals.total / chartData.length).toFixed(1)}
          </p>
        </div>
      )}
    </div>
  );
}
