import { useEffect, useState, lazy, Suspense } from 'react';
import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import ProtectedRoute from './components/ProtectedRoute';
import { AccessDenied } from './components/AccessDenied';
import ForcePasswordChange from './components/ForcePasswordChange';
import {
  getLastSessionActivity,
  recordSessionActivity,
  SESSION_IDLE_TIMEOUT_MS,
  useAuthStore,
} from './stores/authStore';
import { usePermissionsStore } from './stores/permissionsStore';

// Lazy load all page components for code splitting
const Login = lazy(() => import('./pages/Login'));
const SamlCallback = lazy(() => import('./pages/SamlCallback'));
const Dashboard = lazy(() => import('./pages/Dashboard'));
const Nodes = lazy(() => import('./pages/Nodes'));
const NodeDetail = lazy(() => import('./pages/NodeDetail'));
const AddNode = lazy(() => import('./pages/AddNode'));
const Groups = lazy(() => import('./pages/Groups'));
const Reports = lazy(() => import('./pages/Reports'));
const Facts = lazy(() => import('./pages/Facts'));
const FacterTemplates = lazy(() => import('./pages/FacterTemplates'));
const Analytics = lazy(() => import('./pages/Analytics'));
const Alerting = lazy(() => import('./pages/Alerting'));
const Updates = lazy(() => import('./pages/Updates'));
const Settings = lazy(() => import('./pages/Settings'));
const Roles = lazy(() => import('./pages/Roles'));
const Users = lazy(() => import('./pages/Users'));
const Permissions = lazy(() => import('./pages/Permissions'));
const Profile = lazy(() => import('./pages/Profile'));
const CA = lazy(() => import('./pages/CA'));
const CodeDeploy = lazy(() => import('./pages/CodeDeploy'));
const Backup = lazy(() => import('./pages/Backup'));
const About = lazy(() => import('./pages/About'));

// Loading spinner component for Suspense fallback
const PageLoader = () => (
  <div className="flex items-center justify-center min-h-screen">
    <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500"></div>
  </div>
);

function App() {
  const user = useAuthStore((state) => state.user);
  const login = useAuthStore((state) => state.login);
  const logout = useAuthStore((state) => state.logout);
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

  useEffect(() => {
    if (!isAuthenticated) {
      return undefined;
    }

    let lastRecordedAt = getLastSessionActivity();
    recordSessionActivity(lastRecordedAt);

    const handleActivity = () => {
      const now = Date.now();
      if (now - lastRecordedAt >= 10_000) {
        lastRecordedAt = now;
        recordSessionActivity(now);
      }
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        handleActivity();
      }
    };

    const checkIdleTimeout = () => {
      if (Date.now() - getLastSessionActivity() > SESSION_IDLE_TIMEOUT_MS) {
        logout('Your session expired after 30 minutes of inactivity.');
        window.location.replace('/login');
      }
    };

    const events: Array<keyof WindowEventMap> = [
      'mousedown',
      'keydown',
      'scroll',
      'touchstart',
      'click',
      'mousemove',
    ];

    events.forEach((eventName) => window.addEventListener(eventName, handleActivity, { passive: true }));
    document.addEventListener('visibilitychange', handleVisibilityChange);
    const intervalId = window.setInterval(checkIdleTimeout, 60_000);

    return () => {
      events.forEach((eventName) => window.removeEventListener(eventName, handleActivity));
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.clearInterval(intervalId);
    };
  }, [isAuthenticated, logout]);

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
          <Route path="/saml-callback" element={<SamlCallback />} />
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
                      <Route path="/nodes/add" element={<AddNode />} />
                      <Route path="/nodes/:certname" element={<NodeDetail />} />
                      <Route path="/groups" element={<Groups />} />
                      <Route path="/reports" element={<Reports />} />
                      <Route path="/facts" element={<Facts />} />
                      <Route path="/facter-templates" element={<FacterTemplates />} />
                      <Route path="/analytics" element={<Analytics />} />
                      <Route path="/alerting" element={<Alerting />} />
                      <Route path="/updates" element={<Updates />} />
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
                      <Route path="/code-deploy" element={<CodeDeploy />} />
                      <Route path="/backup" element={<Backup />} />
                      <Route path="/about" element={<About />} />
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
