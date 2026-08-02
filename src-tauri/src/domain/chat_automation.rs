use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SummaryBackend {
    #[default]
    AdditionalContext,
    KeyMemories,
}

impl SummaryBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AdditionalContext => "additional_context",
            Self::KeyMemories => "key_memories",
        }
    }

    pub fn limit(&self) -> usize {
        match self {
            Self::AdditionalContext => 2500,
            Self::KeyMemories => 1000,
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "key_memories" => Self::KeyMemories,
            _ => Self::AdditionalContext,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SummaryBootstrapMode {
    #[default]
    FullHistory,
    IncrementalOnly,
}

impl SummaryBootstrapMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullHistory => "full_history",
            Self::IncrementalOnly => "incremental_only",
        }
    }
    pub fn parse(value: &str) -> Self {
        if value == "incremental_only" {
            Self::IncrementalOnly
        } else {
            Self::FullHistory
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StableMessageCursor {
    pub timestamp: i64,
    pub kindroid_msg_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatAutomationState {
    pub ai_id: String,
    pub auto_journal_enabled: bool,
    pub auto_summary_enabled: bool,
    pub interval: u32,
    pub journal_cap: u32,
    pub summary_backend: SummaryBackend,
    pub bootstrap_mode: SummaryBootstrapMode,
    pub journal_instructions_override: Option<String>,
    pub summary_instructions_override: Option<String>,
    pub journal_cursor: Option<StableMessageCursor>,
    pub summary_cursor: Option<StableMessageCursor>,
    pub journal_initialised: bool,
    pub summary: Option<String>,
    pub summary_backend_stored: SummaryBackend,
    pub pending_summary_candidate: Option<String>,
    pub pending_summary_backend: Option<SummaryBackend>,
    pub pending_summary_created_at: Option<DateTime<Utc>>,
    pub pending_summary_cursor: Option<StableMessageCursor>,
    pub pending_reformat: bool,
    pub journal_last_error: Option<String>,
    pub summary_last_error: Option<String>,
    pub journal_last_run_at: Option<DateTime<Utc>>,
    pub summary_last_run_at: Option<DateTime<Utc>>,
}

impl Default for ChatAutomationState {
    fn default() -> Self {
        Self {
            ai_id: String::new(),
            auto_journal_enabled: false,
            auto_summary_enabled: false,
            interval: 10,
            journal_cap: 1,
            summary_backend: SummaryBackend::default(),
            bootstrap_mode: SummaryBootstrapMode::default(),
            journal_instructions_override: None,
            summary_instructions_override: None,
            journal_cursor: None,
            summary_cursor: None,
            journal_initialised: false,
            summary: None,
            summary_backend_stored: SummaryBackend::default(),
            pending_summary_candidate: None,
            pending_summary_backend: None,
            pending_summary_created_at: None,
            pending_summary_cursor: None,
            pending_reformat: false,
            journal_last_error: None,
            summary_last_error: None,
            journal_last_run_at: None,
            summary_last_run_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoJournalRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoJournalEntryStatus {
    Pending,
    Sent,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoJournalRun {
    pub id: String,
    pub ai_id: String,
    pub start_cursor: Option<StableMessageCursor>,
    pub end_cursor: Option<StableMessageCursor>,
    pub status: AutoJournalRunStatus,
    pub attempts: u32,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoJournalEntry {
    pub id: String,
    pub run_id: String,
    pub ai_id: String,
    pub entry: String,
    pub keyphrases: Vec<String>,
    pub source_start: Option<StableMessageCursor>,
    pub source_end: Option<StableMessageCursor>,
    pub status: AutoJournalEntryStatus,
    pub response_status: Option<u16>,
    pub response_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SummaryCandidate {
    pub text: String,
    pub backend: SummaryBackend,
    pub created_at: DateTime<Utc>,
}
