import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { backupApi } from '../services/api';
import type {
  CreateBackupRequest,
  RestoreBackupRequest,
  VerifyBackupRequest,
  UpdateBackupScheduleRequest,
  ListBackupsQuery,
} from '../types';

// ============================================================================
// Feature Status
// ============================================================================

export function useBackupFeatureStatus() {
  return useQuery({
    queryKey: ['backup-status'],
    queryFn: () => backupApi.getFeatureStatus(),
  });
}

// ============================================================================
// Backups
// ============================================================================

export function useBackups(query?: ListBackupsQuery) {
  return useQuery({
    queryKey: ['backups', query],
    queryFn: () => backupApi.listBackups(query),
  });
}

export function useBackup(id: string) {
  return useQuery({
    queryKey: ['backup', id],
    queryFn: () => backupApi.getBackup(id),
    enabled: !!id,
  });
}

export function useCreateBackup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: CreateBackupRequest) => backupApi.createBackup(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['backups'] });
      queryClient.invalidateQueries({ queryKey: ['backup-status'] });
    },
  });
}

export function useDeleteBackup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => backupApi.deleteBackup(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['backups'] });
      queryClient.invalidateQueries({ queryKey: ['backup-status'] });
    },
  });
}

export function useVerifyBackup() {
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: VerifyBackupRequest }) =>
      backupApi.verifyBackup(id, request),
  });
}

export function useRestoreBackup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: RestoreBackupRequest }) =>
      backupApi.restoreBackup(id, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['backup-restores'] });
    },
  });
}

export function useDownloadBackup() {
  return useMutation({
    mutationFn: async (id: string) => {
      const blob = await backupApi.downloadBackup(id);
      return blob;
    },
  });
}

// ============================================================================
// Schedule
// ============================================================================

export function useBackupSchedule() {
  return useQuery({
    queryKey: ['backup-schedule'],
    queryFn: () => backupApi.getSchedule(),
  });
}

export function useUpdateBackupSchedule() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: UpdateBackupScheduleRequest) => backupApi.updateSchedule(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['backup-schedule'] });
      queryClient.invalidateQueries({ queryKey: ['backup-status'] });
    },
  });
}

// ============================================================================
// Restore History
// ============================================================================

export function useBackupRestores() {
  return useQuery({
    queryKey: ['backup-restores'],
    queryFn: () => backupApi.listRestores(),
  });
}
