import { ReactNode, useState, useRef, useEffect, useMemo } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import {
  LayoutDashboard,
  Server,
  FolderTree,
  FileText,
  Database,
  FileCode2,
  BarChart3,
  Bell,
  Settings,
  Shield,
  ShieldCheck,
  Users,
  BookUser,
  UserCog,
  Lock,
  User,
  LogOut,
  ChevronDown,
  Info,
  Rocket,
  HardDrive,
  RefreshCw,
  Puzzle,
} from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import { useAuthStore } from '../stores/authStore';
import { usePermissionsStore } from '../stores/permissionsStore';
import { usePluginSlots } from '../hooks/usePlugins';
import PluginSlot from './plugins/PluginSlot';
import PluginToasts from './plugins/PluginToasts';
import { mergeMenu, type CoreMenuItem } from '../utils/pluginMenu';
import NotificationBell from './NotificationBell';
import NotificationManager from './NotificationManager';

interface LayoutProps {
  children: ReactNode;
}

const navigation: CoreMenuItem[] = [
  { id: 'core:dashboard', name: 'Dashboard', href: '/', icon: LayoutDashboard },
  { id: 'core:nodes', name: 'Nodes', href: '/nodes', icon: Server },
  { id: 'core:groups', name: 'Groups', href: '/groups', icon: FolderTree },
  { id: 'core:reports', name: 'Reports', href: '/reports', icon: FileText },
  { id: 'core:facts', name: 'Facts', href: '/facts', icon: Database },
  {
    id: 'core:facter-templates',
    name: 'Facter Templates',
    href: '/facter-templates',
    icon: FileCode2,
  },
  { id: 'core:analytics', name: 'Analytics', href: '/analytics', icon: BarChart3 },
  { id: 'core:alerting', name: 'Alerting', href: '/alerting', icon: Bell },
  { id: 'core:updates', name: 'Updates', href: '/updates', icon: RefreshCw },
];

const adminNavigation: CoreMenuItem[] = [
  { id: 'core:code-deploy', name: 'Code Deploy', href: '/code-deploy', icon: Rocket },
  { id: 'core:backup', name: 'Backup', href: '/backup', icon: HardDrive },
  { id: 'core:ca', name: 'Certificate Authority', href: '/ca', icon: ShieldCheck },
  { id: 'core:users', name: 'Users', href: '/users', icon: Users },
  { id: 'core:directories', name: 'Directories', href: '/directories', icon: BookUser },
  { id: 'core:scim', name: 'SCIM Provisioning', href: '/scim', icon: UserCog },
  { id: 'core:roles', name: 'Roles', href: '/roles', icon: Shield },
  { id: 'core:permissions', name: 'Permissions', href: '/permissions', icon: Lock },
  { id: 'core:plugins', name: 'Plugins', href: '/plugins', icon: Puzzle },
  { id: 'core:settings', name: 'Settings', href: '/settings', icon: Settings },
];

const userMenuNavigation: CoreMenuItem[] = [
  { id: 'core:profile', name: 'Profile', href: '/profile', icon: User },
  { id: 'core:about', name: 'About', href: '/about', icon: Info },
];

