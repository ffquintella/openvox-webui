-- Add variables column to node_groups table
-- Variables are key-value pairs that become facts when exported to facter

ALTER TABLE node_groups ADD COLUMN variables TEXT NOT NULL DEFAULT '{}';
