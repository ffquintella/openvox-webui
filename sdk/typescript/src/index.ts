/**
 * `@openvox/plugin-sdk`: the client side of the host bridge for plugin pages
 * served in a sandboxed iframe (PLUGIN_PLATFORM.md section 9.3).
 *
 *   const app = await connect();
 *   const ctx = app.getContext();
 *   const nodes = await app.api.fetch('/nodes?limit=20');
 */

import {
  PROTOCOL_NS,
  PROTOCOL_VERSION,
  THEME_TOKEN_NAMES,
  type HelloMessage,
  type HostEvent,
  type InitMessage,
  type NotifyLevel,
  type PluginContext,
  type RpcRequest,
  type RpcResponse,
  type ThemePayload,
  type ThemeTokens,
} from "./protocol";

export * from "./protocol";

export class OpenVoxError extends Error {
  constructor(
    public readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "OpenVoxError";
  }
}

type Listener = (data: unknown) => void;

export interface OpenVoxApp {
  getContext(): PluginContext;
  showNotification(n: { level?: NotifyLevel; text: string }): Promise<void>;
  navigate(path: string): Promise<void>;
  /** Asks the host to size the frame; ignored for full pages. */
  resize(height: number): Promise<void>;
  api: {
    /** Read-only call to the host API as the signed-in user (GET only). */
    fetch<T = unknown>(path: string): Promise<T>;
  };
  on(event: HostEvent["event"], listener: Listener): () => void;
}

export interface ConnectOptions {
  /** Window to talk to; defaults to `window.parent`. */
  parent?: Window;
  /** Window whose document receives the theme; defaults to `window`. */
  target?: Window;
  timeoutMs?: number;
}

const SAFE_TOKEN_VALUE =
  /^(#[0-9a-fA-F]{3,8}|(rgb|rgba|hsl|hsla)\([0-9 ,.%/-]+\)|[A-Za-z0-9 ,'"_.-]{1,200})$/;

/** Applies only allowlisted token names with plain values, never raw CSS. */
export function applyTheme(doc: Document, payload: ThemePayload): void {
  const root = doc.documentElement;
  root.setAttribute("data-theme", payload.theme);
  root.style.colorScheme = payload.theme;
  const names = new Set<string>(THEME_TOKEN_NAMES);
  for (const [name, value] of Object.entries(payload.tokens as ThemeTokens)) {
    if (
      names.has(name) &&
      typeof value === "string" &&
      SAFE_TOKEN_VALUE.test(value)
    ) {
      root.style.setProperty(name, value);
    }
  }
}

export function connect(options: ConnectOptions = {}): Promise<OpenVoxApp> {
  const target = options.target ?? window;
  const parent = options.parent ?? window.parent;
  const timeoutMs = options.timeoutMs ?? 10_000;

  return new Promise<OpenVoxApp>((resolve, reject) => {
    let settled = false;
    const timer = setTimeout(() => {
      if (!settled) {
        settled = true;
        target.removeEventListener("message", onInit);
        reject(
          new OpenVoxError(
            "handshake_timeout",
            "The host did not answer the handshake",
          ),
        );
      }
    }, timeoutMs);

    function onInit(e: MessageEvent) {
      const msg = e.data as Partial<InitMessage> | null;
      if (
        settled ||
        e.source !== parent ||
        !msg ||
        msg.ns !== PROTOCOL_NS ||
        msg.v !== PROTOCOL_VERSION ||
        msg.type !== "init" ||
        !e.ports[0] ||
        !msg.context ||
        !msg.theme
      ) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      target.removeEventListener("message", onInit);
      resolve(buildApp(e.ports[0], msg.context, msg.theme, target));
    }

    target.addEventListener("message", onInit);
    const hello: HelloMessage = {
      ns: PROTOCOL_NS,
      v: PROTOCOL_VERSION,
      type: "hello",
    };
    // The host page is the only possible receiver; a frame cannot learn its
    // parent's origin from inside an opaque-origin sandbox.
    parent.postMessage(hello, "*");
  });
}

function buildApp(
  port: MessagePort,
  initial: PluginContext,
  theme: ThemePayload,
  target: Window,
): OpenVoxApp {
  let context = initial;
  let nextId = 1;
  const pending = new Map<
    number,
    { ok: (v: unknown) => void; err: (e: OpenVoxError) => void }
  >();
  const listeners = new Map<string, Set<Listener>>();

  applyTheme(target.document, theme);

  const emit = (event: string, data: unknown) => {
    for (const l of listeners.get(event) ?? []) l(data);
  };

  port.onmessage = (e: MessageEvent) => {
    const m = e.data as RpcResponse | HostEvent;
    if (m && typeof m === "object" && "event" in m) {
      if (m.event === "theme.changed") {
        const payload = m.data as ThemePayload;
        applyTheme(target.document, payload);
        context = { ...context, theme: payload.theme };
      } else if (m.event === "context.changed") {
        context = { ...context, ...(m.data as Partial<PluginContext>) };
      }
      emit(m.event, m.data);
      return;
    }
    if (m && typeof m === "object" && "id" in m) {
      const p = pending.get(m.id);
      if (!p) return;
      pending.delete(m.id);
      if (m.ok) p.ok(m.result);
      else p.err(new OpenVoxError(m.error.code, m.error.message));
    }
  };

  const call = <T>(
    method: RpcRequest["method"],
    params: Record<string, unknown>,
  ) =>
    new Promise<T>((ok, err) => {
      const id = nextId++;
      pending.set(id, { ok: (v) => ok(v as T), err });
      const req: RpcRequest = { id, method, params };
      port.postMessage(req);
    });

  return {
    getContext: () => context,
    showNotification: (n) =>
      call<void>("notify", { level: n.level ?? "info", text: n.text }).then(
        () => undefined,
      ),
    navigate: (path) => call<void>("navigate", { path }).then(() => undefined),
    resize: (height) => call<void>("resize", { height }).then(() => undefined),
    api: { fetch: <T = unknown>(path: string) => call<T>("fetch", { path }) },
    on(event, listener) {
      const set = listeners.get(event) ?? new Set<Listener>();
      set.add(listener);
      listeners.set(event, set);
      return () => set.delete(listener);
    },
  };
}
