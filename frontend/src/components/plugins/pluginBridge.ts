import { THEME_TOKEN_NAMES, type ThemeTokens } from '../../utils/pluginTheme';

/**
 * Host side of the contract with `@openvox/plugin-sdk` (T83, T84). Pure
 * logic with injected dependencies, so it is tested without a browser.
 *
 * Trust model: a sandboxed frame (no allow-same-origin) can only reach the
 * host by postMessage. The handshake is accepted only from the frame's own
 * window with the opaque origin "null"; every later message travels over a
 * private MessageChannel. Requests are validated here, the plugin's approved
 * capabilities gate what `fetch` may read, and the server still enforces the
 * signed-in user's RBAC on the real call.
 */

export const PROTOCOL_NS = 'openvox.plugin';
export const PROTOCOL_VERSION = 1;
export { THEME_TOKEN_NAMES };

export type NotifyLevel = 'info' | 'success' | 'warning' | 'error';
const NOTIFY_LEVELS: NotifyLevel[] = ['info', 'success', 'warning', 'error'];

export const MIN_FRAME_HEIGHT = 40;
export const MAX_FRAME_HEIGHT = 4000;
export const MAX_NOTIFY_TEXT = 300;
export const NOTIFY_BURST = 5;
export const NOTIFY_WINDOW_MS = 10_000;

/** Area of the host API → capability that opens it to a plugin page. */
export const API_AREA_SCOPES: Record<string, string> = {
  nodes: 'nodes:read',
  facts: 'facts:read',
  reports: 'reports:read',
  groups: 'groups:read',
  users: 'users:read',
};

export interface PluginFrameContext {
  user: { username: string };
  locale: string;
  theme: 'light' | 'dark';
  slot: string;
  params: Record<string, string>;
  plugin_id: string;
  contribution_id: string;
  api_scopes: string[];
}

export interface ThemePayload {
  theme: 'light' | 'dark';
  tokens: ThemeTokens;
}

export interface BridgeDeps {
  frame: () => Window | null;
  context: () => PluginFrameContext;
  theme: () => ThemePayload;
  apiScopes: string[];
  notify: (level: NotifyLevel, text: string) => void;
  navigate: (path: string) => void;
  resize: (height: number) => void;
  get: (path: string) => Promise<unknown>;
  /** Window that receives the frame's messages; defaults to `window`. */
  listen?: Window;
  now?: () => number;
}

export interface Bridge {
  pushTheme: () => void;
  pushContext: (patch: Partial<PluginFrameContext>) => void;
  detach: () => void;
}

export class BridgeError extends Error {
  constructor(
    public readonly code: string,
    message: string
  ) {
    super(message);
  }
}

function hasUnsafeChar(path: string): boolean {
  for (let i = 0; i < path.length; i++) {
    if (path.charCodeAt(i) < 0x20 || path[i] === '\\') return true;
  }
  return false;
}

/** Validates a proxied API path and returns the one to request. */
export function checkProxyPath(path: unknown, scopes: string[]): string {
  if (typeof path !== 'string' || path.length === 0 || path.length > 2048) {
    throw new BridgeError('invalid_path', 'The path must be a non-empty string');
  }
  if (!path.startsWith('/') || path.startsWith('//') || hasUnsafeChar(path)) {
    throw new BridgeError('invalid_path', 'The path must start with a single "/"');
  }
  const [pathname] = path.split('?', 1);
  const segments = pathname.split('/').slice(1);
  for (const raw of segments) {
    let seg: string;
    try {
      seg = decodeURIComponent(raw);
    } catch {
      throw new BridgeError('invalid_path', 'The path is not valid');
    }
    if (seg === '.' || seg === '..' || seg.includes('/') || seg.includes('\\')) {
      throw new BridgeError('invalid_path', 'The path is not valid');
    }
  }
  const area = segments[0] ?? '';
  const scope = Object.prototype.hasOwnProperty.call(API_AREA_SCOPES, area)
    ? API_AREA_SCOPES[area]
    : undefined;
  if (!scope) {
    throw new BridgeError('forbidden', `The host API area "${area}" is not available to plugins`);
  }
  if (!scopes.includes(scope)) {
    throw new BridgeError('forbidden', `This plugin has not been granted ${scope}`);
  }
  return path;
}

