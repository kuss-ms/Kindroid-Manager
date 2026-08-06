-- v14: distinguish AI vs Group targets
ALTER TABLE targets         ADD COLUMN kind TEXT NOT NULL DEFAULT 'ai';
ALTER TABLE chat_messages   ADD COLUMN kind TEXT NOT NULL DEFAULT 'ai';
ALTER TABLE chat_sync_state ADD COLUMN kind TEXT NOT NULL DEFAULT 'ai';

-- Foreign-key enforcement is disabled on this connection by
-- `run_migrations` (per the SQLite docs, `PRAGMA foreign_keys` is a
-- no-op inside an active transaction, so the rebuild has to run
-- outside one).
BEGIN;

-- Rebuild chat_messages with kind and a composite FK.
CREATE TABLE chat_messages_new (
  id                TEXT PRIMARY KEY,
  ai_id             TEXT NOT NULL,
  kind              TEXT NOT NULL DEFAULT 'ai',
  kindroid_msg_id   TEXT NOT NULL,
  sender            TEXT NOT NULL,
  display_name      TEXT,
  timestamp         INTEGER NOT NULL,
  message           TEXT NOT NULL,
  image_urls        TEXT NOT NULL DEFAULT '[]',
  image_description TEXT,
  video_description TEXT,
  internet_response TEXT,
  link_url          TEXT,
  link_description  TEXT,
  fetched_at        TEXT NOT NULL,
  favourite         INTEGER NOT NULL DEFAULT 0,
  UNIQUE (ai_id, kind, kindroid_msg_id),
  FOREIGN KEY (ai_id, kind) REFERENCES targets(ai_id, kind) ON DELETE CASCADE
);
INSERT INTO chat_messages_new
  SELECT id, ai_id, kind, kindroid_msg_id, sender, display_name, timestamp,
         message, image_urls, image_description, video_description,
         internet_response, link_url, link_description, fetched_at, favourite
  FROM chat_messages;
DROP TABLE chat_messages;
ALTER TABLE chat_messages_new RENAME TO chat_messages;

CREATE INDEX chat_messages_ai_ts_idx
  ON chat_messages(ai_id, kind, timestamp DESC);

-- Migration 0010 added a partial index on favourite for the
-- favourites-only filter path. Recreating chat_messages dropped it;
-- recreate so the query path stays O(rows-in-favourites), not a full
-- scan. The index definition must match 0010 so this is intentional,
-- not a fresh addition.
CREATE INDEX IF NOT EXISTS idx_chat_messages_favourite
  ON chat_messages(favourite) WHERE favourite = 1;

-- Recreate FTS5 + triggers (verbatim from migration 0004).
DROP TRIGGER IF EXISTS chat_messages_ai;
DROP TRIGGER IF EXISTS chat_messages_ad;
DROP TRIGGER IF EXISTS chat_messages_au;
DROP TABLE IF EXISTS chat_messages_fts;
CREATE VIRTUAL TABLE chat_messages_fts USING fts5(
  message,
  content='chat_messages',
  content_rowid='rowid',
  tokenize='porter unicode61'
);
CREATE TRIGGER chat_messages_ai AFTER INSERT ON chat_messages BEGIN
  INSERT INTO chat_messages_fts(rowid, message) VALUES (new.rowid, new.message);
END;
CREATE TRIGGER chat_messages_ad AFTER DELETE ON chat_messages BEGIN
  INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message)
  VALUES ('delete', old.rowid, old.message);
END;
CREATE TRIGGER chat_messages_au AFTER UPDATE ON chat_messages BEGIN
  INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message)
  VALUES ('delete', old.rowid, old.message);
  INSERT INTO chat_messages_fts(rowid, message) VALUES (new.rowid, new.message);
END;

-- Re-populate FTS5 from existing chat_messages so historical search still
-- works without re-syncing every target.
INSERT INTO chat_messages_fts(rowid, message)
  SELECT rowid, message FROM chat_messages;

-- Rebuild chat_sync_state with composite PK + composite FK.
CREATE TABLE chat_sync_state_new (
  ai_id          TEXT NOT NULL,
  kind           TEXT NOT NULL DEFAULT 'ai',
  last_synced_at TEXT NOT NULL,
  last_timestamp INTEGER NOT NULL DEFAULT 0,
  full_sync_done INTEGER NOT NULL DEFAULT 0,
  is_syncing     INTEGER NOT NULL DEFAULT 0,
  status_kind    TEXT NOT NULL DEFAULT 'idle',
  status_message TEXT,
  backoff_until  TEXT,
  total          INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (ai_id, kind),
  FOREIGN KEY (ai_id, kind) REFERENCES targets(ai_id, kind) ON DELETE CASCADE
);
INSERT INTO chat_sync_state_new
  SELECT ai_id, kind, last_synced_at, last_timestamp, full_sync_done,
         is_syncing, status_kind, status_message, backoff_until, total
  FROM chat_sync_state;
DROP TABLE chat_sync_state;
ALTER TABLE chat_sync_state_new RENAME TO chat_sync_state;

