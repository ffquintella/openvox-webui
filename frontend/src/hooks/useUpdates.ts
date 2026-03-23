import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api, cveApi } from '../services/api';
import type {
  CreateUpdateJobRequest,
  UpdatePreviewRequest,
  UpdatePreviewResponse,
  ApproveUpdateJobRequest,
  UpdateJob,
  InventoryDashboardReport,
  InventoryFleetStatusSummary,
  RepositoryVersionCatalogEntry,
  OutdatedSoftwareNodeDetail,
  ComplianceCategoryNode,
} from '../types';

export function useUpdateJobs(limit?: number) {
  return useQuery<UpdateJob[]>({
    queryKey: ['update-jobs', limit],
    queryFn: () => api.getUpdateJobs(),
  });
}

export function useUpdateJob(jobId: string | undefined) {
  return useQuery<UpdateJob>({
    queryKey: ['update-job', jobId],
    queryFn: () => api.getUpdateJob(jobId!),
    enabled: !!jobId,
  });
}

export function useInventoryDashboard() {
  return useQuery<InventoryDashboardReport>({
    queryKey: ['inventory-dashboard'],
    queryFn: api.getInventoryDashboard,
  });
}

export function useInventorySummary() {
  return useQuery<InventoryFleetStatusSummary>({
    queryKey: ['inventory-summary'],
    queryFn: api.getInventorySummary,
  });
}

export function useInventoryCatalog() {
  return useQuery<RepositoryVersionCatalogEntry[]>({
    queryKey: ['inventory-catalog'],
    queryFn: api.getInventoryCatalog,
  });
}

export function useCreateUpdateJob() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: CreateUpdateJobRequest) => api.createUpdateJob(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['update-jobs'] });
    },
  });
}

export function useApproveUpdateJob() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ jobId, request }: { jobId: string; request: ApproveUpdateJobRequest }) =>
      api.approveUpdateJob(jobId, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['update-jobs'] });
      queryClient.invalidateQueries({ queryKey: ['update-job'] });
    },
  });
}

export function usePreviewUpdateJob() {
  return useMutation<UpdatePreviewResponse, Error, UpdatePreviewRequest>({
    mutationFn: (request: UpdatePreviewRequest) => cveApi.previewUpdateJob(request),
  });
}

export function useOutdatedSoftwareNodes(
  name: string | null,
  softwareType?: string
) {
  return useQuery<OutdatedSoftwareNodeDetail[]>({
    queryKey: ['outdated-software-nodes', name, softwareType],
    queryFn: () => api.getOutdatedSoftwareNodes(name!, softwareType),
    enabled: !!name,
  });
}

export function useComplianceCategoryNodes(category: string | null) {
  return useQuery<ComplianceCategoryNode[]>({
    queryKey: ['compliance-category-nodes', category],
    queryFn: () => api.getComplianceCategoryNodes(category!),
    enabled: !!category,
  });
}
