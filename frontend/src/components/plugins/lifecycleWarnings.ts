/**
 * Warnings a plugin lifecycle answer carries (OQ-23: disabling or
 * uninstalling a factor plugin names the users it leaves without a usable
 * second factor). The action itself succeeded.
 */
export function lifecycleWarnings(out: unknown): string[] {
  const warnings = (out as { warnings?: unknown } | null | undefined)?.warnings;
  if (!Array.isArray(warnings)) return [];
  return warnings
    .map((w) => (w as { message?: unknown })?.message)
    .filter((m): m is string => typeof m === 'string' && m.length > 0);
}
