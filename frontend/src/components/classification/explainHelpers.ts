import type { ExplainOutcome, ExplainProvenance, ExprTrace, ProvenanceSource } from '../../types';

export const OUTCOME_LABELS: Record<ExplainOutcome, string> = {
  matched: 'Matched',
  not_matched: 'Not matched',
  environment_mismatch: 'Environment mismatch',
  inactive: 'Inactive',
  excluded: 'Excluded',
  not_evaluated: 'Not evaluated',
};

export function outcomeLabel(outcome: ExplainOutcome): string {
  return OUTCOME_LABELS[outcome] ?? outcome;
}

/** Tailwind classes of the outcome badge. */
export function outcomeTone(outcome: ExplainOutcome): string {
  switch (outcome) {
    case 'matched':
      return 'bg-green-100 text-green-800';
    case 'not_matched':
      return 'bg-gray-100 text-gray-700';
    case 'environment_mismatch':
    case 'excluded':
      return 'bg-orange-100 text-orange-800';
    case 'inactive':
      return 'bg-yellow-100 text-yellow-800';
    default:
      return 'bg-gray-100 text-gray-500';
  }
}

/** Compact one-line rendering of an arbitrary JSON value. */
export function formatValue(value: unknown): string {
  if (value === undefined) return '-';
  if (value === null) return 'null';
  if (typeof value === 'string') return JSON.stringify(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export interface TraceRow {
  trace: ExprTrace;
  depth: number;
  /** Path of child indexes, unique within one tree */
  key: string;
}

/** Depth-first flattening of a trace into indented rows (parents before children). */
export function flattenTrace(trace: ExprTrace, depth = 0, key = '0'): TraceRow[] {
  const rows: TraceRow[] = [{ trace, depth, key }];
  (trace.children ?? []).forEach((child, index) => {
    rows.push(...flattenTrace(child, depth + 1, `${key}.${index}`));
  });
  return rows;
}

/** Operand values worth showing next to a trace line (left/right of a comparison, a function result). */
export function traceDetails(trace: ExprTrace): string[] {
  const details: string[] = [];
  if (trace.left !== undefined) details.push(`left = ${formatValue(trace.left)}`);
  if (trace.right !== undefined) details.push(`right = ${formatValue(trace.right)}`);
  if (trace.value !== undefined) details.push(`value = ${formatValue(trace.value)}`);
  return details;
}

export interface ProvenanceRow {
  key: string;
  /** Contributing groups in merge order (lowest precedence first) */
  sources: ProvenanceSource[];
  /** The group whose value won (last contributor) */
  winner: ProvenanceSource | null;
}

/** Rows of a provenance map, sorted by key; the last source is the winner. */
export function provenanceRows(
  map: Record<string, ProvenanceSource[]> | undefined
): ProvenanceRow[] {
  return Object.keys(map ?? {})
    .sort()
    .map((key) => {
      const sources = map![key] ?? [];
      return { key, sources, winner: sources.length > 0 ? sources[sources.length - 1] : null };
    });
}

export function hasProvenance(provenance: ExplainProvenance | undefined): boolean {
  if (!provenance) return false;
  return (
    Object.keys(provenance.classes ?? {}).length > 0 ||
    Object.keys(provenance.variables ?? {}).length > 0 ||
    provenance.environment != null
  );
}

export function formatElapsed(us: number): string {
  if (us < 1000) return `${us} us`;
  return `${(us / 1000).toFixed(1)} ms`;
}
