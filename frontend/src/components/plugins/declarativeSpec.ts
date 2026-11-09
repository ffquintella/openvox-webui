// Pure helpers of the declarative plugin UI (H-6). The server validates every
// spec at install time (src/services/plugins/ui_spec.rs); the renderer still
// treats it as untrusted: values are rendered as text, data paths never reach
// a prototype and routes are filled only from the declared templates.
import { t } from '../../i18n';
import { isPluginRoutePath } from '../../services/api';
import type { UiRouteAccess } from '../../types';
import { toText } from './pluginHelpers';

export type Json = Record<string, unknown>;

export function isObject(v: unknown): v is Json {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

export function asArray(v: unknown): Json[] {
  return Array.isArray(v) ? v.filter(isObject) : [];
}

export function str(v: unknown): string | undefined {
  return typeof v === 'string' ? v : undefined;
}

function own(o: Json, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(o, key);
}

const FORBIDDEN_SEGMENTS = new Set(['__proto__', 'prototype', 'constructor']);

/** Value at a dotted path (`a.b.0.c`), own properties only. */
export function getPath(data: unknown, path: unknown): unknown {
  if (typeof path !== 'string' || path === '') return undefined;
  let cur: unknown = data;
  for (const seg of path.split('.')) {
    if (FORBIDDEN_SEGMENTS.has(seg)) return undefined;
    if (Array.isArray(cur)) {
      if (!/^\d+$/.test(seg)) return undefined;
      cur = cur[Number(seg)];
    } else if (isObject(cur) && own(cur, seg)) {
      cur = cur[seg];
    } else {
      return undefined;
    }
  }
  return cur;
}

/** An application path a link may point to: never a scheme, `//` or the API. */
export function isHostRoute(target: string): boolean {
  return (
    target.startsWith('/') &&
    !target.startsWith('//') &&
    target !== '/api' &&
    !target.startsWith('/api/') &&
    !target.startsWith('/api?') &&
    target.length <= 512 &&
    !/[\s\\\p{Cc}]/u.test(target)
  );
}

const TEMPLATE_RE = /^(\/([a-z0-9][a-z0-9_-]*|\{[a-z][a-z0-9_]*\})){1,8}$/;

/** A route parameter has no usable value (or the template is not a route). */
export class MissingParamError extends Error {
  constructor(name: string) {
    super(`missing value for ${name}`);
    this.name = 'MissingParamError';
  }
}

/**
 * Fills `{param}` placeholders of a declared route from `row.<path>` or
 * `context.<key>` (row by default when a record exists), encoded as one
 * path segment each.
 */
export function fillRoute(
  template: string,
  params: unknown,
  row: unknown,
  context: Record<string, string>
): string {
  if (!TEMPLATE_RE.test(template)) throw new MissingParamError('route');
  const mapping = isObject(params) ? params : {};
  const filled = template.replace(/\{([a-z][a-z0-9_]*)\}/g, (_m, name: string) => {
    const source = str(mapping[name]) ?? (row !== undefined ? `row.${name}` : `context.${name}`);
    let value: unknown;
    if (source.startsWith('row.') && row !== undefined) {
      value = getPath(row, source.slice(4));
    } else if (source.startsWith('context.') && own(context, source.slice(8))) {
      value = context[source.slice(8)];
    }
    if (typeof value === 'number' && Number.isFinite(value)) value = String(value);
    if (typeof value !== 'string' || value === '') throw new MissingParamError(name);
    return encodeURIComponent(value);
  });
  if (!isPluginRoutePath(filled)) throw new MissingParamError('route');
  return filled;
}

export function findRoute(
  routes: UiRouteAccess[] | undefined,
  method: string,
  path: unknown
): UiRouteAccess | undefined {
  return (routes ?? []).find((r) => r.method === method && r.path === path);
}

/** Static query values of a source, as strings. */
export function staticQuery(query: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (!isObject(query)) return out;
  for (const [k, v] of Object.entries(query)) {
    if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
      out[k] = String(v);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

export type Tone = 'neutral' | 'info' | 'success' | 'warning' | 'danger';
const TONES: Tone[] = ['neutral', 'info', 'success', 'warning', 'danger'];

export function isEmptyValue(v: unknown): boolean {
  return v === null || v === undefined || v === '' || (Array.isArray(v) && v.length === 0);
}

function scalarKey(v: unknown): string | undefined {
  return typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean'
    ? String(v)
    : undefined;
}

function listItem(item: unknown): string {
  if (isObject(item) && typeof item.message === 'string') {
    return typeof item.path === 'string' && item.path
      ? `${item.path}: ${item.message}`
      : item.message;
  }
  return toText(item);
}

/** The text shown for a value under a column, stats item or detail field. */
export function displayText(value: unknown, display: Json): string {
  if (isEmptyValue(value)) return str(display.empty) ?? '';
  const key = scalarKey(value);
  const labels = isObject(display.labels) ? display.labels : undefined;
  if (labels && key !== undefined && own(labels, key) && typeof labels[key] === 'string') {
    return labels[key] as string;
  }
  switch (display.format) {
    case 'number': {
      const n = typeof value === 'number' ? value : Number(value);
      return Number.isFinite(n) ? n.toLocaleString() : toText(value);
    }
    case 'boolean':
      if (value === true) return t('plugin_ui.yes');
      if (value === false) return t('plugin_ui.no');
      return toText(value);
    case 'date':
    case 'datetime': {
      if (typeof value !== 'string' && typeof value !== 'number') return toText(value);
      const d = new Date(value);
      if (Number.isNaN(d.getTime())) return toText(value);
      return display.format === 'date' ? d.toLocaleDateString() : d.toLocaleString();
    }
    case 'list':
      return Array.isArray(value) ? value.map(listItem).join(', ') : toText(value);
    default:
      return toText(value);
  }
}

export function statusTone(value: unknown, display: Json): Tone {
  const key = scalarKey(value);
  const tones = isObject(display.tones) ? display.tones : undefined;
  const tone = tones && key !== undefined && own(tones, key) ? tones[key] : undefined;
  return TONES.includes(tone as Tone) ? (tone as Tone) : 'neutral';
}

/** `{key, equals | not_equals | in}` over a record; true without a condition. */
export function matchesWhen(when: unknown, row: unknown): boolean {
  if (!isObject(when)) return true;
  const raw = getPath(row, when.key);
  const v = raw === undefined ? null : raw;
  if (own(when, 'equals')) return v === when.equals;
  if (own(when, 'not_equals')) return v !== when.not_equals;
  if (Array.isArray(when.in)) return when.in.some((x) => x === v);
  return true;
}

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

export type FieldValues = Record<string, string | boolean>;

function fieldName(f: Json): string {
  return str(f.name) ?? '';
}

export function fieldType(f: Json): string {
  return str(f.type) ?? 'text';
}

/** Starting values: the record's value (`initial`), the default, or empty. */
export function initialValues(fields: Json[], row?: unknown): FieldValues {
  const out: FieldValues = {};
  for (const f of fields) {
    const name = fieldName(f);
    if (fieldType(f) === 'checkbox') {
      const init = row !== undefined && f.initial ? getPath(row, f.initial) : undefined;
      out[name] = typeof init === 'boolean' ? init : f.default === true;
      continue;
    }
    const init = row !== undefined && f.initial ? getPath(row, f.initial) : undefined;
    const value = scalarKey(init) ?? scalarKey(f.default) ?? '';
    out[name] = value;
  }
  return out;
}

export interface BuiltBody {
  body: Json;
  errors: Record<string, string>;
}

/** Checks the values against the field rules and builds the JSON body. */
export function buildBody(fields: Json[], values: FieldValues): BuiltBody {
  const body: Json = {};
  const errors: Record<string, string> = {};
  for (const f of fields) {
    const name = fieldName(f);
    const label = str(f.label) ?? name;
    const type = fieldType(f);
    const raw = values[name];
    if (type === 'checkbox') {
      body[name] = raw === true;
      continue;
    }
    const text = typeof raw === 'string' ? raw : '';
    if (text.trim() === '') {
      if (f.required === true) errors[name] = t('plugin_ui.required', { label });
      continue;
    }
    if (type === 'number') {
      const n = Number(text);
      if (!Number.isFinite(n)) errors[name] = t('plugin_ui.not_a_number', { label });
      else if (typeof f.min === 'number' && n < f.min)
        errors[name] = t('plugin_ui.number_min', { label, min: f.min });
      else if (typeof f.max === 'number' && n > f.max)
        errors[name] = t('plugin_ui.number_max', { label, max: f.max });
      else body[name] = n;
      continue;
    }
    if (typeof f.min_length === 'number' && text.length < f.min_length) {
      errors[name] = t('plugin_ui.too_short', { label, min: f.min_length });
      continue;
    }
    if (typeof f.max_length === 'number' && text.length > f.max_length) {
      errors[name] = t('plugin_ui.too_long', { label, max: f.max_length });
      continue;
    }
    if (type === 'select') {
      const option = asArray(f.options).find((o) => scalarKey(o.value) === text);
      body[name] = option ? option.value : text;
    } else if (type === 'json') {
      try {
        body[name] = JSON.parse(text);
      } catch {
        errors[name] = t('plugin_ui.invalid_json', { label });
      }
    } else {
      body[name] = text;
    }
  }
  return { body, errors };
}

/** Applied filter values as query parameters (empty ones left out). */
export function filterQuery(values: FieldValues): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(values)) {
    if (v === true) out[k] = 'true';
    else if (typeof v === 'string' && v.trim() !== '') out[k] = v.trim();
  }
  return out;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/** A message for a failed plugin route call, never the raw response. */
export function routeErrorMessage(err: unknown): string {
  if (err instanceof MissingParamError) return t('plugin_ui.missing_param');
  const e = err as { response?: { status?: number; data?: unknown } };
  const status = e?.response?.status;
  const data = e?.response?.data;
  const nested = isObject(data) && isObject(data.error) ? data.error : undefined;
  const code = str(nested?.code) ?? (isObject(data) ? str(data.code) : undefined);
  if (status === 401 && code === 'step_up_required') return t('plugin_ui.step_up_required');
  if (status === 403 && code === 'enrollment_only_session') return t('auth.error.enrollment_only');
  if (status === 403) return t('plugin_ui.forbidden');
  if (status === 404) return t('plugin_ui.not_found');
  if (status === 429) return t('plugin_ui.rate_limited');
  if (status === undefined || status >= 500) {
    return status === undefined ? t('plugin_ui.request_failed') : t('plugin_ui.unavailable');
  }
  // 400, 409, 413, 422: the plugin's own message when it is short text.
  const message = str(nested?.message) ?? (isObject(data) ? str(data.message) : undefined);
  if (message && message.trim() && message.length <= 300) return message;
  return t('plugin_ui.request_failed');
}
