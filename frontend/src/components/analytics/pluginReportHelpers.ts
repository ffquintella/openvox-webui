import type {
  DatasetColumn,
  ExportFormatInfo,
  JsonSchema,
  PluginReportSeries,
  ReportAnnotation,
  ReportAnnotationColumn,
  AnnotationShowIn,
  ReportTypeInfo,
} from '../../types';
import { toText } from '../plugins/pluginHelpers';

const PLUGIN_PREFIX = 'plugin:';

/** Plugin report type ids look like `plugin:<plugin-id>:<type-id>`. */
export function isPluginReportType(id: string | null | undefined): boolean {
  return typeof id === 'string' && id.startsWith(PLUGIN_PREFIX);
}

export function parsePluginTypeId(id: string): { pluginId: string; typeId: string } | null {
  if (!isPluginReportType(id)) return null;
  const rest = id.slice(PLUGIN_PREFIX.length);
  const idx = rest.indexOf(':');
  if (idx <= 0 || idx === rest.length - 1) return null;
  return { pluginId: rest.slice(0, idx), typeId: rest.slice(idx + 1) };
}

const CORE_REPORT_LABELS: Record<string, string> = {
  node_health: 'Node Health',
  compliance: 'Compliance',
  change_tracking: 'Change Tracking',
  drift_detection: 'Drift Detection',
  custom: 'Custom',
};

/** Human label for a report type id, using the server catalog for plugin types. */
export function reportTypeLabel(id: string, types?: ReportTypeInfo[]): string {
  const known = types?.find((t) => t.id === id);
  if (known) return known.label;
  if (CORE_REPORT_LABELS[id]) return CORE_REPORT_LABELS[id];
  const parsed = parsePluginTypeId(id);
  return parsed ? `${parsed.typeId} (${parsed.pluginId})` : id;
}

// ---------------------------------------------------------------------------
// Parameter forms
// ---------------------------------------------------------------------------

export type ParamFieldKind = 'string' | 'integer' | 'number' | 'boolean' | 'enum' | 'json';

export interface ParamField {
  name: string;
  kind: ParamFieldKind;
  /** Underlying schema type (for enums, the type of the values). */
  baseType: string | null;
  title: string;
  description: string | null;
  required: boolean;
  options: string[];
  format: 'date' | 'date-time' | null;
  defaultValue: unknown;
  minimum: number | null;
  maximum: number | null;
}

/** Raw form values: text inputs hold strings, checkboxes hold booleans. */
export type ParamValues = Record<string, unknown>;

const SIMPLE_TYPES = new Set(['string', 'integer', 'number', 'boolean']);

export function schemaToFields(schema: JsonSchema | null | undefined): ParamField[] {
  const props = schema?.properties;
  if (!props || typeof props !== 'object') return [];
  const required = new Set(Array.isArray(schema?.required) ? schema.required : []);
  return Object.entries(props).map(([name, prop]) => {
    const p = prop ?? {};
    const type = typeof p.type === 'string' ? p.type : null;
    const options = Array.isArray(p.enum) ? p.enum.map((v) => toText(v)) : [];
    let kind: ParamFieldKind;
    if (options.length > 0) kind = 'enum';
    else if (type && SIMPLE_TYPES.has(type)) kind = type as ParamFieldKind;
    else kind = 'json';
    return {
      name,
      kind,
      baseType: type,
      title: p.title || name,
      description: p.description ?? null,
      required: required.has(name),
      options,
      format: p.format === 'date' || p.format === 'date-time' ? p.format : null,
      defaultValue: p.default,
      minimum: typeof p.minimum === 'number' ? p.minimum : null,
      maximum: typeof p.maximum === 'number' ? p.maximum : null,
    };
  });
}

/** Form values seeded from schema defaults, overlaid with already stored params. */
export function initialParamValues(
  schema: JsonSchema | null | undefined,
  stored?: Record<string, unknown>
): ParamValues {
  const values: ParamValues = {};
  for (const f of schemaToFields(schema)) {
    const src = stored && f.name in stored ? stored[f.name] : f.defaultValue;
    if (src === undefined || src === null) {
      if (f.kind === 'boolean') values[f.name] = false;
      continue;
    }
    values[f.name] = f.kind === 'boolean' ? Boolean(src) : toText(src);
  }
  return values;
}

function isEmpty(v: unknown): boolean {
  return v === undefined || v === null || (typeof v === 'string' && v.trim() === '');
}

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

