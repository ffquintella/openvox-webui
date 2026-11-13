import type { BoltJob, BoltJobStatus, BoltLimits, BoltParameterMeta, BoltRunnable } from '../types';

// ---------------------------------------------------------------------------
// Job status
// ---------------------------------------------------------------------------

export const BOLT_JOB_STATUSES: BoltJobStatus[] = [
  'pending_approval',
  'queued',
  'running',
  'succeeded',
  'partial',
  'failed',
  'cancelled',
  'timed_out',
  'rejected',
];

const ACTIVE_STATUSES: BoltJobStatus[] = ['pending_approval', 'queued', 'running'];

/** No further change will happen to a job in a terminal status. */
export function isTerminalStatus(status: BoltJobStatus): boolean {
  return !ACTIVE_STATUSES.includes(status);
}

export function hasActiveJob(jobs: Array<Pick<BoltJob, 'status'>> | undefined): boolean {
  return !!jobs && jobs.some((j) => !isTerminalStatus(j.status));
}

export const BOLT_STATUS_LABELS: Record<BoltJobStatus, string> = {
  pending_approval: 'Pending approval',
  queued: 'Queued',
  running: 'Running',
  succeeded: 'Succeeded',
  partial: 'Partial',
  failed: 'Failed',
  cancelled: 'Cancelled',
  timed_out: 'Timed out',
  rejected: 'Rejected',
};

export function statusLabel(status: string): string {
  return BOLT_STATUS_LABELS[status as BoltJobStatus] ?? status;
}

export const BOLT_STATUS_CLASSES: Record<BoltJobStatus, string> = {
  pending_approval: 'bg-yellow-100 text-yellow-800',
  queued: 'bg-blue-100 text-blue-800',
  running: 'bg-indigo-100 text-indigo-800',
  succeeded: 'bg-green-100 text-green-800',
  partial: 'bg-orange-100 text-orange-800',
  failed: 'bg-red-100 text-red-800',
  cancelled: 'bg-gray-100 text-gray-800',
  timed_out: 'bg-red-100 text-red-800',
  rejected: 'bg-red-100 text-red-800',
};

// ---------------------------------------------------------------------------
// Approval and cancellation rules (the server enforces them; this only
// decides which buttons to offer)
// ---------------------------------------------------------------------------

export interface ActionState {
  visible: boolean;
  disabled: boolean;
  reason?: string;
}

export interface JobActor {
  userId: string | undefined;
  canApprove: boolean;
  canCreate: boolean;
}

/** Approve and reject: only for a job waiting on approval, by someone with `approve` other than the requester. */
export function decisionState(
  job: Pick<BoltJob, 'status' | 'requested_by_id'>,
  actor: JobActor
): ActionState {
  if (job.status !== 'pending_approval' || !actor.canApprove) {
    return { visible: false, disabled: true };
  }
  if (actor.userId && job.requested_by_id === actor.userId) {
    return {
      visible: true,
      disabled: true,
      reason:
        'You cannot approve or reject your own request. Another user with approval rights must decide.',
    };
  }
  return { visible: true, disabled: false };
}

/** Cancel: the requester (who can run jobs) or anyone with `approve`, while the job is not finished. */
export function cancelState(
  job: Pick<BoltJob, 'status' | 'requested_by_id'>,
  actor: JobActor
): ActionState {
  if (isTerminalStatus(job.status)) return { visible: false, disabled: true };
  const own = !!actor.userId && job.requested_by_id === actor.userId;
  const allowed = actor.canApprove || (own && actor.canCreate);
  return allowed ? { visible: true, disabled: false } : { visible: false, disabled: true };
}

/** Whether a run needs approval: flagged on the allowlist or above the target threshold. */
export function runNeedsApproval(
  item: Pick<BoltRunnable, 'requires_approval'> | null | undefined,
  targetCount: number,
  limits: Pick<BoltLimits, 'approval_target_threshold'> | undefined
): boolean {
  if (item?.requires_approval) return true;
  return !!limits && targetCount > limits.approval_target_threshold;
}

export function exceedsTargetLimit(
  targetCount: number,
  limits: Pick<BoltLimits, 'max_targets_per_run'> | undefined
): boolean {
  return !!limits && targetCount > limits.max_targets_per_run;
}

// ---------------------------------------------------------------------------
// Parameter form generated from task/plan metadata
// ---------------------------------------------------------------------------

export type ParamFieldKind =
  'string' | 'integer' | 'float' | 'boolean' | 'enum' | 'array' | 'hash' | 'json';

export interface ParamField {
  name: string;
  kind: ParamFieldKind;
  /** The Puppet type as declared, for display. */
  typeLabel: string;
  description?: string;
  sensitive: boolean;
  optional: boolean;
  hasDefault: boolean;
  defaultValue?: unknown;
  /** Allowed values of an Enum. */
  options?: string[];
}

export type ParamValue = string | boolean;
export type ParamValues = Record<string, ParamValue>;

function splitTopLevel(inner: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let quote: string | null = null;
  let cur = '';
  for (const ch of inner) {
    if (quote) {
      cur += ch;
      if (ch === quote) quote = null;
    } else if (ch === "'" || ch === '"') {
      quote = ch;
      cur += ch;
    } else if (ch === '[' || ch === '(' || ch === '{') {
      depth++;
      cur += ch;
    } else if (ch === ']' || ch === ')' || ch === '}') {
      depth--;
      cur += ch;
    } else if (ch === ',' && depth === 0) {
      parts.push(cur);
      cur = '';
    } else {
      cur += ch;
    }
  }
  if (cur.trim()) parts.push(cur);
  return parts;
}

