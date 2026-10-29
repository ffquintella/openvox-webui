import { useMemo, useState } from 'react';
import { BookOpen, FileJson } from 'lucide-react';
import {
  useClassificationFunctions,
  useClassificationOperators,
} from '../../hooks/useClassification';
import {
  EXPRESSION_MAX_DEPTH,
  EXPRESSION_MAX_NODES,
  expressionToText,
  rulesToExpression,
} from './expressionHelpers';
import type { ClassificationRule, RuleMatchType } from '../../types';

interface Props {
  text: string;
  onChange: (text: string) => void;
  /** Parse error of `text`, if any */
  error: string | null;
  /** The group's existing flat rules (empty when creating) */
  rules: ClassificationRule[];
  ruleMatchType: RuleMatchType;
}

/** JSON editor for the composite expression plus a catalog and a flat-rules converter. */
export default function ExpressionSection({ text, onChange, error, rules, ruleMatchType }: Props) {
  const [showCatalog, setShowCatalog] = useState(false);
  const [showEquivalent, setShowEquivalent] = useState(false);
  const functions = useClassificationFunctions();
  const operators = useClassificationOperators();

  const equivalent = useMemo(() => {
    const expression = rulesToExpression(rules, ruleMatchType);
    return expression ? expressionToText(expression) : '';
  }, [rules, ruleMatchType]);

  return (
    <fieldset className="space-y-2 rounded border border-gray-200 p-3">
      <legend className="px-1 text-sm font-semibold text-gray-900">Expression</legend>
      <p className="text-xs text-gray-500">
        Optional composite match built from <code>all</code>, <code>any</code>, <code>not</code>,
        fact comparisons and functions (max depth {EXPRESSION_MAX_DEPTH}, {EXPRESSION_MAX_NODES}{' '}
        nodes). Leave empty to use only the flat rules. When set, it is evaluated together with the
        flat rules.
      </p>
      <textarea
        aria-label="Expression JSON"
        value={text}
        onChange={(e) => onChange(e.target.value)}
        className="input font-mono text-xs"
        rows={8}
        spellCheck={false}
        placeholder={
          '{\n  "all": [\n    { "fact": "os.family", "op": "=", "value": "RedHat" }\n  ]\n}'
        }
      />
      {error && (
        <p className="text-xs text-red-700" role="alert">
          {error}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          className="btn btn-secondary flex items-center text-xs"
          onClick={() => setShowCatalog((v) => !v)}
          aria-expanded={showCatalog}
        >
          <BookOpen className="w-4 h-4 mr-1" />
          {showCatalog ? 'Hide' : 'Show'} function and operator catalog
        </button>
        {rules.length > 0 && (
          <button
            type="button"
            className="btn btn-secondary flex items-center text-xs"
            onClick={() => setShowEquivalent((v) => !v)}
            aria-expanded={showEquivalent}
          >
            <FileJson className="w-4 h-4 mr-1" />
            Convert flat rules to expression
          </button>
        )}
      </div>

      {showEquivalent && equivalent && (
        <div className="rounded bg-gray-50 border border-gray-200 p-2 space-y-2">
          <p className="text-xs text-gray-600">
            Equivalent of the {rules.length} flat rule{rules.length === 1 ? '' : 's'} (
            {ruleMatchType === 'all' ? 'all must match' : 'any may match'}):
          </p>
          <pre data-testid="equivalent-expression" className="text-xs font-mono overflow-x-auto">
            {equivalent}
          </pre>
          <button
            type="button"
            className="btn btn-secondary text-xs"
            onClick={() => {
              onChange(equivalent);
              setShowEquivalent(false);
            }}
          >
            Use this expression
          </button>
        </div>
      )}

      {showCatalog && (
        <div className="rounded bg-gray-50 border border-gray-200 p-2 space-y-3 max-h-60 overflow-y-auto">
          <div>
            <p className="text-xs font-semibold text-gray-700 mb-1">Operators</p>
            {operators.isLoading ? (
              <p className="text-xs text-gray-500">Loading...</p>
            ) : operators.isError || !operators.data ? (
              <p className="text-xs text-gray-500">Operator catalog unavailable.</p>
            ) : (
              <ul className="text-xs space-y-0.5">
                {operators.data.map((o) => (
                  <li key={o.op}>
                    <code className="font-mono">{o.op}</code>{' '}
                    <span className="text-gray-600">{o.description}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div>
            <p className="text-xs font-semibold text-gray-700 mb-1">Functions</p>
            {functions.isLoading ? (
              <p className="text-xs text-gray-500">Loading...</p>
            ) : functions.isError || !functions.data ? (
              <p className="text-xs text-gray-500">Function catalog unavailable.</p>
            ) : (
              <ul className="text-xs space-y-2">
                {functions.data.map((f) => (
                  <li key={f.name}>
                    <code className="font-mono font-semibold">
                      {f.name}(
                      {f.min_args === f.max_args ? f.min_args : `${f.min_args}-${f.max_args}`} args)
                    </code>{' '}
                    <span className="text-gray-500">returns {f.returns}</span>
                    <p className="text-gray-600">{f.description}</p>
                    <code className="block font-mono text-gray-700 break-all">{f.example}</code>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
    </fieldset>
  );
}
