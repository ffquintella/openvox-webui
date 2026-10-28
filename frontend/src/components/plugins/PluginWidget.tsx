import { Loader2 } from 'lucide-react';
import { usePluginContribution } from '../../hooks/usePlugins';
import DeclarativeRenderer from './DeclarativeRenderer';
import type { UiContribution } from '../../types';

/** Dashboard card for one `dashboard.widgets` contribution; fetches its spec lazily. */
export default function PluginWidget({ contribution }: { contribution: UiContribution }) {
  const { data, isLoading, isError } = usePluginContribution(
    contribution.plugin_id,
    contribution.id
  );

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-gray-900 dark:text-white">
          {contribution.title ?? contribution.id}
        </h3>
        <span className="text-xs text-gray-400">{contribution.plugin_name}</span>
      </div>
      {isLoading ? (
        <Loader2 className="w-5 h-5 animate-spin text-primary-600" />
      ) : isError || !data ? (
        <p className="text-sm text-gray-500">This widget could not be loaded.</p>
      ) : (
        <DeclarativeRenderer spec={data.spec} />
      )}
    </div>
  );
}
