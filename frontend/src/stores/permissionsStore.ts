import { create } from 'zustand';
import type { EffectivePermissions, Resource, Action, Permission } from '../types';
import { api } from '../services/api';

interface PermissionsState {
  permissions: EffectivePermissions | null;
  isLoading: boolean;
  error: string | null;
  fetchPermissions: (userId: string) => Promise<void>;
  hasPermission: (resource: Resource, action: Action) => boolean;
  hasAnyPermission: (resource: Resource, actions: Action[]) => boolean;
  clearPermissions: () => void;
}

export const usePermissionsStore = create<PermissionsState>((set, get) => ({
  permissions: null,
  isLoading: false,
  error: null,

  fetchPermissions: async (userId: string) => {
    set({ isLoading: true, error: null });
    try {
      const permissions = await api.getUserPermissions(userId);
      set({ permissions, isLoading: false });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : 'Failed to fetch permissions',
        isLoading: false,
      });
    }
  },

  hasPermission: (resource: Resource, action: Action): boolean => {
    const { permissions } = get();
    if (!permissions) return false;

    return permissions.permissions.some((perm: Permission) => {
      // Check for exact match
      if (perm.resource === resource && perm.action === action) {
        return true;
      }
      // Admin action grants all actions on the resource
      if (perm.resource === resource && perm.action === 'admin') {
        return true;
      }
      return false;
    });
  },

  hasAnyPermission: (resource: Resource, actions: Action[]): boolean => {
    const { hasPermission } = get();
    return actions.some((action) => hasPermission(resource, action));
  },

  clearPermissions: () => {
    set({ permissions: null, error: null });
  },
}));
