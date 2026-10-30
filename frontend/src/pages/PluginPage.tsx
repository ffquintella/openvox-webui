import { useLocation } from 'react-router-dom';
import { Info, Loader2 } from 'lucide-react';
import { usePluginSlots, usePluginContribution } from '../hooks/usePlugins';
import DeclarativeRenderer from '../components/plugins/DeclarativeRenderer';
import PluginFrame from '../components/plugins/PluginFrame';
import TrustedModule from '../components/plugins/TrustedModule';

function Notice({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="max-w-2xl mx-auto mt-12 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-6">
      <div className="flex items-start gap-3">
        <Info className="w-5 h-5 text-gray-400 mt-0.5" />
        <div>
          <h2 className="text-lg font-semibold text-gray-900 dark:text-white">{title}</h2>
          <p className="mt-1 text-sm text-gray-600 dark:text-gray-300">{children}</p>
        </div>
      </div>
    </div>
  );
}

function normalizePath(p: string): string {
  return p.length > 1 ? p.replace(/\/+$/, '') : p;
}

/** Page contributed by a plugin (`page` slot), mounted at /p/:pluginId/*. */
export default function PluginPage() {
  const location = useLocation();
  const { data: slots, isLoading } = usePluginSlots();

  const pathname = normalizePath(location.pathname);
  const pluginId = pathname.split('/')[2];
  const contribution = (slots?.slots['page'] ?? []).find(
    (c) => c.plugin_id === pluginId && c.path !== null && normalizePath(c.path) === pathname
  );

  const {
    data: detail,
    isLoading: detailLoading,
    isError,
  } = usePluginContribution(
    contribution?.render === 'declarative' ? contribution.plugin_id : undefined,
    contribution?.id
  );

  if (isLoading || (contribution?.render === 'declarative' && detailLoading)) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="w-8 h-8 animate-spin text-primary-600" />
      </div>
    );
  }

  if (!contribution) {
    return (
      <Notice title="Page not found">
        No enabled plugin provides this page, or you do not have access to it.
      </Notice>
    );
  }

  if (
    contribution.render !== 'declarative' &&
    contribution.render !== 'iframe' &&
    contribution.render !== 'module'
  ) {
    return (
      <Notice title={contribution.title ?? contribution.plugin_name}>
        The {contribution.render} render type is not supported.
      </Notice>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold text-gray-900 dark:text-white">
          {contribution.title ?? contribution.plugin_name}
        </h1>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          Provided by {contribution.plugin_name}
        </p>
      </div>
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        {contribution.render === 'iframe' ? (
          <PluginFrame contribution={contribution} slot="page" fill />
        ) : contribution.render === 'module' ? (
          <TrustedModule contribution={contribution} slot="page" />
        ) : isError || !detail ? (
          <p className="text-sm text-gray-500">This page could not be loaded.</p>
        ) : (
          <DeclarativeRenderer spec={detail.spec} />
        )}
      </div>
    </div>
  );
}
