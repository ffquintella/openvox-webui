import { useMutation, useQuery } from '@tanstack/react-query';
import { api } from '../services/api';
import type { GroupPreviewRequest } from '../types';

// Catalogs change only with a server release, so cache them for the session.
const CATALOG_STALE_MS = 10 * 60 * 1000;

export function useClassificationFunctions() {
  return useQuery({
    queryKey: ['classification-functions'],
    queryFn: api.getClassificationFunctions,
    staleTime: CATALOG_STALE_MS,
    retry: false,
  });
}

export function useClassificationOperators() {
  return useQuery({
    queryKey: ['classification-operators'],
    queryFn: api.getClassificationOperators,
    staleTime: CATALOG_STALE_MS,
    retry: false,
  });
}

export function useClassificationMergeStrategies() {
  return useQuery({
    queryKey: ['classification-merge-strategies'],
    queryFn: api.getClassificationMergeStrategies,
    staleTime: CATALOG_STALE_MS,
    retry: false,
  });
}

export function useClassificationBehaviors() {
  return useQuery({
    queryKey: ['classification-behaviors'],
    queryFn: api.getClassificationBehaviors,
    staleTime: CATALOG_STALE_MS,
    retry: false,
  });
}

/** Classification of a node; with `explain` the response carries the evaluation trace. */
export function useNodeClassification(
  certname: string | undefined,
  options: { explain?: boolean; enabled?: boolean } = {}
) {
  const explain = options.explain ?? false;
  return useQuery({
    queryKey: ['node-classification', certname, explain ? 'explain' : 'plain'],
    queryFn: () => api.getNodeClassification(certname!, { explain }),
    enabled: !!certname && (options.enabled ?? true),
    retry: false,
  });
}

/** Dry-run of a group change; nothing is persisted. */
export function usePreviewGroupChange() {
  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: GroupPreviewRequest }) =>
      api.previewGroupChange(id, request),
  });
}
