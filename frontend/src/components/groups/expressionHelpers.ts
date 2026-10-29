import type {
  ClassificationExpression,
  ClassificationRule,
  GroupBehavior,
  InheritanceMode,
  MatchModeKind,
  MergeStrategyName,
  RuleMatchType,
} from '../../types';

/** Expression limits enforced by the backend (shown to the user, validated server side). */
export const EXPRESSION_MAX_DEPTH = 16;
export const EXPRESSION_MAX_NODES = 200;

export const DEFAULT_PRIORITY = 100;

export const INHERITANCE_MODES: { value: InheritanceMode; label: string }[] = [
  { value: 'inherit', label: 'Inherit from parent' },
  { value: 'independent', label: 'Independent (ignore parent values)' },
  { value: 'block', label: 'Block (stop inheritance for children)' },
  { value: 'select', label: 'Select (inherit only listed keys)' },
];

export const MATCH_MODES: { value: '' | MatchModeKind; label: string }[] = [
  { value: '', label: 'Default (rule match type)' },
  { value: 'all', label: 'All conditions' },
  { value: 'any', label: 'Any condition' },
  { value: 'n_of', label: 'At least N conditions' },
];

export const FALLBACK_MERGE_STRATEGIES: { name: MergeStrategyName; description: string }[] = [
  { name: 'deep', description: 'Merge nested values recursively' },
  { name: 'replace', description: 'Higher priority value replaces the lower one' },
  { name: 'union', description: 'Combine lists and maps from all groups' },
  { name: 'fail_on_conflict', description: 'Report a merge error when values differ' },
];

export function defaultBehavior(): GroupBehavior {
  return {
    inheritance: { mode: 'inherit' },
    exclusive_set: null,
    match_mode: null,
    merge: 'deep',
    active_from: null,
    expires_at: null,
  };
}

/** Behavior of a group, tolerating servers that do not send one yet. */
export function behaviorOf(behavior: GroupBehavior | undefined | null): GroupBehavior {
  const base = defaultBehavior();
  if (!behavior) return base;
  return {
    inheritance: behavior.inheritance ?? base.inheritance,
    exclusive_set: behavior.exclusive_set ?? null,
    match_mode: behavior.match_mode ?? null,
    merge: behavior.merge ?? base.merge,
    active_from: behavior.active_from ?? null,
    expires_at: behavior.expires_at ?? null,
  };
}

/** Convert the group's flat rules to the equivalent composite expression. */
export function rulesToExpression(
  rules: Pick<ClassificationRule, 'fact_path' | 'operator' | 'value'>[],
  matchType: RuleMatchType
): ClassificationExpression | null {
  if (rules.length === 0) return null;
  return {
    [matchType]: rules.map((rule) => ({
      fact: rule.fact_path,
      op: rule.operator,
      value: rule.value,
    })),
  };
}

export type ExpressionParseResult =
  { ok: true; expression: ClassificationExpression | null } | { ok: false; error: string };

/**
 * Parse the JSON editor text. Blank text and `{}` both mean "no expression".
 * Only structural JSON validity is checked here; grammar errors come back from the server (HTTP 400).
 */
export function parseExpressionText(text: string): ExpressionParseResult {
  const trimmed = text.trim();
  if (trimmed === '') return { ok: true, expression: null };
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (e) {
    return { ok: false, error: `Invalid JSON: ${e instanceof Error ? e.message : String(e)}` };
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return { ok: false, error: 'The expression must be a JSON object.' };
  }
  if (Object.keys(parsed as object).length === 0) return { ok: true, expression: null };
  return { ok: true, expression: parsed as ClassificationExpression };
}

export function expressionToText(expression: ClassificationExpression | null | undefined): string {
  if (!expression || Object.keys(expression).length === 0) return '';
  return JSON.stringify(expression, null, 2);
}