-- Rebuild targets with composite UNIQUE(ai_id, kind) FIRST so the
-- automation-table rebuilds below can reference the new schema.
CREATE TABLE targets_new (
  id          TEXT PRIMARY KEY,
  ai_id       TEXT NOT NULL,
  kind        TEXT NOT NULL DEFAULT 'ai',
  label       TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  UNIQUE (ai_id, kind)
);
INSERT INTO targets_new SELECT id, ai_id, kind, label, created_at FROM targets;
DROP TABLE targets;
ALTER TABLE targets_new RENAME TO targets;

-- Automation tables still key on `ai_id` only (no `kind` column —
-- groups can never get automation rows because the backend rejects on
-- save, so there's no per-kind scoping needed). The original FK to
-- `targets(ai_id)` is dropped: after the rebuild, `ai_id` alone is
-- not unique on the parent (an AI and a Group can share the same
-- string), so a UNIQUE FK parent doesn't exist. The application is
-- responsible for clearing leftover automation rows when a target is
-- deleted (see `commands::targets::delete_target` which calls
-- `reset_chat_summary` etc. for AI targets). Group targets have no
-- automation rows to clean up.
CREATE TABLE chat_automation_state_new (
  ai_id                             TEXT PRIMARY KEY,
  auto_journal_enabled              INTEGER NOT NULL DEFAULT 0,
  auto_summary_enabled              INTEGER NOT NULL DEFAULT 0,
  interval                          INTEGER NOT NULL DEFAULT 10,
  journal_cap                       INTEGER NOT NULL DEFAULT 1,
  summary_backend                   TEXT NOT NULL,
  bootstrap_mode                    TEXT NOT NULL,
  journal_instructions_override     TEXT,
  summary_instructions_override     TEXT,
  journal_cursor_timestamp          INTEGER,
  journal_cursor_msg_id             TEXT,
  summary_cursor_timestamp          INTEGER,
  summary_cursor_msg_id             TEXT,
  journal_initialised               INTEGER NOT NULL DEFAULT 0,
  summary                           TEXT,
  summary_backend_stored            TEXT NOT NULL,
  pending_summary_candidate         TEXT,
  pending_summary_backend           TEXT,
  pending_summary_created_at        TEXT,
  pending_summary_cursor_timestamp  INTEGER,
  pending_summary_cursor_msg_id     TEXT,
  pending_reformat                  INTEGER NOT NULL DEFAULT 0,
  journal_last_error                TEXT,
  summary_last_error                TEXT,
  journal_last_run_at               TEXT,
  summary_last_run_at               TEXT
);
INSERT INTO chat_automation_state_new
  SELECT ai_id,
         auto_journal_enabled, auto_summary_enabled, interval, journal_cap,
         summary_backend, bootstrap_mode,
         journal_instructions_override, summary_instructions_override,
         journal_cursor_timestamp, journal_cursor_msg_id,
         summary_cursor_timestamp, summary_cursor_msg_id,
         journal_initialised, summary, summary_backend_stored,
         pending_summary_candidate, pending_summary_backend,
         pending_summary_created_at,
         pending_summary_cursor_timestamp, pending_summary_cursor_msg_id,
         pending_reformat, journal_last_error, summary_last_error,
         journal_last_run_at, summary_last_run_at
  FROM chat_automation_state;
DROP TABLE chat_automation_state;
ALTER TABLE chat_automation_state_new RENAME TO chat_automation_state;

CREATE TABLE auto_journal_runs_new (
  id                          TEXT PRIMARY KEY,
  ai_id                       TEXT NOT NULL,
  start_cursor_timestamp      INTEGER,
  start_cursor_msg_id         TEXT,
  end_cursor_timestamp        INTEGER,
  end_cursor_msg_id           TEXT,
  status                      TEXT NOT NULL,
  attempts                    INTEGER NOT NULL DEFAULT 0,
  completed_at                TEXT,
  last_error                  TEXT,
  created_at                  TEXT NOT NULL
);
INSERT INTO auto_journal_runs_new
  SELECT id, ai_id,
         start_cursor_timestamp, start_cursor_msg_id,
         end_cursor_timestamp, end_cursor_msg_id,
         status, attempts, completed_at, last_error, created_at
  FROM auto_journal_runs;
DROP TABLE auto_journal_runs;
ALTER TABLE auto_journal_runs_new RENAME TO auto_journal_runs;

CREATE TABLE auto_journal_entries_new (
  id                          TEXT PRIMARY KEY,
  run_id                      TEXT NOT NULL REFERENCES auto_journal_runs(id) ON DELETE CASCADE,
  ai_id                       TEXT NOT NULL,
  entry                       TEXT NOT NULL,
  keyphrases                  TEXT NOT NULL DEFAULT '[]',
  source_start_timestamp      INTEGER,
  source_start_msg_id         TEXT,
  source_end_timestamp        INTEGER,
  source_end_msg_id           TEXT,
  status                      TEXT NOT NULL,
  response_status             INTEGER,
  response_message            TEXT,
  created_at                  TEXT NOT NULL,
  updated_at                  TEXT NOT NULL
);
INSERT INTO auto_journal_entries_new
  SELECT id, run_id, ai_id, entry, keyphrases,
         source_start_timestamp, source_start_msg_id,
         source_end_timestamp, source_end_msg_id,
         status, response_status, response_message, created_at, updated_at
  FROM auto_journal_entries;
DROP TABLE auto_journal_entries;
ALTER TABLE auto_journal_entries_new RENAME TO auto_journal_entries;

COMMIT;