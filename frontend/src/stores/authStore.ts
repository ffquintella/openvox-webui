import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { User } from '../types';
import { usePermissionsStore } from './permissionsStore';

export const SESSION_IDLE_TIMEOUT_MS = 30 * 60 * 1000;

const AUTH_LOGOUT_REASON_KEY = 'auth_logout_reason';
const LAST_ACTIVITY_STORAGE_KEY = 'auth_last_activity_at';

function safeLocalStorageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeLocalStorageSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore storage failures to prevent UI lockups in restricted browser contexts.
  }
}

function safeLocalStorageRemove(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // Ignore storage failures to prevent UI lockups in restricted browser contexts.
  }
}

function safeSessionStorageGet(key: string): string | null {
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeSessionStorageSet(key: string, value: string): void {
  try {
    sessionStorage.setItem(key, value);
  } catch {
    // Ignore storage failures to prevent UI lockups in restricted browser contexts.
  }
}

function safeSessionStorageRemove(key: string): void {
  try {
    sessionStorage.removeItem(key);
  } catch {
    // Ignore storage failures to prevent UI lockups in restricted browser contexts.
  }
}

export function recordSessionActivity(timestamp = Date.now()) {
  safeLocalStorageSet(LAST_ACTIVITY_STORAGE_KEY, timestamp.toString());
}

export function getLastSessionActivity(): number {
  const rawValue = safeLocalStorageGet(LAST_ACTIVITY_STORAGE_KEY);
  const parsedValue = rawValue ? Number.parseInt(rawValue, 10) : NaN;
  return Number.isFinite(parsedValue) ? parsedValue : Date.now();
}

export function consumeSessionLogoutReason(): string | null {
  const reason = safeSessionStorageGet(AUTH_LOGOUT_REASON_KEY);
  if (reason) {
    safeSessionStorageRemove(AUTH_LOGOUT_REASON_KEY);
  }
  return reason;
}

interface AuthState {
  user: User | null;
  token: string | null;
  isAuthenticated: boolean;
  hasHydrated: boolean;
  login: (user: User, token: string, refreshToken?: string) => void;
  logout: (reason?: string) => void;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      user: null,
      token: typeof window !== 'undefined' ? safeLocalStorageGet('auth_token') : null,
      isAuthenticated: typeof window !== 'undefined' ? !!safeLocalStorageGet('auth_token') : false,
      hasHydrated: false,

      login: (user: User, token: string, refreshToken?: string) => {
        safeLocalStorageSet('auth_token', token);
        if (refreshToken !== undefined) {
          safeLocalStorageSet('refresh_token', refreshToken);
        }
        safeSessionStorageRemove(AUTH_LOGOUT_REASON_KEY);
        recordSessionActivity();
        set({ user, token, isAuthenticated: true });
      },

      logout: (reason?: string) => {
        safeLocalStorageRemove('auth_token');
        safeLocalStorageRemove('refresh_token');
        safeLocalStorageRemove(LAST_ACTIVITY_STORAGE_KEY);
        if (reason) {
          safeSessionStorageSet(AUTH_LOGOUT_REASON_KEY, reason);
        }
        // Clear permissions when logging out
        usePermissionsStore.getState().clearPermissions();
        set({ user: null, token: null, isAuthenticated: false });
      },
    }),
    {
      name: 'auth-storage',
      partialize: (state) => ({
        user: state.user,
        token: state.token,
        isAuthenticated: state.isAuthenticated,
      }),
      onRehydrateStorage: () => (state, _error) => {
        const storedToken = typeof window !== 'undefined' ? safeLocalStorageGet('auth_token') : null;
        if (state) {
          state.hasHydrated = true;
          state.token = state.token ?? storedToken;
          state.isAuthenticated = !!(state.token ?? storedToken);
        }
      },
    }
  )
);
