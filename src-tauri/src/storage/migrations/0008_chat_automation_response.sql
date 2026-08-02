-- Bring `chat_automation_state` up to the schema that includes
-- `journal_last_response` / `summary_last_response`. The source table may
-- or may not have those columns (they were added then removed during a
-- mid-flight change). The recreate approach handles both cases safely:
-- we rename the existing table out of the way, recreate the canonical
-- schema, copy data across (treating missing columns as NULL), and drop
-- the rename. FK relationships only flow INTO `chat_automation_state`
-- (from `targets`), so no other tables need re-linking.

ALTER TABLE chat_automation_state RENAME TO chat_automation_state__before_0008;

CREATE TABLE chat_automation_state (
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

INSERT INTO chat_automation_state (
  ai_id, auto_journal_enabled, auto_summary_enabled, interval, journal_cap,
  summary_backend, bootstrap_mode, journal_instructions_override, summary_instructions_override,
  journal_cursor_timestamp, journal_cursor_msg_id, summary_cursor_timestamp, summary_cursor_msg_id,
  journal_initialised, summary, summary_backend_stored, pending_summary_candidate,
  pending_summary_backend, pending_summary_created_at, pending_summary_cursor_timestamp,
  pending_summary_cursor_msg_id, pending_reformat, journal_last_error, summary_last_error,
  journal_last_run_at, summary_last_run_at, journal_last_response, summary_last_response
)
SELECT
  ai_id, auto_journal_enabled, auto_summary_enabled, interval, journal_cap,
  summary_backend, bootstrap_mode, journal_instructions_override, summary_instructions_override,
  journal_cursor_timestamp, journal_cursor_msg_id, summary_cursor_timestamp, summary_cursor_msg_id,
  journal_initialised, summary, summary_backend_stored, pending_summary_candidate,
  pending_summary_backend, pending_summary_created_at, pending_summary_cursor_timestamp,
  pending_summary_cursor_msg_id, pending_reformat, journal_last_error, summary_last_error,
  journal_last_run_at, summary_last_run_at,
  NULL, NULL
FROM chat_automation_state__before_0008;

DROP TABLE chat_automation_state__before_0008;
