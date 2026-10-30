import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { pluginsApi } from '../services/api';
import type {
  PluginInstallOptions,
  AddTrustedKeyRequest,
  UpdateTrustedKeyRequest,
  RevokeTrustedKeyRequest,
  PluginUpdatePolicyRequest,
  PluginFactSettingsRequest,
  CreateTemplateInstanceRequest,
  TrustPluginModulesRequest,
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
    queryClient.invalidateQueries({ queryKey: ['plugin-versions'] });
    queryClient.invalidateQueries({ queryKey: ['plugin-updates'] });
    queryClient.invalidateQueries({ queryKey: ['plugin-governance'] });
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

/** Session for a trusted module page (M31); kept for the life of the page. */
export function usePluginModuleSession(pluginId: string, contributionId: string) {
  return useQuery({
    queryKey: ['plugin-ui', 'module', pluginId, contributionId],
    queryFn: () => pluginsApi.createModuleSession(pluginId, contributionId),
    staleTime: Infinity,
    gcTime: 30 * 60 * 1000,
    retry: false,
    refetchOnWindowFocus: false,
  });
}

export function useTrustPluginModules() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: TrustPluginModulesRequest }) =>
      pluginsApi.trustModules(id, request),
    onSuccess: invalidate,
  });
}

export function useUntrustPluginModules() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (id: string) => pluginsApi.untrustModules(id),
    onSuccess: invalidate,
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

// ============================================================================
// Updates, rollback and revocation (M24)
// ============================================================================

export function usePluginVersions(id: string | null) {
  return useQuery({
    queryKey: ['plugin-versions', id],
    queryFn: () => pluginsApi.versions(id as string),
    enabled: !!id,
  });
}

export function usePluginUpdates(enabled = true) {
  return useQuery({
    queryKey: ['plugin-updates'],
    queryFn: () => pluginsApi.updates(),
    enabled,
  });
}

export function useApplyPluginUpdates() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: (pluginIds: string[]) => pluginsApi.applyUpdates(pluginIds),
    onSuccess: invalidate,
  });
}

export function useUpdatePlugin() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ id, version }: { id: string; version?: string }) =>
      pluginsApi.updatePlugin(id, version),
    onSettled: invalidate,
  });
}

export function useRollbackPlugin() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ id, version }: { id: string; version?: string }) =>
      pluginsApi.rollback(id, version),
    onSettled: invalidate,
  });
}

export function useSetPluginUpdatePolicy() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: PluginUpdatePolicyRequest }) =>
      pluginsApi.setUpdatePolicy(id, request),
    onSuccess: invalidate,
  });
}

export function useRevokePluginVersion() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ id, version, reason }: { id: string; version: string; reason: string }) =>
      pluginsApi.revokeVersion(id, version, reason),
    onSuccess: invalidate,
  });
}

export function useUnrevokePluginVersion() {
  const invalidate = useInvalidatePlugins();
  return useMutation({
    mutationFn: ({ id, version }: { id: string; version: string }) =>
      pluginsApi.unrevokeVersion(id, version),
    onSuccess: invalidate,
  });
}

// ============================================================================
// Plugin facts (M25)
// ============================================================================

const FACT_SOURCES_STALE_MS = 5 * 60_000;

export function usePluginFactCatalogs(enabled = true) {
  return useQuery({
    queryKey: ['plugin-facts'],
    queryFn: () => pluginsApi.getPluginFactCatalogs(),
    enabled,
  });
}

// ============================================================================
// Classification extensions (M27)
// ============================================================================

export function usePluginClassification(enabled = true) {
  return useQuery({
    queryKey: ['plugin-classification'],
    queryFn: () => pluginsApi.getPluginClassification(),
    enabled,
    staleTime: 10_000,
  });
}

function useClassificationMutation<TVars, TResult>(fn: (vars: TVars) => Promise<TResult>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['plugin-classification'] });
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['held-membership'] });
      queryClient.invalidateQueries({ queryKey: ['group-members'] });
    },
  });
}

export function useCreateTemplateInstance() {
  return useClassificationMutation(
    ({
      pluginId,
      templateId,
      request,
    }: {
      pluginId: string;
      templateId: string;
      request: CreateTemplateInstanceRequest;
    }) => pluginsApi.createTemplateInstance(pluginId, templateId, request)
  );
}

export function useApproveTemplateInstance() {
  return useClassificationMutation((id: string) => pluginsApi.approveTemplateInstance(id));
}

export function useRunTemplateInstance() {
  return useClassificationMutation((id: string) => pluginsApi.runTemplateInstance(id));
}

export function useSetTemplateInstanceEnabled() {
  return useClassificationMutation(({ id, enabled }: { id: string; enabled: boolean }) =>
    pluginsApi.setTemplateInstanceEnabled(id, enabled)
  );
}

export function useDeleteTemplateInstance() {
  return useClassificationMutation((id: string) => pluginsApi.deleteTemplateInstance(id));
}

export function useRefreshGroupMembership() {
  return useClassificationMutation((groupId: string) => pluginsApi.refreshGroupMembership(groupId));
}

export function useApproveHeldMembership() {
  return useClassificationMutation((groupId: string) => pluginsApi.approveHeldMembership(groupId));
}

export function useRejectHeldMembership() {
  return useClassificationMutation((groupId: string) => pluginsApi.rejectHeldMembership(groupId));
}

export function useHeldMembership(groupId: string | null) {
  return useQuery({
    queryKey: ['held-membership', groupId],
    queryFn: () => pluginsApi.getHeldMembership(groupId as string),
    enabled: !!groupId,
  });
}

export function useGroupMembers(groupId: string | null) {
  return useQuery({
    queryKey: ['group-members', groupId],
    queryFn: () => pluginsApi.getGroupMembers(groupId as string),
    enabled: !!groupId,
  });
}

export function usePluginFacts(id: string | null) {
  return useQuery({
    queryKey: ['plugin-facts', id],
    queryFn: () => pluginsApi.getPluginFacts(id as string),
    enabled: !!id,
  });
}

export function useUpdatePluginFactSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      name,
      request,
    }: {
      id: string;
      name: string;
      request: PluginFactSettingsRequest;
    }) => pluginsApi.updatePluginFactSettings(id, name, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['plugin-facts'] });
      queryClient.invalidateQueries({ queryKey: ['plugin-fact-preview'] });
      queryClient.invalidateQueries({ queryKey: ['plugin-fact-sources'] });
    },
  });
}

export function usePluginFactPreview(id: string, name: string, enabled = true) {
  return useQuery({
    queryKey: ['plugin-fact-preview', id, name],
    queryFn: () => pluginsApi.getPluginFactPreview(id, name),
    enabled,
    // The dialog must show the values as they are right now.
    gcTime: 0,
    staleTime: 0,
  });
}

/**
 * Fact path entries contributed by plugins, used to badge plugin facts on other
 * pages. It must never break them: errors (403 for viewers, 503) just leave it empty.
 */
export function usePluginFactSources() {
  return useQuery({
    queryKey: ['plugin-fact-sources'],
    queryFn: async () => {
      try {
        return await pluginsApi.getPluginFactSources();
      } catch {
        return [];
      }
    },
    staleTime: FACT_SOURCES_STALE_MS,
    retry: false,
  });
}
