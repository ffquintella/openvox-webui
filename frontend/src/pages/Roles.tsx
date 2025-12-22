import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, Shield, Trash2, Users, Lock, X } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import type { Role, ResourceInfo, Resource, Action, CreatePermissionRequest } from '../types';

export default function Roles() {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedRole, setSelectedRole] = useState<Role | null>(null);
  const [newRoleName, setNewRoleName] = useState('');
  const [newRoleDisplayName, setNewRoleDisplayName] = useState('');
  const [newRoleDescription, setNewRoleDescription] = useState('');
  const [isAddPermissionOpen, setIsAddPermissionOpen] = useState(false);
  const [newPermResource, setNewPermResource] = useState<Resource | ''>('');
  const [newPermAction, setNewPermAction] = useState<Action | ''>('');
  const queryClient = useQueryClient();

  const { data: roles = [], isLoading } = useQuery({
    queryKey: ['roles'],
    queryFn: api.getRoles,
  });

  const { data: resources = [] } = useQuery<ResourceInfo[]>({
    queryKey: ['resources'],
    queryFn: api.getResources,
    retry: false,
    staleTime: 5 * 60 * 1000, // 5 minutes - resources rarely change
  });

  const { data: usersWithRole = [] } = useQuery({
    queryKey: ['roleUsers', selectedRole?.id],
    queryFn: async () => {
      if (!selectedRole) return [];
      const users = await api.getUsers();
      return users.filter((u) => u.roles?.some((r) => r.id === selectedRole.id));
    },
    enabled: !!selectedRole,
  });

  const createMutation = useMutation({
    mutationFn: api.createRole,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['roles'] });
      setIsCreateOpen(false);
      resetForm();
    },
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteRole,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['roles'] });
      setSelectedRole(null);
    },
  });

  const addPermissionMutation = useMutation({
    mutationFn: ({ roleId, permission }: { roleId: string; permission: CreatePermissionRequest }) =>
      api.addPermissionToRole(roleId, permission),
    onSuccess: (updatedRole) => {
      queryClient.invalidateQueries({ queryKey: ['roles'] });
      setSelectedRole(updatedRole);
      setIsAddPermissionOpen(false);
      setNewPermResource('');
      setNewPermAction('');
    },
  });

  const removePermissionMutation = useMutation({
    mutationFn: ({ roleId, permissionId }: { roleId: string; permissionId: string }) =>
      api.removePermissionFromRole(roleId, permissionId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['roles'] });
      // Refresh selected role
      if (selectedRole) {
        api.getRole(selectedRole.id).then((role) => setSelectedRole(role));
      }
    },
  });

  const resetForm = () => {
    setNewRoleName('');
    setNewRoleDisplayName('');
    setNewRoleDescription('');
  };

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({
      name: newRoleName,
      display_name: newRoleDisplayName,
      description: newRoleDescription || undefined,
    });
  };

  const handleAddPermission = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedRole || !newPermResource || !newPermAction) return;

    addPermissionMutation.mutate({
      roleId: selectedRole.id,
      permission: {
        resource: newPermResource,
        action: newPermAction,
      },
    });
  };

  const getAvailableActions = (resourceName: string): string[] => {
    const resource = resources.find((r) => r.name === resourceName);
    return resource?.available_actions || [];
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Roles</h1>
          <p className="text-gray-500 mt-1">Manage roles and their permissions</p>
        </div>
        <button
          onClick={() => setIsCreateOpen(true)}
          className="btn btn-primary flex items-center"
        >
          <Plus className="w-4 h-4 mr-2" />
          New Role
        </button>
      </div>

      {/* Create Modal */}
      {isCreateOpen && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-md">
            <h2 className="text-lg font-semibold mb-4">Create Role</h2>
            <form onSubmit={handleCreate}>
              <div className="mb-4">
                <label className="label">Name (identifier)</label>
                <input
                  type="text"
                  value={newRoleName}
                  onChange={(e) => setNewRoleName(e.target.value.toLowerCase().replace(/\s/g, '_'))}
                  className="input"
                  placeholder="e.g., team_lead"
                  required
                />
              </div>
              <div className="mb-4">
                <label className="label">Display Name</label>
                <input
                  type="text"
                  value={newRoleDisplayName}
                  onChange={(e) => setNewRoleDisplayName(e.target.value)}
                  className="input"
                  placeholder="e.g., Team Lead"
                  required
                />
              </div>
              <div className="mb-4">
                <label className="label">Description</label>
                <textarea
                  value={newRoleDescription}
                  onChange={(e) => setNewRoleDescription(e.target.value)}
                  className="input"
                  rows={3}
                  placeholder="Describe what this role can do..."
                />
              </div>
              <div className="flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => {
                    setIsCreateOpen(false);
                    resetForm();
                  }}
                  className="btn btn-secondary"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={createMutation.isPending}
                  className="btn btn-primary"
                >
                  {createMutation.isPending ? 'Creating...' : 'Create'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Roles List */}
        <div className="lg:col-span-1">
          <div className="card p-0">
            <div className="p-4 border-b border-gray-200">
              <h2 className="font-semibold text-gray-900">All Roles</h2>
            </div>
            <div className="divide-y divide-gray-200">
              {roles.map((role: Role) => (
                <button
                  key={role.id}
                  onClick={() => setSelectedRole(role)}
                  className={clsx(
                    'w-full px-4 py-3 flex items-center justify-between text-left hover:bg-gray-50',
                    selectedRole?.id === role.id && 'bg-primary-50'
                  )}
                >
                  <div className="flex items-center">
                    <Shield
                      className={clsx(
                        'w-5 h-5 mr-3',
                        role.is_system ? 'text-primary-600' : 'text-gray-400'
                      )}
                    />
                    <div>
                      <p className="font-medium text-gray-900">{role.display_name}</p>
                      <p className="text-sm text-gray-500">{role.name}</p>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-gray-400">
                      {role.permissions?.length || 0} perms
                    </span>
                    {role.is_system && (
                      <span className="text-xs bg-gray-100 text-gray-600 px-2 py-1 rounded">
                        System
                      </span>
                    )}
                  </div>
                </button>
              ))}
              {roles.length === 0 && (
                <div className="p-4 text-center text-gray-500">No roles defined</div>
              )}
            </div>
          </div>
        </div>

        {/* Role Details */}
        <div className="lg:col-span-2">
          {selectedRole ? (
            <div className="card">
              <div className="flex items-center justify-between mb-6">
                <div className="flex items-center">
                  <Shield className="w-8 h-8 text-primary-600 mr-3" />
                  <div>
                    <h2 className="text-xl font-semibold text-gray-900">
                      {selectedRole.display_name}
                    </h2>
                    <p className="text-gray-500">{selectedRole.name}</p>
                  </div>
                </div>
                {!selectedRole.is_system && (
                  <button
                    onClick={() => deleteMutation.mutate(selectedRole.id)}
                    className="btn btn-danger flex items-center"
                  >
                    <Trash2 className="w-4 h-4 mr-2" />
                    Delete
                  </button>
                )}
              </div>

              {selectedRole.description && (
                <p className="text-gray-600 mb-6">{selectedRole.description}</p>
              )}

              {/* Permissions */}
              <div className="mb-6">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="font-semibold text-gray-900 flex items-center">
                    <Lock className="w-4 h-4 mr-2" />
                    Permissions
                  </h3>
                  {!selectedRole.is_system && (
                    <button
                      onClick={() => setIsAddPermissionOpen(true)}
                      className="btn btn-secondary text-sm flex items-center"
                    >
                      <Plus className="w-4 h-4 mr-1" />
                      Add Permission
                    </button>
                  )}
                </div>

                {/* Add Permission Form */}
                {isAddPermissionOpen && (
                  <div className="bg-gray-50 rounded-lg p-4 mb-4 border border-gray-200">
                    <form onSubmit={handleAddPermission} className="space-y-4">
                      <div className="grid grid-cols-2 gap-4">
                        <div>
                          <label className="label">Resource</label>
                          <select
                            value={newPermResource}
                            onChange={(e) => {
                              setNewPermResource(e.target.value as Resource);
                              setNewPermAction('');
                            }}
                            className="input"
                            required
                          >
                            <option value="">Select resource...</option>
                            {resources.map((r) => (
                              <option key={r.name} value={r.name}>
                                {r.display_name}
                              </option>
                            ))}
                          </select>
                        </div>
                        <div>
                          <label className="label">Action</label>
                          <select
                            value={newPermAction}
                            onChange={(e) => setNewPermAction(e.target.value as Action)}
                            className="input"
                            required
                            disabled={!newPermResource}
                          >
                            <option value="">Select action...</option>
                            {newPermResource &&
                              getAvailableActions(newPermResource).map((a) => (
                                <option key={a} value={a}>
                                  {a}
                                </option>
                              ))}
                          </select>
                        </div>
                      </div>
                      <div className="flex justify-end gap-2">
                        <button
                          type="button"
                          onClick={() => {
                            setIsAddPermissionOpen(false);
                            setNewPermResource('');
                            setNewPermAction('');
                          }}
                          className="btn btn-secondary text-sm"
                        >
                          Cancel
                        </button>
                        <button
                          type="submit"
                          disabled={addPermissionMutation.isPending}
                          className="btn btn-primary text-sm"
                        >
                          {addPermissionMutation.isPending ? 'Adding...' : 'Add'}
                        </button>
                      </div>
                    </form>
                  </div>
                )}

                <div className="bg-gray-50 rounded-lg p-4">
                  {selectedRole.permissions && selectedRole.permissions.length > 0 ? (
                    <div className="space-y-2">
                      {selectedRole.permissions.map((perm) => (
                        <div
                          key={perm.id}
                          className="flex items-center justify-between bg-white px-3 py-2 rounded border border-gray-200"
                        >
                          <div className="flex items-center gap-3">
                            <span className="font-medium text-gray-900">{perm.resource}</span>
                            <span className="text-sm bg-primary-100 text-primary-700 px-2 py-1 rounded">
                              {perm.action}
                            </span>
                            {perm.scope && perm.scope.type !== 'all' && (
                              <span className="text-xs bg-gray-100 text-gray-600 px-2 py-1 rounded">
                                {perm.scope.type}
                                {'value' in perm.scope && `: ${perm.scope.value}`}
                              </span>
                            )}
                          </div>
                          {!selectedRole.is_system && (
                            <button
                              onClick={() =>
                                removePermissionMutation.mutate({
                                  roleId: selectedRole.id,
                                  permissionId: perm.id,
                                })
                              }
                              className="text-gray-400 hover:text-red-600"
                              title="Remove permission"
                            >
                              <X className="w-4 h-4" />
                            </button>
                          )}
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="text-gray-500 text-center py-4">No permissions assigned</p>
                  )}
                </div>
              </div>

              {/* Users with this role */}
              <div>
                <h3 className="font-semibold text-gray-900 mb-3 flex items-center">
                  <Users className="w-4 h-4 mr-2" />
                  Users with this Role ({usersWithRole.length})
                </h3>
                <div className="bg-gray-50 rounded-lg p-4">
                  {usersWithRole.length > 0 ? (
                    <div className="space-y-2">
                      {usersWithRole.map((user) => (
                        <div
                          key={user.id}
                          className="flex items-center bg-white px-3 py-2 rounded border border-gray-200"
                        >
                          <div className="w-8 h-8 rounded-full bg-primary-100 flex items-center justify-center mr-3">
                            <Users className="w-4 h-4 text-primary-600" />
                          </div>
                          <div>
                            <p className="font-medium text-gray-900">{user.username}</p>
                            <p className="text-sm text-gray-500">{user.email}</p>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="text-gray-500 text-center py-4">No users have this role</p>
                  )}
                </div>
              </div>
            </div>
          ) : (
            <div className="card flex items-center justify-center h-64">
              <div className="text-center text-gray-500">
                <Shield className="w-12 h-12 mx-auto mb-4 text-gray-300" />
                <p>Select a role to view details</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
