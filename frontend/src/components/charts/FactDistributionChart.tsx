import { useMemo } from 'react';
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Cell,
} from 'recharts';

interface FactDistributionChartProps {
  facts: Array<{
    certname: string;
    name: string;
    value: unknown;
  }>;
  factName: string;
  title?: string;
  maxBars?: number;
}

const COLORS = [
  '#3b82f6', // blue
  '#22c55e', // green
  '#f59e0b', // amber
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#06b6d4', // cyan
  '#f97316', // orange
  '#84cc16', // lime
  '#ef4444', // red
  '#14b8a6', // teal
];

function CustomTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: Array<{ value: number }>;
  label?: string;
}) {
  if (!active || !payload || !payload.length) return null;

  return (
    <div className="bg-white shadow-lg rounded-lg p-3 border border-gray-200">
      <p className="font-semibold text-gray-900">{label}</p>
      <p className="text-sm text-gray-600">{payload[0].value} nodes</p>
    </div>
  );
}

export default function FactDistributionChart({
  facts,
  factName,
  title,
  maxBars = 10,
}: FactDistributionChartProps) {
  const chartData = useMemo(() => {
    // Filter facts by name and count values
    const valueCounts = new Map<string, number>();

    facts
      .filter((f) => f.name === factName)
      .forEach((fact) => {
        const value = typeof fact.value === 'object'
          ? JSON.stringify(fact.value)
          : String(fact.value);

        valueCounts.set(value, (valueCounts.get(value) || 0) + 1);
      });

    // Convert to array and sort by count
    const data = Array.from(valueCounts.entries())
      .map(([value, count]) => ({
        value: value.length > 20 ? `${value.slice(0, 17)}...` : value,
        fullValue: value,
        count,
      }))
      .sort((a, b) => b.count - a.count)
      .slice(0, maxBars);

    return data;
  }, [facts, factName, maxBars]);

  const totalNodes = chartData.reduce((sum, d) => sum + d.count, 0);
  const uniqueValues = chartData.length;

  if (chartData.length === 0) {
    return (
      <div className="w-full">
        <h3 className="text-lg font-semibold text-gray-900 mb-4">
          {title || `Distribution of ${factName}`}
        </h3>
        <div className="h-64 flex items-center justify-center text-gray-500">
          <p>No data available for fact: {factName}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="w-full">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold text-gray-900">
          {title || `Distribution of ${factName}`}
        </h3>
        <span className="text-sm text-gray-500">
          {uniqueValues} values | {totalNodes} nodes
        </span>
      </div>

      <div className="h-64">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={chartData}
            layout="vertical"
            margin={{ top: 5, right: 30, left: 80, bottom: 5 }}
          >
            <CartesianGrid strokeDasharray="3 3" horizontal={true} vertical={false} />
            <XAxis type="number" />
            <YAxis
              type="category"
              dataKey="value"
              tick={{ fontSize: 12 }}
              width={75}
            />
            <Tooltip content={<CustomTooltip />} />
            <Bar dataKey="count" radius={[0, 4, 4, 0]}>
              {chartData.map((_, index) => (
                <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>

      {/* Value breakdown */}
      <div className="mt-4 space-y-1">
        {chartData.slice(0, 5).map((item, index) => (
          <div key={item.fullValue} className="flex items-center justify-between text-sm">
            <div className="flex items-center gap-2">
              <div
                className="w-3 h-3 rounded-sm"
                style={{ backgroundColor: COLORS[index % COLORS.length] }}
              />
              <span className="text-gray-700">{item.value}</span>
            </div>
            <span className="text-gray-500">
              {item.count} ({((item.count / totalNodes) * 100).toFixed(1)}%)
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
