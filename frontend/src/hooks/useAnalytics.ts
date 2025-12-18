import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../services/api';
import type {
  CreateSavedReportRequest,
  UpdateSavedReportRequest,
  ExecuteReportRequest,
  CreateScheduleRequest,
  UpdateScheduleRequest,
  CreateComplianceBaselineRequest,
  CreateDriftBaselineRequest,
  GenerateReportRequest,
  ReportType,
  ReportQueryConfig,
} from '../types';

// Query keys
export const analyticsKeys = {
  all: ['analytics'] as const,
  savedReports: () => [...analyticsKeys.all, 'saved-reports'] as const,
  savedReport: (id: string) => [...analyticsKeys.savedReports(), id] as const,
  savedReportsByType: (type: ReportType) => [...analyticsKeys.savedReports(), 'type', type] as const,
  templates: () => [...analyticsKeys.all, 'templates'] as const,
  template: (id: string) => [...analyticsKeys.templates(), id] as const,
  schedules: () => [...analyticsKeys.all, 'schedules'] as const,
  schedule: (id: string) => [...analyticsKeys.schedules(), id] as const,
  executions: (reportId: string) => [...analyticsKeys.all, 'executions', reportId] as const,
  complianceBaselines: () => [...analyticsKeys.all, 'compliance-baselines'] as const,
  complianceBaseline: (id: string) => [...analyticsKeys.complianceBaselines(), id] as const,
  driftBaselines: () => [...analyticsKeys.all, 'drift-baselines'] as const,
  driftBaseline: (id: string) => [...analyticsKeys.driftBaselines(), id] as const,
};

// Saved Reports
export function useSavedReports(reportType?: ReportType) {
  return useQuery({
    queryKey: reportType ? analyticsKeys.savedReportsByType(reportType) : analyticsKeys.savedReports(),
    queryFn: () => api.getSavedReports(reportType),
  });
}

export function useSavedReport(id: string) {
  return useQuery({
    queryKey: analyticsKeys.savedReport(id),
    queryFn: () => api.getSavedReport(id),
    enabled: !!id,
  });
}

export function useCreateSavedReport() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateSavedReportRequest) => api.createSavedReport(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.savedReports() });
    },
  });
}

export function useUpdateSavedReport() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: UpdateSavedReportRequest }) =>
      api.updateSavedReport(id, request),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.savedReport(id) });
      queryClient.invalidateQueries({ queryKey: analyticsKeys.savedReports() });
    },
  });
}

export function useDeleteSavedReport() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => api.deleteSavedReport(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.savedReports() });
    },
  });
}

export function useExecuteReport() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, request }: { id: string; request?: ExecuteReportRequest }) =>
      api.executeReport(id, request),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.executions(id) });
    },
  });
}

export function useReportExecutions(reportId: string, limit?: number) {
  return useQuery({
    queryKey: analyticsKeys.executions(reportId),
    queryFn: () => api.getReportExecutions(reportId, limit),
    enabled: !!reportId,
  });
}

// Report Templates
export function useReportTemplates(reportType?: ReportType) {
  return useQuery({
    queryKey: analyticsKeys.templates(),
    queryFn: () => api.getReportTemplates(reportType),
  });
}

export function useReportTemplate(id: string) {
  return useQuery({
    queryKey: analyticsKeys.template(id),
    queryFn: () => api.getReportTemplate(id),
    enabled: !!id,
  });
}

// Schedules
export function useSchedules() {
  return useQuery({
    queryKey: analyticsKeys.schedules(),
    queryFn: api.getSchedules,
  });
}

export function useSchedule(id: string) {
  return useQuery({
    queryKey: analyticsKeys.schedule(id),
    queryFn: () => api.getSchedule(id),
    enabled: !!id,
  });
}

export function useCreateSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateScheduleRequest) => api.createSchedule(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.schedules() });
    },
  });
}

export function useUpdateSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: UpdateScheduleRequest }) =>
      api.updateSchedule(id, request),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.schedule(id) });
      queryClient.invalidateQueries({ queryKey: analyticsKeys.schedules() });
    },
  });
}

export function useDeleteSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => api.deleteSchedule(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.schedules() });
    },
  });
}

// Generate Reports (on-demand)
export function useGenerateReport() {
  return useMutation({
    mutationFn: (request: GenerateReportRequest) => api.generateReport(request),
  });
}

export function useGenerateReportByType() {
  return useMutation({
    mutationFn: ({ reportType, config }: { reportType: ReportType; config?: ReportQueryConfig }) =>
      api.generateReportByType(reportType, config),
  });
}

// Compliance Baselines
export function useComplianceBaselines() {
  return useQuery({
    queryKey: analyticsKeys.complianceBaselines(),
    queryFn: api.getComplianceBaselines,
  });
}

export function useComplianceBaseline(id: string) {
  return useQuery({
    queryKey: analyticsKeys.complianceBaseline(id),
    queryFn: () => api.getComplianceBaseline(id),
    enabled: !!id,
  });
}

export function useCreateComplianceBaseline() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateComplianceBaselineRequest) => api.createComplianceBaseline(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.complianceBaselines() });
    },
  });
}

export function useDeleteComplianceBaseline() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => api.deleteComplianceBaseline(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.complianceBaselines() });
    },
  });
}

// Drift Baselines
export function useDriftBaselines() {
  return useQuery({
    queryKey: analyticsKeys.driftBaselines(),
    queryFn: api.getDriftBaselines,
  });
}

export function useDriftBaseline(id: string) {
  return useQuery({
    queryKey: analyticsKeys.driftBaseline(id),
    queryFn: () => api.getDriftBaseline(id),
    enabled: !!id,
  });
}

export function useCreateDriftBaseline() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateDriftBaselineRequest) => api.createDriftBaseline(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.driftBaselines() });
    },
  });
}

export function useDeleteDriftBaseline() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => api.deleteDriftBaseline(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: analyticsKeys.driftBaselines() });
    },
  });
}

// Export
export function useExportExecution() {
  return useMutation({
    mutationFn: ({ id, format }: { id: string; format?: string }) => api.exportExecution(id, format),
  });
}
