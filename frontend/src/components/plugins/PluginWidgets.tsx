import { usePluginSlots } from '../../hooks/usePlugins';
import PluginWidget from './PluginWidget';

/** Extra dashboard cards from plugins (declarative render only). */
export default function PluginWidgets() {
  const { data } = usePluginSlots();
  const widgets = (data?.slots['dashboard.widgets'] ?? []).filter(
    (c) => c.render === 'declarative'
  );
  if (widgets.length === 0) return null;
  return (
    <div className="mt-8" data-testid="plugin-widgets">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">Plugins</h2>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {widgets.map((w) => (
          <PluginWidget key={`${w.plugin_id}:${w.id}`} contribution={w} />
        ))}
      </div>
    </div>
  );
}
