import { useEffect, useRef, useState } from 'react';
import { Loader2 } from 'lucide-react';
import {
  useBoltAllowlist,
  useBoltDiscover,
  useSaveBoltAllowlist,
} from '../../hooks/useOrchestration';
import { errorText } from '../../utils/orchestration';
import type { BoltDiscoveredItem } from '../../types';

type Row = Pick<
  BoltDiscoveredItem,
  'kind' | 'name' | 'description' | 'allowed' | 'requires_approval'
>;

const keyOf = (r: Pick<Row, 'kind' | 'name'>) => `${r.kind}:${r.name}`;

/** Discovered tasks and plans with the Allowed and Requires approval toggles. */
export default function AllowlistPanel() {
  const { data: found, isLoading, error: loadError } = useBoltDiscover(true);
  const { data: current } = useBoltAllowlist(true);
  const save = useSaveBoltAllowlist();
  const [rows, setRows] = useState<Row[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const dirty = useRef(false);

  useEffect(() => {
    // A refetch must not overwrite edits that are not saved yet.
    if (!found || dirty.current) return;
    const base: Row[] = found.map((f) => ({ ...f }));
    // Entries allowlisted earlier that Bolt no longer finds stay listed so a
    // save does not drop them silently.
    const known = new Set(base.map(keyOf));
    for (const e of current?.entries ?? []) {
      if (!known.has(keyOf(e))) {
        base.push({
          kind: e.kind,
          name: e.name,
          description: 'Not found by Bolt any more',
          allowed: true,
          requires_approval: e.requires_approval,
        });
      }
    }
    setRows(base);
  }, [found, current]);

  const update = (k: string, patch: Partial<Row>) => {
    setSaved(false);
    dirty.current = true;
    setRows((prev) => prev.map((r) => (keyOf(r) === k ? { ...r, ...patch } : r)));
  };

  const onSave = () => {
    setError(null);
    setSaved(false);
    save.mutate(
      rows
        .filter((r) => r.allowed)
        .map((r) => ({ kind: r.kind, name: r.name, requires_approval: r.requires_approval })),
      {
        onSuccess: () => {
          dirty.current = false;
          setSaved(true);
        },
        onError: (e) => setError(errorText(e)),
      }
    );
  };

  return (
    <div>
      <p className="text-sm text-gray-500 mb-4">
        Only enabled tasks and plans can be run. Saving replaces the whole list.
      </p>
      {(error || loadError) && (
        <div role="alert" className="mb-4 p-3 rounded bg-red-50 text-red-700 text-sm">
          {error ?? errorText(loadError)}
        </div>
      )}
      {saved && (
        <div role="status" className="mb-4 p-3 rounded bg-green-50 text-green-700 text-sm">
          Allowlist saved.
        </div>
      )}
      <div className="card overflow-x-auto">
        {isLoading ? (
          <Loader2 className="w-5 h-5 animate-spin" />
        ) : rows.length === 0 ? (
          <p className="text-sm text-gray-500">Bolt found no tasks or plans.</p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-gray-500">
                <th className="py-2">Name</th>
                <th>Kind</th>
                <th>Allowed</th>
                <th>Requires approval</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const k = keyOf(r);
                return (
                  <tr key={k} className="border-t border-gray-200 dark:border-gray-700">
                    <td className="py-2">
                      <div className="font-mono">{r.name}</div>
                      {r.description && (
                        <div className="text-xs text-gray-500">{r.description}</div>
                      )}
                    </td>
                    <td>{r.kind}</td>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`Allow ${r.name}`}
                        checked={r.allowed}
                        onChange={(e) =>
                          update(k, {
                            allowed: e.target.checked,
                            requires_approval: e.target.checked ? r.requires_approval : false,
                          })
                        }
                      />
                    </td>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`Require approval for ${r.name}`}
                        checked={r.requires_approval}
                        disabled={!r.allowed}
                        onChange={(e) => update(k, { requires_approval: e.target.checked })}
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      <div className="mt-4">
        <button
          type="button"
          className="btn btn-primary"
          disabled={save.isPending}
          onClick={onSave}
        >
          {save.isPending ? 'Saving...' : 'Save allowlist'}
        </button>
      </div>
    </div>
  );
}
