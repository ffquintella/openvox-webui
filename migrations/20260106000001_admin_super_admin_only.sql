-- Remove regular admin role from default admin user, keep only super_admin
-- The super_admin role provides all permissions via bypass, making the regular admin role redundant

-- Remove the admin role assignment (keeping only super_admin)
DELETE FROM user_roles
WHERE user_id = '00000000-0000-0000-0000-000000000001'
  AND role_id = '00000000-0000-0000-0000-000000000001';
