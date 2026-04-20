-- Fix admin user password hash
-- The initial migration had a placeholder hash that is not valid.
-- This migration updates the admin user with a proper Argon2id hash for password "admin"
-- IMPORTANT: Change this password in production!

-- The hash below is for password "admin" generated with Argon2id
UPDATE users
SET password_hash = '$argon2id$v=19$m=19456,t=2,p=1$6lraESNyn+1SEJDdyFRpPw$qbQx7yXDmc/v8zFJ/JfQkGDgLTpdYpsGovYh1T6c3TY',
    updated_at = CURRENT_TIMESTAMP
WHERE username = 'admin';
