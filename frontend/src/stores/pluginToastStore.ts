import { create } from 'zustand';

export type PluginToastLevel = 'info' | 'success' | 'warning' | 'error';

export interface PluginToast {
  id: number;
  level: PluginToastLevel;
  text: string;
  source: string;
}

interface PluginToastState {
  toasts: PluginToast[];
  push: (t: Omit<PluginToast, 'id'>) => void;
  dismiss: (id: number) => void;
}

const MAX_TOASTS = 4;
let nextId = 1;

/** Notifications raised by plugin pages through the SDK; shown by the host. */
export const usePluginToastStore = create<PluginToastState>((set) => ({
  toasts: [],
  push: (t) =>
    set((s) => ({
      toasts: [...s.toasts, { ...t, id: nextId++ }].slice(-MAX_TOASTS),
    })),
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
