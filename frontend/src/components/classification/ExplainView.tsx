import { AlertTriangle } from 'lucide-react';
import ExprTraceTree from './ExprTraceTree';
import ProvenanceTable from './ProvenanceTable';
import {
  formatElapsed,
  formatValue,
  hasProvenance,
  outcomeLabel,
  outcomeTone,
} from './explainHelpers';
import clsx from 'clsx';
import type { ClassificationExplain } from '../../types';

/** Renders the explain block of a classification: per-group outcomes, provenance, merge errors. */
export default function ExplainView({ explain }: { explain: ClassificationExplain }) {
  const { provenance } = explain;
  return (
    <div className="space-y-4" data-testid="explain-view">
      <p className="text-xs text-gray-500">
        Evaluated {new Date(explain.evaluated_at).toLocaleString()} in{' '}
        {formatElapsed(explain.elapsed_us)}.
      </p>

      {explain.merge_errors.length > 0 && (
        <div className="rounded border border-red-200 bg-red-50 p-3" role="alert">
          <p className="text-sm font-medium text-red-800 flex items-center gap-2 mb-1">
            <AlertTriangle className="w-4 h-4" />
            Merge errors ({explain.merge_errors.length})
          </p>
          <ul className="text-xs text-red-700 list-disc list-inside space-y-0.5">
            {explain.merge_errors.map((message, index) => (
              <li key={index}>{message}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="space-y-2">
        <h4 className="text-sm font-medium text-gray-700">Groups ({explain.groups.length})</h4>
        {explain.groups.map((group) => (
          <div
            key={group.id}
            data-testid={`explain-group-${group.id}`}
            className="rounded border border-gray-200 p-3 space-y-2"
          >
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-medium text-gray-900">{group.name}</span>
              <span
                className={clsx('text-xs px-2 py-0.5 rounded-full', outcomeTone(group.outcome))}
              >
                {outcomeLabel(group.outcome)}
              </span>
              {group.match_type && (
                <span className="text-xs text-gray-500">via {group.match_type}</span>
              )}
              <span className="text-xs text-gray-500 ml-auto">priority {group.priority}</span>
            </div>
            <p className="text-xs text-gray-600">{group.reason}</p>

            {group.rules.length > 0 && (
              <table className="w-full text-xs border border-gray-200">
                <thead className="bg-gray-50 text-gray-600">
                  <tr>
                    <th className="text-left px-2 py-1 font-medium">Fact</th>
                    <th className="text-left px-2 py-1 font-medium">Op</th>
                    <th className="text-left px-2 py-1 font-medium">Expected</th>
                    <th className="text-left px-2 py-1 font-medium">Actual</th>
                    <th className="text-left px-2 py-1 font-medium">Result</th>
                  </tr>
                </thead>
                <tbody>
                  {group.rules.map((rule) => (
                    <tr key={rule.rule_id} className="border-t border-gray-100">
                      <td className="px-2 py-1 font-mono">{rule.fact_path}</td>
                      <td className="px-2 py-1 font-mono">{rule.operator}</td>
                      <td className="px-2 py-1 font-mono break-all">
                        {formatValue(rule.expected)}
                      </td>
                      <td className="px-2 py-1 font-mono break-all">
                        {rule.actual === null ? 'missing' : formatValue(rule.actual)}
                      </td>
                      <td
                        className={clsx(
                          'px-2 py-1 font-medium',
                          rule.matched ? 'text-green-700' : 'text-red-600'
                        )}
                      >
                        {rule.matched ? 'matched' : 'not matched'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}

            {group.expression && (
              <div>
                <p className="text-xs font-semibold text-gray-700 mb-1">Expression</p>
                <ExprTraceTree trace={group.expression} />
              </div>
            )}
          </div>
        ))}
      </div>

      <div className="space-y-3">
        <h4 className="text-sm font-medium text-gray-700">Provenance</h4>
        {provenance.environment && (
          <p className="text-xs text-gray-700" data-testid="environment-origin">
            Environment comes from group{' '}
            <span className="font-semibold">{provenance.environment.group_name}</span>.
          </p>
        )}
        {hasProvenance(provenance) ? (
          <>
            <ProvenanceTable title="Classes" keyLabel="Class" entries={provenance.classes} />
            <ProvenanceTable title="Variables" keyLabel="Variable" entries={provenance.variables} />
          </>
        ) : (
          <p className="text-xs text-gray-500">No classes or variables were assigned.</p>
        )}
      </div>
    </div>
  );
}
