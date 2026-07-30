use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub id: Uuid,
    pub ai_id: String,
    pub kindroid_msg_id: String,
    pub sender: String,
    pub sender_type: String,
    pub display_name: Option<String>,
    pub timestamp: i64,
    pub message: String,
    pub image_urls: Vec<String>,
    pub image_description: Option<String>,
    pub video_description: Option<String>,
    pub internet_response: Option<String>,
    pub link_url: Option<String>,
    pub link_description: Option<String>,
    pub fetched_at: DateTime<Utc>,
    // Local-only source of truth: the Kindroid `get-chat-messages` endpoint
    // does not return `isPinned`, so any server-side pin state set by
    // another client is invisible here until the user re-toggles.
    #[serde(default)]
    pub favourite: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatusKind {
    Idle,
    Running,
    Backoff,
    Cancelled,
    Error,
}

impl SyncStatusKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncStatusKind::Idle => "idle",
            SyncStatusKind::Running => "running",
            SyncStatusKind::Backoff => "backoff",
            SyncStatusKind::Cancelled => "cancelled",
            SyncStatusKind::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "running" => SyncStatusKind::Running,
            "backoff" => SyncStatusKind::Backoff,
            "cancelled" => SyncStatusKind::Cancelled,
            "error" => SyncStatusKind::Error,
            _ => SyncStatusKind::Idle,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatSyncState {
    pub ai_id: String,
    pub last_synced_at: DateTime<Utc>,
    pub last_timestamp: i64,
    pub full_sync_done: bool,
    pub is_syncing: bool,
    pub status_kind: SyncStatusKind,
    pub status_message: Option<String>,
    pub backoff_until: Option<DateTime<Utc>>,
    pub total: u64,
}
