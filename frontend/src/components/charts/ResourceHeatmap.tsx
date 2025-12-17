import { useMemo } from 'react';

interface HeatmapCell {
  day: string;
  hour: number;
  value: number;
  label: string;
}

interface ResourceHeatmapProps {
  data: Array<{
    timestamp: string;
    changes: number;
  }>;
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

export default function ResourceHeatmap({ data, title = 'Resource Changes Heatmap' }: ResourceHeatmapProps) {
  const { heatmapData, maxValue } = useMemo(() => {
    // Initialize grid with zeros
    const grid: Record<string, Record<number, number>> = {};
    DAYS.forEach((day) => {
      grid[day] = {};
      HOURS.forEach((hour) => {
        grid[day][hour] = 0;
      });
    });

    // Populate grid from data
    let max = 0;
    data.forEach((item) => {
      const date = new Date(item.timestamp);
      const day = DAYS[date.getDay()];
      const hour = date.getHours();
      grid[day][hour] += item.changes;
      max = Math.max(max, grid[day][hour]);
    });

    // Convert to array format
    const cells: HeatmapCell[] = [];
    DAYS.forEach((day) => {
      HOURS.forEach((hour) => {
        cells.push({
          day,
          hour,
          value: grid[day][hour],
          label: `${day} ${hour}:00 - ${grid[day][hour]} changes`,
        });
      });
    });

    return { heatmapData: cells, maxValue: max };
  }, [data]);

  return (
    <div className="w-full">
      <h3 className="text-lg font-semibold text-gray-900 mb-4">{title}</h3>

      {/* Legend */}
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

      {/* Heatmap Grid */}
      <div className="overflow-x-auto">
        <div className="min-w-[600px]">
          {/* Hour labels */}
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

          {/* Grid rows */}
          {DAYS.map((day) => (
            <div key={day} className="flex items-center gap-1 mb-0.5">
              <div className="w-8 text-xs text-gray-500 text-right pr-1">{day}</div>
              <div className="flex-1 flex gap-0.5">
                {HOURS.map((hour) => {
                  const cell = heatmapData.find((c) => c.day === day && c.hour === hour);
                  const value = cell?.value ?? 0;
                  return (
                    <div
                      key={`${day}-${hour}`}
                      className={`flex-1 h-4 rounded-sm ${getColorIntensity(value, maxValue)} cursor-pointer transition-transform hover:scale-110`}
                      title={cell?.label}
                    />
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Summary */}
      <div className="mt-4 text-sm text-gray-500">
        Total changes: {data.reduce((sum, d) => sum + d.changes, 0)} | Peak: {maxValue} changes/hour
      </div>
    </div>
  );
}
