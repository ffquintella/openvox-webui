import type { PluginConfigSchema } from '../../types';

type Obj = Record<string, unknown>;

function isObj(v: unknown): v is Obj {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** Drops empty strings, lists, objects and nulls, so untouched optional settings are not stored. */
export function pruneConfig(value: unknown): unknown {
  if (Array.isArray(value)) {
    const items = value.map(pruneConfig).filter((v) => v !== undefined);
    return items.length ? items : undefined;
  }
  if (isObj(value)) {
    const out: Obj = {};
    for (const [k, v] of Object.entries(value)) {
      const p = pruneConfig(v);
      if (p !== undefined) out[k] = p;
    }
    return Object.keys(out).length ? out : undefined;
  }
  if (value === null || value === '') return undefined;
  return value;
}

export function pruneConfigObject(value: Obj): Obj {
  return (pruneConfig(value) as Obj | undefined) ?? {};
}

export function listToLines(v: unknown): string {
  return Array.isArray(v) ? v.map(String).join('\n') : '';
}

export function linesToList(text: string): string[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
}

/**
 * Values the browser's own address offers for a string setting: its host
 * name and origin, when the schema's pattern accepts them. A schema without
 * a pattern gets none, so unrelated fields are never prefilled.
 */
export function addressSuggestions(
  schema: PluginConfigSchema | undefined,
  location: { hostname: string; origin: string }
): string[] {
  if (!schema?.pattern) return [];
  let re: RegExp;
  try {
    re = new RegExp(schema.pattern);
  } catch {
    return [];
  }
  return [location.hostname, location.origin].filter((c) => re.test(c));
}

export type FieldKind = 'object' | 'enum' | 'boolean' | 'number' | 'string' | 'strings' | 'json';

/** How a setting is edited; shapes the form cannot express fall back to JSON. */
export function fieldKind(schema: PluginConfigSchema): FieldKind {
  if (schema.enum?.length) return 'enum';
  switch (schema.type) {
    case 'object':
      return schema.properties ? 'object' : 'json';
    case 'boolean':
      return 'boolean';
    case 'integer':
    case 'number':
      return 'number';
    case 'string':
      return 'string';
    case 'array':
      return schema.items?.type === 'string' && !schema.items.enum ? 'strings' : 'json';
    default:
      return 'json';
  }
}
