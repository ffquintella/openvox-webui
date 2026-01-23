-- Remove default notification channels
-- These were non-functional placeholders that caused confusion
-- Only delete if they exist to avoid errors on fresh installations

DELETE FROM notification_channels 
WHERE id IN ('system-webhook', 'system-email')
AND id IN (SELECT id FROM notification_channels WHERE id IN ('system-webhook', 'system-email'));
