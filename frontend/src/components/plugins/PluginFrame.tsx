import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Loader2 } from 'lucide-react';
import { pluginsApi } from '../../services/api';
import { usePluginCatalog, usePluginUiSession } from '../../hooks/usePlugins';
import { useAuthStore } from '../../stores/authStore';
import { useThemeStore } from '../../stores/themeStore';
import { usePluginToastStore } from '../../stores/pluginToastStore';
import { readThemeTokens } from '../../utils/pluginTheme';
import { attachBridge, type Bridge, type PluginFrameContext } from './pluginBridge';
import type { UiContribution } from '../../types';

interface Props {
  contribution: UiContribution;
  /** Slot the frame is mounted in; also the context contract it receives. */
  slot: string;
  params?: Record<string, string>;
  /** Full pages fill the viewport instead of following `resize`. */
  fill?: boolean;
}

/**
 * Sandboxed iframe for one plugin contribution: `allow-scripts allow-forms`
 * and no `allow-same-origin`, so the page gets an opaque origin, no cookies
 * and no access to the host DOM or token. It loads lazily and talks to the
 * host only through the SDK bridge.
 */
export default function PluginFrame({ contribution, slot, params, fill }: Props) {
  const { plugin_id: pluginId, id: contributionId } = contribution;
  const { data: session, isLoading, isError } = usePluginUiSession(pluginId, contributionId);
  const { data: catalog } = usePluginCatalog();
  const theme = useThemeStore((s) => s.theme);
  const user = useAuthStore((s) => s.user);
  const navigate = useNavigate();
  const pushToast = usePluginToastStore((s) => s.push);
  const frameRef = useRef<HTMLIFrameElement>(null);
  const bridgeRef = useRef<Bridge | null>(null);

  const startHeight = catalog?.slots.find((s) => s.name === slot)?.default_height ?? 240;
  const [height, setHeight] = useState<number | null>(null);

  // `params` is compared by value, so an inline object does not reload effects.
  const paramsKey = JSON.stringify(params ?? {});
  const context: PluginFrameContext = useMemo(
    () => ({
      user: { username: user?.username ?? '' },
      locale: typeof navigator !== 'undefined' ? navigator.language : 'en',
      theme,
      slot,
      params: JSON.parse(paramsKey) as Record<string, string>,
      plugin_id: pluginId,
      contribution_id: contributionId,
      api_scopes: session?.api_scopes ?? [],
    }),
    [user?.username, theme, slot, paramsKey, pluginId, contributionId, session]
  );
  const contextRef = useRef(context);
  const themeRef = useRef(theme);
  useEffect(() => {
    contextRef.current = context;
    themeRef.current = theme;
  }, [context, theme]);
  const source = contribution.plugin_name;

  useEffect(() => {
    if (!session) return;
    const bridge = attachBridge({
      frame: () => frameRef.current?.contentWindow ?? null,
      context: () => contextRef.current,
      theme: () => ({ theme: themeRef.current, tokens: readThemeTokens(themeRef.current) }),
      apiScopes: session.api_scopes,
      notify: (level, text) => pushToast({ level, text, source }),
      navigate: (path) => navigate(path),
      resize: (h) => setHeight(h),
      get: async (path) => (await pluginsApi.proxyGet(path)).data,
    });
    bridgeRef.current = bridge;
    return () => {
      bridge.detach();
      bridgeRef.current = null;
    };
  }, [session, navigate, pushToast, source]);

  useEffect(() => {
    bridgeRef.current?.pushTheme();
  }, [theme]);

  useEffect(() => {
    bridgeRef.current?.pushContext({ params: JSON.parse(paramsKey) as Record<string, string> });
  }, [paramsKey]);

  if (isLoading) {
    return <Loader2 className="w-5 h-5 animate-spin text-primary-600" />;
  }
  if (isError || !session) {
    return <p className="text-sm text-gray-500">This content could not be loaded.</p>;
  }

  return (
    <iframe
      ref={frameRef}
      src={session.url}
      title={contribution.title ?? `${source}: ${contributionId}`}
      sandbox="allow-scripts allow-forms"
      loading="lazy"
      referrerPolicy="no-referrer"
      data-testid={`plugin-frame-${pluginId}-${contributionId}`}
      className={fill ? 'w-full border-0 min-h-[480px] h-[calc(100vh-16rem)]' : 'w-full border-0'}
      style={fill ? undefined : { height: `${height ?? startHeight}px` }}
    />
  );
}
