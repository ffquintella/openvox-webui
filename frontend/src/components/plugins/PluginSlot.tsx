import { Loader2 } from 'lucide-react';
import clsx from 'clsx';
import { usePluginContribution, usePluginSlot } from '../../hooks/usePlugins';
import DeclarativeRenderer from './DeclarativeRenderer';
import PluginFrame from './PluginFrame';
import type { UiContribution } from '../../types';

export type SlotVariant = 'card' | 'bare';

function DeclarativeBody({ contribution }: { contribution: UiContribution }) {
  const { data, isLoading, isError } = usePluginContribution(
    contribution.plugin_id,
    contribution.id
  );
  if (isLoading) return <Loader2 className="w-5 h-5 animate-spin text-primary-600" />;
  if (isError || !data)
    return <p className="text-sm text-gray-500">This content could not be loaded.</p>;
  return <DeclarativeRenderer spec={data.spec} />;
}

/** One contribution, whichever way it renders. */
export function PluginContributionView({
  contribution,
  slot,
  params,
  variant = 'card',
}: {
  contribution: UiContribution;
  slot: string;
  params?: Record<string, string>;
  variant?: SlotVariant;
}) {
  const body =
    contribution.render === 'iframe' ? (
      <PluginFrame contribution={contribution} slot={slot} params={params} />
    ) : contribution.render === 'declarative' ? (
      <DeclarativeBody contribution={contribution} />
    ) : null;
  if (!body) return null;
  if (variant === 'bare') return body;
  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-gray-900 dark:text-white">
          {contribution.title ?? contribution.id}
        </h3>
        <span className="text-xs text-gray-400">{contribution.plugin_name}</span>
      </div>
      {body}
    </div>
  );
}

/**
 * Mount point for a slot of the catalog: renders every contribution the
 * server allows this user to see, in order, up to the slot's `max_items`.
 * Renders nothing when no plugin contributes.
 */
export default function PluginSlot({
  slot,
  params,
  variant = 'card',
  className,
  heading,
}: {
  slot: string;
  params?: Record<string, string>;
  variant?: SlotVariant;
  className?: string;
  /** Shown above the contributions, only when there are any. */
  heading?: string;
}) {
  const items = usePluginSlot(slot).filter(
    (c) => c.render === 'declarative' || c.render === 'iframe'
  );
  if (items.length === 0) return null;
  return (
    <div className={clsx(className)} data-testid={`plugin-slot-${slot}`}>
      {heading && (
        <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">{heading}</h2>
      )}
      {items.map((c) => (
        <PluginContributionView
          key={`${c.plugin_id}:${c.id}`}
          contribution={c}
          slot={slot}
          params={params}
          variant={variant}
        />
      ))}
    </div>
  );
}
