use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::character_revision::{CharacterRevision, CharacterRevisionSummary};
use crate::domain::chat_automation::{
    AutoJournalEntry, AutoJournalRun, ChatAutomationState, StableMessageCursor, SummaryCandidate,
};
use crate::domain::chat_message::{ChatMessage, ChatSyncState};
use crate::domain::journal_entry::JournalEntry;
use crate::domain::push_log::PushLogEntry;
use crate::domain::target::{Target, TargetKind};

pub mod sqlite;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

#[async_trait]
pub trait Repository: Send + Sync {
    async fn list_characters(&self) -> Result<Vec<Character>, StorageError>;
    async fn get_character(&self, id: Uuid) -> Result<Character, StorageError>;
    /// Upsert by id. Sets `created_at` if new, always updates `updated_at`.
    async fn upsert_character(&self, character: Character) -> Result<Character, StorageError>;
    async fn delete_character(&self, id: Uuid) -> Result<(), StorageError>;

    async fn list_targets(&self) -> Result<Vec<Target>, StorageError>;
    async fn get_target(&self, id: Uuid) -> Result<Target, StorageError>;
    /// Look up a target by `(ai_id, kind)` rather than its local UUID.
    /// Returns `None` when no row matches. Used by the chat-history +
    /// automation entry points so callers can disambiguate an AI and a
    /// Group that happen to share the same Kindroid identifier string.
    async fn get_target_by_kind(
        &self,
        ai_id: &str,
        kind: TargetKind,
    ) -> Result<Option<Target>, StorageError>;
    /// Upsert by id; if a row with the same `(ai_id, kind)` already
    /// exists, update that row's `label` (`UNIQUE(ai_id, kind)`). The
    /// caller passes the canonical id back via `id` when a merge
    /// happened — the returned Target reflects the merged row. Returns
    /// `StorageError::Invalid("target kind cannot be changed")` if an
    /// existing row with the same `ai_id` exists under a different kind
    /// (kind is immutable after creation).
    async fn upsert_target(&self, target: Target) -> Result<Target, StorageError>;
    async fn delete_target(&self, id: Uuid) -> Result<(), StorageError>;

