# Notification System

The OpenVox WebUI notification system provides real-time notifications to users with support for multiple notification types, SSE streaming, and a floating UI.

## Features

- **Real-time Updates**: Server-Sent Events (SSE) for instant notification delivery
- **Multiple Types**: Info, success, warning, and error notifications
- **Floating UI**: Non-intrusive bell icon with dropdown panel
- **Toast Notifications**: Pop-up alerts for new notifications
- **Persistent Storage**: SQLite database with user-scoped notifications
- **Full CRUD**: Create, read, update, and delete notifications
- **Filtering**: View all or unread notifications only
- **Batch Operations**: Mark multiple notifications as read at once

## Architecture

### Backend (Rust + Axum)

#### Database Schema
```sql
-- notifications table with full audit trail
CREATE TABLE notifications (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    organization_id TEXT,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    type TEXT CHECK(type IN ('info', 'success', 'warning', 'error')),
    category TEXT,
    link TEXT,
    read INTEGER DEFAULT 0,
    dismissed INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    read_at TEXT,
    expires_at TEXT,
    metadata TEXT
);
```

#### API Endpoints

- `GET /api/v1/notifications` - List notifications (with query filters)
- `GET /api/v1/notifications/stats` - Get notification statistics
- `GET /api/v1/notifications/stream` - SSE stream for real-time updates
- `GET /api/v1/notifications/:id` - Get specific notification
- `POST /api/v1/notifications` - Create notification
- `PUT /api/v1/notifications/:id/read` - Mark as read/unread
- `POST /api/v1/notifications/mark-all-read` - Mark all as read
- `POST /api/v1/notifications/bulk-mark-read` - Bulk mark operations
- `POST /api/v1/notifications/:id/dismiss` - Dismiss notification
- `DELETE /api/v1/notifications/:id` - Delete notification

#### Service Layer

**[src/services/notification.rs](../src/services/notification.rs)**
- `NotificationService` - Core business logic
- Broadcast channel for real-time event distribution
- CRUD operations with automatic SSE broadcasting
- Statistics and cleanup methods

### Frontend (React + TypeScript)

#### Components

1. **NotificationBell** - Bell icon button with unread badge
2. **NotificationPanel** - Dropdown showing notification list
3. **NotificationToast** - Pop-up toast for new notifications
4. **NotificationManager** - Manages SSE connection and toast lifecycle

#### State Management

**Zustand Store** - [frontend/src/stores/useNotificationStore.ts](../frontend/src/stores/useNotificationStore.ts)
- Notifications array
- Statistics (total, unread, by type)
- SSE connection state
- CRUD action methods

**React Query Hooks** - [frontend/src/hooks/useNotifications.ts](../frontend/src/hooks/useNotifications.ts)
- `useNotifications()` - Fetch notifications with query filters
- `useNotificationStats()` - Fetch statistics (auto-refetch every 30s)
- `useMarkNotificationRead()` - Mark notification as read/unread
- `useMarkAllNotificationsRead()` - Mark all as read
- `useBulkMarkNotifications()` - Bulk operations
- `useDismissNotification()` - Dismiss notification
- `useDeleteNotification()` - Delete notification
- `useNotificationStream()` - Manage SSE connection lifecycle

## Usage Examples

### Creating Notifications (Backend)

```rust
use openvox_webui::models::{CreateNotificationRequest, NotificationType};

// Create a notification via the service
let req = CreateNotificationRequest {
    user_id: "user-123".to_string(),
    organization_id: Some("org-456".to_string()),
    title: "Deployment Complete".to_string(),
    message: "Your application has been deployed successfully.".to_string(),
    r#type: NotificationType::Success,
    category: Some("deployment".to_string()),
    link: Some("/deployments/abc123".to_string()),
    expires_at: None,
    metadata: None,
};

let notification = notification_service.create_notification(req).await?;
```

### Creating Notifications (API)

```bash
# Create a notification via API
curl -X POST http://localhost:8080/api/v1/notifications \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user-123",
    "title": "Test Notification",
    "message": "This is a test notification",
    "type": "info",
    "category": "test"
  }'
```

### Using in React Components

```tsx
import { useNotificationStore } from '../stores/useNotificationStore';

function MyComponent() {
  const notifications = useNotificationStore((state) => state.notifications);
  const stats = useNotificationStore((state) => state.stats);
  const markAsRead = useNotificationStore((state) => state.markAsRead);

  return (
    <div>
      <p>Unread: {stats?.unread || 0}</p>
      {notifications.map((notification) => (
        <div key={notification.id}>
          <h3>{notification.title}</h3>
          <p>{notification.message}</p>
          <button onClick={() => markAsRead(notification.id, true)}>
            Mark as read
          </button>
        </div>
      ))}
    </div>
  );
}
```

## Testing

### Test Scripts

**[scripts/create-test-notifications.sh](../scripts/create-test-notifications.sh)**
- Creates sample notifications of all types
- Useful for testing the UI

**[scripts/test-notifications-and-enc.sh](../scripts/test-notifications-and-enc.sh)**
- Creates notifications based on environment classification diagnostics
- Tests integration with ENC system

### Manual Testing

1. Start the backend: `cargo run`
2. Start the frontend: `cd frontend && npm run dev`
3. Run test script: `./scripts/create-test-notifications.sh`
4. Open browser to `http://localhost:3000`
5. Look for notification bell icon in top-right header
6. Click bell to view notification panel
7. New notifications will appear as toast pop-ups

### SSE Testing

Monitor SSE stream:
```bash
curl -N http://localhost:8080/api/v1/notifications/stream
```

## Configuration

### Backend

No additional configuration needed. The notification service is automatically initialized with the database pool.

### Frontend

The notification system is integrated into the Layout component and automatically connects on mount.

## Database Maintenance

The notification service includes cleanup methods:

```rust
// Clean up expired notifications
notification_service.cleanup_expired().await?;

// Clean up old read notifications (>30 days)
notification_service.cleanup_old_read(30).await?;
```

Consider running these periodically via a scheduled task.

## Security

- All notification endpoints require authentication
- Notifications are scoped to user_id (users can only see their own)
- SSE streams are filtered by user_id
- Organization scoping available via organization_id field

## Future Enhancements

Potential improvements:
- Email/SMS notification delivery
- Notification preferences per user
- Notification templates
- Scheduled notifications
- Push notifications for mobile apps
- Notification channels (e.g., critical, normal, low priority)
- Delivery receipts and read confirmations
