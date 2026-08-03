-- v11: drop journal_last_response / summary_last_response from
-- chat_automation_state.
--
-- These columns held the raw AI provider response (JSON or summary text)
-- alongside `*_last_error`. They were written by the automation cycle and
-- surfaced in the UI for debugging, but the response can be large, can
-- echo chat content, and is not used by the sync loop, the push pipeline,
-- or any export path — so persisting them on disk is unnecessary
-- exposure.
--
-- The recreate-table pattern matches 0008 / 0009 so the migration is
-- idempotent on re-run. The rename-and-copy SELECT defaults the two
-- removed columns to NULL, which is what the new schema wants.
--
-- After this migration the only persisted diagnostics on the row are
-- `*_last_error` and `*_last_run_at`. The raw response now lives only in
-- a process-memory cache, gated by the SettingsPage debug toggle
-- `debug_show_automation_response`; when the toggle is off the cycle
-- never captures anything at all.
ALTER TABLE chat_automation_state RENAME TO chat_automation_state__before_0011;

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
  summary_last_run_at TEXT
);

INSERT INTO chat_automation_state (
  ai_id, auto_journal_enabled, auto_summary_enabled, interval, journal_cap,
  summary_backend, bootstrap_mode, journal_instructions_override, summary_instructions_override,
  journal_cursor_timestamp, journal_cursor_msg_id, summary_cursor_timestamp, summary_cursor_msg_id,
  journal_initialised, summary, summary_backend_stored, pending_summary_candidate,
  pending_summary_backend, pending_summary_created_at, pending_summary_cursor_timestamp,
  pending_summary_cursor_msg_id, pending_reformat, journal_last_error, summary_last_error,
  journal_last_run_at, summary_last_run_at
)
SELECT
  ai_id, auto_journal_enabled, auto_summary_enabled, interval, journal_cap,
  summary_backend, bootstrap_mode, journal_instructions_override, summary_instructions_override,
  journal_cursor_timestamp, journal_cursor_msg_id, summary_cursor_timestamp, summary_cursor_msg_id,
  journal_initialised, summary, summary_backend_stored, pending_summary_candidate,
  pending_summary_backend, pending_summary_created_at, pending_summary_cursor_timestamp,
  pending_summary_cursor_msg_id, pending_reformat, journal_last_error, summary_last_error,
  journal_last_run_at, summary_last_run_at
FROM chat_automation_state__before_0011;

DROP TABLE chat_automation_state__before_0011;
