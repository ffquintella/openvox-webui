import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Legend,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import type { PluginReportResult, PluginReportSection } from '../../types';
import { toText } from '../plugins/pluginHelpers';
import { chartRows } from './pluginReportHelpers';

const PALETTE = ['#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#8b5cf6', '#14b8a6', '#ec4899'];

function SectionTitle({ title }: { title: string | null }) {
  if (!title) return null;
  return <h4 className="text-sm font-semibold text-gray-900 dark:text-white mb-2">{title}</h4>;
}

function KpiSection({ section }: { section: Extract<PluginReportSection, { kind: 'kpi' }> }) {
  return (
    <div>
      <SectionTitle title={section.title} />
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {section.items.map((item, i) => (
          <div
            key={`${item.label}-${i}`}
            className="text-center p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg"
          >
            <p className="text-2xl font-bold text-gray-900 dark:text-white">
              {toText(item.value)}
              {item.unit ? (
                <span className="text-sm font-normal text-gray-500 ml-1">{item.unit}</span>
              ) : null}
            </p>
            <p className="text-sm text-gray-500 dark:text-gray-400">{item.label}</p>
          </div>
        ))}
      </div>
    </div>
  );
}

export function RowsTable({
  columns,
  rows,
}: {
  columns: { key: string; label: string }[];
  rows: Record<string, unknown>[];
}) {
  if (rows.length === 0) {
    return <p className="text-sm text-gray-500 dark:text-gray-400">No rows.</p>;
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 dark:border-gray-700">
            {columns.map((c) => (
              <th
                key={c.key}
                className="px-3 py-2 text-left font-medium text-gray-500 dark:text-gray-400"
              >
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-100 dark:divide-gray-700">
          {rows.map((row, i) => (
            <tr key={i}>
              {columns.map((c) => (
                <td key={c.key} className="px-3 py-2 text-gray-900 dark:text-gray-100">
                  {toText(row[c.key])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ChartSection({ section }: { section: Extract<PluginReportSection, { kind: 'chart' }> }) {
  const rows = chartRows(section.series);
  return (
    <div>
      <SectionTitle title={section.title} />
      <div className="h-64" data-testid={`chart-${section.id}`}>
        <ResponsiveContainer width="100%" height="100%">
          {section.chart === 'pie' ? (
            <PieChart>
              <Pie
                data={(section.series[0]?.points ?? []).map((p) => ({
                  name: String(p.x),
                  value: p.y,
                }))}
                dataKey="value"
                nameKey="name"
                outerRadius="80%"
              >
                {(section.series[0]?.points ?? []).map((_, i) => (
                  <Cell key={i} fill={PALETTE[i % PALETTE.length]} />
                ))}
              </Pie>
              <Tooltip />
              <Legend />
            </PieChart>
          ) : section.chart === 'line' ? (
            <LineChart data={rows}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="x" />
              <YAxis />
              <Tooltip />
              <Legend />
              {section.series.map((s, i) => (
                <Line key={s.name} dataKey={s.name} stroke={PALETTE[i % PALETTE.length]} />
              ))}
            </LineChart>
          ) : (
            <BarChart data={rows}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="x" />
              <YAxis />
              <Tooltip />
              <Legend />
              {section.series.map((s, i) => (
                <Bar key={s.name} dataKey={s.name} fill={PALETTE[i % PALETTE.length]} />
              ))}
            </BarChart>
          )}
        </ResponsiveContainer>
      </div>
    </div>
  );
}

function Section({ section }: { section: PluginReportSection }) {
  switch (section.kind) {
    case 'kpi':
      return <KpiSection section={section} />;
    case 'table':
      return (
        <div>
          <SectionTitle title={section.title} />
          <RowsTable columns={section.columns} rows={section.rows} />
        </div>
      );
    case 'chart':
      return <ChartSection section={section} />;
    case 'text':
      // Plain text only: React escapes it, never rendered as HTML.
      return (
        <div>
          <SectionTitle title={section.title} />
          <p className="text-sm text-gray-700 dark:text-gray-200 whitespace-pre-wrap">
            {section.text}
          </p>
        </div>
      );
    default:
      return null;
  }
}

/** Generic renderer for the sections of a plugin report result. */
export default function PluginReportView({ result }: { result: PluginReportResult }) {
  const sections = Array.isArray(result.sections) ? result.sections : [];
  return (
    <div className="space-y-6">
      <div className="text-sm text-gray-500 dark:text-gray-400">
        {result.title && (
          <p className="text-base font-semibold text-gray-900 dark:text-white">{result.title}</p>
        )}
        <p>
          Plugin <span className="font-mono">{result.plugin_id}</span>
          {result.generated_at
            ? ` | generated ${new Date(result.generated_at).toLocaleString()}`
            : ''}
          {` | ${result.row_count} rows`}
        </p>
        {result.truncated && (
          <p role="status" className="text-amber-700 dark:text-amber-400">
            The result was truncated; export it or narrow the parameters to see everything.
          </p>
        )}
      </div>
      {sections.length === 0 && (
        <p className="text-sm text-gray-500 dark:text-gray-400">The report has no sections.</p>
      )}
      {sections.map((s) => (
        <Section key={s.id} section={s} />
      ))}
    </div>
  );
}
