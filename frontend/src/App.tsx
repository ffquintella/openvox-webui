import { useEffect } from 'react';
import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import ProtectedRoute from './components/ProtectedRoute';
import { AccessDenied } from './components/AccessDenied';
import Login from './pages/Login';
import Dashboard from './pages/Dashboard';
import Nodes from './pages/Nodes';
import NodeDetail from './pages/NodeDetail';
import Groups from './pages/Groups';
import Reports from './pages/Reports';
import Facts from './pages/Facts';
import FacterTemplates from './pages/FacterTemplates';
import Analytics from './pages/Analytics';
import Settings from './pages/Settings';
import Roles from './pages/Roles';
import Users from './pages/Users';
import Permissions from './pages/Permissions';
import { useAuthStore } from './stores/authStore';
import { usePermissionsStore } from './stores/permissionsStore';

function App() {
  const user = useAuthStore((state) => state.user);
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const permissions = usePermissionsStore((state) => state.permissions);
  const fetchPermissions = usePermissionsStore((state) => state.fetchPermissions);

  // Fetch permissions on app load if user is authenticated but permissions are not loaded
  useEffect(() => {
    if (isAuthenticated && user && !permissions) {
      fetchPermissions(user.id);
    }
  }, [isAuthenticated, user, permissions, fetchPermissions]);
  return (
    <Routes>
      {/* Public routes */}
      <Route path="/login" element={<Login />} />
      <Route path="/access-denied" element={<AccessDenied />} />

      {/* Protected routes */}
      <Route
        path="/*"
        element={
          <ProtectedRoute>
            <Layout>
              <Routes>
                <Route path="/" element={<Dashboard />} />
                <Route path="/nodes" element={<Nodes />} />
                <Route path="/nodes/:certname" element={<NodeDetail />} />
                <Route path="/groups" element={<Groups />} />
                <Route path="/reports" element={<Reports />} />
                <Route path="/facts" element={<Facts />} />
                <Route path="/facter-templates" element={<FacterTemplates />} />
                <Route path="/analytics" element={<Analytics />} />
                <Route
                  path="/roles"
                  element={
                    <ProtectedRoute requiredPermission={{ resource: 'roles', action: 'read' }}>
                      <Roles />
                    </ProtectedRoute>
                  }
                />
                <Route
                  path="/users"
                  element={
                    <ProtectedRoute requiredPermission={{ resource: 'users', action: 'read' }}>
                      <Users />
                    </ProtectedRoute>
                  }
                />
                <Route
                  path="/permissions"
                  element={
                    <ProtectedRoute requiredPermission={{ resource: 'roles', action: 'read' }}>
                      <Permissions />
                    </ProtectedRoute>
                  }
                />
                <Route
                  path="/settings"
                  element={
                    <ProtectedRoute requiredPermission={{ resource: 'settings', action: 'read' }}>
                      <Settings />
                    </ProtectedRoute>
                  }
                />
              </Routes>
            </Layout>
          </ProtectedRoute>
        }
      />
    </Routes>
  );
}

export default App;
