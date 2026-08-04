use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::journal_entry::JournalEntry;

/// Persona + notes snapshot of a character, captured before every
/// mutating save (character save, journal entry create/update/delete).
///
/// `cover_image` is intentionally excluded — image rollback is out of
/// scope. The live `cover_image` and `created_at` columns are preserved
/// through every restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CharacterSnapshotFields {
    pub name: String,
    pub ai_name: Option<String>,
    pub ai_gender: Option<String>,
    pub ai_backstory: Option<String>,
    pub ai_memory: Option<String>,
    pub ai_directive: Option<String>,
    pub ai_example_message: Option<String>,
    pub ai_additional_context: Option<String>,
    pub current_scene: Option<String>,
    pub user_name: Option<String>,
    pub user_gender: Option<String>,
    pub greeting: Option<String>,
    pub notes: Option<String>,
    pub ai_avatar_description: Option<String>,
}

/// Lightweight row used by the history list. `journal_entry_count` is
/// computed in Rust from the decoded `journal_entries` JSON rather than
/// via SQLite's `json_array_length` so we don't depend on the JSON1
/// extension being available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterRevisionSummary {
    pub id: Uuid,
    pub saved_at: DateTime<Utc>,
    pub journal_entry_count: usize,
}

/// Full snapshot row returned by `get_character_revision` and consumed
/// by the history page's detail expansion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterRevision {
    pub id: Uuid,
    pub character_id: Uuid,
    pub saved_at: DateTime<Utc>,
    pub character_payload: CharacterSnapshotFields,
    pub journal_entries: Vec<JournalEntry>,
}
