import { ReactNode } from 'react';
import { Link, useLocation } from 'react-router-dom';
import {
  LayoutDashboard,
  Server,
  FolderTree,
  FileText,
  Database,
  Settings,
  Shield,
  Users,
  Lock,
} from 'lucide-react';
import clsx from 'clsx';

interface LayoutProps {
  children: ReactNode;
}

const navigation = [
  { name: 'Dashboard', href: '/', icon: LayoutDashboard },
  { name: 'Nodes', href: '/nodes', icon: Server },
  { name: 'Groups', href: '/groups', icon: FolderTree },
  { name: 'Reports', href: '/reports', icon: FileText },
  { name: 'Facts', href: '/facts', icon: Database },
];

const adminNavigation = [
  { name: 'Users', href: '/users', icon: Users },
  { name: 'Roles', href: '/roles', icon: Shield },
  { name: 'Permissions', href: '/permissions', icon: Lock },
  { name: 'Settings', href: '/settings', icon: Settings },
];

export default function Layout({ children }: LayoutProps) {
  const location = useLocation();

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Sidebar */}
      <aside className="fixed inset-y-0 left-0 w-64 bg-white border-r border-gray-200">
        <div className="flex flex-col h-full">
          {/* Logo */}
          <div className="flex items-center h-16 px-6 border-b border-gray-200">
            <span className="text-xl font-bold text-primary-600">OpenVox</span>
            <span className="ml-1 text-xl font-light text-gray-600">WebUI</span>
          </div>

          {/* Navigation */}
          <nav className="flex-1 px-4 py-4 overflow-y-auto">
            <div className="space-y-1">
              {navigation.map((item) => {
                const isActive =
                  item.href === '/'
                    ? location.pathname === '/'
                    : location.pathname.startsWith(item.href);

                return (
                  <Link
                    key={item.name}
                    to={item.href}
                    className={clsx(
                      'flex items-center px-3 py-2 rounded-md text-sm font-medium transition-colors',
                      isActive
                        ? 'bg-primary-50 text-primary-700'
                        : 'text-gray-700 hover:bg-gray-100'
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
              <p className="px-3 text-xs font-semibold text-gray-400 uppercase tracking-wider">
                Administration
              </p>
              <div className="mt-2 space-y-1">
                {adminNavigation.map((item) => {
                  const isActive = location.pathname.startsWith(item.href);

                  return (
                    <Link
                      key={item.name}
                      to={item.href}
                      className={clsx(
                        'flex items-center px-3 py-2 rounded-md text-sm font-medium transition-colors',
                        isActive
                          ? 'bg-primary-50 text-primary-700'
                          : 'text-gray-700 hover:bg-gray-100'
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

          {/* Footer */}
          <div className="p-4 border-t border-gray-200">
            <p className="text-xs text-gray-500">OpenVox WebUI v0.1.0</p>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <main className="pl-64">
        <div className="p-8">{children}</div>
      </main>
    </div>
  );
}
