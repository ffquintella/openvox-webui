import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../services/api';
import type { CreateGroupRequest, UpdateGroupRequest, CreateRuleRequest } from '../types';

export function useGroups() {
  return useQuery({
    queryKey: ['groups'],
    queryFn: api.getGroups,
  });
}

export function useGroup(id: string | undefined) {
  return useQuery({
    queryKey: ['group', id],
    queryFn: () => api.getGroup(id!),
    enabled: !!id,
  });
}

export function useCreateGroup() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateGroupRequest) => api.createGroup(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
    },
  });
}

export function useUpdateGroup() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: UpdateGroupRequest }) =>
      api.updateGroup(id, data),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['group', variables.id] });
    },
  });
}

export function useDeleteGroup() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => api.deleteGroup(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
    },
  });
}

export function useGroupNodes(id: string | undefined) {
  return useQuery({
    queryKey: ['group-nodes', id],
    queryFn: () => api.getGroupNodes(id!),
    enabled: !!id,
  });
}

export function useGroupRules(id: string | undefined) {
  return useQuery({
    queryKey: ['group-rules', id],
    queryFn: () => api.getGroupRules(id!),
    enabled: !!id,
  });
}

export function useAddGroupRule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ groupId, rule }: { groupId: string; rule: CreateRuleRequest }) =>
      api.addGroupRule(groupId, rule),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['group', variables.groupId] });
      queryClient.invalidateQueries({ queryKey: ['group-rules', variables.groupId] });
    },
  });
}

export function useDeleteGroupRule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ groupId, ruleId }: { groupId: string; ruleId: string }) =>
      api.deleteGroupRule(groupId, ruleId),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['group', variables.groupId] });
      queryClient.invalidateQueries({ queryKey: ['group-rules', variables.groupId] });
    },
  });
}

export function useAddPinnedNode() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ groupId, certname }: { groupId: string; certname: string }) =>
      api.addPinnedNode(groupId, certname),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['group', variables.groupId] });
    },
  });
}

export function useRemovePinnedNode() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ groupId, certname }: { groupId: string; certname: string }) =>
      api.removePinnedNode(groupId, certname),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['group', variables.groupId] });
    },
  });
}
