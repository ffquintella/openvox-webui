import clsx from 'clsx';
import { provenanceRows } from './explainHelpers';
import type { ProvenanceSource } from '../../types';

interface Props {
  title: string;
  /** Column header for the keys, e.g. "Class" */
  keyLabel: string;
  entries: Record<string, ProvenanceSource[]> | undefined;
}

/** Table of key -> contributing groups; the last contributor (the winner) is highlighted. */
export default function ProvenanceTable({ title, keyLabel, entries }: Props) {
  const rows = provenanceRows(entries);
  if (rows.length === 0) return null;
  return (
    <div>
      <h5 className="text-xs font-semibold text-gray-700 mb-1">{title}</h5>
      <table className="w-full text-xs border border-gray-200">
        <thead className="bg-gray-50 text-gray-600">
          <tr>
            <th className="text-left px-2 py-1 font-medium">{keyLabel}</th>
            <th className="text-left px-2 py-1 font-medium">
              Contributing groups (winner highlighted)
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.key} className="border-t border-gray-100">
              <td className="px-2 py-1 font-mono align-top">{row.key}</td>
              <td className="px-2 py-1">
                <div className="flex flex-wrap gap-1">
                  {row.sources.map((source, index) => {
                    const isWinner = index === row.sources.length - 1;
                    return (
                      <span
                        key={`${source.group_id}-${index}`}
                        data-winner={isWinner ? 'true' : 'false'}
                        className={clsx(
                          'px-1.5 py-0.5 rounded',
                          isWinner
                            ? 'bg-green-100 text-green-800 font-semibold ring-1 ring-green-300'
                            : 'bg-gray-100 text-gray-600'
                        )}
                      >
                        {source.group_name}
                        {isWinner && row.sources.length > 1 ? ' (wins)' : ''}
                      </span>
                    );
                  })}
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
