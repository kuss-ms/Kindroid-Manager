use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::kindroid::ai::AiError;
use crate::kindroid::KindroidError;
use crate::security::secrets::SecretStoreError;
use crate::storage::StorageError;

/// All errors surfaced to the frontend as a tagged JSON enum so the UI can
/// render friendly text.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("invalid input: {message}")]
    Invalid { message: String },
    #[error("nothing to push")]
    NothingToPush,
    #[error("missing greeting")]
    MissingGreeting,
    #[error("no token stored")]
    TokenMissing,
    #[error("invalid share code: {message}")]
    ShareCode { message: String },
    #[error("database: {message}")]
    Database { message: String },
    #[error("internal: {message}")]
    Internal { message: String },
    #[error("sync already running for {ai_id}")]
    SyncConflict { ai_id: String },
    #[error(transparent)]
    Secret(#[from] SecretStoreError),
    #[error(transparent)]
    Kindroid(#[from] KindroidError),
    #[error(transparent)]
    Ai(#[from] AiError),
}

impl AppError {
    pub fn invalid<S: Into<String>>(msg: S) -> Self {
        AppError::Invalid {
            message: msg.into(),
        }
    }
    pub fn share<S: Into<String>>(msg: S) -> Self {
        AppError::ShareCode {
            message: msg.into(),
        }
    }
    pub fn database<S: Into<String>>(msg: S) -> Self {
        AppError::Database {
            message: msg.into(),
        }
    }
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        AppError::Internal {
            message: msg.into(),
        }
    }
}

impl From<StorageError> for AppError {
    fn from(value: StorageError) -> Self {
        match value {
            StorageError::NotFound => AppError::NotFound,
            StorageError::Invalid(s) => AppError::Invalid { message: s },
            StorageError::Database(s) => AppError::Database { message: s },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateNewKinResult {
    pub create_new_ai: StepResult,
    pub update_info: Option<StepResult>,
    pub journal_entries: Vec<JournalEntryStep>,
    pub log_id: uuid::Uuid,
    pub target: crate::domain::target::Target,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushResult {
    pub update_info: StepResult,
    /// Per-entry journal-create result. Empty when the step was skipped.
    #[serde(default)]
    pub journal_entries: Vec<JournalEntryStep>,
    pub chat_break: Option<StepResult>,
    pub log_id: uuid::Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JournalEntryStep {
    pub id: String,
    pub status: u16,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StepResult {
    pub status: u16,
    pub ok: bool,
    pub message: String,
}

impl StepResult {
    pub fn from_response(status: u16, body: &str) -> Self {
        Self {
            status,
            ok: (200..300).contains(&status),
            message: friendly_message_for_status(status, body),
        }
    }
}

fn friendly_message_for_status(status: u16, body: &str) -> String {
    match status {
        200..=299 => "OK".into(),
        400..=499 => format!("Kindroid rejected the request: {body}"),
        _ => format!("Server error: {body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_all_variants_as_json() {
        let variants = [
            AppError::NotFound,
            AppError::invalid("token is required"),
            AppError::NothingToPush,
            AppError::MissingGreeting,
            AppError::TokenMissing,
            AppError::share("malformed"),
            AppError::database("db down"),
            AppError::internal("nope"),
            AppError::SyncConflict {
                ai_id: "ai_1".into(),
            },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).expect("serialize");
            assert!(json.contains("\"kind\":"), "missing kind tag: {json}");
        }
    }
}
