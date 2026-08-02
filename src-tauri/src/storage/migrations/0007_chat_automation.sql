CREATE TABLE IF NOT EXISTS chat_automation_state (
  ai_id TEXT PRIMARY KEY REFERENCES targets(ai_id) ON DELETE CASCADE,
  auto_journal_enabled INTEGER NOT NULL DEFAULT 0,
  auto_summary_enabled INTEGER NOT NULL DEFAULT 0,
  interval INTEGER NOT NULL DEFAULT 10,
  journal_cap INTEGER NOT NULL DEFAULT 1,
  summary_backend TEXT NOT NULL DEFAULT 'additional_context',
  bootstrap_mode TEXT NOT NULL DEFAULT 'full_history',
  journal_instructions_override TEXT,
  summary_instructions_override TEXT,
  journal_cursor_timestamp INTEGER,
  journal_cursor_msg_id TEXT,
  summary_cursor_timestamp INTEGER,
  summary_cursor_msg_id TEXT,
  journal_initialised INTEGER NOT NULL DEFAULT 0,
  summary TEXT,
  summary_backend_stored TEXT NOT NULL DEFAULT 'additional_context',
  pending_summary_candidate TEXT,
  pending_summary_backend TEXT,
  pending_summary_created_at TEXT,
  pending_summary_cursor_timestamp INTEGER,
  pending_summary_cursor_msg_id TEXT,
  pending_reformat INTEGER NOT NULL DEFAULT 0,
  journal_last_error TEXT,
  summary_last_error TEXT,
  journal_last_run_at TEXT,
  summary_last_run_at TEXT,
  journal_last_response TEXT,
  summary_last_response TEXT
);

CREATE TABLE IF NOT EXISTS auto_journal_runs (
  id TEXT PRIMARY KEY,
  ai_id TEXT NOT NULL REFERENCES targets(ai_id) ON DELETE CASCADE,
  start_cursor_timestamp INTEGER,
  start_cursor_msg_id TEXT,
  end_cursor_timestamp INTEGER,
  end_cursor_msg_id TEXT,
  status TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  completed_at TEXT,
  last_error TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS auto_journal_entries (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES auto_journal_runs(id) ON DELETE CASCADE,
  ai_id TEXT NOT NULL REFERENCES targets(ai_id) ON DELETE CASCADE,
  entry TEXT NOT NULL,
  keyphrases TEXT NOT NULL DEFAULT '[]',
  source_start_timestamp INTEGER,
  source_start_msg_id TEXT,
  source_end_timestamp INTEGER,
  source_end_msg_id TEXT,
  status TEXT NOT NULL,
  response_status INTEGER,
  response_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS auto_journal_entries_ai_status_idx ON auto_journal_entries(ai_id, status, created_at DESC);
