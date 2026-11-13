import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from '../services/api';
import { hasActiveJob, isTerminalStatus } from '../utils/orchestration';
import type {
  BoltAllowlist,
  BoltAllowlistEntry,
  BoltDiscoveredItem,
  BoltJob,
  BoltJobDetail,
  BoltJobsQuery,
  BoltRunnable,
  BoltStatus,
  CreateBoltJobRequest,
} from '../types';

/** Polling period while any job can still change. */
export const BOLT_POLL_MS = 3000;

export function useBoltStatus() {
  return useQuery<BoltStatus>({
    queryKey: ['bolt-status'],
    queryFn: api.getBoltStatus,
  });
}

export function useBoltTasks(enabled: boolean) {
  return useQuery<BoltRunnable[]>({
    queryKey: ['bolt-tasks'],
    queryFn: api.getBoltTasks,
    enabled,
  });
}

export function useBoltPlans(enabled: boolean) {
  return useQuery<BoltRunnable[]>({
    queryKey: ['bolt-plans'],
    queryFn: api.getBoltPlans,
    enabled,
  });
}

export function useBoltDiscover(enabled: boolean) {
  return useQuery<BoltDiscoveredItem[]>({
    queryKey: ['bolt-discover'],
    queryFn: api.discoverBolt,
    enabled,
  });
}

export function useBoltAllowlist(enabled: boolean) {
  return useQuery<BoltAllowlist>({
    queryKey: ['bolt-allowlist'],
    queryFn: api.getBoltAllowlist,
    enabled,
  });
}

export function useBoltJobs(query: BoltJobsQuery, enabled = true) {
  return useQuery<BoltJob[]>({
    queryKey: ['bolt-jobs', query],
    queryFn: () => api.getBoltJobs(query),
    enabled,
    refetchInterval: (q) => (hasActiveJob(q.state.data) ? BOLT_POLL_MS : false),
  });
}

export function useBoltJob(id: string | undefined) {
  return useQuery<BoltJobDetail>({
    queryKey: ['bolt-job', id],
    queryFn: () => api.getBoltJob(id!),
    enabled: !!id,
    refetchInterval: (q) =>
      q.state.data && !isTerminalStatus(q.state.data.status) ? BOLT_POLL_MS : false,
  });
}

export function useNodeBoltJobs(certname: string | undefined, enabled = true) {
  return useQuery<BoltJob[]>({
    queryKey: ['bolt-node-jobs', certname],
    queryFn: () => api.getNodeBoltJobs(certname!, 10),
    enabled: !!certname && enabled,
    refetchInterval: (q) => (hasActiveJob(q.state.data) ? BOLT_POLL_MS : false),
  });
}

function useInvalidateJobs() {
  const queryClient = useQueryClient();
  return () => {
    queryClient.invalidateQueries({ queryKey: ['bolt-jobs'] });
    queryClient.invalidateQueries({ queryKey: ['bolt-job'] });
    queryClient.invalidateQueries({ queryKey: ['bolt-node-jobs'] });
  };
}

export function useCreateBoltJob() {
  const invalidate = useInvalidateJobs();
  return useMutation({
    mutationFn: (request: CreateBoltJobRequest) => api.createBoltJob(request),
    onSuccess: invalidate,
  });
}

export function useDecideBoltJob() {
  const invalidate = useInvalidateJobs();
  return useMutation({
    mutationFn: ({ id, approve, notes }: { id: string; approve: boolean; notes?: string }) =>
      approve ? api.approveBoltJob(id, notes) : api.rejectBoltJob(id, notes),
    onSuccess: invalidate,
  });
}

export function useCancelBoltJob() {
  const invalidate = useInvalidateJobs();
  return useMutation({
    mutationFn: (id: string) => api.cancelBoltJob(id),
    onSuccess: invalidate,
  });
}

export function useSaveBoltAllowlist() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (entries: Array<Pick<BoltAllowlistEntry, 'kind' | 'name' | 'requires_approval'>>) =>
      api.putBoltAllowlist(entries),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['bolt-allowlist'] });
      queryClient.invalidateQueries({ queryKey: ['bolt-discover'] });
      queryClient.invalidateQueries({ queryKey: ['bolt-tasks'] });
      queryClient.invalidateQueries({ queryKey: ['bolt-plans'] });
    },
  });
}
