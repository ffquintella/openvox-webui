import { CheckCircle, XCircle } from 'lucide-react';
import clsx from 'clsx';
import { flattenTrace, traceDetails } from './explainHelpers';
import type { ExprTrace } from '../../types';

/** Indented tree of an expression evaluation with a matched/unmatched marker per node. */
export default function ExprTraceTree({ trace }: { trace: ExprTrace }) {
  const rows = flattenTrace(trace);
  return (
    <ul role="tree" aria-label="Expression trace" className="text-xs font-mono space-y-0.5">
      {rows.map(({ trace: t, depth, key }) => {
        const details = traceDetails(t);
        return (
          <li
            key={key}
            role="treeitem"
            aria-level={depth + 1}
            data-matched={t.matched ? 'true' : 'false'}
            style={{ paddingLeft: `${depth * 16}px` }}
            className="flex items-start gap-1.5"
          >
            {t.matched ? (
              <CheckCircle
                aria-label="matched"
                className="w-3.5 h-3.5 mt-0.5 text-green-600 flex-shrink-0"
              />
            ) : (
              <XCircle
                aria-label="not matched"
                className="w-3.5 h-3.5 mt-0.5 text-red-500 flex-shrink-0"
              />
            )}
            <span className="min-w-0 break-words">
              <span className={clsx(t.matched ? 'text-gray-900' : 'text-gray-600')}>{t.label}</span>
              {details.length > 0 && <span className="text-gray-500"> ({details.join(', ')})</span>}
              {t.error && <span className="text-red-700"> error: {t.error}</span>}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
