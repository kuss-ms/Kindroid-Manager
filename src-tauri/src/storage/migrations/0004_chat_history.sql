-- v4: chat history (per-target, FTS5-searchable)
CREATE TABLE IF NOT EXISTS chat_messages (
  id                TEXT PRIMARY KEY,
  ai_id             TEXT NOT NULL REFERENCES targets(ai_id) ON DELETE CASCADE,
  kindroid_msg_id   TEXT NOT NULL,
  sender            TEXT NOT NULL,
  sender_type       TEXT NOT NULL,
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
  UNIQUE (ai_id, kindroid_msg_id)
);

CREATE INDEX IF NOT EXISTS chat_messages_ai_ts_idx
  ON chat_messages(ai_id, timestamp DESC);

CREATE VIRTUAL TABLE IF NOT EXISTS chat_messages_fts USING fts5(
  message,
  content='chat_messages',
  content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS chat_messages_ai
  AFTER INSERT ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(rowid, message) VALUES (new.rowid, new.message);
  END;
CREATE TRIGGER IF NOT EXISTS chat_messages_ad
  AFTER DELETE ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message)
    VALUES ('delete', old.rowid, old.message);
  END;
CREATE TRIGGER IF NOT EXISTS chat_messages_au
  AFTER UPDATE ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message)
    VALUES ('delete', old.rowid, old.message);
    INSERT INTO chat_messages_fts(rowid, message) VALUES (new.rowid, new.message);
  END;

CREATE TABLE IF NOT EXISTS chat_sync_state (
  ai_id          TEXT PRIMARY KEY REFERENCES targets(ai_id) ON DELETE CASCADE,
  last_synced_at TEXT NOT NULL,
  last_timestamp INTEGER NOT NULL DEFAULT 0,
  full_sync_done INTEGER NOT NULL DEFAULT 0,
  is_syncing     INTEGER NOT NULL DEFAULT 0,
  status_kind    TEXT NOT NULL DEFAULT 'idle',
  status_message TEXT,
  backoff_until  TEXT,
  total          INTEGER NOT NULL DEFAULT 0
);
