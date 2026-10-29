import { ArrowDown, ArrowUp } from 'lucide-react';
import clsx from 'clsx';
import { useReportAnnotations } from '../../hooks/useAnalytics';
import type { ReportAnnotation, ReportAnnotationColumn } from '../../types';
import { inputClass } from '../plugins/pluginHelpers';
import {
  annotationColumnKey,
  findAnnotation,
  formatAnnotationValue,
  type AnnotationMap,
} from '../analytics/pluginReportHelpers';

const SEVERITY_STYLES: Record<string, string> = {
  info: 'bg-blue-100 text-blue-800',
  low: 'bg-green-100 text-green-800',
  medium: 'bg-yellow-100 text-yellow-800',
  high: 'bg-orange-100 text-orange-800',
  critical: 'bg-red-100 text-red-800',
};

export function SeverityBadge({ severity }: { severity: string | null | undefined }) {
  if (!severity) return null;
  return (
    <span
      className={clsx(
        'px-2 py-0.5 text-xs rounded-full font-medium',
        SEVERITY_STYLES[severity] ?? 'bg-gray-100 text-gray-700'
      )}
    >
      {severity}
    </span>
  );
}

export interface AnnotationSort {
  column: string;
  direction: 'asc' | 'desc';
}

/** Sort buttons for sortable columns and filter inputs for filterable ones. */
export function AnnotationToolbar({
  columns,
  sort,
  onSort,
  filters,
  onFilter,
}: {
  columns: ReportAnnotationColumn[];
  sort: AnnotationSort | null;
  onSort: (sort: AnnotationSort | null) => void;
  filters: Record<string, string>;
  onFilter: (key: string, text: string) => void;
}) {
  const interactive = columns.filter((c) => c.sortable || c.filterable);
  if (interactive.length === 0) return null;
  return (
    <div
      role="group"
      className="flex flex-wrap items-end gap-4 mb-4"
      aria-label="Plugin annotation columns"
    >
      {interactive.map((c) => {
        const key = annotationColumnKey(c);
        const active = sort?.column === key;
        return (
          <div key={key} className="flex items-end gap-2">
            {c.filterable && (
              <div>
                <label htmlFor={`annotation-filter-${key}`} className="block text-xs text-gray-500">
                  Filter {c.label}
                </label>
                <input
                  id={`annotation-filter-${key}`}
                  type="text"
                  value={filters[key] ?? ''}
                  onChange={(e) => onFilter(key, e.target.value)}
                  className={clsx(inputClass, 'w-40')}
                />
              </div>
            )}
            {c.sortable && (
              <button
                type="button"
                aria-label={`Sort by ${c.label}`}
                aria-pressed={active}
                onClick={() =>
                  onSort(
                    !active
                      ? { column: key, direction: 'asc' }
                      : sort.direction === 'asc'
                        ? { column: key, direction: 'desc' }
                        : null
                  )
                }
                className={clsx(
                  'flex items-center gap-1 px-3 py-2 text-sm border rounded-lg',
                  active
                    ? 'border-primary-300 bg-primary-50 text-primary-700'
                    : 'border-gray-300 text-gray-700 hover:bg-gray-50'
                )}
              >
                {c.label}
                {active &&
                  (sort.direction === 'asc' ? (
                    <ArrowUp className="w-3 h-3" />
                  ) : (
                    <ArrowDown className="w-3 h-3" />
                  ))}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** One extra field per list column, shown in a report row. */
export function AnnotationFields({
  columns,
  annotations,
  hash,
}: {
  columns: ReportAnnotationColumn[];
  annotations: AnnotationMap | undefined;
  hash: string;
}) {
  return (
    <>
      {columns.map((c) => {
        const a = findAnnotation(annotations, hash, c);
        const text = a ? formatAnnotationValue(a.value) : '';
        return (
          <div key={annotationColumnKey(c)} data-testid={`annotation-${annotationColumnKey(c)}`}>
            <span className="text-gray-500">{c.label}: </span>
            <span className="font-medium">{text || '-'}</span>
            {a?.severity && (
              <span className="ml-1">
                <SeverityBadge severity={a.severity} />
              </span>
            )}
          </div>
        );
      })}
    </>
  );
}

/** "Plugin annotations" panel of the report detail; renders nothing when there is none. */
export function AnnotationsPanel({ hash }: { hash: string }) {
  const { data } = useReportAnnotations(hash);
  const items: ReportAnnotation[] = (data ?? []).filter(
    (a) => Array.isArray(a.show_in) && a.show_in.includes('detail_panel')
  );
  if (items.length === 0) return null;
  return (
    <div className="bg-white rounded-lg p-4 border border-gray-100">
      <p className="text-sm font-medium text-gray-700 mb-3">Plugin annotations</p>
      <ul className="divide-y divide-gray-100">
        {items.map((a) => (
          <li
            key={`${a.plugin_id}:${a.key}`}
            className="py-2 flex items-center justify-between gap-3 text-sm"
          >
            <span className="text-gray-500">{a.label}</span>
            <span className="flex items-center gap-2 min-w-0">
              <span className="font-medium text-gray-900 break-all">
                {formatAnnotationValue(a.value) || '-'}
              </span>
              <SeverityBadge severity={a.severity} />
              <span className="text-xs text-gray-400 font-mono">{a.plugin_id}</span>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
