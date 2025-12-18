import { Moon, Sun } from 'lucide-react';
import { useThemeStore } from '../stores/themeStore';

export default function ThemeToggle({ className = '' }: { className?: string }) {
  const theme = useThemeStore((s) => s.theme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);

  return (
    <button
      type="button"
      aria-label="Toggle theme"
      onClick={toggleTheme}
      className={`inline-flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium transition-colors bg-gray-200 text-gray-900 hover:bg-gray-300 dark:bg-gray-700 dark:text-gray-100 dark:hover:bg-gray-600 ${className}`}
    >
      {theme === 'dark' ? (
        <>
          <Moon className="w-4 h-4" />
          Dark
        </>
      ) : (
        <>
          <Sun className="w-4 h-4" />
          Light
        </>
      )}
    </button>
  );
}
