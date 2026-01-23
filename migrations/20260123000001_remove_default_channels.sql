-- Remove default notification channels
-- These were non-functional placeholders that caused confusion

DELETE FROM notification_channels WHERE id IN ('system-webhook', 'system-email');
