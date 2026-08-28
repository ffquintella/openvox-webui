import { useState, type FormEvent } from 'react';
import { useQuery } from '@tanstack/react-query';
import { ChevronLeft, ChevronRight, Search, X } from 'lucide-react';
import { api } from '../../services/api';

const PAGE_SIZE = 50;

interface VariableAssignmentsTabProps {
  active: boolean;
}

function formatValue(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value === null) return 'null';
  if (value === undefined) return '';

  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export default function VariableAssignmentsTab({ active }: VariableAssignmentsTabProps) {
  const [machineInput, setMachineInput] = useState('');
  const [variableInput, setVariableInput] = useState('');
  const [filters, setFilters] = useState({ machine: '', variable: '' });
  const [page, setPage] = useState(0);

  const { data, isLoading, isFetching, isError, refetch } = useQuery({
    queryKey: ['analytics', 'variable-assignments', filters.machine, filters.variable, page],
    queryFn: () =>
      api.getVariableAssignments({
        machine: filters.machine || undefined,
        variable: filters.variable || undefined,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      }),
    enabled: active,
    staleTime: 30_000,
  });

  const applySearch = (event: FormEvent) => {
    event.preventDefault();
    setPage(0);
    setFilters({ machine: machineInput.trim(), variable: variableInput.trim() });
  };

  const clearSearch = () => {
    setMachineInput('');
    setVariableInput('');
    setFilters({ machine: '', variable: '' });
    setPage(0);
  };

  const total = data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const firstRow = total === 0 ? 0 : page * PAGE_SIZE + 1;
  const lastRow = Math.min((page + 1) * PAGE_SIZE, total);

  return (
    <div className="space-y-6">
      <div className="card">
        <div className="mb-5">
          <h2 className="text-lg font-semibold text-gray-900">Machine Variable Assignments</h2>
          <p className="mt-1 text-sm text-gray-500">
            Effective external variables each machine receives after group inheritance and overrides
            are applied.
          </p>
        </div>

        <form onSubmit={applySearch} className="grid gap-4 md:grid-cols-[1fr_1fr_auto]">
          <div>
            <label htmlFor="variable-machine-search" className="label">
              Machine
            </label>
            <input
              id="variable-machine-search"
              type="search"
              value={machineInput}
              onChange={(event) => setMachineInput(event.target.value)}
              className="input"
              placeholder="Search machine name"
            />
          </div>
          <div>
            <label htmlFor="variable-name-search" className="label">
              Variable
            </label>
            <input
              id="variable-name-search"
              type="search"
              value={variableInput}
              onChange={(event) => setVariableInput(event.target.value)}
              className="input"
              placeholder="Search variable name"
            />
          </div>
          <div className="flex items-end gap-2">
            <button type="submit" className="btn btn-primary flex items-center gap-2">
              <Search className="h-4 w-4" />
              Search
            </button>
            {(filters.machine || filters.variable) && (
              <button
                type="button"
                onClick={clearSearch}
                className="btn btn-secondary flex items-center gap-2"
                aria-label="Clear variable assignment search"
              >
                <X className="h-4 w-4" />
                Clear
              </button>
            )}
          </div>
        </form>
      </div>

      <div className="card !p-0 overflow-hidden">
        <div className="flex items-center justify-between border-b border-gray-200 px-6 py-4">
          <div>
            <h3 className="font-semibold text-gray-900">Assignments</h3>
            <p className="text-sm text-gray-500">
              {isLoading ? 'Calculating assignments…' : `${total.toLocaleString()} assignments`}
            </p>
          </div>
          {isFetching && !isLoading && <span className="text-sm text-gray-500">Refreshing…</span>}
        </div>

        {isLoading ? (
          <div className="flex h-64 items-center justify-center">
            <div className="h-8 w-8 animate-spin rounded-full border-b-2 border-primary-600" />
          </div>
        ) : isError ? (
          <div className="px-6 py-16 text-center">
            <p className="font-medium text-gray-900">Could not load variable assignments</p>
            <p className="mt-1 text-sm text-gray-500">
              Check the PuppetDB connection and try again.
            </p>
            <button type="button" onClick={() => refetch()} className="btn btn-primary mt-4">
              Try again
            </button>
          </div>
        ) : data?.assignments.length === 0 ? (
          <div className="px-6 py-16 text-center">
            <p className="font-medium text-gray-900">No assignments found</p>
            <p className="mt-1 text-sm text-gray-500">
              Try a different machine or variable search.
            </p>
          </div>
        ) : (
          <>
            <div className="overflow-x-auto">
              <table className="min-w-full divide-y divide-gray-200 text-sm">
                <thead className="bg-gray-50">
                  <tr>
                    <th className="px-6 py-3 text-left font-medium uppercase tracking-wide text-gray-500">
                      Machine
                    </th>
                    <th className="px-6 py-3 text-left font-medium uppercase tracking-wide text-gray-500">
                      Variable
                    </th>
                    <th className="px-6 py-3 text-left font-medium uppercase tracking-wide text-gray-500">
                      Value
                    </th>
                    <th className="px-6 py-3 text-left font-medium uppercase tracking-wide text-gray-500">
                      Environment
                    </th>
                    <th className="px-6 py-3 text-left font-medium uppercase tracking-wide text-gray-500">
                      Matched Groups
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-200 bg-white">
                  {data?.assignments.map((assignment) => {
                    const displayValue = formatValue(assignment.value);
                    return (
                      <tr
                        key={`${assignment.machine}:${assignment.variable}`}
                        className="hover:bg-gray-50"
                      >
                        <td className="whitespace-nowrap px-6 py-4 font-medium text-gray-900">
                          {assignment.machine}
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 font-mono text-primary-700">
                          {assignment.variable}
                        </td>
                        <td className="max-w-sm px-6 py-4 text-gray-700">
                          <span className="block truncate font-mono" title={displayValue}>
                            {displayValue}
                          </span>
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 text-gray-600">
                          {assignment.environment || '—'}
                        </td>
                        <td className="max-w-xs px-6 py-4 text-gray-600">
                          <span className="block truncate" title={assignment.groups.join(', ')}>
                            {assignment.groups.length > 0 ? assignment.groups.join(', ') : '—'}
                          </span>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>

            <div className="flex flex-col gap-3 border-t border-gray-200 px-6 py-4 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-sm text-gray-500">
                Showing {firstRow.toLocaleString()}–{lastRow.toLocaleString()} of{' '}
                {total.toLocaleString()}
              </p>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => setPage((current) => Math.max(0, current - 1))}
                  disabled={page === 0 || isFetching}
                  className="btn btn-secondary flex items-center gap-1 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <ChevronLeft className="h-4 w-4" />
                  Previous
                </button>
                <span className="px-2 text-sm text-gray-600">
                  Page {page + 1} of {pageCount}
                </span>
                <button
                  type="button"
                  onClick={() => setPage((current) => Math.min(pageCount - 1, current + 1))}
                  disabled={page + 1 >= pageCount || isFetching}
                  className="btn btn-secondary flex items-center gap-1 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Next
                  <ChevronRight className="h-4 w-4" />
                </button>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
