-- v9: persist create-new-ai step result on push_log rows.
--
-- Uses the recreate-table pattern (matching 0008) so this migration is
-- idempotent: rolling the user_version back and re-running it does not
-- fail with "duplicate column name".

ALTER TABLE push_log RENAME TO push_log__before_0009;

CREATE TABLE push_log (
  id                    TEXT PRIMARY KEY,
  at                    TEXT NOT NULL,
  character_id          TEXT NOT NULL,
  character_name        TEXT NOT NULL,
  target_id             TEXT NOT NULL,
  target_ai_id          TEXT NOT NULL,
  fields_sent           TEXT NOT NULL,
  did_chat_break        INTEGER NOT NULL,
  greeting              TEXT,
  wipe_cascaded         INTEGER,
  update_info_status    INTEGER NOT NULL,
  update_info_body      TEXT NOT NULL,
  chat_break_status     INTEGER,
  chat_break_body       TEXT,
  journal_entry_ids     TEXT,
  create_new_ai_status  INTEGER,
  create_new_ai_body    TEXT
);

INSERT INTO push_log (
  id, at, character_id, character_name, target_id, target_ai_id, fields_sent,
  did_chat_break, greeting, wipe_cascaded, update_info_status, update_info_body,
  chat_break_status, chat_break_body, journal_entry_ids,
  create_new_ai_status, create_new_ai_body
)
SELECT
  id, at, character_id, character_name, target_id, target_ai_id, fields_sent,
  did_chat_break, greeting, wipe_cascaded, update_info_status, update_info_body,
  chat_break_status, chat_break_body, journal_entry_ids,
  NULL, NULL
FROM push_log__before_0009;

DROP TABLE push_log__before_0009;
