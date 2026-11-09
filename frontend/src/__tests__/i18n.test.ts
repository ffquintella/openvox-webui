import { describe, expect, it } from 'vitest';
import { catalogs, resolveLocale, t, type MessageKey } from '../i18n';

describe('i18n', () => {
  it('interpolates placeholders and leaves unknown ones', () => {
    expect(t('auth.error.attempts_left', { count: 3 })).toBe('Attempts left: 3.');
    expect(t('security.added_on', {})).toBe('Added {date}');
  });

  it('falls back to English for unknown locales and keys', () => {
    expect(resolveLocale(['xx-YY', 'zz'])).toBe('en');
    expect(resolveLocale(['en-GB'])).toBe('en');
    expect(t('mfa.verify', undefined, 'xx')).toBe('Verify');
    expect(t('no.such.key' as MessageKey)).toBe('no.such.key');
  });

  it('every locale has exactly the English keys', () => {
    const source = Object.keys(catalogs.en).sort();
    for (const [locale, catalog] of Object.entries(catalogs)) {
      expect(Object.keys(catalog).sort(), locale).toEqual(source);
      for (const value of Object.values(catalog)) {
        expect(typeof value === 'string' && value.length > 0, locale).toBe(true);
      }
    }
  });
});
