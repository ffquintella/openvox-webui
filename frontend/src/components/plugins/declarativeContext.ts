import { createContext, useContext } from 'react';
import type { UiRouteAccess } from '../../types';

/**
 * What a declarative spec may reach (H-6): its plugin's own routes, as the
 * server listed them for this user, and the slot context.
 */
export interface DeclarativeEnv {
  pluginId: string;
  routes: UiRouteAccess[];
  params: Record<string, string>;
}

export const DeclarativeContext = createContext<DeclarativeEnv | null>(null);

export function useDeclarative(): DeclarativeEnv | null {
  return useContext(DeclarativeContext);
}
