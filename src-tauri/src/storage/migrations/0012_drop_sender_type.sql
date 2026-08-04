-- v12: drop chat_messages.sender_type
--
-- The Kindroid `get-chat-messages` endpoint never returns a
-- `sender_type` field; only `sender` (`"ai"` or `"user"`) and
-- `display_name`. The column was a hallucinated add-on that always
-- stored empty strings, so dropping it is safe and removes a source
-- of confusion (the AI extractor used to see `<message sender="">`
-- because the column was empty for every synced row).
--
-- We use the same rename-recreate-copy pattern as 0008 / 0009 / 0011
-- so the migration is idempotent on re-run after a `user_version`
-- rollback (which the existing migration tests do). The trigger drop
-- + recreate is unavoidable because `chat_messages_ai/ad/au` reference
-- `chat_messages` by rowid.

DROP TRIGGER IF EXISTS chat_messages_ai;
DROP TRIGGER IF EXISTS chat_messages_ad;
DROP TRIGGER IF EXISTS chat_messages_au;

ALTER TABLE chat_messages RENAME TO chat_messages__before_0012;

CREATE TABLE chat_messages (
  id                TEXT PRIMARY KEY,
  ai_id             TEXT NOT NULL REFERENCES targets(ai_id) ON DELETE CASCADE,
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
  UNIQUE (ai_id, kindroid_msg_id)
);

INSERT INTO chat_messages (
  id, ai_id, kindroid_msg_id, sender, display_name,
  timestamp, message, image_urls, image_description, video_description,
  internet_response, link_url, link_description, fetched_at, favourite
)
SELECT
  id, ai_id, kindroid_msg_id, sender, display_name,
  timestamp, message, image_urls, image_description, video_description,
  internet_response, link_url, link_description, fetched_at, favourite
FROM chat_messages__before_0012;

DROP TABLE chat_messages__before_0012;

CREATE TRIGGER chat_messages_ai
  AFTER INSERT ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(rowid, message) VALUES (new.rowid, new.message);
  END;
CREATE TRIGGER chat_messages_ad
  AFTER DELETE ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message)
    VALUES ('delete', old.rowid, old.message);
  END;
CREATE TRIGGER chat_messages_au
  AFTER UPDATE ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message)
    VALUES ('delete', old.rowid, old.message);
    INSERT INTO chat_messages_fts(rowid, message) VALUES (new.rowid, new.message);
  END;

CREATE INDEX chat_messages_ai_ts_idx
  ON chat_messages(ai_id, timestamp DESC);

-- The 0010 partial favourite index is bound to the underlying table
-- name, so `ALTER TABLE ... RENAME` above moved it onto
-- `chat_messages__before_0012` and the final `DROP TABLE` then
-- removed it. Recreate it on the canonical table so favourites_only
-- lookups still hit the index instead of falling back to a scan.
CREATE INDEX idx_chat_messages_favourite
    ON chat_messages(ai_id, favourite) WHERE favourite = 1;