use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub mod http;

pub use http::KindroidClient;

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
pub struct HttpResponse {
    pub status: u16,
    pub ok: bool,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
    #[error("(network) {0}")]
    Network(String),
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
