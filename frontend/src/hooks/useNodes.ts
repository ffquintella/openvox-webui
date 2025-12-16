import { useQuery } from '@tanstack/react-query';
import { api } from '../services/api';

export function useNodes() {
  return useQuery({
    queryKey: ['nodes'],
    queryFn: api.getNodes,
  });
}

export function useNode(certname: string | undefined) {
  return useQuery({
    queryKey: ['node', certname],
    queryFn: () => api.getNode(certname!),
    enabled: !!certname,
  });
}

export function useNodeFacts(certname: string | undefined) {
  return useQuery({
    queryKey: ['node-facts', certname],
    queryFn: () => api.getNodeFacts(certname!),
    enabled: !!certname,
  });
}

export function useNodeReports(certname: string | undefined) {
  return useQuery({
    queryKey: ['node-reports', certname],
    queryFn: () => api.getNodeReports(certname!),
    enabled: !!certname,
  });
}
