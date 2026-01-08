import { create } from 'zustand';
import { Notification, NotificationStats, NotificationEvent } from '../types/notification';
import { notificationApi } from '../services/api';

interface NotificationStore {
  // State
  notifications: Notification[];
  stats: NotificationStats | null;
  isConnected: boolean;
  eventSource: EventSource | null;

  // Actions
  setNotifications: (notifications: Notification[]) => void;
  setStats: (stats: NotificationStats) => void;
  addNotification: (notification: Notification) => void;
  updateNotification: (notification: Notification) => void;
  removeNotification: (id: string) => void;
  markAsRead: (id: string, read: boolean) => Promise<void>;
  markAllAsRead: () => Promise<void>;
  bulkMarkRead: (ids: string[], read: boolean) => Promise<void>;
  dismissNotification: (id: string) => Promise<void>;
  deleteNotification: (id: string) => Promise<void>;

  // SSE Connection
  connectSSE: () => void;
  disconnectSSE: () => void;

  // Fetch data
  fetchNotifications: (unreadOnly?: boolean) => Promise<void>;
  fetchStats: () => Promise<void>;
}

export const useNotificationStore = create<NotificationStore>((set, get) => ({
  // Initial state
  notifications: [],
  stats: null,
  isConnected: false,
  eventSource: null,

  // Set notifications
  setNotifications: (notifications) => set({ notifications }),

  // Set stats
  setStats: (stats) => set({ stats }),

  // Add new notification
  addNotification: (notification) => {
    set((state) => ({
      notifications: [notification, ...state.notifications],
    }));
    // Update stats
    get().fetchStats();
  },

  // Update existing notification
  updateNotification: (notification) => {
    set((state) => ({
      notifications: state.notifications.map((n) =>
        n.id === notification.id ? notification : n
      ),
    }));
    // Update stats
    get().fetchStats();
  },

  // Remove notification
  removeNotification: (id) => {
    set((state) => ({
      notifications: state.notifications.filter((n) => n.id !== id),
    }));
    // Update stats
    get().fetchStats();
  },

  // Mark notification as read/unread
  markAsRead: async (id, read) => {
    try {
      const response = await notificationApi.markAsRead(id, read);
      get().updateNotification(response.notification);
    } catch (error) {
      console.error('Failed to mark notification as read:', error);
      throw error;
    }
  },

  // Mark all notifications as read
  markAllAsRead: async () => {
    try {
      await notificationApi.markAllAsRead();
      // Refresh notifications and stats
      await Promise.all([get().fetchNotifications(), get().fetchStats()]);
    } catch (error) {
      console.error('Failed to mark all as read:', error);
      throw error;
    }
  },

  // Bulk mark notifications
  bulkMarkRead: async (ids, read) => {
    try {
      await notificationApi.bulkMarkRead(ids, read);
      // Refresh notifications and stats
      await Promise.all([get().fetchNotifications(), get().fetchStats()]);
    } catch (error) {
      console.error('Failed to bulk mark notifications:', error);
      throw error;
    }
  },

  // Dismiss notification
  dismissNotification: async (id) => {
    try {
      await notificationApi.dismissNotification(id);
      get().removeNotification(id);
    } catch (error) {
      console.error('Failed to dismiss notification:', error);
      throw error;
    }
  },

  // Delete notification
  deleteNotification: async (id) => {
    try {
      await notificationApi.deleteNotification(id);
      get().removeNotification(id);
    } catch (error) {
      console.error('Failed to delete notification:', error);
      throw error;
    }
  },

  // Connect to SSE stream
  connectSSE: () => {
    const currentEventSource = get().eventSource;

    // Don't create a new connection if already connected
    if (currentEventSource) {
      return;
    }

    const eventSource = new EventSource('/api/v1/notifications/stream', {
      withCredentials: true,
    });

    eventSource.onopen = () => {
      console.log('SSE connection established');
      set({ isConnected: true });
    };

    eventSource.onmessage = (event) => {
      try {
        const data: NotificationEvent = JSON.parse(event.data);

        switch (data.type) {
          case 'new':
            if (data.notification) {
              get().addNotification(data.notification);
            }
            break;
          case 'updated':
            if (data.notification) {
              get().updateNotification(data.notification);
            }
            break;
          case 'deleted':
            if (data.notification_id) {
              get().removeNotification(data.notification_id);
            }
            break;
          case 'bulk_read':
            // Refresh notifications to reflect bulk changes
            get().fetchNotifications();
            break;
        }
      } catch (error) {
        console.error('Failed to parse SSE message:', error);
      }
    };

    eventSource.onerror = (error) => {
      console.error('SSE connection error:', error);
      set({ isConnected: false });

      // Close the connection
      eventSource.close();
      set({ eventSource: null });

      // Attempt to reconnect after 5 seconds
      setTimeout(() => {
        console.log('Attempting to reconnect SSE...');
        get().connectSSE();
      }, 5000);
    };

    set({ eventSource, isConnected: true });
  },

  // Disconnect from SSE stream
  disconnectSSE: () => {
    const eventSource = get().eventSource;
    if (eventSource) {
      eventSource.close();
      set({ eventSource: null, isConnected: false });
      console.log('SSE connection closed');
    }
  },

  // Fetch notifications
  fetchNotifications: async (unreadOnly = false) => {
    try {
      const response = await notificationApi.getNotifications({ unread_only: unreadOnly });
      set({ notifications: response.notifications });
    } catch (error) {
      console.error('Failed to fetch notifications:', error);
      throw error;
    }
  },

  // Fetch stats
  fetchStats: async () => {
    try {
      const response = await notificationApi.getNotificationStats();
      set({ stats: response.stats });
    } catch (error) {
      console.error('Failed to fetch notification stats:', error);
      throw error;
    }
  },
}));
