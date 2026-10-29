/**
 * Wire contract between the host page and a sandboxed plugin page.
 *
 * Handshake: the page posts `hello` to its parent; the host answers with
 * `init` carrying one end of a MessageChannel. Everything after that travels
 * over the channel, so no other frame can forge or read it.
 */

export const PROTOCOL_NS = "openvox.plugin";
export const PROTOCOL_VERSION = 1;

export type NotifyLevel = "info" | "success" | "warning" | "error";
export type ThemeName = "light" | "dark";

/** Design tokens the host may send; the SDK ignores any other name. */
export const THEME_TOKEN_NAMES = [
  "--ov-color-bg",
  "--ov-color-surface",
  "--ov-color-surface-alt",
  "--ov-color-border",
  "--ov-color-text",
  "--ov-color-text-muted",
  "--ov-color-primary",
  "--ov-color-primary-contrast",
  "--ov-color-success",
  "--ov-color-warning",
  "--ov-color-danger",
  "--ov-color-info",
  "--ov-font-body",
  "--ov-font-mono",
  "--ov-font-size-base",
  "--ov-radius",
  "--ov-radius-lg",
] as const;

export type ThemeTokenName = (typeof THEME_TOKEN_NAMES)[number];
export type ThemeTokens = Partial<Record<ThemeTokenName, string>>;

export interface PluginContext {
  user: { username: string };
  locale: string;
  theme: ThemeName;
  /** The slot the page is mounted in, for example `node.detail.tabs`. */
  slot: string;
  /** Slot context, for example `{ certname: "web01" }`. */
  params: Record<string, string>;
  plugin_id: string;
  contribution_id: string;
  /** Host API areas `app.api.fetch` may read, for example `nodes:read`. */
  api_scopes: string[];
}

export interface ThemePayload {
  theme: ThemeName;
  tokens: ThemeTokens;
}

export interface InitMessage {
  ns: typeof PROTOCOL_NS;
  v: typeof PROTOCOL_VERSION;
  type: "init";
  context: PluginContext;
  theme: ThemePayload;
}

export interface HelloMessage {
  ns: typeof PROTOCOL_NS;
  v: typeof PROTOCOL_VERSION;
  type: "hello";
}

export interface RpcRequest {
  id: number;
  method: "notify" | "fetch" | "navigate" | "resize";
  params: Record<string, unknown>;
}

export type RpcResponse =
  | { id: number; ok: true; result: unknown }
  | { id: number; ok: false; error: { code: string; message: string } };

export interface HostEvent {
  event: "theme.changed" | "context.changed";
  data: unknown;
}
