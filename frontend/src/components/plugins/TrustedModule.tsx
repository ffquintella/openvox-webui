import { Component, Suspense, lazy, useMemo, type ErrorInfo, type ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { Loader2 } from 'lucide-react';
import { usePluginModuleSession } from '../../hooks/usePlugins';
import { useAuthStore } from '../../stores/authStore';
import { useThemeStore } from '../../stores/themeStore';
import { usePluginToastStore } from '../../stores/pluginToastStore';
import {
  isValidIntegrity,
  loadTrustedModule,
  type ModuleTarget,
  type TrustedModuleContext,
  type TrustedModuleHost,
} from './moduleLoader';
import type { UiContribution } from '../../types';

const Unavailable = () => (
  <p className="text-sm text-gray-500">This content could not be loaded.</p>
);

/** Keeps a failing trusted module from taking the host page down with it. */
class ModuleErrorBoundary extends Component<
  { children: ReactNode; label: string },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`Trusted module ${this.props.label} failed`, error, info.componentStack);
  }

  render() {
    return this.state.failed ? <Unavailable /> : this.props.children;
  }
}

interface Props {
  contribution: UiContribution;
  slot: string;
  params?: Record<string, string>;
  /** Replaces the browser loader (tests). */
  loader?: typeof loadTrustedModule;
}

/**
 * A `module` contribution of a plugin version an administrator trusted (M31).
 * It runs in the host page without a sandbox; it is only listed by the server
 * while `plugins.allow_trusted_modules` is on, and is loaded on first render
 * after the browser checked its integrity.
 */
export default function TrustedModule({ contribution, slot, params, loader }: Props) {
  const { plugin_id: pluginId, id: contributionId } = contribution;
  const { data: session, isLoading, isError } = usePluginModuleSession(pluginId, contributionId);
  const theme = useThemeStore((s) => s.theme);
  const username = useAuthStore((s) => s.user?.username ?? '');
  const navigate = useNavigate();
  const pushToast = usePluginToastStore((s) => s.push);
  const load = loader ?? loadTrustedModule;

  // The SRI listed with the slot and the one of the session must agree.
  const target: ModuleTarget | null = useMemo(() => {
    if (!session || !isValidIntegrity(session.integrity)) return null;
    if (contribution.integrity && contribution.integrity !== session.integrity) return null;
    return {
      url: session.url,
      integrity: session.integrity,
      expose: session.expose,
      cacheKey: `${pluginId}@${session.version}/${contributionId}/${session.integrity}`,
    };
  }, [session, contribution.integrity, pluginId, contributionId]);

  const Page = useMemo(
    () => (target ? lazy(() => load(target).then((c) => ({ default: c }))) : null),
    [target, load]
  );

  const paramsKey = JSON.stringify(params ?? {});
  const context: TrustedModuleContext = useMemo(
    () => ({
      user: { username },
      locale: typeof navigator !== 'undefined' ? navigator.language : 'en',
      theme,
      slot,
      params: JSON.parse(paramsKey) as Record<string, string>,
      plugin_id: pluginId,
      contribution_id: contributionId,
    }),
    [username, theme, slot, paramsKey, pluginId, contributionId]
  );
  const source = contribution.plugin_name;
  const host: TrustedModuleHost = useMemo(
    () => ({
      navigate: (path) => navigate(path),
      notify: (level, text) => pushToast({ level, text: String(text).slice(0, 300), source }),
    }),
    [navigate, pushToast, source]
  );

  if (isLoading) return <Loader2 className="w-5 h-5 animate-spin text-primary-600" />;
  if (isError || !Page || !target) return <Unavailable />;
  return (
    <div data-testid={`plugin-module-${pluginId}-${contributionId}`}>
      <ModuleErrorBoundary key={target.cacheKey} label={`${pluginId}/${contributionId}`}>
        <Suspense fallback={<Loader2 className="w-5 h-5 animate-spin text-primary-600" />}>
          <Page context={context} host={host} />
        </Suspense>
      </ModuleErrorBoundary>
    </div>
  );
}
