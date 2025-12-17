import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Check, X, Shield, Loader2 } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import type { PermissionMatrix, Resource, Action } from '../types';

interface PermissionMatrixEditorProps {
  onError?: (error: string) => void;
}

export function PermissionMatrixEditor({ onError }: PermissionMatrixEditorProps) {
  const queryClient = useQueryClient();
  const [pendingChanges, setPendingChanges] = useState<
    Map<string, { roleId: string; resource: Resource; action: Action; granted: boolean }>
  >(new Map());

  const { data: matrix, isLoading, error } = useQuery<PermissionMatrix>({
    queryKey: ['permissionMatrix'],
    queryFn: api.getPermissionMatrix,
  });

  const bulkMutation = useMutation({
    mutationFn: api.bulkUpdatePermissions,
    onSuccess: (result) => {
      if (result.failed > 0) {
        const errors = result.results
          .filter((r) => !r.success)
          .map((r) => r.error)
          .join(', ');
        onError?.(`Some operations failed: ${errors}`);
      }
      queryClient.invalidateQueries({ queryKey: ['permissionMatrix'] });
      queryClient.invalidateQueries({ queryKey: ['roles'] });
      setPendingChanges(new Map());
    },
    onError: (error) => {
      onError?.(error instanceof Error ? error.message : 'Failed to update permissions');
    },
  });

  const handleToggle = (roleId: string, resource: Resource, action: Action, currentValue: boolean) => {
    const key = `${roleId}-${resource}-${action}`;
    const newChanges = new Map(pendingChanges);

    // Check if this change is already pending
    const existing = pendingChanges.get(key);
    if (existing) {
      // If toggling back to original value, remove the pending change
      if (existing.granted === currentValue) {
        newChanges.delete(key);
      } else {
        newChanges.set(key, { roleId, resource, action, granted: !currentValue });
      }
    } else {
      newChanges.set(key, { roleId, resource, action, granted: !currentValue });
    }

    setPendingChanges(newChanges);
  };

  const getEffectiveValue = (roleId: string, resource: string, action: string): boolean => {
    const key = `${roleId}-${resource}-${action}`;
    const pending = pendingChanges.get(key);
    if (pending) {
      return pending.granted;
    }
    return matrix?.matrix[roleId]?.[resource]?.[action] ?? false;
  };

  const hasPendingChange = (roleId: string, resource: string, action: string): boolean => {
    const key = `${roleId}-${resource}-${action}`;
    return pendingChanges.has(key);
  };

  const applyChanges = () => {
    const operations = Array.from(pendingChanges.values()).map((change) => ({
      op: change.granted ? ('add' as const) : ('remove' as const),
      role_id: change.roleId,
      permission: {
        resource: change.resource,
        action: change.action,
      },
    }));

    bulkMutation.mutate({ operations });
  };

  const discardChanges = () => {
    setPendingChanges(new Map());
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  if (error || !matrix) {
    return (
      <div className="bg-red-50 text-red-700 p-4 rounded-lg">
        Failed to load permission matrix
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Pending changes toolbar */}
      {pendingChanges.size > 0 && (
        <div className="bg-amber-50 border border-amber-200 rounded-lg p-4 flex items-center justify-between">
          <div className="flex items-center">
            <Shield className="w-5 h-5 text-amber-600 mr-2" />
            <span className="text-amber-800">
              {pendingChanges.size} pending change{pendingChanges.size > 1 ? 's' : ''}
            </span>
          </div>
          <div className="flex gap-2">
            <button
              onClick={discardChanges}
              className="btn btn-secondary text-sm"
              disabled={bulkMutation.isPending}
            >
              Discard
            </button>
            <button
              onClick={applyChanges}
              className="btn btn-primary text-sm flex items-center"
              disabled={bulkMutation.isPending}
            >
              {bulkMutation.isPending && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
              Apply Changes
            </button>
          </div>
        </div>
      )}

      {/* Matrix table */}
      <div className="overflow-x-auto">
        <table className="min-w-full border border-gray-200 rounded-lg">
          <thead>
            <tr className="bg-gray-50">
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider border-b border-r border-gray-200 sticky left-0 bg-gray-50 z-10">
                Resource / Action
              </th>
              {matrix.roles.map((role) => (
                <th
                  key={role.id}
                  className="px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider border-b border-gray-200"
                >
                  <div className="flex items-center justify-center">
                    <Shield
                      className={clsx(
                        'w-4 h-4 mr-1',
                        role.is_system ? 'text-primary-600' : 'text-gray-400'
                      )}
                    />
                    {role.display_name}
                  </div>
                  {role.is_system && (
                    <span className="text-xs text-gray-400 font-normal">(system)</span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="bg-white divide-y divide-gray-200">
            {matrix.resources.map((resource) => (
              <>
                {/* Resource header row */}
                <tr key={resource.name} className="bg-gray-100">
                  <td
                    colSpan={matrix.roles.length + 1}
                    className="px-4 py-2 text-sm font-semibold text-gray-700"
                  >
                    {resource.display_name}
                  </td>
                </tr>
                {/* Action rows */}
                {resource.actions.map((action) => (
                  <tr key={`${resource.name}-${action}`} className="hover:bg-gray-50">
                    <td className="px-4 py-2 text-sm text-gray-600 border-r border-gray-200 pl-8 sticky left-0 bg-white">
                      {action}
                    </td>
                    {matrix.roles.map((role) => {
                      const isGranted = getEffectiveValue(role.id, resource.name, action);
                      const isPending = hasPendingChange(role.id, resource.name, action);

                      return (
                        <td
                          key={`${role.id}-${resource.name}-${action}`}
                          className="px-4 py-2 text-center"
                        >
                          <button
                            onClick={() =>
                              handleToggle(
                                role.id,
                                resource.name as Resource,
                                action as Action,
                                matrix.matrix[role.id]?.[resource.name]?.[action] ?? false
                              )
                            }
                            className={clsx(
                              'w-8 h-8 rounded-full flex items-center justify-center transition-all',
                              isGranted
                                ? 'bg-green-100 text-green-600 hover:bg-green-200'
                                : 'bg-gray-100 text-gray-400 hover:bg-gray-200',
                              isPending && 'ring-2 ring-amber-400'
                            )}
                            title={isGranted ? 'Granted' : 'Not granted'}
                          >
                            {isGranted ? (
                              <Check className="w-4 h-4" />
                            ) : (
                              <X className="w-4 h-4" />
                            )}
                          </button>
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