    async fn append_push_log(&self, entry: PushLogEntry) -> Result<PushLogEntry, StorageError>;
    async fn list_push_history(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<PushLogEntry>, StorageError>;
    async fn get_push_log(&self, id: Uuid) -> Result<PushLogEntry, StorageError>;

    async fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Persist `bytes` as the cover image for `character_id`. Returns the
    /// relative path stored in the DB. Format is detected by magic bytes.
    async fn save_character_image_bytes(
        &self,
        character_id: Uuid,
        bytes: &[u8],
    ) -> Result<String, StorageError>;

    /// Read the cover image for `character_id`. Returns `None` if no
    /// image is stored.
    async fn read_character_image_bytes(&self, id: Uuid) -> Result<Option<Vec<u8>>, StorageError>;

    /// Delete the cover image (if any) for `character_id`.
    async fn delete_character_image_bytes(&self, id: Uuid) -> Result<(), StorageError>;

    /// Insert chat messages, ignoring duplicates by `(ai_id, kind, kindroid_msg_id)`.
    /// Returns the count of newly-inserted rows. The local `id`, `fetched_at`
    /// and `favourite` columns are preserved on UPDATE so user-set pin state
    /// survives subsequent syncs.
    async fn upsert_chat_messages(
        &self,
        ai_id: &str,
        kind: TargetKind,
        msgs: &[ChatMessage],
    ) -> Result<usize, StorageError>;

    /// List chat messages for `(ai_id, kind)`, paginated by `before_ts`
    /// (DESC, exclusive). When `favourites_only` is true, only messages
    /// with `favourite = 1` are returned.
    async fn list_chat_messages(
        &self,
        ai_id: &str,
        kind: TargetKind,
        before_ts: Option<i64>,
        limit: u32,
        favourites_only: bool,
    ) -> Result<Vec<ChatMessage>, StorageError>;

    /// FTS5 search within a single `(ai_id, kind)`. The query is
    /// expected to be already escaped by the caller. `favourites_only`
    /// adds a SQL predicate to the outer join (not to the FTS MATCH
    /// clause, so Porter stemming keeps working).
    async fn search_chat(
        &self,
        ai_id: &str,
        kind: TargetKind,
        query: &str,
        limit: u32,
        offset: u32,
        favourites_only: bool,
    ) -> Result<Vec<ChatMessage>, StorageError>;

    /// Set the local `favourite` flag for a message identified by
    /// `(ai_id, kind, kindroid_msg_id)`. Returns the new value, or
    /// `false` if no matching row exists.
    async fn set_chat_message_favourite(
        &self,
        ai_id: &str,
        kind: TargetKind,
        kindroid_msg_id: &str,
        favourite: bool,
    ) -> Result<bool, StorageError>;

    /// Total number of messages known locally for `(ai_id, kind)`.
    async fn chat_message_count(&self, ai_id: &str, kind: TargetKind) -> Result<u64, StorageError>;

    async fn get_chat_sync_state(
        &self,
        ai_id: &str,
        kind: TargetKind,
    ) -> Result<Option<ChatSyncState>, StorageError>;

    async fn upsert_chat_sync_state(&self, state: &ChatSyncState) -> Result<(), StorageError>;

    /// Wipe all locally-cached chat history and sync state for `(ai_id, kind)`.
    /// The next sync will start cleanly from a zero cursor. Returns the
    /// number of chat_messages rows that were deleted.
    async fn reset_chat_history(
        &self,
        ai_id: &str,
        kind: TargetKind,
    ) -> Result<usize, StorageError>;

    /// Delete chat messages for `(ai_id, kind)` whose timestamp is in
    /// `(start_after, last_timestamp_inclusive]` AND whose
    /// `kindroid_msg_id` is NOT in `keep_ids`. Used after a sync response
    /// to remove messages that were deleted on the server side. The
    /// `keep_ids` list may be empty — in that case every row in the
    /// range is removed (e.g. the API returned an empty page). The
    /// `chat_messages_ad` FTS5 trigger fires per row, so the search
    /// index stays consistent. Returns the number of rows deleted.
    async fn delete_missing_chat_messages(
        &self,
        ai_id: &str,
        kind: TargetKind,
        start_after: i64,
        last_timestamp_inclusive: i64,
        keep_ids: &[&str],
    ) -> Result<usize, StorageError>;

    async fn list_journal_entries(
        &self,
        character_id: Uuid,
    ) -> Result<Vec<JournalEntry>, StorageError>;

    /// Insert or replace by id. Caller is responsible for `created_at`
    /// preservation (the commands layer looks up the existing entry on
    /// edits and reuses its `created_at`).
    async fn upsert_journal_entry(&self, entry: &JournalEntry) -> Result<(), StorageError>;

    async fn delete_journal_entry(
        &self,
        character_id: Uuid,
        entry_id: &str,
    ) -> Result<(), StorageError>;

    async fn list_stable_chat_messages(
        &self,
        ai_id: &str,
        kind: TargetKind,
        after_cursor: Option<&StableMessageCursor>,
        limit: u32,
        exclude_newest_n: u32,
    ) -> Result<Vec<ChatMessage>, StorageError>;
    async fn latest_stable_cursor(
        &self,
        ai_id: &str,
        kind: TargetKind,
        exclude_newest_n: u32,
    ) -> Result<Option<StableMessageCursor>, StorageError>;
    async fn get_chat_automation_state(
        &self,
        ai_id: &str,
    ) -> Result<ChatAutomationState, StorageError>;
    async fn upsert_chat_automation_state(
        &self,
        state: &ChatAutomationState,
    ) -> Result<(), StorageError>;
    async fn create_auto_journal_run(&self, run: &AutoJournalRun) -> Result<(), StorageError>;
    async fn get_auto_journal_run(&self, id: &str) -> Result<AutoJournalRun, StorageError>;
    async fn list_pending_auto_journal_runs(
        &self,
        ai_id: &str,
    ) -> Result<Vec<AutoJournalRun>, StorageError>;
    async fn update_auto_journal_run(&self, run: &AutoJournalRun) -> Result<(), StorageError>;
    /// Remove a stuck auto-journal run and all of its entries. The
    /// cursor is **not** advanced — the next automation cycle will
    /// generate a fresh run for the same message window.
    async fn delete_auto_journal_run(&self, run_id: &str) -> Result<(), StorageError>;
    async fn create_auto_journal_entry(&self, entry: &AutoJournalEntry)
        -> Result<(), StorageError>;
    async fn list_auto_journal_entries(
        &self,
        run_id: &str,
    ) -> Result<Vec<AutoJournalEntry>, StorageError>;
    async fn update_auto_journal_entry(&self, entry: &AutoJournalEntry)
        -> Result<(), StorageError>;
    async fn commit_summary_candidate(
        &self,
        ai_id: &str,
        candidate: &SummaryCandidate,
        cursor: Option<&StableMessageCursor>,
    ) -> Result<(), StorageError>;
    async fn clear_summary_candidate(&self, ai_id: &str) -> Result<(), StorageError>;
    async fn reset_chat_summary(&self, ai_id: &str) -> Result<(), StorageError>;
    async fn list_recent_successful_auto_journal_entries(
        &self,
        ai_id: &str,
        limit: u32,
    ) -> Result<Vec<AutoJournalEntry>, StorageError>;

    /// Capture a pre-save snapshot of `character_id`'s persona fields,
    /// notes, and current journal entries. Prunes to the most recent 50
    /// rows for that character. Returns `StorageError::NotFound` if the
    /// character row no longer exists (callers log and swallow).
    async fn snapshot_character(&self, character_id: Uuid) -> Result<(), StorageError>;

    async fn list_character_revisions(
        &self,
        character_id: Uuid,
    ) -> Result<Vec<CharacterRevisionSummary>, StorageError>;

    async fn get_character_revision(&self, id: Uuid) -> Result<CharacterRevision, StorageError>;

    /// Restore `revision_id` for `character_id`. The SQL filter
    /// (`id = ? AND character_id = ?`) collapses "unknown revision id"
    /// and "revision belongs to a different character" into a single
    /// `StorageError::NotFound`. Returns the updated `Character` row
    /// (with `cover_image` and `created_at` preserved).
    async fn restore_character_revision(
        &self,
        character_id: Uuid,
        revision_id: Uuid,
    ) -> Result<Character, StorageError>;
}
