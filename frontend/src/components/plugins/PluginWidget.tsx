import type { UiContribution } from '../../types';
import { PluginContributionView } from './PluginSlot';

/** Dashboard card for one `dashboard.widgets` contribution (declarative or iframe). */
export default function PluginWidget({ contribution }: { contribution: UiContribution }) {
  return <PluginContributionView contribution={contribution} slot="dashboard.widgets" />;
}
