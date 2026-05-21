import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../services/api';

interface ResourceHeatmapProps {
  /** How many days of history to fold into the heatmap. Defaults to 30. */
  days?: number;
  title?: string;
}

const HOURS = Array.from({ length: 24 }, (_, i) => i);
const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

function getColorIntensity(value: number, max: number): string {
  if (value === 0) return 'bg-gray-100';
  const intensity = Math.min(value / Math.max(max, 1), 1);

  if (intensity < 0.25) return 'bg-success-200';
  if (intensity < 0.5) return 'bg-success-400';
  if (intensity < 0.75) return 'bg-warning-400';
  return 'bg-danger-400';
}

export default function ResourceHeatmap({
  days = 30,
  title = 'Resource Changes Heatmap',
}: ResourceHeatmapProps) {
  // Pre-aggregated server-side from report_hourly_summary. The endpoint
  // always returns a dense 7×24 grid keyed by UTC day-of-week and hour.
  const { data: cells = [], isLoading } = useQuery({
    queryKey: ['reports', 'activity-heatmap', days],
    queryFn: () => api.getActivityHeatmap(days),
  });

  const { grid, maxValue, totalChanges } = useMemo(() => {
    const grid: Record<string, Record<number, number>> = {};
    DAYS.forEach((day) => {
      grid[day] = {};
      HOURS.forEach((hour) => {
        grid[day][hour] = 0;
      });
    });
    let max = 0;
    let total = 0;
    for (const cell of cells) {
      const dayLabel = DAYS[cell.day_of_week] ?? DAYS[0];
      // `changed` is the activity signal: count of `changed`-status
      // reports in that (dow, hour) bucket over the window. Resource-
      // level change counts aren't tracked in the summary tables.
      grid[dayLabel][cell.hour_of_day] = cell.changed;
      max = Math.max(max, cell.changed);
      total += cell.changed;
    }
    return { grid, maxValue: max, totalChanges: total };
  }, [cells]);

  return (
    <div className="w-full">
      <h3 className="text-lg font-semibold text-gray-900 mb-4">{title}</h3>

      <div className="flex items-center gap-2 mb-4 text-xs text-gray-500">
        <span>Less</span>
        <div className="flex gap-0.5">
          <div className="w-3 h-3 bg-gray-100 rounded-sm" />
          <div className="w-3 h-3 bg-success-200 rounded-sm" />
          <div className="w-3 h-3 bg-success-400 rounded-sm" />
          <div className="w-3 h-3 bg-warning-400 rounded-sm" />
          <div className="w-3 h-3 bg-danger-400 rounded-sm" />
        </div>
        <span>More</span>
      </div>

      <div className="overflow-x-auto">
        <div className="min-w-[600px]">
          <div className="flex ml-10 mb-1">
            {HOURS.filter((h) => h % 3 === 0).map((hour) => (
              <div
                key={hour}
                className="text-xs text-gray-400"
                style={{ width: `${(100 / 24) * 3}%` }}
              >
                {hour}:00
              </div>
            ))}
          </div>

          {DAYS.map((day) => (
            <div key={day} className="flex items-center gap-1 mb-0.5">
              <div className="w-8 text-xs text-gray-500 text-right pr-1">{day}</div>
              <div className="flex-1 flex gap-0.5">
                {HOURS.map((hour) => {
                  const value = grid[day][hour];
                  return (
                    <div
                      key={`${day}-${hour}`}
                      className={`flex-1 h-4 rounded-sm ${getColorIntensity(value, maxValue)} cursor-pointer transition-transform hover:scale-110`}
                      title={`${day} ${hour}:00 UTC — ${value} changed report(s)`}
                    />
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="mt-4 text-sm text-gray-500">
        {isLoading
          ? 'Loading…'
          : `Total changed reports: ${totalChanges} | Peak: ${maxValue} per UTC hour bucket (window: ${days}d)`}
      </div>
    </div>
  );
}
