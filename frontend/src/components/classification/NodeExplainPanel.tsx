import { useState } from 'react';
import { Search, Loader2 } from 'lucide-react';
import { useNodeClassification } from '../../hooks/useClassification';
import ExplainView from './ExplainView';

/** "Explain" toggle for a node's classification; fetches with explain=true on demand. */
export default function NodeExplainPanel({ certname }: { certname: string }) {
  const [open, setOpen] = useState(false);
  const { data, isLoading, isError } = useNodeClassification(certname, {
    explain: true,
    enabled: open,
  });

  return (
    <div>
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-medium text-gray-700 flex items-center gap-2">
          <Search className="w-4 h-4" />
          Classification Explain
        </h3>
        <button
          type="button"
          className="btn btn-secondary text-xs"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? 'Hide explain' : 'Explain'}
        </button>
      </div>
      {!open ? (
        <p className="text-xs text-gray-500">
          Show why each group did or did not match this node and where classes, variables and the
          environment come from.
        </p>
      ) : isLoading ? (
        <p className="text-sm text-gray-500 flex items-center gap-2">
          <Loader2 className="w-4 h-4 animate-spin" />
          Evaluating classification…
        </p>
      ) : isError || !data ? (
        <p className="text-sm text-gray-500">Unable to explain the classification of this node.</p>
      ) : data.explain ? (
        <ExplainView explain={data.explain} />
      ) : (
        <p className="text-sm text-gray-500">The server did not return an explanation.</p>
      )}
    </div>
  );
}
