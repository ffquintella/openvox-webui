import { useState, useEffect } from 'react';
import { X, Check, CheckCheck, Trash2, AlertCircle, CheckCircle, Info, AlertTriangle, Bell } from 'lucide-react';
import { useNotificationStore } from '../stores/useNotificationStore';
import { Notification, NotificationType } from '../types/notification';
import { formatDistanceToNow } from 'date-fns';

interface NotificationPanelProps {
  onClose: () => void;
}

export default function NotificationPanel({ onClose }: NotificationPanelProps) {
  const [filter, setFilter] = useState<'all' | 'unread'>('all');
  const notifications = useNotificationStore((state) => state.notifications);
  const markAsRead = useNotificationStore((state) => state.markAsRead);
  const markAllAsRead = useNotificationStore((state) => state.markAllAsRead);
  const deleteNotification = useNotificationStore((state) => state.deleteNotification);
  const fetchNotifications = useNotificationStore((state) => state.fetchNotifications);

  // Fetch notifications on mount
  useEffect(() => {
    fetchNotifications(filter === 'unread');
  }, [filter, fetchNotifications]);

  const filteredNotifications = filter === 'unread'
    ? notifications.filter(n => !n.read)
    : notifications;

  const getNotificationIcon = (type: NotificationType) => {
    switch (type) {
      case 'success':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'error':
        return <AlertCircle className="h-5 w-5 text-red-500" />;
      case 'warning':
        return <AlertTriangle className="h-5 w-5 text-yellow-500" />;
      case 'info':
      default:
        return <Info className="h-5 w-5 text-blue-500" />;
    }
  };

  const getNotificationBgColor = (notification: Notification) => {
    if (!notification.read) {
      return 'bg-blue-50 hover:bg-blue-100';
    }
    return 'bg-white hover:bg-gray-50';
  };

  const handleMarkAsRead = async (id: string, read: boolean) => {
    try {
      await markAsRead(id, read);
    } catch (error) {
      console.error('Failed to mark notification:', error);
    }
  };

  const handleMarkAllAsRead = async () => {
    try {
      await markAllAsRead();
    } catch (error) {
      console.error('Failed to mark all as read:', error);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteNotification(id);
    } catch (error) {
      console.error('Failed to delete notification:', error);
    }
  };

  const formatTime = (dateString: string) => {
    try {
      return formatDistanceToNow(new Date(dateString), { addSuffix: true });
    } catch {
      return dateString;
    }
  };

  return (
    <div className="w-96 bg-white rounded-lg shadow-xl border border-gray-200 max-h-[600px] flex flex-col">
      {/* Header */}
      <div className="px-4 py-3 border-b border-gray-200 flex items-center justify-between bg-gray-50 rounded-t-lg">
        <h3 className="text-lg font-semibold text-gray-900">Notifications</h3>
        <button
          onClick={onClose}
          className="text-gray-400 hover:text-gray-500 transition-colors"
          aria-label="Close"
        >
          <X className="h-5 w-5" />
        </button>
      </div>

      {/* Filters and Actions */}
      <div className="px-4 py-2 border-b border-gray-200 flex items-center justify-between">
        <div className="flex space-x-2">
          <button
            onClick={() => setFilter('all')}
            className={`px-3 py-1 text-sm rounded-md transition-colors ${
              filter === 'all'
                ? 'bg-indigo-100 text-indigo-700 font-medium'
                : 'text-gray-600 hover:bg-gray-100'
            }`}
          >
            All
          </button>
          <button
            onClick={() => setFilter('unread')}
            className={`px-3 py-1 text-sm rounded-md transition-colors ${
              filter === 'unread'
                ? 'bg-indigo-100 text-indigo-700 font-medium'
                : 'text-gray-600 hover:bg-gray-100'
            }`}
          >
            Unread
          </button>
        </div>

        {filteredNotifications.length > 0 && (
          <button
            onClick={handleMarkAllAsRead}
            className="text-sm text-indigo-600 hover:text-indigo-700 flex items-center space-x-1 transition-colors"
          >
            <CheckCheck className="h-4 w-4" />
            <span>Mark all read</span>
          </button>
        )}
      </div>

      {/* Notification List */}
      <div className="flex-1 overflow-y-auto">
        {filteredNotifications.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-gray-500">
            <Bell className="h-12 w-12 mb-3 text-gray-300" />
            <p className="text-sm">No notifications</p>
          </div>
        ) : (
          <div className="divide-y divide-gray-200">
            {filteredNotifications.map((notification) => (
              <div
                key={notification.id}
                className={`px-4 py-3 transition-colors ${getNotificationBgColor(notification)}`}
              >
                <div className="flex items-start space-x-3">
                  {/* Icon */}
                  <div className="flex-shrink-0 pt-1">
                    {getNotificationIcon(notification.type)}
                  </div>

                  {/* Content */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-start justify-between">
                      <p className="text-sm font-medium text-gray-900">
                        {notification.title}
                      </p>
                    </div>
                    <p className="text-sm text-gray-600 mt-1">
                      {notification.message}
                    </p>
                    <p className="text-xs text-gray-500 mt-1">
                      {formatTime(notification.created_at)}
                    </p>

                    {/* Link if available */}
                    {notification.link && (
                      <a
                        href={notification.link}
                        className="text-xs text-indigo-600 hover:text-indigo-700 mt-1 inline-block"
                        onClick={onClose}
                      >
                        View details →
                      </a>
                    )}
                  </div>

                  {/* Actions */}
                  <div className="flex-shrink-0 flex items-center space-x-1">
                    {!notification.read ? (
                      <button
                        onClick={() => handleMarkAsRead(notification.id, true)}
                        className="p-1 text-gray-400 hover:text-indigo-600 transition-colors"
                        title="Mark as read"
                      >
                        <Check className="h-4 w-4" />
                      </button>
                    ) : (
                      <button
                        onClick={() => handleMarkAsRead(notification.id, false)}
                        className="p-1 text-gray-400 hover:text-gray-600 transition-colors"
                        title="Mark as unread"
                      >
                        <div className="h-2 w-2 rounded-full border-2 border-current" />
                      </button>
                    )}
                    <button
                      onClick={() => handleDelete(notification.id)}
                      className="p-1 text-gray-400 hover:text-red-600 transition-colors"
                      title="Delete"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
