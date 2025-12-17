import { Navigate, useLocation } from 'react-router-dom';
import { useAuthStore } from '../stores/authStore';
import { usePermissionsStore } from '../stores/permissionsStore';
import type { Resource, Action } from '../types';

interface RequiredPermission {
  resource: Resource;
  action: Action;
}

interface ProtectedRouteProps {
  children: React.ReactNode;
  requiredPermission?: RequiredPermission;
}

export default function ProtectedRoute({ children, requiredPermission }: ProtectedRouteProps) {
  const location = useLocation();
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const hasPermission = usePermissionsStore((state) => state.hasPermission);

  // Check authentication
  if (!isAuthenticated) {
    // Redirect to login page, saving the attempted location
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  // Check permission if required
  if (requiredPermission && !hasPermission(requiredPermission.resource, requiredPermission.action)) {
    // User is authenticated but doesn't have permission
    return <Navigate to="/access-denied" replace />;
  }

  return <>{children}</>;
}
