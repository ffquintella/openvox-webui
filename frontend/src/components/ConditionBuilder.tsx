import { useState } from 'react';
import { Plus, Trash2, Copy } from 'lucide-react';
import type { AlertCondition, AlertConditionType, ConditionValueOperator } from '../types';

interface ConditionBuilderProps {
  conditions: AlertCondition[];
  operator: 'AND' | 'OR';
  onChange: (conditions: AlertCondition[], operator: 'AND' | 'OR') => void;
}

const CONDITION_TYPES: { value: AlertConditionType; label: string; description: string }[] = [
  {
    value: 'NodeStatus',
    label: 'Node Status',
    description: 'Match nodes by their current status (failed, changed, unchanged)',
  },
  {
    value: 'NodeFact',
    label: 'Node Fact',
    description: 'Match nodes by fact values (e.g., os.family = "RedHat")',
  },
  {
    value: 'ReportMetric',
    label: 'Report Metric',
    description: 'Match nodes by report metrics (e.g., failed_resources > 5)',
  },
  {
    value: 'LastReportTime',
    label: 'Stale Nodes',
    description: 'Match nodes that haven\'t reported in X hours',
  },
  {
    value: 'ConsecutiveFailures',
    label: 'Consecutive Failures',
    description: 'Match nodes with N consecutive failures in time window',
  },
  {
    value: 'ConsecutiveChanges',
    label: 'Consecutive Changes',
    description: 'Match nodes with N consecutive changes in time window',
  },
  {
    value: 'ClassChangeFrequency',
    label: 'Class Change Frequency',
    description: 'Match nodes with class changed N+ times in time window',
  },
  {
    value: 'EnvironmentFilter',
    label: 'Environment Filter',
    description: 'Filter nodes by environment name',
  },
  {
    value: 'GroupFilter',
    label: 'Group Filter',
    description: 'Filter nodes by group membership',
  },
  {
    value: 'NodeCountThreshold',
    label: 'Node Count Threshold',
    description: 'Alert when node count is outside min/max range',
  },
  {
    value: 'TimeWindowFilter',
    label: 'Time Window Filter',
    description: 'Only evaluate during specific hours/days',
  },
];

const STRING_OPERATORS: ConditionValueOperator[] = ['=', '!=', '~', '!~', 'contains', 'not_contains', 'in', 'not_in'];
const NUMERIC_OPERATORS: ConditionValueOperator[] = ['=', '!=', '>', '>=', '<', '<='];
const BOOLEAN_OPERATORS: ConditionValueOperator[] = ['=', '!='];
const EXISTENCE_OPERATORS: ConditionValueOperator[] = ['exists', 'not_exists'];

const NODE_STATUSES = ['failed', 'changed', 'unchanged', 'unreported', 'unknown'];

function getOperatorsForType(type: AlertConditionType): ConditionValueOperator[] {
  switch (type) {
    case 'NodeStatus':
    case 'EnvironmentFilter':
    case 'GroupFilter':
      return STRING_OPERATORS;
    case 'NodeFact':
      return [...STRING_OPERATORS, ...BOOLEAN_OPERATORS, ...EXISTENCE_OPERATORS];
    case 'ReportMetric':
    case 'NodeCountThreshold':
      return NUMERIC_OPERATORS;
    case 'LastReportTime':
    case 'ConsecutiveFailures':
    case 'ConsecutiveChanges':
    case 'ClassChangeFrequency':
      return ['>'];
    default:
      return STRING_OPERATORS;
  }
}

