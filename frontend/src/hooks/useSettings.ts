import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../services/api';
import type { DashboardConfig, UpdateSmtpSettingsRequest } from '../types';

export function useSettings() {
  return useQuery({
    queryKey: ['settings'],
    queryFn: api.getSettings,
  });
}

export function useDashboardConfig() {
  return useQuery({
    queryKey: ['settings', 'dashboard'],
    queryFn: api.getDashboardConfig,
  });
}

export function useUpdateDashboardConfig() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (config: Partial<DashboardConfig>) => api.updateDashboardConfig(config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] });
    },
  });
}

export function useRbacConfig() {
  return useQuery({
    queryKey: ['settings', 'rbac'],
    queryFn: api.getRbacConfig,
  });
}

export function useExportConfig() {
  return useQuery({
    queryKey: ['settings', 'export'],
    queryFn: api.exportConfig,
    enabled: false, // Manual trigger only
  });
}

export function useImportConfig() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ content, dryRun }: { content: string; dryRun: boolean }) =>
      api.importConfig(content, dryRun),
    onSuccess: (_, { dryRun }) => {
      if (!dryRun) {
        queryClient.invalidateQueries({ queryKey: ['settings'] });
      }
    },
  });
}

export function useValidateConfig() {
  return useMutation({
    mutationFn: (content: string) => api.validateConfig(content),
  });
}

export function useConfigHistory() {
  return useQuery({
    queryKey: ['settings', 'history'],
    queryFn: api.getConfigHistory,
  });
}

export function useServerInfo() {
  return useQuery({
    queryKey: ['settings', 'server'],
    queryFn: api.getServerInfo,
  });
}

export function useSmtpSettings() {
  return useQuery({
    queryKey: ['settings', 'smtp'],
    queryFn: api.getSmtpSettings,
  });
}

export function useUpdateSmtpSettings() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (config: UpdateSmtpSettingsRequest) => api.updateSmtpSettings(config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings', 'smtp'] });
    },
  });
}
