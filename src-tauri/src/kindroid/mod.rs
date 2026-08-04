use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub mod http;

pub mod ai;

pub use http::KindroidClient;

#[derive(Debug, Clone)]
pub struct CreateNewAiRequest {
    pub body: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct UpdateInfoRequest {
    /// Free-form JSON object containing `ai_id` + selected persona fields.
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatBreakRequest {
    pub ai_id: String,
    pub greeting: String,
    pub wipe_cascaded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToggleMessagePinRequest {
    pub ai_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone)]
pub struct JournalCreateRequest<'a> {
    pub ai_id: &'a str,
    pub entry: &'a str,
    pub keyphrases: &'a [String],
}

/// Server response from `POST /toggle-message-pin` — the canonical pin state
/// after the toggle. The frontend should reconcile the local cache to this
/// value rather than relying on the optimistic flip.
#[derive(Debug, Clone, Deserialize)]
pub struct ToggleMessagePinResponse {
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpResponse {
    pub status: u16,
    pub ok: bool,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ListChatMessagesRequest {
    pub ai_id: String,
    pub limit: u32,
    pub start_after_timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessagesPage {
    pub messages: Vec<RawChatMessage>,
    pub has_more: bool,
    pub limit: u32,
    /// The cursor returned by the API in `pagination.lastTimestamp`. The
    /// sync loop uses this to advance to the next page; it falls back to
    /// the max of the inserted rows when this is `None` or `0`.
    #[serde(default)]
    pub pagination_last_timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RawChatMessage {
    pub id: String,
    /// The speaker. Kindroid only returns `"ai"` or `"user"` here (see
    /// https://kindroid.ai/docs/article/api-documentation/); the human
    /// name lives in `display_name`.
    pub sender: String,
    pub display_name: Option<String>,
    pub timestamp: i64,
    pub message: Option<String>,
    pub image_urls: Option<Vec<String>>,
    pub image_description: Option<String>,
    pub video_description: Option<String>,
    pub internet_response: Option<String>,
    pub link_url: Option<String>,
    pub link_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Error)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum KindroidError {
    #[error("invalid or missing API key")]
    Auth { status: u16, body: String },
    #[error("rate limited")]
    RateLimited {
        status: u16,
        body: String,
        retry_after: Option<Duration>,
    },
    #[error("bad request: {body}")]
    BadRequest { status: u16, body: String },
    #[error("not found")]
    NotFound { status: u16, body: String },
    #[error("server error")]
    Server { status: u16, body: String },
    #[error("(network) {message}")]
    Network { message: String },
}

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Parse a Retry-After header per RFC 7231 §7.1.3 — either an integer
/// (seconds) or an HTTP-date.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(secs) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(value) {
        let now = chrono::Utc::now();
        let date_utc = date.with_timezone(&chrono::Utc);
        if date_utc > now {
            let diff = (date_utc - now).to_std().ok()?;
            return Some(diff);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_seconds() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
    }

    #[test]
    fn retry_after_invalid() {
        assert_eq!(parse_retry_after("not-a-thing"), None);
    }
}
