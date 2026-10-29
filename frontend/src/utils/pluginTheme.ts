import type { Theme } from '../stores/themeStore';

/**
 * Design tokens exposed to plugins as CSS custom properties (T85). The same
 * names are defined on `:root` in index.css, so declarative and host-rendered
 * contributions inherit them, and are posted to sandboxed iframes by the
 * bridge. Mirrors THEME_TOKEN_NAMES of `@openvox/plugin-sdk`.
 */
export const THEME_TOKEN_NAMES = [
  '--ov-color-bg',
  '--ov-color-surface',
  '--ov-color-surface-alt',
  '--ov-color-border',
  '--ov-color-text',
  '--ov-color-text-muted',
  '--ov-color-primary',
  '--ov-color-primary-contrast',
  '--ov-color-success',
  '--ov-color-warning',
  '--ov-color-danger',
  '--ov-color-info',
  '--ov-font-body',
  '--ov-font-mono',
  '--ov-font-size-base',
  '--ov-radius',
  '--ov-radius-lg',
] as const;

export type ThemeTokenName = (typeof THEME_TOKEN_NAMES)[number];
export type ThemeTokens = Partial<Record<ThemeTokenName, string>>;

const SHARED: Record<ThemeTokenName, string> = {
  '--ov-color-bg': '#f9fafb',
  '--ov-color-surface': '#ffffff',
  '--ov-color-surface-alt': '#f3f4f6',
  '--ov-color-border': '#e5e7eb',
  '--ov-color-text': '#111827',
  '--ov-color-text-muted': '#6b7280',
  '--ov-color-primary': '#0284c7',
  '--ov-color-primary-contrast': '#ffffff',
  '--ov-color-success': '#16a34a',
  '--ov-color-warning': '#d97706',
  '--ov-color-danger': '#dc2626',
  '--ov-color-info': '#0284c7',
  '--ov-font-body': 'Inter, system-ui, Avenir, Helvetica, Arial, sans-serif',
  '--ov-font-mono': 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
  '--ov-font-size-base': '14px',
  '--ov-radius': '6px',
  '--ov-radius-lg': '12px',
};

export const DEFAULT_TOKENS: Record<Theme, Record<ThemeTokenName, string>> = {
  light: SHARED,
  dark: {
    ...SHARED,
    '--ov-color-bg': '#111827',
    '--ov-color-surface': '#1f2937',
    '--ov-color-surface-alt': '#374151',
    '--ov-color-border': '#374151',
    '--ov-color-text': '#f3f4f6',
    '--ov-color-text-muted': '#9ca3af',
    '--ov-color-primary': '#38bdf8',
    '--ov-color-primary-contrast': '#0c1a24',
    '--ov-color-success': '#4ade80',
    '--ov-color-warning': '#fbbf24',
    '--ov-color-danger': '#f87171',
    '--ov-color-info': '#38bdf8',
  },
};

const COLOR = /^(#[0-9a-fA-F]{3,8}|(rgb|rgba|hsl|hsla)\([0-9 ,.%/-]+\))$/;
const FONT = /^[A-Za-z0-9 ,'"_.-]{1,200}$/;
const LENGTH = /^[0-9]+(\.[0-9]+)?(px|rem|em)$/;

function valueFits(name: ThemeTokenName, value: string): boolean {
  if (name.startsWith('--ov-color-')) return COLOR.test(value);
  if (name.startsWith('--ov-font-size') || name.startsWith('--ov-radius')) {
    return LENGTH.test(value);
  }
  return FONT.test(value);
}

/**
 * Keeps only allowlisted token names whose value has the shape the token
 * expects. Plugins never contribute CSS, and the host only ever sends what
 * passes this filter.
 */
export function sanitizeTokens(raw: Record<string, unknown>): ThemeTokens {
  const out: ThemeTokens = {};
  for (const name of THEME_TOKEN_NAMES) {
    const v = raw[name];
    if (typeof v === 'string' && valueFits(name, v.trim())) out[name] = v.trim();
  }
  return out;
}

/** Current token values: what index.css resolves to, else the built-in defaults. */
export function readThemeTokens(theme: Theme, el?: Element | null): ThemeTokens {
  const raw: Record<string, unknown> = { ...DEFAULT_TOKENS[theme] };
  const target = el ?? (typeof document !== 'undefined' ? document.documentElement : null);
  if (target && typeof getComputedStyle === 'function') {
    const cs = getComputedStyle(target);
    for (const name of THEME_TOKEN_NAMES) {
      const v = cs.getPropertyValue(name).trim();
      if (v) raw[name] = v;
    }
  }
  // Computed values that fail the filter fall back to the defaults.
  const clean = sanitizeTokens(raw);
  return { ...DEFAULT_TOKENS[theme], ...clean };
}