function parseNumber(v: unknown): number {
  if (typeof v === 'number') return v;
  if (typeof v === 'string' && v.trim() !== '') return Number(v);
  return Number.NaN;
}

/**
 * Validate raw form values against the schema subset. Returns a map of field name to
 * error message; an empty map means the values are valid.
 */
export function validateParams(
  schema: JsonSchema | null | undefined,
  values: ParamValues
): Record<string, string> {
  const errors: Record<string, string> = {};
  for (const f of schemaToFields(schema)) {
    const v = values[f.name];
    if (isEmpty(v)) {
      if (f.required) errors[f.name] = 'Required';
      continue;
    }
    switch (f.kind) {
      case 'boolean':
        if (typeof v !== 'boolean') errors[f.name] = 'Must be true or false';
        break;
      case 'enum':
        if (!f.options.includes(toText(v))) {
          errors[f.name] = `Must be one of: ${f.options.join(', ')}`;
        }
        break;
      case 'integer':
      case 'number': {
        const n = parseNumber(v);
        if (!Number.isFinite(n)) errors[f.name] = 'Must be a number';
        else if (f.kind === 'integer' && !Number.isInteger(n))
          errors[f.name] = 'Must be an integer';
        else if (f.minimum !== null && n < f.minimum)
          errors[f.name] = `Must be at least ${f.minimum}`;
        else if (f.maximum !== null && n > f.maximum)
          errors[f.name] = `Must be at most ${f.maximum}`;
        break;
      }
      case 'json':
        try {
          JSON.parse(String(v));
        } catch {
          errors[f.name] = 'Must be valid JSON';
        }
        break;
      default: {
        const s = String(v);
        if (f.format === 'date' && !DATE_RE.test(s)) errors[f.name] = 'Must be a date (YYYY-MM-DD)';
        else if (f.format === 'date-time' && Number.isNaN(Date.parse(s))) {
          errors[f.name] = 'Must be a valid date and time';
        }
      }
    }
  }
  return errors;
}

/** Convert valid raw form values into the typed `custom_params` object. */
export function buildParams(
  schema: JsonSchema | null | undefined,
  values: ParamValues
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const f of schemaToFields(schema)) {
    const v = values[f.name];
    if (isEmpty(v)) {
      // An unchecked optional checkbox is still an explicit false.
      if (f.kind === 'boolean' && typeof v === 'boolean') out[f.name] = v;
      continue;
    }
    if (f.kind === 'boolean') out[f.name] = v === true;
    else if (f.kind === 'integer' || f.kind === 'number') out[f.name] = parseNumber(v);
    else if (f.kind === 'json') out[f.name] = JSON.parse(String(v));
    else if (f.kind === 'enum' && (f.baseType === 'integer' || f.baseType === 'number')) {
      out[f.name] = parseNumber(v);
    } else if (f.kind === 'enum' && f.baseType === 'boolean') out[f.name] = toText(v) === 'true';
    else out[f.name] = String(v);
  }
  return out;
}

// ---------------------------------------------------------------------------
// Chart data
// ---------------------------------------------------------------------------

/** Merge series into one row per x value, as recharts expects. */
export function chartRows(series: PluginReportSeries[]): Record<string, string | number>[] {
  const order: string[] = [];
  const byX = new Map<string, Record<string, string | number>>();
  for (const s of series) {
    for (const p of s.points) {
      const key = String(p.x);
      let row = byX.get(key);
      if (!row) {
        row = { x: p.x };
        byX.set(key, row);
        order.push(key);
      }
      row[s.name] = p.y;
    }
  }
  return order.map((k) => byX.get(k)!);
}

// ---------------------------------------------------------------------------
// Datasets
// ---------------------------------------------------------------------------

/** Turn the raw per-column filter inputs into typed equality filters (empty ones are dropped). */
export function buildDatasetFilter(
  columns: DatasetColumn[],
  raw: Record<string, string>
): Record<string, string | number | boolean> {
  const out: Record<string, string | number | boolean> = {};
  for (const c of columns) {
    const v = raw[c.name];
    if (v === undefined || v.trim() === '') continue;
    if (c.type === 'integer' || c.type === 'number') {
      const n = Number(v);
      out[c.name] = Number.isFinite(n) ? n : v;
    } else if (c.type === 'boolean') {
      out[c.name] = v === 'true';
    } else {
      out[c.name] = v;
    }
  }
  return out;
}

export const DEFAULT_DATASET_LIMIT = 100;

