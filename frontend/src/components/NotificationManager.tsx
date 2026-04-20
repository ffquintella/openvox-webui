import { useEffect, useState } from 'react';
import { useNotificationStore } from '../stores/useNotificationStore';
import { Notification } from '../types/notification';
import { NotificationToastContainer } from './NotificationToast';

/**
 * NotificationManager handles:
 * 1. SSE connection lifecycle
 * 2. Toast notifications for new notifications
 * 3. Initial data fetching
 */
export default function NotificationManager() {
  const [toastNotifications, setToastNotifications] = useState<Notification[]>([]);
  const connectSSE = useNotificationStore((state) => state.connectSSE);
  const disconnectSSE = useNotificationStore((state) => state.disconnectSSE);
  const fetchNotifications = useNotificationStore((state) => state.fetchNotifications);
  const fetchStats = useNotificationStore((state) => state.fetchStats);
  const notifications = useNotificationStore((state) => state.notifications);

  // Track notification IDs we've already shown as toasts
  const [shownNotificationIds, setShownNotificationIds] = useState<Set<string>>(new Set());

  // Initialize: fetch data and connect SSE
  useEffect(() => {
    // Fetch initial data
    Promise.all([fetchNotifications(), fetchStats()]);

    // Connect to SSE stream
    connectSSE();

    // Cleanup on unmount
    return () => {
      disconnectSSE();
    };
  }, [connectSSE, disconnectSSE, fetchNotifications, fetchStats]);

  // Monitor notifications and show toasts for new ones
  useEffect(() => {
    const newNotifications = notifications.filter(
      (notification) =>
        !shownNotificationIds.has(notification.id) && !notification.read
    );

    if (newNotifications.length > 0) {
      // Add new notifications to toast list
      setToastNotifications((prev) => [...newNotifications, ...prev]);

      // Mark them as shown
      setShownNotificationIds((prev) => {
        const newSet = new Set(prev);
        newNotifications.forEach((n) => newSet.add(n.id));
        return newSet;
      });
    }
  }, [notifications, shownNotificationIds]);

  const handleRemoveToast = (id: string) => {
    setToastNotifications((prev) => prev.filter((n) => n.id !== id));
  };

  return (
    <NotificationToastContainer
      notifications={toastNotifications}
      onRemove={handleRemoveToast}
    />
  );
}
