import { useEffect, useState } from 'react';
import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import ProtectedRoute from './components/ProtectedRoute';
import { AccessDenied } from './components/AccessDenied';
import ForcePasswordChange from './components/ForcePasswordChange';
import Login from './pages/Login';
import Dashboard from './pages/Dashboard';
import Nodes from './pages/Nodes';
import NodeDetail from './pages/NodeDetail';
import Groups from './pages/Groups';
import Reports from './pages/Reports';
import Facts from './pages/Facts';
import FacterTemplates from './pages/FacterTemplates';
import Analytics from './pages/Analytics';
import Alerting from './pages/Alerting';
import Settings from './pages/Settings';
import Roles from './pages/Roles';
import Users from './pages/Users';
import Permissions from './pages/Permissions';
import Profile from './pages/Profile';
import CA from './pages/CA';
import { useAuthStore } from './stores/authStore';
import { usePermissionsStore } from './stores/permissionsStore';

function App() {
  const user = useAuthStore((state) => state.user);
  const login = useAuthStore((state) => state.login);
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const permissions = usePermissionsStore((state) => state.permissions);
  const fetchPermissions = usePermissionsStore((state) => state.fetchPermissions);
  const [showForcePasswordChange, setShowForcePasswordChange] = useState(false);

  // Fetch permissions on app load if user is authenticated but permissions are not loaded
  useEffect(() => {
    if (isAuthenticated && user && !permissions) {
      fetchPermissions(user.id);
    }
  }, [isAuthenticated, user, permissions, fetchPermissions]);

  // Check if force password change is required
  useEffect(() => {
    if (isAuthenticated && user?.force_password_change) {
      setShowForcePasswordChange(true);
    }
  }, [isAuthenticated, user?.force_password_change]);

  const handlePasswordChangeSuccess = () => {
    // Update user state to clear force_password_change flag
    if (user) {
      login({ ...user, force_password_change: false }, localStorage.getItem('auth_token') || '');
    }
    setShowForcePasswordChange(false);
  };

  return (
    <>
      {/* Force Password Change Modal */}
      {showForcePasswordChange && (
        <ForcePasswordChange onSuccess={handlePasswordChangeSuccess} />
      )}

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
                  <Route path="/alerting" element={<Alerting />} />
                  <Route path="/profile" element={<Profile />} />
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
                  <Route path="/ca" element={<CA />} />
                </Routes>
              </Layout>
            </ProtectedRoute>
          }
        />
      </Routes>
    </>
  );
}

export default App;
