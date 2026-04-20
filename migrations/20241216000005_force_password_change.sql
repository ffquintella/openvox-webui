-- Add force_password_change flag to users table
-- This flag indicates whether the user must change their password on next login

ALTER TABLE users ADD COLUMN force_password_change BOOLEAN NOT NULL DEFAULT 0;

-- Set force_password_change to true for the default admin user
-- This ensures the admin password is changed from the default on first login
UPDATE users
SET force_password_change = 1
WHERE username = 'admin';
