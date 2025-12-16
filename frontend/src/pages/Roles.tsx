import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, Shield, ChevronRight, Trash2, Users, Lock } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import { Role } from '../types';

export default function Roles() {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedRole, setSelectedRole] = useState<Role | null>(null);
  const [newRoleName, setNewRoleName] = useState('');
  const [newRoleDisplayName, setNewRoleDisplayName] = useState('');
  const [newRoleDescription, setNewRoleDescription] = useState('');
  const queryClient = useQueryClient();

  const { data: roles = [], isLoading } = useQuery({
    queryKey: ['roles'],
    queryFn: api.getRoles,
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
                  {role.is_system && (
                    <span className="text-xs bg-gray-100 text-gray-600 px-2 py-1 rounded">
                      System
                    </span>
                  )}
                </button>
              ))}
              {roles.length === 0 && (
                <div className="p-4 text-center text-gray-500">
                  No roles defined
                </div>
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
                <h3 className="font-semibold text-gray-900 mb-3 flex items-center">
                  <Lock className="w-4 h-4 mr-2" />
                  Permissions
                </h3>
                <div className="bg-gray-50 rounded-lg p-4">
                  {selectedRole.permissions && selectedRole.permissions.length > 0 ? (
                    <div className="space-y-2">
                      {selectedRole.permissions.map((perm, index) => (
                        <div
                          key={index}
                          className="flex items-center justify-between bg-white px-3 py-2 rounded border border-gray-200"
                        >
                          <span className="font-medium text-gray-900">
                            {perm.resource}
                          </span>
                          <span className="text-sm bg-primary-100 text-primary-700 px-2 py-1 rounded">
                            {perm.action}
                          </span>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="text-gray-500 text-center py-4">
                      No permissions assigned
                    </p>
                  )}
                </div>
              </div>

              {/* Users with this role */}
              <div>
                <h3 className="font-semibold text-gray-900 mb-3 flex items-center">
                  <Users className="w-4 h-4 mr-2" />
                  Users with this Role
                </h3>
                <div className="bg-gray-50 rounded-lg p-4">
                  <p className="text-gray-500 text-center py-4">
                    User assignment coming soon
                  </p>
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
