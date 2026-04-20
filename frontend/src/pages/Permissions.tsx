import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Grid, List, Shield, AlertCircle } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import { PermissionMatrixEditor } from '../components/PermissionMatrixEditor';
import type { ResourceInfo, ActionInfo } from '../types';

type ViewMode = 'matrix' | 'list';

export default function Permissions() {
  const [viewMode, setViewMode] = useState<ViewMode>('matrix');
  const [error, setError] = useState<string | null>(null);

  const { data: resources = [] } = useQuery<ResourceInfo[]>({
    queryKey: ['resources'],
    queryFn: api.getResources,
  });

  const { data: actions = [] } = useQuery<ActionInfo[]>({
    queryKey: ['actions'],
    queryFn: api.getActions,
  });

  return (
    <div>
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Permissions</h1>
          <p className="text-gray-500 mt-1">View and manage role permissions</p>
        </div>
        <div className="flex items-center gap-2">
          <div className="bg-gray-100 rounded-lg p-1 flex">
            <button
              onClick={() => setViewMode('matrix')}
              className={clsx(
                'px-3 py-1.5 rounded text-sm flex items-center transition-colors',
                viewMode === 'matrix'
                  ? 'bg-white text-gray-900 shadow'
                  : 'text-gray-600 hover:text-gray-900'
              )}
            >
              <Grid className="w-4 h-4 mr-1.5" />
              Matrix
            </button>
            <button
              onClick={() => setViewMode('list')}
              className={clsx(
                'px-3 py-1.5 rounded text-sm flex items-center transition-colors',
                viewMode === 'list'
                  ? 'bg-white text-gray-900 shadow'
                  : 'text-gray-600 hover:text-gray-900'
              )}
            >
              <List className="w-4 h-4 mr-1.5" />
              Resources
            </button>
          </div>
        </div>
      </div>

      {/* Error Alert */}
      {error && (
        <div className="mb-6 bg-red-50 border border-red-200 rounded-lg p-4 flex items-start">
          <AlertCircle className="w-5 h-5 text-red-600 mr-3 flex-shrink-0 mt-0.5" />
          <div className="flex-1">
            <p className="text-red-800 font-medium">Error</p>
            <p className="text-red-600 text-sm">{error}</p>
          </div>
          <button
            onClick={() => setError(null)}
            className="text-red-400 hover:text-red-600"
          >
            &times;
          </button>
        </div>
      )}

      {viewMode === 'matrix' ? (
        <div className="card">
          <div className="mb-4">
            <h2 className="font-semibold text-gray-900">Permission Matrix</h2>
            <p className="text-sm text-gray-500">
              Click on a cell to toggle the permission. Changes are staged until you apply them.
            </p>
          </div>
          <PermissionMatrixEditor onError={setError} />
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Resources */}
          <div className="card">
            <h2 className="font-semibold text-gray-900 mb-4 flex items-center">
              <Shield className="w-5 h-5 mr-2 text-primary-600" />
              Resources
            </h2>
            <div className="space-y-4">
              {resources.map((resource) => (
                <div
                  key={resource.name}
                  className="p-4 bg-gray-50 rounded-lg border border-gray-200"
                >
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="font-medium text-gray-900">{resource.display_name}</h3>
                    <span className="text-xs bg-gray-200 text-gray-600 px-2 py-1 rounded">
                      {resource.name}
                    </span>
                  </div>
                  <p className="text-sm text-gray-600 mb-3">{resource.description}</p>
                  <div className="flex flex-wrap gap-2">
                    {resource.available_actions.map((action) => (
                      <span
                        key={action}
                        className="text-xs bg-primary-100 text-primary-700 px-2 py-1 rounded"
                      >
                        {action}
                      </span>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Actions */}
          <div className="card">
            <h2 className="font-semibold text-gray-900 mb-4 flex items-center">
              <List className="w-5 h-5 mr-2 text-primary-600" />
              Actions
            </h2>
            <div className="space-y-4">
              {actions.map((action) => (
                <div
                  key={action.name}
                  className="p-4 bg-gray-50 rounded-lg border border-gray-200"
                >
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="font-medium text-gray-900">{action.display_name}</h3>
                    <span className="text-xs bg-primary-100 text-primary-700 px-2 py-1 rounded">
                      {action.name}
                    </span>
                  </div>
                  <p className="text-sm text-gray-600">{action.description}</p>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
