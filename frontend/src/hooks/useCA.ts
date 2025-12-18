import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../services/api';
import type { SignRequest, RenewCARequest } from '../types';

// Query keys
export const caKeys = {
  all: ['ca'] as const,
  status: () => [...caKeys.all, 'status'] as const,
  requests: () => [...caKeys.all, 'requests'] as const,
  certificates: () => [...caKeys.all, 'certificates'] as const,
  certificate: (certname: string) => [...caKeys.all, 'certificate', certname] as const,
};

// CA Status
export function useCAStatus() {
  return useQuery({
    queryKey: caKeys.status(),
    queryFn: api.getCAStatus,
    refetchInterval: 30000, // Refresh every 30 seconds
  });
}

// Certificate Requests
export function useCertificateRequests() {
  return useQuery({
    queryKey: caKeys.requests(),
    queryFn: api.getCertificateRequests,
    refetchInterval: 10000, // Refresh every 10 seconds for pending requests
  });
}

// Certificates
export function useCertificates() {
  return useQuery({
    queryKey: caKeys.certificates(),
    queryFn: api.getCertificates,
  });
}

// Single Certificate
export function useCertificate(certname: string) {
  return useQuery({
    queryKey: caKeys.certificate(certname),
    queryFn: () => api.getCertificate(certname),
    enabled: !!certname,
  });
}

// Sign Certificate
export function useSignCertificate() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ certname, request }: { certname: string; request?: SignRequest }) =>
      api.signCertificate(certname, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: caKeys.status() });
      queryClient.invalidateQueries({ queryKey: caKeys.requests() });
      queryClient.invalidateQueries({ queryKey: caKeys.certificates() });
    },
  });
}

// Reject Certificate
export function useRejectCertificate() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (certname: string) => api.rejectCertificate(certname),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: caKeys.status() });
      queryClient.invalidateQueries({ queryKey: caKeys.requests() });
    },
  });
}

// Revoke Certificate
export function useRevokeCertificate() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (certname: string) => api.revokeCertificate(certname),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: caKeys.status() });
      queryClient.invalidateQueries({ queryKey: caKeys.certificates() });
    },
  });
}

// Renew CA
export function useRenewCA() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: RenewCARequest) => api.renewCA(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: caKeys.status() });
    },
  });
}
