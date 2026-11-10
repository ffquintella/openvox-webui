import { describe, expect, it } from 'vitest';
import {
  addressSuggestions,
  fieldKind,
  linesToList,
  pruneConfigObject,
} from '../components/plugins/configForm';

describe('plugin settings form helpers', () => {
  it('prunes empty values but keeps false and zero', () => {
    expect(
      pruneConfigObject({
        rp: { id: 'a.example.com', name: '', origins: [] },
        authenticators: { allow_backup_eligible: false, algorithms: [] },
        n: 0,
        gone: null,
      })
    ).toEqual({
      rp: { id: 'a.example.com' },
      authenticators: { allow_backup_eligible: false },
      n: 0,
    });
  });

  it('splits one entry per line', () => {
    expect(linesToList(' https://a \n\n https://b\n')).toEqual(['https://a', 'https://b']);
  });

  it('suggests the address only where the pattern accepts it', () => {
    const loc = { hostname: 'puppet.example.com', origin: 'https://puppet.example.com' };
    expect(addressSuggestions({ type: 'string', pattern: '^https://[a-z0-9.-]+$' }, loc)).toEqual([
      'https://puppet.example.com',
    ]);
    expect(
      addressSuggestions({ type: 'string', pattern: '^[a-z0-9.-]+\\.[a-z]{2,}$' }, loc)
    ).toEqual(['puppet.example.com']);
    expect(addressSuggestions({ type: 'string' }, loc)).toEqual([]);
    expect(addressSuggestions({ type: 'string', pattern: '(' }, loc)).toEqual([]);
  });

  it('chooses an editor for each kind of setting', () => {
    expect(fieldKind({ type: 'string', enum: ['a'] })).toBe('enum');
    expect(fieldKind({ type: 'array', items: { type: 'string' } })).toBe('strings');
    expect(fieldKind({ type: 'array', items: { type: 'integer' } })).toBe('json');
    expect(fieldKind({ type: 'object', properties: {} })).toBe('object');
    expect(fieldKind({ type: 'boolean' })).toBe('boolean');
  });
});

describe('security key error for a plugin without settings', () => {
  it('tells the user an administrator must finish the settings', async () => {
    const { authErrorMessage } = await import('../components/auth/authErrors');
    const err = {
      response: {
        status: 503,
        data: {
          code: 'factor_unavailable',
          details: { factor_error: { kind: 'unavailable', code: 'config_missing' } },
        },
      },
    };
    expect(authErrorMessage(err, 'auth.error.generic')).toContain('Plugins page');
  });
});
