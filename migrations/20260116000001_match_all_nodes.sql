-- Add match_all_nodes column to node_groups table
-- When true, groups with no rules will match all nodes (that pass environment filtering)
-- When false (default), groups with no rules will match no nodes unless they have a parent that matched
ALTER TABLE node_groups ADD COLUMN match_all_nodes INTEGER NOT NULL DEFAULT 0;
