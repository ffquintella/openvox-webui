import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type Theme = 'light' | 'dark';

interface ThemeState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

const storageKey = 'ov-theme';

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
  root.setAttribute('data-theme', theme);
  try {
    localStorage.setItem(storageKey, theme);
  } catch {
    /* noop */
  }
}

function initialTheme(): Theme {
  try {
    const saved = localStorage.getItem(storageKey) as Theme | null;
    if (saved === 'dark' || saved === 'light') return saved;
  } catch {
    /* noop */
  }
  const preferDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
  return preferDark ? 'dark' : 'light';
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      theme: initialTheme(),
      setTheme: (theme: Theme) => {
        applyTheme(theme);
        set({ theme });
      },
      toggleTheme: () => {
        const next: Theme = get().theme === 'dark' ? 'light' : 'dark';
        applyTheme(next);
        set({ theme: next });
      },
    }),
    {
      name: 'theme-storage',
      partialize: (state) => ({ theme: state.theme }),
      onRehydrateStorage: () => (state) => {
        if (state) applyTheme(state.theme);
      },
    }
  )
);
