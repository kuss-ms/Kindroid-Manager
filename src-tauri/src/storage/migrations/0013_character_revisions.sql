-- v13: per-character rollback snapshots
-- Captured automatically before every mutating save (character save,
-- journal entry save, journal entry delete). The list endpoint is
-- DESC by `saved_at`; the restore endpoint looks up by revision id and
-- rejects cross-character references via the AND character_id = ?2
-- clause in the SELECT.
--
-- CREATE TABLE / CREATE INDEX use IF NOT EXISTS so the rollback
-- migration-test pattern (run to v13, rewind user_version, rerun)
-- doesn't trip over the table that was created on the first pass.
CREATE TABLE IF NOT EXISTS character_revisions (
    id                TEXT PRIMARY KEY,
    character_id      TEXT NOT NULL,
    saved_at          TEXT NOT NULL,
    character_payload TEXT NOT NULL,
    journal_entries   TEXT NOT NULL,
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_character_revisions_character
    ON character_revisions (character_id, saved_at DESC);