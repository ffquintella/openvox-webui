import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { cveApi } from '../services/api';
import type {
  CreateCveFeedSourceRequest,
  UpdateCveFeedSourceRequest,
  VulnerabilityDashboardReport,
  NodeVulnerabilitySummary,
  HostVulnerabilityMatch,
  CveEntry,
  CveDetailResponse,
  CveFeedSource,
} from '../types';

export function useVulnerabilityDashboard() {
  return useQuery<VulnerabilityDashboardReport>({
    queryKey: ['vulnerability-dashboard'],
    queryFn: cveApi.getVulnerabilityDashboard,
    retry: false,
  });
}

export function useVulnerableNodes(severity?: string, limit?: number) {
  return useQuery<NodeVulnerabilitySummary[]>({
    queryKey: ['vulnerable-nodes', severity, limit],
    queryFn: () => cveApi.getVulnerableNodes(severity, limit),
    retry: false,
  });
}

export function useNodeVulnerabilities(certname: string | undefined) {
  return useQuery<HostVulnerabilityMatch[]>({
    queryKey: ['node-vulnerabilities', certname],
    queryFn: () => cveApi.getNodeVulnerabilities(certname!),
    enabled: !!certname,
    retry: false,
  });
}

export function useCveSearch(query?: string, severity?: string, isKev?: boolean) {
  return useQuery<CveEntry[]>({
    queryKey: ['cve-search', query, severity, isKev],
    queryFn: () => cveApi.searchCves(query, severity, isKev),
    retry: false,
  });
}

export function useCveDetail(cveId: string | undefined) {
  return useQuery<CveDetailResponse>({
    queryKey: ['cve-detail', cveId],
    queryFn: () => cveApi.getCveDetail(cveId!),
    enabled: !!cveId,
    retry: false,
  });
}

export function useCveFeeds() {
  return useQuery<CveFeedSource[]>({
    queryKey: ['cve-feeds'],
    queryFn: cveApi.getCveFeeds,
    retry: false,
  });
}

export function useCreateCveFeed() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: CreateCveFeedSourceRequest) => cveApi.createCveFeed(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cve-feeds'] });
    },
  });
}

export function useUpdateCveFeed() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: UpdateCveFeedSourceRequest }) =>
      cveApi.updateCveFeed(id, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cve-feeds'] });
    },
  });
}

export function useDeleteCveFeed() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => cveApi.deleteCveFeed(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cve-feeds'] });
    },
  });
}

export function useTriggerFeedSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => cveApi.triggerFeedSync(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cve-feeds'] });
      queryClient.invalidateQueries({ queryKey: ['vulnerability-dashboard'] });
    },
  });
}

export function useTriggerMatchRefresh() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => cveApi.triggerMatchRefresh(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['vulnerability-dashboard'] });
      queryClient.invalidateQueries({ queryKey: ['vulnerable-nodes'] });
      queryClient.invalidateQueries({ queryKey: ['node-vulnerabilities'] });
    },
  });
}
