// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  attachBridge,
  checkProxyPath,
  NOTIFY_BURST,
  PROTOCOL_NS,
  PROTOCOL_VERSION,
  type Bridge,
  type BridgeDeps,
  type PluginFrameContext,
} from '../components/plugins/pluginBridge';
import {
  connect,
  OpenVoxError,
  THEME_TOKEN_NAMES as SDK_TOKENS,
} from '../../../sdk/typescript/src';
import {
  DEFAULT_TOKENS,
  THEME_TOKEN_NAMES,
  readThemeTokens,
  sanitizeTokens,
} from '../utils/pluginTheme';

const ctx: PluginFrameContext = {
  user: { username: 'alice' },
  locale: 'en',
  theme: 'light',
  slot: 'node.detail.tabs',
  params: { certname: 'web01' },
  plugin_id: 'acme',
  contribution_id: 'crm',
  api_scopes: ['nodes:read'],
};

/** A fake sandboxed frame wired to the host listener and to the SDK. */
function harness(over: Partial<BridgeDeps> = {}, origin = 'null') {
  const hostWin = new EventTarget() as unknown as Window;
  const sdkWin = Object.assign(new EventTarget(), { document }) as unknown as Window;
  // What the plugin sees as window.parent: posting a message reaches the host.
  const parent = {
    postMessage: (data: unknown) =>
      hostWin.dispatchEvent(Object.assign(new Event('message'), { data, source: frame, origin })),
  } as unknown as Window;
  // The frame's contentWindow as the host sees it: posting reaches the SDK.
  const frame = {
    postMessage: (data: unknown, _t: string, ports: MessagePort[]) =>
      sdkWin.dispatchEvent(Object.assign(new Event('message'), { data, source: parent, ports })),
  } as unknown as Window;

  const calls = {
    notify: vi.fn(),
    navigate: vi.fn(),
    resize: vi.fn(),
    get: vi.fn(async (path: string) => ({ got: path })),
  };
  let theme: 'light' | 'dark' = 'light';
  const bridge: Bridge = attachBridge({
    frame: () => frame,
    context: () => ctx,
    theme: () => ({ theme, tokens: readThemeTokens(theme) }),
    apiScopes: ctx.api_scopes,
    listen: hostWin,
    ...calls,
    ...over,
  });
  return {
    bridge,
    calls,
    connect: () => connect({ parent, target: sdkWin, timeoutMs: 200 }),
    setTheme: (t: 'light' | 'dark') => (theme = t),
    sdkWin,
  };
}

afterEach(() => {
  document.documentElement.removeAttribute('style');
});

describe('checkProxyPath', () => {
  it('accepts reads of the granted areas', () => {
    expect(checkProxyPath('/nodes?limit=20', ['nodes:read'])).toBe('/nodes?limit=20');
    expect(checkProxyPath('/nodes/web01/facts', ['nodes:read'])).toBe('/nodes/web01/facts');
  });

  it('refuses areas that are not granted or not exposed', () => {
    expect(() => checkProxyPath('/facts', ['nodes:read'])).toThrow(/facts:read/);
    for (const p of ['/plugins', '/auth/login', '/settings', '/api-keys', '/roles']) {
      expect(() => checkProxyPath(p, ['nodes:read', 'facts:read'])).toThrow(/not available/);
    }
  });

  it('refuses traversal, scheme-relative and malformed paths', () => {
    for (const p of [
      '',
      'nodes',
      '//evil.com/nodes',
      '/nodes/../plugins',
      '/nodes/%2e%2e/plugins',
      '/nodes/..%2fplugins',
      '/nodes\\x',
      '/nodes/%zz',
      '/nodes/\u0000',
    ]) {
      expect(() => checkProxyPath(p, ['nodes:read']), p).toThrow();
    }
    expect(() => checkProxyPath(42, ['nodes:read'])).toThrow();
    expect(() => checkProxyPath('/' + 'a'.repeat(3000), ['nodes:read'])).toThrow();
  });
});

