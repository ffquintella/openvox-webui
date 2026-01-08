// Notification types and interfaces

export type NotificationType = 'info' | 'success' | 'warning' | 'error';

export interface Notification {
  id: string;
  user_id: string;
  organization_id?: string;
  title: string;
  message: string;
  type: NotificationType;
  category?: string;
  link?: string;
  read: boolean;
  dismissed: boolean;
  created_at: string;
  read_at?: string;
  expires_at?: string;
  metadata?: string;
}

export interface NotificationStats {
  total: number;
  unread: number;
  by_type: Record<string, number>;
}

export interface CreateNotificationRequest {
  user_id: string;
  organization_id?: string;
  title: string;
  message: string;
  type: NotificationType;
  category?: string;
  link?: string;
  expires_at?: string;
  metadata?: Record<string, any>;
}

export interface NotificationQuery {
  unread_only?: boolean;
  type?: NotificationType;
  category?: string;
  limit?: number;
  offset?: number;
}

export interface NotificationEvent {
  type: 'new' | 'updated' | 'deleted' | 'bulk_read';
  notification?: Notification;
  notification_id?: string;
  notification_ids?: string[];
}
