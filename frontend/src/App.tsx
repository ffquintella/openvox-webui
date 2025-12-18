import { useEffect, useState, lazy, Suspense } from 'react';
import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import ProtectedRoute from './components/ProtectedRoute';
import { AccessDenied } from './components/AccessDenied';
import ForcePasswordChange from './components/ForcePasswordChange';
import { useAuthStore } from './stores/authStore';
import { usePermissionsStore } from './stores/permissionsStore';

// Lazy load all page components for code splitting
const Login = lazy(() => import('./pages/Login'));
const Dashboard = lazy(() => import('./pages/Dashboard'));
const Nodes = lazy(() => import('./pages/Nodes'));
const NodeDetail = lazy(() => import('./pages/NodeDetail'));
const Groups = lazy(() => import('./pages/Groups'));
const Reports = lazy(() => import('./pages/Reports'));
const Facts = lazy(() => import('./pages/Facts'));
const FacterTemplates = lazy(() => import('./pages/FacterTemplates'));
const Analytics = lazy(() => import('./pages/Analytics'));
const Alerting = lazy(() => import('./pages/Alerting'));
const Settings = lazy(() => import('./pages/Settings'));
const Roles = lazy(() => import('./pages/Roles'));
const Users = lazy(() => import('./pages/Users'));
const Permissions = lazy(() => import('./pages/Permissions'));
const Profile = lazy(() => import('./pages/Profile'));
const CA = lazy(() => import('./pages/CA'));

// Loading spinner component for Suspense fallback
const PageLoader = () => (
  <div className="flex items-center justify-center min-h-screen">
    <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500"></div>
  </div>
);

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

      <Suspense fallback={<PageLoader />}>
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
                  <Suspense fallback={<PageLoader />}>
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
                  </Suspense>
                </Layout>
              </ProtectedRoute>
            }
          />
        </Routes>
      </Suspense>
    </>
  );
}

export default App;