function checkNavigatePath(path: unknown): string {
  if (
    typeof path !== 'string' ||
    path.length === 0 ||
    path.length > 512 ||
    !path.startsWith('/') ||
    path.startsWith('//') ||
    path.startsWith('/api') ||
    hasUnsafeChar(path)
  ) {
    throw new BridgeError('invalid_path', 'Only routes of the application can be opened');
  }
  return path;
}

function describeError(e: unknown): { code: string; message: string } {
  if (e instanceof BridgeError) return { code: e.code, message: e.message };
  const r = e as { response?: { status?: number; data?: { message?: unknown } } };
  if (r?.response?.status) {
    const m = r.response.data?.message;
    return {
      code: `http_${r.response.status}`,
      message: typeof m === 'string' ? m : 'The request failed',
    };
  }
  return { code: 'failed', message: 'The request failed' };
}

export function attachBridge(deps: BridgeDeps): Bridge {
  const win = deps.listen ?? window;
  const now = deps.now ?? (() => Date.now());
  let port: MessagePort | null = null;
  let notifyTimes: number[] = [];

  async function handle(p: MessagePort, req: unknown) {
    const r = req as { id?: unknown; method?: unknown; params?: unknown };
    if (typeof r?.id !== 'number' || typeof r.method !== 'string') return;
    const id = r.id;
    const params = (r.params && typeof r.params === 'object' ? r.params : {}) as Record<
      string,
      unknown
    >;
    try {
      let result: unknown = null;
      switch (r.method) {
        case 'notify': {
          const t = now();
          notifyTimes = notifyTimes.filter((x) => t - x < NOTIFY_WINDOW_MS);
          if (notifyTimes.length >= NOTIFY_BURST) {
            throw new BridgeError('rate_limited', 'Too many notifications');
          }
          const text = typeof params.text === 'string' ? params.text.trim() : '';
          if (!text || text.length > MAX_NOTIFY_TEXT) {
            throw new BridgeError('invalid_params', 'The text must be 1 to 300 characters');
          }
          const level = NOTIFY_LEVELS.includes(params.level as NotifyLevel)
            ? (params.level as NotifyLevel)
            : 'info';
          notifyTimes.push(t);
          deps.notify(level, text);
          break;
        }
        case 'navigate':
          deps.navigate(checkNavigatePath(params.path));
          break;
        case 'resize': {
          const h = Number(params.height);
          if (!Number.isFinite(h)) throw new BridgeError('invalid_params', 'Invalid height');
          deps.resize(Math.min(MAX_FRAME_HEIGHT, Math.max(MIN_FRAME_HEIGHT, Math.round(h))));
          break;
        }
        case 'fetch':
          result = await deps.get(checkProxyPath(params.path, deps.apiScopes));
          break;
        default:
          throw new BridgeError('unknown_method', `Unknown method "${r.method}"`);
      }
      p.postMessage({ id, ok: true, result });
    } catch (e) {
      p.postMessage({ id, ok: false, error: describeError(e) });
    }
  }

  const onMessage = (e: MessageEvent) => {
    const frame = deps.frame();
    const d = e.data as { ns?: unknown; v?: unknown; type?: unknown } | null;
    // Only the frame's own window, and only with the opaque origin a sandbox
    // without allow-same-origin produces; anything else is not a plugin page.
    if (!frame || e.source !== frame || e.origin !== 'null') return;
    if (!d || d.ns !== PROTOCOL_NS || d.v !== PROTOCOL_VERSION || d.type !== 'hello') return;
    port?.close();
    const ch = new MessageChannel();
    const mine = ch.port1;
    port = mine;
    mine.onmessage = (ev: MessageEvent) => void handle(mine, ev.data);
    frame.postMessage(
      {
        ns: PROTOCOL_NS,
        v: PROTOCOL_VERSION,
        type: 'init',
        context: deps.context(),
        theme: deps.theme(),
      },
      '*',
      [ch.port2]
    );
  };

  win.addEventListener('message', onMessage);

  return {
    pushTheme: () => port?.postMessage({ event: 'theme.changed', data: deps.theme() }),
    pushContext: (patch) => port?.postMessage({ event: 'context.changed', data: patch }),
    detach: () => {
      win.removeEventListener('message', onMessage);
      port?.close();
      port = null;
    },
  };
}
