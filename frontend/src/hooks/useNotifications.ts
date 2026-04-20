import { useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { notificationApi } from '../services/api';
import { NotificationQuery } from '../types/notification';
import { useNotificationStore } from '../stores/useNotificationStore';

// Query keys
export const notificationKeys = {
  all: ['notifications'] as const,
  lists: () => [...notificationKeys.all, 'list'] as const,
  list: (filters: NotificationQuery) => [...notificationKeys.lists(), filters] as const,
  stats: () => [...notificationKeys.all, 'stats'] as const,
};

// Fetch notifications
export function useNotifications(query?: NotificationQuery) {
  return useQuery({
    queryKey: notificationKeys.list(query || {}),
    queryFn: async () => {
      const response = await notificationApi.getNotifications(query);
      return response.notifications;
    },
  });
}

// Fetch notification stats
export function useNotificationStats() {
  return useQuery({
    queryKey: notificationKeys.stats(),
    queryFn: async () => {
      const response = await notificationApi.getNotificationStats();
      return response.stats;
    },
    refetchInterval: 30000, // Refetch every 30 seconds
  });
}

// Mark notification as read
export function useMarkNotificationRead() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, read }: { id: string; read: boolean }) =>
      notificationApi.markAsRead(id, read),
    onSuccess: () => {
      // Invalidate and refetch
      queryClient.invalidateQueries({ queryKey: notificationKeys.all });
    },
  });
}

// Mark all notifications as read
export function useMarkAllNotificationsRead() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => notificationApi.markAllAsRead(),
    onSuccess: () => {
      // Invalidate and refetch
      queryClient.invalidateQueries({ queryKey: notificationKeys.all });
    },
  });
}

// Bulk mark notifications
export function useBulkMarkNotifications() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ ids, read }: { ids: string[]; read: boolean }) =>
      notificationApi.bulkMarkRead(ids, read),
    onSuccess: () => {
      // Invalidate and refetch
      queryClient.invalidateQueries({ queryKey: notificationKeys.all });
    },
  });
}

// Dismiss notification
export function useDismissNotification() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => notificationApi.dismissNotification(id),
    onSuccess: () => {
      // Invalidate and refetch
      queryClient.invalidateQueries({ queryKey: notificationKeys.all });
    },
  });
}

// Delete notification
export function useDeleteNotification() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => notificationApi.deleteNotification(id),
    onSuccess: () => {
      // Invalidate and refetch
      queryClient.invalidateQueries({ queryKey: notificationKeys.all });
    },
  });
}

// Hook to manage SSE connection lifecycle
export function useNotificationStream() {
  const { connectSSE, disconnectSSE } = useNotificationStore();

  // Connect on mount, disconnect on unmount
  useEffect(() => {
    connectSSE();

    return () => {
      disconnectSSE();
    };
  }, [connectSSE, disconnectSSE]);
}
