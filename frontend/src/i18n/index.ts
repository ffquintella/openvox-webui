// Minimal message catalog. `locales/en.json` is the source and defines every
// key; other locales are optional JSON files with the same keys registered in
// `catalogs`, and a missing key falls back to English. Placeholders: `{name}`.
import en from './locales/en.json';

export type MessageKey = keyof typeof en;
export type Catalog = Partial<Record<MessageKey, string>>;
export type MessageParams = Record<string, string | number>;

export const SOURCE_LOCALE = 'en';

/** Registered locales. Add new locale files here. */
export const catalogs: Record<string, Catalog> = {
  en,
};

function preferredLanguages(): readonly string[] {
  if (typeof navigator === 'undefined') {
    return [];
  }
  if (Array.isArray(navigator.languages) && navigator.languages.length > 0) {
    return navigator.languages;
  }
  return navigator.language ? [navigator.language] : [];
}

/** Best registered locale for a list of BCP 47 tags, English by default. */
export function resolveLocale(preferred: readonly string[] = preferredLanguages()): string {
  for (const tag of preferred) {
    if (!tag) {
      continue;
    }
    if (catalogs[tag]) {
      return tag;
    }
    const base = tag.split('-')[0];
    if (catalogs[base]) {
      return base;
    }
  }
  return SOURCE_LOCALE;
}

/** Translates `key`, replacing `{name}` placeholders with `params`. */
export function t(
  key: MessageKey,
  params?: MessageParams,
  locale: string = resolveLocale()
): string {
  const template = catalogs[locale]?.[key] ?? en[key] ?? key;
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match
  );
}
