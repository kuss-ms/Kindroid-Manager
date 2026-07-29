use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::chat_message::{ChatMessage, ChatSyncState};
use crate::domain::push_log::PushLogEntry;
use crate::domain::target::Target;

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
    /// Upsert by id; if a row with the same `ai_id` already exists, update
    /// that row's `label`/`updated_at` (we have UNIQUE(ai_id)). The caller
    /// passes the canonical id back via `id` when a merge happened — the
    /// returned Target reflects the merged row.
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

    /// Insert chat messages, ignoring duplicates by (ai_id, kindroid_msg_id).
    /// Returns the count of newly-inserted rows.
    async fn upsert_chat_messages(
        &self,
        ai_id: &str,
        msgs: &[ChatMessage],
    ) -> Result<usize, StorageError>;

    /// List chat messages for `ai_id`, paginated by `before_ts` (DESC, exclusive).
    async fn list_chat_messages(
        &self,
        ai_id: &str,
        before_ts: Option<i64>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>, StorageError>;

    /// FTS5 search within a single `ai_id`. The query is expected to be
    /// already escaped by the caller.
    async fn search_chat(
        &self,
        ai_id: &str,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ChatMessage>, StorageError>;

    /// Total number of messages known locally for `ai_id`.
    async fn chat_message_count(&self, ai_id: &str) -> Result<u64, StorageError>;

    async fn get_chat_sync_state(&self, ai_id: &str)
        -> Result<Option<ChatSyncState>, StorageError>;

    async fn upsert_chat_sync_state(&self, state: &ChatSyncState) -> Result<(), StorageError>;

    /// Wipe all locally-cached chat history for `ai_id`: every row in
    /// `chat_messages` (the FTS5 index is updated via the existing
    /// `chat_messages_ad` trigger) plus the `chat_sync_state` row, so the
    /// next sync starts cleanly from a zero cursor. Returns the number of
    /// chat_messages rows that were deleted.
    async fn reset_chat_history(&self, ai_id: &str) -> Result<usize, StorageError>;
}
