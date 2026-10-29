-- Advanced classification (M26): additive, the defaults reproduce the
-- behavior from before expressions and group policies existed.
ALTER TABLE node_groups ADD COLUMN priority INTEGER NOT NULL DEFAULT 100;
ALTER TABLE node_groups ADD COLUMN expression_json TEXT;
ALTER TABLE node_groups ADD COLUMN behavior_json TEXT;
