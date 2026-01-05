import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../services/api';
import type {
  CreateSshKeyRequest,
  CreateRepositoryRequest,
  UpdateRepositoryRequest,
  UpdateEnvironmentRequest,
  TriggerDeploymentRequest,
  ApproveDeploymentRequest,
  RejectDeploymentRequest,
  ListDeploymentsQuery,
  ListEnvironmentsQuery,
} from '../types';

// ============================================================================
// SSH Keys
// ============================================================================

export function useSshKeys() {
  return useQuery({
    queryKey: ['code-ssh-keys'],
    queryFn: () => api.getSshKeys(),
  });
}

export function useCreateSshKey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: CreateSshKeyRequest) => api.createSshKey(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['code-ssh-keys'] });
    },
  });
}

export function useDeleteSshKey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteSshKey(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['code-ssh-keys'] });
    },
  });
}

// ============================================================================
// Repositories
// ============================================================================

export function useCodeRepositories() {
  return useQuery({
    queryKey: ['code-repositories'],
    queryFn: () => api.getCodeRepositories(),
  });
}

export function useCodeRepository(id: string) {
  return useQuery({
    queryKey: ['code-repository', id],
    queryFn: () => api.getCodeRepository(id),
    enabled: !!id,
  });
}

export function useCreateCodeRepository() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: CreateRepositoryRequest) => api.createCodeRepository(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['code-repositories'] });
    },
  });
}

export function useUpdateCodeRepository() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: UpdateRepositoryRequest }) =>
      api.updateCodeRepository(id, request),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['code-repositories'] });
      queryClient.invalidateQueries({ queryKey: ['code-repository', variables.id] });
    },
  });
}

export function useDeleteCodeRepository() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteCodeRepository(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['code-repositories'] });
      queryClient.invalidateQueries({ queryKey: ['code-environments'] });
    },
  });
}

export function useSyncCodeRepository() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.syncCodeRepository(id),
    onSuccess: (_, id) => {
      queryClient.invalidateQueries({ queryKey: ['code-repositories'] });
      queryClient.invalidateQueries({ queryKey: ['code-repository', id] });
      queryClient.invalidateQueries({ queryKey: ['code-environments'] });
      queryClient.invalidateQueries({ queryKey: ['code-deployments'] });
    },
  });
}

// ============================================================================
// Environments
// ============================================================================

export function useCodeEnvironments(query?: ListEnvironmentsQuery) {
  return useQuery({
    queryKey: ['code-environments', query],
    queryFn: () => api.getCodeEnvironments(query),
  });
}

export function useCodeEnvironment(id: string) {
  return useQuery({
    queryKey: ['code-environment', id],
    queryFn: () => api.getCodeEnvironment(id),
    enabled: !!id,
  });
}

export function useUpdateCodeEnvironment() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: UpdateEnvironmentRequest }) =>
      api.updateCodeEnvironment(id, request),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['code-environments'] });
      queryClient.invalidateQueries({ queryKey: ['code-environment', variables.id] });
    },
  });
}

// ============================================================================
// Deployments
// ============================================================================

export function useCodeDeployments(query?: ListDeploymentsQuery) {
  return useQuery({
    queryKey: ['code-deployments', query],
    queryFn: () => api.getCodeDeployments(query),
    refetchInterval: 10000, // Auto-refresh every 10 seconds
  });
}

export function useCodeDeployment(id: string) {
  return useQuery({
    queryKey: ['code-deployment', id],
    queryFn: () => api.getCodeDeployment(id),
    enabled: !!id,
    refetchInterval: 5000, // Auto-refresh every 5 seconds for active deployments
  });
}

export function useTriggerDeployment() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: TriggerDeploymentRequest) => api.triggerDeployment(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['code-deployments'] });
      queryClient.invalidateQueries({ queryKey: ['code-environments'] });
    },
  });
}

export function useApproveDeployment() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request?: ApproveDeploymentRequest }) =>
      api.approveDeployment(id, request),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['code-deployments'] });
      queryClient.invalidateQueries({ queryKey: ['code-deployment', variables.id] });
      queryClient.invalidateQueries({ queryKey: ['code-environments'] });
    },
  });
}

export function useRejectDeployment() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: RejectDeploymentRequest }) =>
      api.rejectDeployment(id, request),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['code-deployments'] });
      queryClient.invalidateQueries({ queryKey: ['code-deployment', variables.id] });
      queryClient.invalidateQueries({ queryKey: ['code-environments'] });
    },
  });
}

export function useRetryDeployment() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.retryDeployment(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['code-deployments'] });
      queryClient.invalidateQueries({ queryKey: ['code-environments'] });
    },
  });
}
