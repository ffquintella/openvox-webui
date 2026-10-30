// Trusted module pages (M31): loads a plugin's ES module into the host page.
// The module runs with the host's privileges, so the browser checks its SRI
// before it executes, and only the host's own React is shared with it.
import * as React from 'react';
import * as ReactDOM from 'react-dom';
import * as ReactDOMClient from 'react-dom/client';
import * as JsxRuntime from 'react/jsx-runtime';
import type { ComponentType } from 'react';
import type { PluginToastLevel } from '../../stores/pluginToastStore';

/** What a trusted module page receives as props. */
export interface TrustedModuleContext {
  user: { username: string };
  locale: string;
  theme: string;
  slot: string;
  params: Record<string, string>;
  plugin_id: string;
  contribution_id: string;
}

export interface TrustedModuleHost {
  navigate: (path: string) => void;
  notify: (level: PluginToastLevel, text: string) => void;
}

export interface TrustedModuleProps {
  context: TrustedModuleContext;
  host: TrustedModuleHost;
}

export interface ModuleTarget {
  url: string;
  integrity: string;
  expose: string;
  /** One module instance per plugin version, contribution and SRI. */
  cacheKey: string;
}

export interface LoaderDeps {
  /** Rejects unless the bytes at `url` match `integrity`. */
  verify: (url: string, integrity: string) => Promise<void>;
  importModule: (url: string) => Promise<unknown>;
}

const SRI_RE = /^(sha384|sha512)-[A-Za-z0-9+/]+={0,2}$/;
const MODULE_URL_RE = /^\/api\/v1\/plugins\/ui-modules\/[0-9a-f]{64}\/[A-Za-z0-9._\-/]+\.m?js$/;
const EXPOSE_RE = /^\.\/[A-Za-z0-9._\-/]{1,62}$/;

export function isValidIntegrity(value: unknown): value is string {
  return typeof value === 'string' && SRI_RE.test(value);
}

/** Only the host's own module route is ever imported. */
export function isModuleUrl(url: string): boolean {
  return MODULE_URL_RE.test(url) && !url.includes('/../') && !url.includes('/./');
}

type Factory = () => unknown;
interface ShareEntry {
  get: () => Promise<Factory>;
  loaded: 1;
  eager: true;
  from: string;
}
export type ShareScope = Record<string, Record<string, ShareEntry>>;

/** Module Federation share scope: react and react-dom are the host's singletons. */
export function createShareScope(): ShareScope {
  const entry = (mod: unknown): Record<string, ShareEntry> => ({
    [React.version]: {
      get: () => Promise.resolve(() => mod),
      loaded: 1,
      eager: true,
      from: 'openvox-webui',
    },
  });
  return {
    react: entry(React),
    'react-dom': entry(ReactDOM),
    'react-dom/client': entry(ReactDOMClient),
    'react/jsx-runtime': entry(JsxRuntime),
  };
}

/** Fetch with SRI (every browser), then modulepreload with SRI where supported. */
async function verifyInBrowser(url: string, integrity: string): Promise<void> {
  const res = await fetch(url, {
    integrity,
    credentials: 'omit',
    cache: 'no-store',
    referrerPolicy: 'no-referrer',
  });
  if (!res.ok) throw new Error(`The module could not be fetched (${res.status})`);
  await res.arrayBuffer();
  const link = document.createElement('link');
  if (!link.relList?.supports?.('modulepreload')) return;
  // The preloaded, verified module is the one import() then takes from the module map.
  await new Promise<void>((resolve, reject) => {
    link.rel = 'modulepreload';
    link.integrity = integrity;
    link.href = url;
    link.onload = () => resolve();
    link.onerror = () => reject(new Error('The module failed its integrity check'));
    document.head.appendChild(link);
  }).finally(() => link.remove());
}

export const browserDeps: LoaderDeps = {
  verify: verifyInBrowser,
  importModule: (url) => import(/* @vite-ignore */ url),
};

interface Container {
  init?: (scope: ShareScope) => unknown;
  get?: (name: string) => unknown;
  default?: unknown;
}

function isComponent(value: unknown): value is ComponentType<TrustedModuleProps> {
  return (
    typeof value === 'function' ||
    (typeof value === 'object' && value !== null && '$$typeof' in value)
  );
}

const cache = new Map<string, Promise<ComponentType<TrustedModuleProps>>>();

/** Forgets loaded modules (tests). */
export function resetModuleCache(): void {
  cache.clear();
}

async function load(
  target: ModuleTarget,
  deps: LoaderDeps
): Promise<ComponentType<TrustedModuleProps>> {
  if (!isValidIntegrity(target.integrity)) {
    throw new Error('The module has no valid integrity value');
  }
  if (!isModuleUrl(target.url)) throw new Error('The module URL is not a host module URL');
  if (!EXPOSE_RE.test(target.expose) || target.expose.includes('..')) {
    throw new Error('The module name is not valid');
  }
  await deps.verify(target.url, target.integrity);
  const container = (await deps.importModule(target.url)) as Container;
  let mod: Container = container;
  if (typeof container?.get === 'function') {
    // A Module Federation container: share the host's React, then pick the page.
    if (typeof container.init === 'function') await container.init(createShareScope());
    const factory = await container.get(target.expose);
    mod = (typeof factory === 'function' ? (factory as Factory)() : factory) as Container;
  }
  const component = mod?.default;
  if (!isComponent(component)) throw new Error('The module does not export a React component');
  return component;
}

/** Loads a trusted module once per cache key; a failure can be retried. */
export function loadTrustedModule(
  target: ModuleTarget,
  deps: LoaderDeps = browserDeps
): Promise<ComponentType<TrustedModuleProps>> {
  const hit = cache.get(target.cacheKey);
  if (hit) return hit;
  const pending = load(target, deps);
  cache.set(target.cacheKey, pending);
  pending.catch(() => cache.delete(target.cacheKey));
  return pending;
}
