import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { pluginsApi } from '../services/api';
import type {
  PluginInstallOptions,
  AddTrustedKeyRequest,
  UpdateTrustedKeyRequest,
  RevokeTrustedKeyRequest,
} from '../types';

const UI_SLOTS_STALE_MS = 30_000;

// ============================================================================
// Plugins
// ============================================================================

export function usePlugins() {
  return useQuery({
    queryKey: ['plugins'],
    queryFn: () => pluginsApi.list(),
  });
}

export function usePlugin(id: string | null) {
  return useQuery({
    queryKey: ['plugin', id],
    queryFn: () => pluginsApi.get(id as string),
    enabled: !!id,
  });
}

export function usePreflightPlugin() {
  return useMutation({
    mutationFn: (pkg: Blob | ArrayBuffer) => pluginsApi.preflight(pkg),
  });
}

function useInvalidatePlugins() {
  const queryClient = useQueryClient();
  return () => {
    queryClient.invalidateQueries({ queryKey: ['plugins'] });
    queryClient.invalidateQueries({ queryKey: ['plugin'] });
    queryClient.invalidateQueries({ queryKey: ['plugin-ui'] });
    queryClient.invalidateQueries({ queryKey: ['plugin-trusted-keys'] });
  };
}

export function useInstallPlugin() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ pkg, options }: { pkg: Blob | ArrayBuffer; options?: PluginInstallOptions }) =>
      pluginsApi.install(pkg, options),
    onSuccess: invalidate,
  });
}

export function useEnablePlugin() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (id: string) => pluginsApi.enable(id),
    onSuccess: invalidate,
  });
}

export function useDisablePlugin() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (id: string) => pluginsApi.disable(id),
    onSuccess: invalidate,
  });
}

export function useVerifyPlugin() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (id: string) => pluginsApi.verify(id),
    onSuccess: invalidate,
  });
}

export function useUninstallPlugin() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (id: string) => pluginsApi.uninstall(id),
    onSuccess: invalidate,
  });
}

// ============================================================================
// Trusted keys
// ============================================================================

export function useTrustedKeys(enabled = true) {
  return useQuery({
    queryKey: ['plugin-trusted-keys'],
    queryFn: () => pluginsApi.listKeys(),
    enabled,
  });
}

export function useAddTrustedKey() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (request: AddTrustedKeyRequest) => pluginsApi.addKey(request),
    onSuccess: invalidate,
  });
}

export function useUpdateTrustedKey() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ keyId, request }: { keyId: string; request: UpdateTrustedKeyRequest }) =>
      pluginsApi.updateKey(keyId, request),
    onSuccess: invalidate,
  });
}

export function useRevokeTrustedKey() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ keyId, request }: { keyId: string; request: RevokeTrustedKeyRequest }) =>
      pluginsApi.revokeKey(keyId, request),
    onSuccess: invalidate,
  });
}

export function useRemoveTrustedKey() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (keyId: string) => pluginsApi.removeKey(keyId),
    onSuccess: invalidate,
  });
}

// ============================================================================
// UI slot registry
// ============================================================================

export function usePluginSlots() {
  return useQuery({
    queryKey: ['plugin-ui', 'slots'],
    queryFn: () => pluginsApi.getUiSlots(),
    staleTime: UI_SLOTS_STALE_MS,
    retry: false,
  });
}

/** Contributions of one slot, already filtered by the server for this user. */
export function usePluginSlot(slot: string) {
  const { data } = usePluginSlots();
  return data?.slots[slot] ?? [];
}

export function usePluginCatalog() {
  return useQuery({
    queryKey: ['plugin-ui', 'catalog'],
    queryFn: () => pluginsApi.getUiCatalog(),
    staleTime: Infinity,
    retry: false,
  });
}

/** Asset session for a sandboxed iframe. Kept for the life of the page so the frame is not reloaded. */
export function usePluginUiSession(pluginId: string, contributionId: string) {
  return useQuery({
    queryKey: ['plugin-ui', 'session', pluginId, contributionId],
    queryFn: () => pluginsApi.createUiSession(pluginId, contributionId),
    staleTime: Infinity,
    gcTime: 30 * 60 * 1000,
    retry: false,
    refetchOnWindowFocus: false,
  });
}

export function usePluginContribution(
  pluginId: string | undefined,
  contributionId: string | undefined
) {
  return useQuery({
    queryKey: ['plugin-ui', 'contribution', pluginId, contributionId],
    queryFn: () => pluginsApi.getContribution(pluginId as string, contributionId as string),
    enabled: !!pluginId && !!contributionId,
    staleTime: UI_SLOTS_STALE_MS,
  });
}

// ============================================================================
// Governance (M22)
// ============================================================================

const GOVERNANCE_REFRESH_MS = 30_000;

export function usePluginGovernance(enabled = true) {
  return useQuery({
    queryKey: ['plugin-governance'],
    queryFn: () => pluginsApi.governance(),
    enabled,
    refetchInterval: GOVERNANCE_REFRESH_MS,
  });
}

export function usePluginMetrics(id: string | null, hours = 24) {
  return useQuery({
    queryKey: ['plugin-metrics', id, hours],
    queryFn: () => pluginsApi.metrics(id as string, hours),
    enabled: !!id,
  });
}

export function useResetPluginCircuit() {
  const invalidate = useInvalidatePlugins();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => pluginsApi.resetCircuit(id),
    onSuccess: () => {
      invalidate();
      queryClient.invalidateQueries({ queryKey: ['plugin-governance'] });
      queryClient.invalidateQueries({ queryKey: ['plugin-metrics'] });
    },
  });
}