/** Number of nodes in an expression tree (rough client-side guard for the 200 node limit). */
export function countExpressionNodes(value: unknown): number {
  if (Array.isArray(value)) return value.reduce<number>((n, v) => n + countExpressionNodes(v), 0);
  if (value !== null && typeof value === 'object') {
    return 1 + Object.values(value).reduce<number>((n, v) => n + countExpressionNodes(v), 0);
  }
  return 0;
}

/** Deterministic JSON (sorted keys) so two equal drafts always compare equal. */
export function stableStringify(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, v]) => v !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([k, v]) => `${JSON.stringify(k)}:${stableStringify(v)}`);
    return `{${entries.join(',')}}`;
  }
  return JSON.stringify(value) ?? 'null';
}

// ---------------------------------------------------------------------------
// Behavior form <-> API conversion
// ---------------------------------------------------------------------------

/** String-based form state of the behavior section (what the inputs hold). */
export interface BehaviorFormState {
  inheritanceMode: InheritanceMode;
  /** Comma separated keys, only used by the select mode */
  inheritanceKeys: string;
  exclusiveSet: string;
  matchMode: '' | MatchModeKind;
  matchK: string;
  merge: MergeStrategyName;
  /** `datetime-local` value (local time), empty when unset */
  activeFrom: string;
  expiresAt: string;
}

export function rfc3339ToLocalInput(value: string | null | undefined): string {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const pad = (n: number) => String(n).padStart(2, '0');
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

export function localInputToRfc3339(value: string): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
}

export function behaviorToForm(behavior: GroupBehavior | undefined | null): BehaviorFormState {
  const b = behaviorOf(behavior);
  return {
    inheritanceMode: b.inheritance.mode,
    inheritanceKeys: (b.inheritance.keys ?? []).join(', '),
    exclusiveSet: b.exclusive_set ?? '',
    matchMode: b.match_mode?.mode ?? '',
    matchK:
      b.match_mode?.mode === 'n_of' && b.match_mode.k !== undefined ? String(b.match_mode.k) : '',
    merge: b.merge,
    activeFrom: rfc3339ToLocalInput(b.active_from),
    expiresAt: rfc3339ToLocalInput(b.expires_at),
  };
}

export function parseKeys(text: string): string[] {
  return text
    .split(',')
    .map((k) => k.trim())
    .filter((k) => k.length > 0);
}

export function formToBehavior(form: BehaviorFormState): GroupBehavior {
  const keys = parseKeys(form.inheritanceKeys);
  const k = Number.parseInt(form.matchK, 10);
  return {
    inheritance:
      form.inheritanceMode === 'select' ? { mode: 'select', keys } : { mode: form.inheritanceMode },
    exclusive_set: form.exclusiveSet.trim() === '' ? null : form.exclusiveSet.trim(),
    match_mode:
      form.matchMode === ''
        ? null
        : form.matchMode === 'n_of'
          ? { mode: 'n_of', k: Number.isNaN(k) ? 0 : k }
          : { mode: form.matchMode },
    merge: form.merge,
    active_from: localInputToRfc3339(form.activeFrom),
    expires_at: localInputToRfc3339(form.expiresAt),
  };
}

/** Client-side checks; returns human readable problems (empty when fine). */
export function validateBehaviorForm(form: BehaviorFormState): string[] {
  const errors: string[] = [];
  if (form.inheritanceMode === 'select' && parseKeys(form.inheritanceKeys).length === 0) {
    errors.push('Select inheritance needs at least one key.');
  }
  if (form.matchMode === 'n_of') {
    const k = Number.parseInt(form.matchK, 10);
    if (Number.isNaN(k) || k < 1) errors.push('"At least N" needs N of 1 or more.');
  }
  const from = localInputToRfc3339(form.activeFrom);
  const to = localInputToRfc3339(form.expiresAt);
  if (from && to && new Date(to).getTime() <= new Date(from).getTime()) {
    errors.push('Expires at must be after Active from.');
  }
  return errors;
}

export function parsePriority(text: string): number | null {
  if (text.trim() === '') return null;
  const n = Number(text);
  return Number.isInteger(n) ? n : null;
}
