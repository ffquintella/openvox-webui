// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as React from 'react';
import {
  createShareScope,
  isModuleUrl,
  isValidIntegrity,
  loadTrustedModule,
  resetModuleCache,
  type LoaderDeps,
  type ModuleTarget,
} from '../components/plugins/moduleLoader';

const SRI = 'sha384-' + 'A'.repeat(64);
const TOKEN = 'a'.repeat(64);
const target = (over: Partial<ModuleTarget> = {}): ModuleTarget => ({
  url: `/api/v1/plugins/ui-modules/${TOKEN}/ui/remoteEntry.js`,
  integrity: SRI,
  expose: './Page',
  cacheKey: `k-${Math.random()}`,
  ...over,
});

function deps(mod: unknown, verify?: LoaderDeps['verify']): LoaderDeps {
  return {
    verify: vi.fn(verify ?? (() => Promise.resolve())),
    importModule: vi.fn(() => Promise.resolve(mod)),
  };
}

afterEach(() => {
  resetModuleCache();
  vi.restoreAllMocks();
});

describe('integrity and URL checks', () => {
  it('accepts only sha384 and sha512 SRI values', () => {
    expect(isValidIntegrity(SRI)).toBe(true);
    expect(isValidIntegrity('sha512-' + 'b'.repeat(86) + '==')).toBe(true);
    for (const bad of ['', 'sha256-abc', 'sha384-', 'sha384-a b', 'sha384-abc?x', null, 42]) {
      expect(isValidIntegrity(bad)).toBe(false);
    }
  });

  it('only imports from the host module route', () => {
    expect(isModuleUrl(target().url)).toBe(true);
    for (const bad of [
      'https://evil.example/x.js',
      '//evil.example/x.js',
      `/api/v1/plugins/ui-assets/${TOKEN}/ui/app.js`,
      `/api/v1/plugins/ui-modules/${TOKEN}/../x.js`,
      `/api/v1/plugins/ui-modules/short/ui/x.js`,
      `/api/v1/plugins/ui-modules/${TOKEN}/ui/app.html`,
      'data:text/javascript,alert(1)',
    ]) {
      expect(isModuleUrl(bad)).toBe(false);
    }
  });
});

describe('loadTrustedModule', () => {
  const Page = () => null;

  it('never imports a module without a valid integrity or from another URL', async () => {
    const d = deps({ default: Page });
    await expect(loadTrustedModule(target({ integrity: 'sha256-x' }), d)).rejects.toThrow(
      /integrity/
    );
    await expect(
      loadTrustedModule(target({ url: 'https://evil.example/x.js' }), d)
    ).rejects.toThrow(/URL/);
    await expect(loadTrustedModule(target({ expose: '../x' }), d)).rejects.toThrow(/name/);
    expect(d.verify).not.toHaveBeenCalled();
    expect(d.importModule).not.toHaveBeenCalled();
  });

  it('does not import when the integrity check fails, and can retry', async () => {
    const t = target();
    const bad = deps({ default: Page }, () => Promise.reject(new Error('integrity mismatch')));
    await expect(loadTrustedModule(t, bad)).rejects.toThrow('integrity mismatch');
    expect(bad.verify).toHaveBeenCalledWith(t.url, SRI);
    expect(bad.importModule).not.toHaveBeenCalled();
    // The failure is not cached.
    const good = deps({ default: Page });
    await expect(loadTrustedModule(t, good)).resolves.toBe(Page);
  });

  it('verifies before importing and loads each module once', async () => {
    const order: string[] = [];
    const d: LoaderDeps = {
      verify: vi.fn(async () => {
        order.push('verify');
      }),
      importModule: vi.fn(async () => {
        order.push('import');
        return { default: Page };
      }),
    };
    const t = target();
    const [a, b] = await Promise.all([loadTrustedModule(t, d), loadTrustedModule(t, d)]);
    expect(a).toBe(Page);
    expect(b).toBe(Page);
    expect(order).toEqual(['verify', 'import']);
  });

  it('initialises a Module Federation container with the host React as a singleton', async () => {
    const init = vi.fn();
    const get = vi.fn(async () => () => ({ default: Page }));
    const d = deps({ init, get });
    await expect(loadTrustedModule(target({ expose: './Console' }), d)).resolves.toBe(Page);
    expect(get).toHaveBeenCalledWith('./Console');
    const scope = init.mock.calls[0][0] as ReturnType<typeof createShareScope>;
    const factory = await scope.react[React.version].get();
    expect(factory()).toBe(React);
    expect(Object.keys(scope).sort()).toEqual([
      'react',
      'react-dom',
      'react-dom/client',
      'react/jsx-runtime',
    ]);
  });

  it('refuses a module that does not export a component', async () => {
    await expect(loadTrustedModule(target(), deps({ default: 'nope' }))).rejects.toThrow(
      /React component/
    );
  });
});
