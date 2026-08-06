ALTER TABLE characters ADD COLUMN default_target_id TEXT
  REFERENCES targets(id) ON DELETE SET NULL;