describe('host bridge and SDK', () => {
  it('hands the context and theme over after a handshake', async () => {
    const h = harness();
    const app = await h.connect();
    expect(app.getContext()).toMatchObject({ plugin_id: 'acme', params: { certname: 'web01' } });
    const root = document.documentElement;
    expect(root.getAttribute('data-theme')).toBe('light');
    expect(root.style.getPropertyValue('--ov-color-primary')).toBe(
      DEFAULT_TOKENS.light['--ov-color-primary']
    );
    h.bridge.detach();
  });

  it('ignores a handshake from another window or a same-origin frame', async () => {
    // A frame with a real origin means the sandbox is not in effect.
    const h = harness({}, 'https://host.example');
    await expect(h.connect()).rejects.toMatchObject({ code: 'handshake_timeout' });
    h.bridge.detach();

    const h2 = harness();
    h2.bridge.detach();
    // Nothing listens any more: the handshake times out.
    await expect(h2.connect()).rejects.toBeInstanceOf(OpenVoxError);
  });

  it('proxies reads only for granted areas', async () => {
    const h = harness();
    const app = await h.connect();
    await expect(app.api.fetch('/nodes?limit=2')).resolves.toEqual({ got: '/nodes?limit=2' });
    await expect(app.api.fetch('/facts')).rejects.toMatchObject({ code: 'forbidden' });
    await expect(app.api.fetch('/nodes/../plugins')).rejects.toMatchObject({
      code: 'invalid_path',
    });
    expect(h.calls.get).toHaveBeenCalledTimes(1);
    h.bridge.detach();
  });

  it('turns an HTTP failure into a coded error without leaking the response', async () => {
    const get = vi.fn(async () => {
      throw { response: { status: 403, data: { message: 'Requires nodes:read' } } };
    });
    const h = harness({ get });
    const app = await h.connect();
    await expect(app.api.fetch('/nodes')).rejects.toMatchObject({
      code: 'http_403',
      message: 'Requires nodes:read',
    });
    h.bridge.detach();
  });

  it('validates notifications and rate limits a noisy page', async () => {
    let t = 1000;
    const h = harness({ now: () => t });
    const app = await h.connect();
    await app.showNotification({ level: 'success', text: 'Synced' });
    expect(h.calls.notify).toHaveBeenCalledWith('success', 'Synced');
    await expect(app.showNotification({ text: '   ' })).rejects.toMatchObject({
      code: 'invalid_params',
    });
    await expect(app.showNotification({ text: 'x'.repeat(301) })).rejects.toMatchObject({
      code: 'invalid_params',
    });
    for (let i = 1; i < NOTIFY_BURST; i++) await app.showNotification({ text: `n${i}` });
    await expect(app.showNotification({ text: 'one too many' })).rejects.toMatchObject({
      code: 'rate_limited',
    });
    t += 11_000;
    await app.showNotification({ text: 'later' });
    h.bridge.detach();
  });

  it('only navigates inside the application and clamps resize', async () => {
    const h = harness();
    const app = await h.connect();
    await app.navigate('/nodes/web01');
    expect(h.calls.navigate).toHaveBeenCalledWith('/nodes/web01');
    for (const bad of ['//evil.com', 'https://evil.com', '/api/v1/users', 'javascript:1']) {
      await expect(app.navigate(bad), bad).rejects.toMatchObject({ code: 'invalid_path' });
    }
    await app.resize(99999);
    expect(h.calls.resize).toHaveBeenLastCalledWith(4000);
    await app.resize(1);
    expect(h.calls.resize).toHaveBeenLastCalledWith(40);
    await expect(app.resize(Number.NaN)).rejects.toMatchObject({ code: 'invalid_params' });
    h.bridge.detach();
  });

  it('pushes theme changes, and the SDK applies only allowlisted tokens', async () => {
    const h = harness();
    const app = await h.connect();
    const seen = vi.fn();
    app.on('theme.changed', seen);
    h.setTheme('dark');
    h.bridge.pushTheme();
    await vi.waitFor(() => expect(seen).toHaveBeenCalled());
    const root = document.documentElement;
    expect(root.getAttribute('data-theme')).toBe('dark');
    expect(app.getContext().theme).toBe('dark');
    h.bridge.detach();
  });

  it('never applies hostile token names or values', async () => {
    const h = harness({
      theme: () => ({
        theme: 'light',
        tokens: {
          '--ov-color-primary': 'red; background: url(//evil)',
          '--evil': '#fff',
          '--ov-radius': '4px',
        } as never,
      }),
    });
    await h.connect();
    const root = document.documentElement;
    expect(root.style.getPropertyValue('--evil')).toBe('');
    expect(root.style.getPropertyValue('--ov-color-primary')).toBe('');
    expect(root.style.getPropertyValue('--ov-radius')).toBe('4px');
    h.bridge.detach();
  });
});

describe('theme tokens', () => {
  it('uses one protocol and one token list on both sides', () => {
    expect(PROTOCOL_NS).toBe('openvox.plugin');
    expect(PROTOCOL_VERSION).toBe(1);
    expect([...SDK_TOKENS]).toEqual([...THEME_TOKEN_NAMES]);
  });

  it('ships a valid value for every token in both themes', () => {
    for (const theme of ['light', 'dark'] as const) {
      const clean = sanitizeTokens(DEFAULT_TOKENS[theme]);
      expect(Object.keys(clean).sort()).toEqual([...THEME_TOKEN_NAMES].sort());
    }
  });

  it('drops names and values outside the allowlist', () => {
    const clean = sanitizeTokens({
      '--ov-color-primary': '#123456',
      '--ov-color-danger': 'expression(alert(1))',
      '--ov-radius': '1e9px',
      '--ov-font-body': 'Inter; color: red',
      '--other': '#fff',
    });
    expect(clean).toEqual({ '--ov-color-primary': '#123456' });
  });
});
