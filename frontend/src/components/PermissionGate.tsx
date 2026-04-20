import type { ReactNode } from 'react';
import { usePermissionsStore } from '../stores/permissionsStore';
import type { Resource, Action } from '../types';

interface PermissionGateProps {
  resource: Resource;
  action: Action;
  children: ReactNode;
  fallback?: ReactNode;
}

/**
 * Conditionally renders children based on user permissions.
 *
 * Usage:
 * <PermissionGate resource="users" action="create">
 *   <button>Create User</button>
 * </PermissionGate>
 */
export function PermissionGate({
  resource,
  action,
  children,
  fallback = null,
}: PermissionGateProps) {
  const hasPermission = usePermissionsStore((state) => state.hasPermission);

  if (hasPermission(resource, action)) {
    return <>{children}</>;
  }

  return <>{fallback}</>;
}

interface RequireAnyPermissionProps {
  resource: Resource;
  actions: Action[];
  children: ReactNode;
  fallback?: ReactNode;
}

/**
 * Renders children if user has ANY of the specified permissions.
 */
export function RequireAnyPermission({
  resource,
  actions,
  children,
  fallback = null,
}: RequireAnyPermissionProps) {
  const hasAnyPermission = usePermissionsStore((state) => state.hasAnyPermission);

  if (hasAnyPermission(resource, actions)) {
    return <>{children}</>;
  }

  return <>{fallback}</>;
}