/** Raw state of the dataset picker; `dataset` is empty when no dataset is used. */
export interface DatasetSource {
  dataset: string;
  /** Raw text per column name; empty means no filter on that column. */
  filter: Record<string, string>;
  limit: string;
}

export const EMPTY_DATASET_SOURCE: DatasetSource = {
  dataset: '',
  filter: {},
  limit: String(DEFAULT_DATASET_LIMIT),
};

/** `query_config.custom_params` for a custom report; null when no dataset is chosen. */
export function datasetSourceToParams(
  source: DatasetSource,
  columns: DatasetColumn[]
): { dataset: string; filter: Record<string, string | number | boolean>; limit: number } | null {
  if (!source.dataset) return null;
  const limit = Math.trunc(Number(source.limit));
  return {
    dataset: source.dataset,
    filter: buildDatasetFilter(columns, source.filter),
    limit: Number.isFinite(limit) && limit > 0 ? limit : DEFAULT_DATASET_LIMIT,
  };
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const CORE_EXPORT_FORMATS: ExportFormatInfo[] = [
  {
    id: 'json',
    label: 'JSON',
    content_type: 'application/json',
    extension: 'json',
    source: 'core',
  },
  { id: 'csv', label: 'CSV', content_type: 'text/csv', extension: 'csv', source: 'core' },
  { id: 'pdf', label: 'PDF', content_type: 'application/pdf', extension: 'pdf', source: 'core' },
];

export function exportFileName(executionId: string, format: ExportFormatInfo): string {
  const ext = format.extension.replace(/^\./, '').replace(/[^A-Za-z0-9]/g, '') || 'bin';
  return `report-${executionId}.${ext}`;
}

/** Save a blob through a temporary link (the server always answers with an attachment). */
export function saveBlob(blob: Blob, fileName: string): void {
  const url = URL.createObjectURL(blob);
  try {
    const a = document.createElement('a');
    a.href = url;
    a.download = fileName;
    document.body.appendChild(a);
    a.click();
    a.remove();
  } finally {
    URL.revokeObjectURL(url);
  }
}

// ---------------------------------------------------------------------------
// Report annotations
// ---------------------------------------------------------------------------

export function annotationColumnsFor(
  columns: ReportAnnotationColumn[] | null | undefined,
  showIn: AnnotationShowIn
): ReportAnnotationColumn[] {
  return (columns ?? []).filter((c) => Array.isArray(c.show_in) && c.show_in.includes(showIn));
}

export const annotationColumnKey = (c: { plugin_id: string; key: string }) =>
  `${c.plugin_id}:${c.key}`;

export type AnnotationMap = Record<string, ReportAnnotation[]>;

export function findAnnotation(
  map: AnnotationMap | undefined,
  hash: string,
  column: Pick<ReportAnnotationColumn, 'plugin_id' | 'key'>
): ReportAnnotation | undefined {
  return map?.[hash]?.find((a) => a.plugin_id === column.plugin_id && a.key === column.key);
}

export function formatAnnotationValue(value: unknown): string {
  if (value === null || value === undefined) return '';
  return toText(value);
}

type HashRow = { hash: string };

/** Sort loaded rows by an annotation value; rows without a value always go last. */
export function sortByAnnotation<T extends HashRow>(
  rows: T[],
  map: AnnotationMap | undefined,
  column: ReportAnnotationColumn,
  direction: 'asc' | 'desc' = 'asc'
): T[] {
  const sign = direction === 'asc' ? 1 : -1;
  const present = (r: T) => findAnnotation(map, r.hash, column)?.value;
  return [...rows].sort((a, b) => {
    const va = present(a);
    const vb = present(b);
    const missingA = va === undefined || va === null;
    const missingB = vb === undefined || vb === null;
    if (missingA || missingB) return missingA === missingB ? 0 : missingA ? 1 : -1;
    if (typeof va === 'number' && typeof vb === 'number') return (va - vb) * sign;
    return (
      formatAnnotationValue(va).localeCompare(formatAnnotationValue(vb), undefined, {
        numeric: true,
      }) * sign
    );
  });
}

/** Keep rows whose annotation value contains the text (case-insensitive); empty text keeps all. */
export function filterByAnnotation<T extends HashRow>(
  rows: T[],
  map: AnnotationMap | undefined,
  column: ReportAnnotationColumn,
  text: string
): T[] {
  const needle = text.trim().toLowerCase();
  if (!needle) return rows;
  return rows.filter((r) => {
    const a = findAnnotation(map, r.hash, column);
    return a ? formatAnnotationValue(a.value).toLowerCase().includes(needle) : false;
  });
}