export default function Layout({ children }: LayoutProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const user = useAuthStore((state) => state.user);
  const logout = useAuthStore((state) => state.logout);
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const hasPermission = usePermissionsStore((state) => state.hasPermission);
  const canSeePlugins = hasPermission('plugins', 'read');
  const canSeeDirectories = hasPermission('users', 'admin');
  const { data: pluginSlots } = usePluginSlots();
  const userMenuRef = useRef<HTMLDivElement>(null);

  // Close user menu when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (userMenuRef.current && !userMenuRef.current.contains(event.target as Node)) {
        setUserMenuOpen(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const mainItems = useMemo(
    () => mergeMenu(navigation, pluginSlots?.slots['nav.main'], pluginSlots?.menu_ops),
    [pluginSlots]
  );
  const adminItems = useMemo(
    () =>
      mergeMenu(
        adminNavigation.filter(
          (item) =>
            (item.id !== 'core:plugins' || canSeePlugins) &&
            ((item.id !== 'core:directories' && item.id !== 'core:scim') || canSeeDirectories)
        ),
        pluginSlots?.slots['nav.admin'],
        pluginSlots?.menu_ops
      ),
    [pluginSlots, canSeePlugins, canSeeDirectories]
  );
  const userMenuItems = useMemo(
    () => mergeMenu(userMenuNavigation, pluginSlots?.slots['nav.user-menu'], pluginSlots?.menu_ops),
    [pluginSlots]
  );

  const handleLogout = async () => {
    try {
      await api.logout();
    } catch {
      // Ignore logout API failures; local logout still clears client state.
    }
    logout();
    navigate('/login');
  };

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-gray-900 dark:text-gray-100">
      {/* Notification Manager - handles SSE and toasts */}
      <NotificationManager />

      {/* Sidebar */}
      <aside className="fixed inset-y-0 left-0 w-64 bg-white dark:bg-gray-800 border-r border-gray-200 dark:border-gray-700">
        <div className="flex flex-col h-full">
          {/* Logo */}
          <div className="flex items-center h-16 px-6 border-b border-gray-200 dark:border-gray-700">
            <span className="text-xl font-bold text-primary-600 dark:text-primary-400">
              OpenVox
            </span>
            <span className="ml-1 text-xl font-light text-gray-600 dark:text-gray-300">WebUI</span>
          </div>

          {/* Navigation */}
          <nav className="flex-1 px-4 py-4 overflow-y-auto">
            <div className="space-y-1">
              {mainItems.map((item) => {
                const isActive =
                  item.href === '/'
                    ? location.pathname === '/'
                    : location.pathname.startsWith(item.href);

                return (
                  <Link
                    key={item.id}
                    to={item.href}
                    className={clsx(
                      'flex items-center px-3 py-2 rounded-md text-sm font-medium transition-colors',
                      isActive
                        ? 'bg-primary-50 text-primary-700 dark:bg-primary-900/40 dark:text-primary-100'
                        : 'text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700'
                    )}
                  >
                    <item.icon className="w-5 h-5 mr-3" />
                    {item.name}
                  </Link>
                );
              })}
            </div>

            {/* Admin Section */}
            <div className="mt-8">
              <p className="px-3 text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider">
                Administration
              </p>
              <div className="mt-2 space-y-1">
                {adminItems.map((item) => {
                  const isActive = location.pathname.startsWith(item.href);

                  return (
                    <Link
                      key={item.id}
                      to={item.href}
                      className={clsx(
                        'flex items-center px-3 py-2 rounded-md text-sm font-medium transition-colors',
                        isActive
                          ? 'bg-primary-50 text-primary-700 dark:bg-primary-900/40 dark:text-primary-100'
                          : 'text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700'
                      )}
                    >
                      <item.icon className="w-5 h-5 mr-3" />
                      {item.name}
                    </Link>
                  );
                })}
              </div>
            </div>
          </nav>

          {/* User Menu */}
          <div className="p-4 border-t border-gray-200" ref={userMenuRef}>
            <div className="relative">
              <button
                onClick={() => setUserMenuOpen(!userMenuOpen)}
                className="w-full flex items-center justify-between px-3 py-2 rounded-md text-sm font-medium text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700 transition-colors"
              >
                <div className="flex items-center min-w-0">
                  <div className="w-8 h-8 rounded-full bg-primary-100 flex items-center justify-center flex-shrink-0">
                    <User className="w-4 h-4 text-primary-600" />
                  </div>
                  <span className="ml-3 truncate">{user?.username || 'User'}</span>
                </div>
                <ChevronDown
                  className={clsx(
                    'w-4 h-4 text-gray-400 transition-transform',
                    userMenuOpen && 'rotate-180'
                  )}
                />
              </button>

              {/* Dropdown Menu */}
              {userMenuOpen && (
                <div className="absolute bottom-full left-0 right-0 mb-1 bg-white dark:bg-gray-800 rounded-md shadow-lg border border-gray-200 dark:border-gray-700 py-1">
                  {userMenuItems.map((item) => (
                    <Link
                      key={item.id}
                      to={item.href}
                      onClick={() => setUserMenuOpen(false)}
                      className="flex items-center px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700"
                    >
                      <item.icon className="w-4 h-4 mr-3" />
                      {item.name}
                    </Link>
                  ))}
                  <button
                    onClick={handleLogout}
                    className="w-full flex items-center px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700"
                  >
                    <LogOut className="w-4 h-4 mr-3" />
                    Logout
                  </button>
                </div>
              )}
            </div>
            <p className="mt-3 text-xs text-gray-500 dark:text-gray-400">
              OpenVox WebUI v{__APP_VERSION__}
            </p>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <main className="pl-64">
        {/* Header */}
        <header className="h-16 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 flex items-center justify-end px-8">
          <PluginSlot
            slot="toolbar.global"
            params={{ route: location.pathname }}
            variant="bare"
            className="flex items-center gap-2 mr-4"
          />
          <NotificationBell />
        </header>

        {/* Content */}
        <div className="p-8">{children}</div>
        <PluginSlot slot="footer.global" variant="bare" className="px-8 pb-4" />
      </main>
      <PluginToasts />
    </div>
  );
}