function unquote(s: string): string {
  const t = s.trim();
  if (t.length >= 2 && (t[0] === "'" || t[0] === '"') && t[t.length - 1] === t[0]) {
    return t.slice(1, -1);
  }
  return t;
}

/** Strips `Optional[...]` wrappers; reports whether any was removed. */
function unwrapOptional(type: string): { type: string; optional: boolean } {
  let t = type.trim();
  let optional = false;
  while (t.startsWith('Optional[') && t.endsWith(']')) {
    t = t.slice('Optional['.length, -1).trim();
    optional = true;
  }
  return { type: t, optional };
}

export function parameterField(name: string, meta: BoltParameterMeta): ParamField {
  const declared = (meta.type ?? '').trim();
  const { type, optional: wrapped } = unwrapOptional(declared);
  const base = type.split('[')[0];
  const hasDefault = meta.default !== undefined && meta.default !== null;
  let kind: ParamFieldKind;
  let options: string[] | undefined;
  switch (base) {
    case 'String':
    case 'Pattern':
    case 'Regexp':
      kind = 'string';
      break;
    case 'Integer':
      kind = 'integer';
      break;
    case 'Float':
    case 'Numeric':
      kind = 'float';
      break;
    case 'Boolean':
      kind = 'boolean';
      break;
    case 'Enum': {
      kind = 'enum';
      const inner = type.slice(type.indexOf('[') + 1, type.lastIndexOf(']'));
      options = splitTopLevel(inner)
        .map(unquote)
        .filter((o) => o !== '');
      if (options.length === 0) kind = 'string';
      break;
    }
    case 'Array':
    case 'Tuple':
      kind = 'array';
      break;
    case 'Hash':
    case 'Struct':
      kind = 'hash';
      break;
    case '':
      kind = 'string';
      break;
    default:
      kind = 'json';
  }
  return {
    name,
    kind,
    typeLabel: declared || 'String',
    description: meta.description ?? undefined,
    sensitive: meta.sensitive === true,
    optional: meta.optional === true || hasDefault || wrapped,
    hasDefault,
    defaultValue: meta.default,
    options,
  };
}

/** One field per declared parameter: required first, then by name. */
export function parameterFields(
  parameters: Record<string, BoltParameterMeta> | undefined
): ParamField[] {
  const fields = Object.entries(parameters ?? {}).map(([n, m]) => parameterField(n, m ?? {}));
  return fields.sort(
    (a, b) => Number(a.optional) - Number(b.optional) || a.name.localeCompare(b.name)
  );
}

export function initialParamValues(fields: ParamField[]): ParamValues {
  const values: ParamValues = {};
  for (const f of fields) {
    values[f.name] = f.kind === 'boolean' ? f.defaultValue === true : '';
  }
  return values;
}

export interface BuiltParams {
  params: Record<string, unknown>;
  errors: Record<string, string>;
}

function parseJsonOf(kind: ParamFieldKind, raw: string): { value?: unknown; error?: string } {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { error: 'Enter valid JSON' };
  }
  if (kind === 'array' && !Array.isArray(parsed)) return { error: 'Must be a JSON array' };
  if (kind === 'hash' && (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed))) {
    return { error: 'Must be a JSON object' };
  }
  return { value: parsed };
}

/**
 * Converts the form values to the `parameters` object of a run request and
 * collects per-field errors. Empty optional fields are left out so Bolt
 * applies its own default.
 */
export function buildParameters(fields: ParamField[], values: ParamValues): BuiltParams {
  const params: Record<string, unknown> = {};
  const errors: Record<string, string> = {};
  for (const f of fields) {
    const v = values[f.name];
    if (f.kind === 'boolean') {
      const checked = v === true;
      const dflt = f.defaultValue === true;
      if (!f.optional || checked !== dflt) params[f.name] = checked;
      continue;
    }
    const raw = typeof v === 'string' ? v : '';
    const text = f.kind === 'string' ? raw : raw.trim();
    if (text === '') {
      if (!f.optional) errors[f.name] = 'Required';
      continue;
    }
    switch (f.kind) {
      case 'string':
        params[f.name] = text;
        break;
      case 'integer':
        if (/^-?\d+$/.test(text)) params[f.name] = Number(text);
        else errors[f.name] = 'Must be a whole number';
        break;
      case 'float': {
        const n = Number(text);
        if (Number.isFinite(n)) params[f.name] = n;
        else errors[f.name] = 'Must be a number';
        break;
      }
      case 'enum':
        if (f.options?.includes(text)) params[f.name] = text;
        else errors[f.name] = 'Choose one of the listed values';
        break;
      default: {
        const r = parseJsonOf(f.kind, text);
        if (r.error) errors[f.name] = r.error;
        else params[f.name] = r.value;
      }
    }
  }
  return { params, errors };
}

/** The message of an axios-style error (`{error, message}` body) or a fallback. */
export function errorText(err: unknown): string {
  const data = (err as { response?: { data?: { message?: string } } })?.response?.data;
  return data?.message || (err as Error)?.message || 'Request failed';
}
