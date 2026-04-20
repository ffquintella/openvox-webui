import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Search, Database } from 'lucide-react';
import { api } from '../services/api';

export default function Facts() {
  const [search, setSearch] = useState('');
  const [selectedFact, setSelectedFact] = useState<string | null>(null);

  const { data: factNames = [], isLoading: namesLoading, isError: namesError, error: namesErrorData } = useQuery({
    queryKey: ['fact-names'],
    queryFn: api.getFactNames,
  });

  const { data: factValues = [], isLoading: valuesLoading } = useQuery({
    queryKey: ['facts', selectedFact],
    queryFn: () => api.getFacts({ name: selectedFact! }),
    enabled: !!selectedFact,
  });

  const filteredNames = factNames.filter((name: string) =>
    name.toLowerCase().includes(search.toLowerCase())
  );

  if (namesLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  if (namesError) {
    const errorMessage = (namesErrorData as Error)?.message || 'Unknown error';
    const isPuppetDBUnavailable = errorMessage.includes('503') || errorMessage.includes('Service Unavailable') || errorMessage.includes('PuppetDB');

    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <Database className="w-12 h-12 mx-auto mb-4 text-gray-300" />
          <h2 className="text-lg font-semibold text-gray-900 mb-2">
            {isPuppetDBUnavailable ? 'PuppetDB Not Available' : 'Error Loading Facts'}
          </h2>
          <p className="text-gray-500">
            {isPuppetDBUnavailable
              ? 'PuppetDB is not configured. Configure PuppetDB connection in settings to view facts.'
              : errorMessage}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-2xl font-bold text-gray-900 mb-8">Facts Explorer</h1>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Fact Names */}
        <div className="card h-[600px] overflow-hidden flex flex-col">
          <div className="mb-4">
            <div className="flex items-stretch border border-gray-300 rounded-lg bg-white hover:border-gray-400 focus-within:border-primary-600 focus-within:ring-1 focus-within:ring-primary-600 transition-colors">
              <div className="flex items-center justify-center px-3 py-2 border-r border-gray-200">
                <Search className="w-5 h-5 text-gray-400" />
              </div>
              <input
                type="text"
                placeholder="Search facts..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="flex-1 px-3 py-2 outline-none bg-transparent rounded-r-lg"
              />
            </div>
          </div>
          <div className="flex-1 overflow-y-auto">
            <ul className="space-y-1">
              {filteredNames.map((name: string) => (
                <li key={name}>
                  <button
                    onClick={() => setSelectedFact(name)}
                    className={`w-full text-left px-3 py-2 rounded-md text-sm ${
                      selectedFact === name
                        ? 'bg-primary-50 text-primary-700'
                        : 'hover:bg-gray-100'
                    }`}
                  >
                    {name}
                  </button>
                </li>
              ))}
            </ul>
            {filteredNames.length === 0 && (
              <p className="text-center text-gray-500 py-4">No facts found</p>
            )}
          </div>
        </div>

        {/* Fact Values */}
        <div className="lg:col-span-2 card h-[600px] overflow-hidden flex flex-col">
          {selectedFact ? (
            <>
              <h2 className="text-lg font-semibold mb-4 flex items-center">
                <Database className="w-5 h-5 mr-2 text-primary-600" />
                {selectedFact}
              </h2>
              {valuesLoading ? (
                <div className="flex-1 flex items-center justify-center">
                  <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
                </div>
              ) : (
                <div className="flex-1 overflow-y-auto">
                  <table className="min-w-full">
                    <thead className="bg-gray-50 sticky top-0">
                      <tr>
                        <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                          Node
                        </th>
                        <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                          Value
                        </th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-200">
                      {factValues.map((fact: { certname: string; value: unknown }, index: number) => (
                        <tr key={`${fact.certname}-${index}`}>
                          <td className="px-4 py-2 text-sm font-medium text-gray-900">
                            {fact.certname}
                          </td>
                          <td className="px-4 py-2 text-sm text-gray-600">
                            <code className="bg-gray-100 px-2 py-1 rounded">
                              {typeof fact.value === 'object'
                                ? JSON.stringify(fact.value)
                                : String(fact.value)}
                            </code>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {factValues.length === 0 && (
                    <p className="text-center text-gray-500 py-4">
                      No values found for this fact
                    </p>
                  )}
                </div>
              )}
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-gray-500">
              <div className="text-center">
                <Database className="w-12 h-12 mx-auto mb-4 text-gray-300" />
                <p>Select a fact to view values across nodes</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