function ConditionEditor({
  condition,
  onChange,
  onDelete,
}: {
  condition: AlertCondition;
  onChange: (condition: AlertCondition) => void;
  onDelete: () => void;
}) {
  const operators = getOperatorsForType(condition.type);

  const updateConfig = (updates: Partial<AlertCondition['config']>) => {
    onChange({
      ...condition,
      config: { ...condition.config, ...updates },
    });
  };

  return (
    <div className="rounded-lg border border-gray-300 bg-gray-50 p-4 dark:border-gray-600 dark:bg-gray-800">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center space-x-2">
          <input
            type="checkbox"
            checked={condition.enabled}
            onChange={(e) => onChange({ ...condition, enabled: e.target.checked })}
            className="rounded border-gray-300"
          />
          <select
            value={condition.type}
            onChange={(e) =>
              onChange({
                type: e.target.value as AlertConditionType,
                enabled: condition.enabled,
                config: {},
              })
            }
            className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
          >
            {CONDITION_TYPES.map((ct) => (
              <option key={ct.value} value={ct.value}>
                {ct.label}
              </option>
            ))}
          </select>
        </div>
        <button
          onClick={onDelete}
          className="text-red-600 hover:text-red-800 dark:text-red-400"
          title="Delete condition"
        >
          <Trash2 className="h-4 w-4" />
        </button>
      </div>

      <div className="space-y-3">
        {/* NodeStatus */}
        {condition.type === 'NodeStatus' && (
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Operator
              </label>
              <select
                value={condition.config.operator || '='}
                onChange={(e) => updateConfig({ operator: e.target.value as ConditionValueOperator })}
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              >
                {operators.map((op) => (
                  <option key={op} value={op}>
                    {op}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Status
              </label>
              {condition.config.operator === 'in' || condition.config.operator === 'not_in' ? (
                <select
                  multiple
                  value={(condition.config.value as string[]) || []}
                  onChange={(e) =>
                    updateConfig({
                      value: Array.from(e.target.selectedOptions, (option) => option.value),
                    })
                  }
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  {NODE_STATUSES.map((status) => (
                    <option key={status} value={status}>
                      {status}
                    </option>
                  ))}
                </select>
              ) : (
                <select
                  value={(condition.config.value as string) || 'failed'}
                  onChange={(e) => updateConfig({ value: e.target.value })}
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  {NODE_STATUSES.map((status) => (
                    <option key={status} value={status}>
                      {status}
                    </option>
                  ))}
                </select>
              )}
            </div>
          </div>
        )}

        {/* NodeFact */}
        {condition.type === 'NodeFact' && (
          <div className="space-y-2">
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Fact Path
              </label>
              <input
                type="text"
                value={condition.config.fact_path || ''}
                onChange={(e) => updateConfig({ fact_path: e.target.value })}
                placeholder="e.g., os.family or memory.system.total_bytes"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  Operator
                </label>
                <select
                  value={condition.config.operator || '='}
                  onChange={(e) => updateConfig({ operator: e.target.value as ConditionValueOperator })}
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  {getOperatorsForType('NodeFact').map((op) => (
                    <option key={op} value={op}>
                      {op}
                    </option>
                  ))}
                </select>
              </div>
              {condition.config.operator !== 'exists' && condition.config.operator !== 'not_exists' && (
                <div>
                  <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                    Value
                  </label>
                  <input
                    type="text"
                    value={(condition.config.value as string) || ''}
                    onChange={(e) => updateConfig({ value: e.target.value })}
                    placeholder="e.g., RedHat or 16384"
                    className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                  />
                </div>
              )}
            </div>
          </div>
        )}

        {/* ReportMetric */}
        {condition.type === 'ReportMetric' && (
          <div className="space-y-2">
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Metric Name
              </label>
              <select
                value={condition.config.metric_name || 'failed_resources'}
                onChange={(e) => updateConfig({ metric_name: e.target.value })}
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              >
                <option value="failed_resources">Failed Resources</option>
                <option value="changed_resources">Changed Resources</option>
                <option value="total_resources">Total Resources</option>
                <option value="execution_time">Execution Time</option>
              </select>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  Operator
                </label>
                <select
                  value={condition.config.operator || '>'}
                  onChange={(e) => updateConfig({ operator: e.target.value as ConditionValueOperator })}
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  {NUMERIC_OPERATORS.map((op) => (
                    <option key={op} value={op}>
                      {op}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  Threshold
                </label>
                <input
                  type="number"
                  value={condition.config.threshold || 0}
                  onChange={(e) => updateConfig({ threshold: parseInt(e.target.value) })}
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                />
              </div>
            </div>
          </div>
        )}

        {/* LastReportTime */}
        {condition.type === 'LastReportTime' && (
          <div>
            <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
              Hours Since Last Report
            </label>
            <input
              type="number"
              value={condition.config.threshold_hours || 24}
              onChange={(e) => updateConfig({ threshold_hours: parseInt(e.target.value) })}
              placeholder="24"
              min="1"
              className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
            />
            <p className="mt-1 text-xs text-gray-500">Alert when nodes haven't reported in this many hours</p>
          </div>
        )}

        {/* ConsecutiveFailures */}
        {condition.type === 'ConsecutiveFailures' && (
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Min Failures
              </label>
              <input
                type="number"
                value={condition.config.threshold || 3}
                onChange={(e) => updateConfig({ threshold: parseInt(e.target.value) })}
                min="1"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Time Window (hours)
              </label>
              <input
                type="number"
                value={condition.config.time_window_hours || 24}
                onChange={(e) => updateConfig({ time_window_hours: parseInt(e.target.value) })}
                min="1"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
          </div>
        )}

        {/* ConsecutiveChanges */}
        {condition.type === 'ConsecutiveChanges' && (
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Min Changes
              </label>
              <input
                type="number"
                value={condition.config.threshold || 5}
                onChange={(e) => updateConfig({ threshold: parseInt(e.target.value) })}
                min="1"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Time Window (hours)
              </label>
              <input
                type="number"
                value={condition.config.time_window_hours || 24}
                onChange={(e) => updateConfig({ time_window_hours: parseInt(e.target.value) })}
                min="1"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
          </div>
        )}

        {/* ClassChangeFrequency */}
        {condition.type === 'ClassChangeFrequency' && (
          <div className="space-y-2">
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Class Name
              </label>
              <input
                type="text"
                value={condition.config.class_name || ''}
                onChange={(e) => updateConfig({ class_name: e.target.value })}
                placeholder="e.g., apache or mysql"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  Min Changes
                </label>
                <input
                  type="number"
                  value={condition.config.threshold || 10}
                  onChange={(e) => updateConfig({ threshold: parseInt(e.target.value) })}
                  min="1"
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                />
              </div>
              <div>
                <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  Time Window (hours)
                </label>
                <input
                  type="number"
                  value={condition.config.time_window_hours || 168}
                  onChange={(e) => updateConfig({ time_window_hours: parseInt(e.target.value) })}
                  min="1"
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                />
              </div>
            </div>
          </div>
        )}

        {/* EnvironmentFilter */}
        {condition.type === 'EnvironmentFilter' && (
          <div>
            <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
              Environment Name
            </label>
            <input
              type="text"
              value={(condition.config.environment as string) || ''}
              onChange={(e) => updateConfig({ environment: e.target.value })}
              placeholder="e.g., production or staging"
              className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
            />
          </div>
        )}

        {/* GroupFilter */}
        {condition.type === 'GroupFilter' && (
          <div>
            <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
              Group ID
            </label>
            <input
              type="text"
              value={condition.config.group_id || ''}
              onChange={(e) => updateConfig({ group_id: e.target.value })}
              placeholder="Enter group ID"
              className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
            />
          </div>
        )}

        {/* NodeCountThreshold */}
        {condition.type === 'NodeCountThreshold' && (
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Min Count
              </label>
              <input
                type="number"
                value={condition.config.min_count || 0}
                onChange={(e) => updateConfig({ min_count: parseInt(e.target.value) })}
                min="0"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Max Count
              </label>
              <input
                type="number"
                value={condition.config.max_count || 100}
                onChange={(e) => updateConfig({ max_count: parseInt(e.target.value) })}
                min="0"
                className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </div>
          </div>
        )}

        {/* TimeWindowFilter */}
        {condition.type === 'TimeWindowFilter' && (
          <div className="space-y-2">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  Start Hour (0-23)
                </label>
                <input
                  type="number"
                  value={condition.config.start_hour || 0}
                  onChange={(e) => updateConfig({ start_hour: parseInt(e.target.value) })}
                  min="0"
                  max="23"
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                />
              </div>
              <div>
                <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  End Hour (0-23)
                </label>
                <input
                  type="number"
                  value={condition.config.end_hour || 23}
                  onChange={(e) => updateConfig({ end_hour: parseInt(e.target.value) })}
                  min="0"
                  max="23"
                  className="mt-1 block w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                />
              </div>
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                Days of Week
              </label>
              <div className="mt-1 flex flex-wrap gap-2">
                {['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'].map((day, idx) => (
                  <label key={day} className="flex items-center space-x-1">
                    <input
                      type="checkbox"
                      checked={(condition.config.days_of_week || []).includes(idx + 1)}
                      onChange={(e) => {
                        const days = condition.config.days_of_week || [];
                        if (e.target.checked) {
                          updateConfig({ days_of_week: [...days, idx + 1].sort() });
                        } else {
                          updateConfig({ days_of_week: days.filter((d) => d !== idx + 1) });
                        }
                      }}
                      className="rounded border-gray-300"
                    />
                    <span className="text-xs text-gray-700 dark:text-gray-300">{day}</span>
                  </label>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default function ConditionBuilder({ conditions, operator, onChange }: ConditionBuilderProps) {
  const [localConditions, setLocalConditions] = useState<AlertCondition[]>(conditions);
  const [localOperator, setLocalOperator] = useState<'AND' | 'OR'>(operator);

  const updateConditions = (newConditions: AlertCondition[]) => {
    setLocalConditions(newConditions);
    onChange(newConditions, localOperator);
  };

  const updateOperator = (newOperator: 'AND' | 'OR') => {
    setLocalOperator(newOperator);
    onChange(localConditions, newOperator);
  };

  const addCondition = () => {
    const newCondition: AlertCondition = {
      type: 'NodeStatus',
      enabled: true,
      config: { operator: '=', value: 'failed' },
    };
    updateConditions([...localConditions, newCondition]);
  };

  const updateCondition = (index: number, condition: AlertCondition) => {
    const newConditions = [...localConditions];
    newConditions[index] = condition;
    updateConditions(newConditions);
  };

  const deleteCondition = (index: number) => {
    updateConditions(localConditions.filter((_, i) => i !== index));
  };

  const duplicateCondition = (index: number) => {
    const condition = { ...localConditions[index] };
    updateConditions([...localConditions, condition]);
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-2">
          <span className="text-sm font-medium text-gray-700 dark:text-gray-300">Match</span>
          <select
            value={localOperator}
            onChange={(e) => updateOperator(e.target.value as 'AND' | 'OR')}
            className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
          >
            <option value="AND">ALL (AND)</option>
            <option value="OR">ANY (OR)</option>
          </select>
          <span className="text-sm text-gray-500">of the following conditions:</span>
        </div>
        <button
          onClick={addCondition}
          className="flex items-center space-x-1 rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
        >
          <Plus className="h-4 w-4" />
          <span>Add Condition</span>
        </button>
      </div>

      {localConditions.length === 0 ? (
        <div className="rounded-lg border-2 border-dashed border-gray-300 p-8 text-center dark:border-gray-600">
          <p className="text-sm text-gray-500 dark:text-gray-400">
            No conditions defined. Click "Add Condition" to create your first condition.
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          {localConditions.map((condition, index) => (
            <div key={index} className="relative">
              {index > 0 && (
                <div className="absolute -top-2 left-1/2 -translate-x-1/2 rounded bg-gray-200 px-2 py-0.5 text-xs font-medium text-gray-700 dark:bg-gray-700 dark:text-gray-300">
                  {localOperator}
                </div>
              )}
              <div className="flex items-start space-x-2">
                <div className="flex-1">
                  <ConditionEditor
                    condition={condition}
                    onChange={(c) => updateCondition(index, c)}
                    onDelete={() => deleteCondition(index)}
                  />
                </div>
                <button
                  onClick={() => duplicateCondition(index)}
                  className="mt-4 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300"
                  title="Duplicate condition"
                >
                  <Copy className="h-4 w-4" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {localConditions.length > 0 && (
        <div className="rounded-lg bg-blue-50 p-3 dark:bg-blue-900/20">
          <p className="text-sm text-blue-800 dark:text-blue-200">
            <strong>Summary:</strong> Alert will trigger when{' '}
            {localOperator === 'AND' ? 'all' : 'any'} of the{' '}
            {localConditions.filter((c) => c.enabled).length} enabled conditions match.
          </p>
        </div>
      )}
    </div>
  );
}
