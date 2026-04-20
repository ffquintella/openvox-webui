import type { AlertCondition } from '../types';

/**
 * DEPRECATED: ConditionBuilder component
 * 
 * This component is kept for backward compatibility but is not currently used.
 * Condition editing is now handled directly in the Alerting.tsx component.
 */

interface ConditionBuilderProps {
  conditions: AlertCondition[];
  operator: 'AND' | 'OR';
  onChange: (conditions: AlertCondition[], operator: 'AND' | 'OR') => void;
}

export default function ConditionBuilder(_props: ConditionBuilderProps) {
  return (
    <div className="rounded-lg bg-yellow-50 p-4 text-yellow-800 dark:bg-yellow-900/20 dark:text-yellow-200">
      <p className="text-sm">
        <strong>Note:</strong> ConditionBuilder is deprecated. Use Alerting.tsx for condition editing.
      </p>
    </div>
  );
}

