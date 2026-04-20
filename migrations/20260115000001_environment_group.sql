-- Add is_environment_group column to node_groups table
-- When true, the group's environment is used to SET the node's environment
-- rather than FILTER by it. This allows classification to assign environments.

ALTER TABLE node_groups ADD COLUMN is_environment_group INTEGER NOT NULL DEFAULT 0;
