-- v1: characters, targets, push_log, settings
CREATE TABLE IF NOT EXISTS characters (
  id                       TEXT PRIMARY KEY,
  name                     TEXT NOT NULL,
  ai_name                  TEXT,
  ai_gender                TEXT,
  ai_backstory             TEXT,
  ai_memory                TEXT,
  ai_directive             TEXT,
  ai_example_message       TEXT,
  ai_additional_context    TEXT,
  current_scene            TEXT,
  user_name                TEXT,
  user_gender              TEXT,
  greeting                 TEXT,
  notes                    TEXT,
  created_at               TEXT NOT NULL,
  updated_at               TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS targets (
  id          TEXT PRIMARY KEY,
  ai_id       TEXT NOT NULL UNIQUE,
  label       TEXT NOT NULL,
  created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS push_log (
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
  chat_break_body       TEXT
);

CREATE INDEX IF NOT EXISTS push_log_at_idx ON push_log(at DESC);

CREATE TABLE IF NOT EXISTS settings (
  key    TEXT PRIMARY KEY,
  value  TEXT NOT NULL
);