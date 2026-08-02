-- v6: character journal entries + per-push journal ids on push_log
CREATE TABLE character_journal_entries (
    id           TEXT PRIMARY KEY,
    character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    entry        TEXT NOT NULL,
    keyphrases   TEXT NOT NULL DEFAULT '[]',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX idx_character_journal_character
    ON character_journal_entries(character_id, created_at DESC);

ALTER TABLE push_log ADD COLUMN journal_entry_ids TEXT;
