import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, User, Shield, Trash2, Mail, ChevronRight, X, Loader2 } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import type { Role } from '../types';

interface UserData {
  id: string;
  username: string;
  email: string;
  role: string;
  roles?: Array<{ id: string; name: string; display_name: string }>;
  created_at: string;
}

export default function Users() {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedUser, setSelectedUser] = useState<UserData | null>(null);
  const [newUsername, setNewUsername] = useState('');
  const [newEmail, setNewEmail] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [pendingRoleChanges, setPendingRoleChanges] = useState<Set<string>>(new Set());
  const queryClient = useQueryClient();

  const { data: users = [], isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: api.getUsers,
  });

  const { data: roles = [] } = useQuery({
    queryKey: ['roles'],
    queryFn: api.getRoles,
  });

  const createMutation = useMutation({
    mutationFn: api.createUser,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      setIsCreateOpen(false);
      resetForm();
    },
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteUser,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      setSelectedUser(null);
    },
  });

  const assignRolesMutation = useMutation({
    mutationFn: ({ userId, roleIds }: { userId: string; roleIds: string[] }) =>
      api.assignUserRoles(userId, roleIds),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      setPendingRoleChanges(new Set());
      // Refresh selected user
      if (selectedUser) {
        api.getUser(selectedUser.id).then((user) => {
          if (user) setSelectedUser(user as UserData);
        });
      }
    },
  });

  const resetForm = () => {
    setNewUsername('');
    setNewEmail('');
    setNewPassword('');
  };

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({
      username: newUsername,
      email: newEmail,
      password: newPassword,
    });
  };

  const handleSelectUser = (user: UserData) => {
    setSelectedUser(user);
    setPendingRoleChanges(new Set());
  };

  const handleRoleToggle = (roleId: string) => {
    const newPending = new Set(pendingRoleChanges);
    if (newPending.has(roleId)) {
      newPending.delete(roleId);
    } else {
      newPending.add(roleId);
    }
    setPendingRoleChanges(newPending);
  };

  const getEffectiveRoles = (): Set<string> => {
    if (!selectedUser) return new Set();

    const currentRoles = new Set(selectedUser.roles?.map((r) => r.id) || []);
    const effective = new Set(currentRoles);

    // Toggle pending changes
    pendingRoleChanges.forEach((roleId) => {
      if (currentRoles.has(roleId)) {
        effective.delete(roleId);
      } else {
        effective.add(roleId);
      }
    });

    return effective;
  };

  const handleSaveRoles = () => {
    if (!selectedUser || pendingRoleChanges.size === 0) return;

    const effectiveRoles = getEffectiveRoles();
    assignRolesMutation.mutate({
      userId: selectedUser.id,
      roleIds: Array.from(effectiveRoles),
    });
  };

  const hasRoleChanged = (roleId: string): boolean => {
    return pendingRoleChanges.has(roleId);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  const effectiveRoles = getEffectiveRoles();

  return (
    <div>
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Users</h1>
          <p className="text-gray-500 mt-1">Manage user accounts and role assignments</p>
        </div>
        <button
          onClick={() => setIsCreateOpen(true)}
          className="btn btn-primary flex items-center"
        >
          <Plus className="w-4 h-4 mr-2" />
          New User
        </button>
      </div>

      {/* Create Modal */}
      {isCreateOpen && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-md">
            <h2 className="text-lg font-semibold mb-4">Create User</h2>
            <form onSubmit={handleCreate}>
              <div className="mb-4">
                <label className="label">Username</label>
                <input
                  type="text"
                  value={newUsername}
                  onChange={(e) => setNewUsername(e.target.value)}
                  className="input"
                  required
                />
              </div>
              <div className="mb-4">
                <label className="label">Email</label>
                <input
                  type="email"
                  value={newEmail}
                  onChange={(e) => setNewEmail(e.target.value)}
                  className="input"
                  required
                />
              </div>
              <div className="mb-4">
                <label className="label">Password</label>
                <input
                  type="password"
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  className="input"
                  required
                  minLength={8}
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

      {/* Users Table */}
      <div className="card p-0 overflow-hidden">
        <table className="min-w-full divide-y divide-gray-200">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                User
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Email
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Roles
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Created
              </th>
              <th className="px-6 py-3"></th>
            </tr>
          </thead>
          <tbody className="bg-white divide-y divide-gray-200">
            {users.map((user: UserData) => (
              <tr
                key={user.id}
                className={clsx(
                  'hover:bg-gray-50 cursor-pointer',
                  selectedUser?.id === user.id && 'bg-primary-50'
                )}
                onClick={() => handleSelectUser(user)}
              >
                <td className="px-6 py-4 whitespace-nowrap">
                  <div className="flex items-center">
                    <div className="w-8 h-8 rounded-full bg-primary-100 flex items-center justify-center mr-3">
                      <User className="w-4 h-4 text-primary-600" />
                    </div>
                    <span className="font-medium text-gray-900">{user.username}</span>
                  </div>
                </td>
                <td className="px-6 py-4 whitespace-nowrap">
                  <div className="flex items-center text-gray-500">
                    <Mail className="w-4 h-4 mr-2" />
                    {user.email}
                  </div>
                </td>
                <td className="px-6 py-4 whitespace-nowrap">
                  <div className="flex items-center gap-2 flex-wrap">
                    {user.roles && user.roles.length > 0 ? (
                      user.roles.map((role) => (
                        <span
                          key={role.id}
                          className="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-primary-100 text-primary-700"
                        >
                          <Shield className="w-3 h-3 mr-1" />
                          {role.display_name}
                        </span>
                      ))
                    ) : (
                      <span className="text-gray-400 text-sm">No roles</span>
                    )}
                  </div>
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                  {new Date(user.created_at).toLocaleDateString()}
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-right">
                  <ChevronRight className="w-5 h-5 text-gray-400" />
                </td>
              </tr>
            ))}
          </tbody>
        </table>

        {users.length === 0 && (
          <div className="text-center py-12 text-gray-500">
            <User className="w-12 h-12 mx-auto mb-4 text-gray-300" />
            No users found
          </div>
        )}
      </div>

      {/* User Detail Sidebar */}
      {selectedUser && (
        <div className="fixed inset-y-0 right-0 w-96 bg-white shadow-xl z-50 border-l border-gray-200">
          <div className="h-full flex flex-col">
            <div className="p-6 border-b border-gray-200">
              <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold">User Details</h2>
                <button
                  onClick={() => {
                    setSelectedUser(null);
                    setPendingRoleChanges(new Set());
                  }}
                  className="text-gray-400 hover:text-gray-600"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-6">
              <div className="text-center mb-6">
                <div className="w-16 h-16 rounded-full bg-primary-100 flex items-center justify-center mx-auto mb-3">
                  <User className="w-8 h-8 text-primary-600" />
                </div>
                <h3 className="font-semibold text-gray-900">{selectedUser.username}</h3>
                <p className="text-gray-500">{selectedUser.email}</p>
              </div>

              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <label className="label mb-0">Assigned Roles</label>
                  {pendingRoleChanges.size > 0 && (
                    <div className="flex gap-2">
                      <button
                        onClick={() => setPendingRoleChanges(new Set())}
                        className="text-sm text-gray-500 hover:text-gray-700"
                      >
                        Cancel
                      </button>
                      <button
                        onClick={handleSaveRoles}
                        disabled={assignRolesMutation.isPending}
                        className="btn btn-primary text-sm flex items-center"
                      >
                        {assignRolesMutation.isPending && (
                          <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                        )}
                        Save
                      </button>
                    </div>
                  )}
                </div>
                <div className="space-y-2">
                  {roles.map((role: Role) => {
                    const isAssigned = effectiveRoles.has(role.id);
                    const isChanged = hasRoleChanged(role.id);

                    return (
                      <label
                        key={role.id}
                        className={clsx(
                          'flex items-center p-3 rounded-lg cursor-pointer transition-all',
                          isAssigned
                            ? 'bg-primary-50 border border-primary-200'
                            : 'bg-gray-50 border border-gray-200 hover:bg-gray-100',
                          isChanged && 'ring-2 ring-amber-400'
                        )}
                      >
                        <input
                          type="checkbox"
                          checked={isAssigned}
                          onChange={() => handleRoleToggle(role.id)}
                          className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
                        />
                        <div className="ml-3 flex-1">
                          <p className="font-medium text-gray-900">{role.display_name}</p>
                          <p className="text-sm text-gray-500">{role.name}</p>
                        </div>
                        {role.is_system && (
                          <span className="text-xs bg-gray-200 text-gray-600 px-2 py-1 rounded">
                            System
                          </span>
                        )}
                      </label>
                    );
                  })}
                </div>

                {/* Effective Permissions Summary */}
                <div className="mt-6">
                  <label className="label">Effective Permissions</label>
                  <div className="bg-gray-50 rounded-lg p-4">
                    {selectedUser.roles && selectedUser.roles.length > 0 ? (
                      <p className="text-sm text-gray-600">
                        User has {selectedUser.roles.length} role(s) with combined permissions.
                        View individual roles for permission details.
                      </p>
                    ) : (
                      <p className="text-sm text-gray-500">
                        No roles assigned. User has no permissions.
                      </p>
                    )}
                  </div>
                </div>
              </div>
            </div>

            <div className="p-6 border-t border-gray-200">
              <button
                onClick={() => deleteMutation.mutate(selectedUser.id)}
                disabled={deleteMutation.isPending}
                className="btn btn-danger w-full flex items-center justify-center"
              >
                {deleteMutation.isPending ? (
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                ) : (
                  <Trash2 className="w-4 h-4 mr-2" />
                )}
                Delete User
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
