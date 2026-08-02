use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PushLogEntry {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    pub character_id: Uuid,
    pub character_name: String,
    pub target_id: Uuid,
    pub target_ai_id: String,
    pub fields_sent: Vec<String>,
    pub did_chat_break: bool,
    pub greeting: Option<String>,
    pub wipe_cascaded: Option<bool>,
    pub update_info_status: u16,
    pub update_info_body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_new_ai_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_new_ai_body: Option<String>,
    pub chat_break_status: Option<u16>,
    pub chat_break_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_entry_ids: Option<Vec<String>>,
}

pub const MAX_LOG_BODY_BYTES: usize = 4 * 1024;

pub fn truncate_body(s: &str) -> String {
    if s.len() <= MAX_LOG_BODY_BYTES {
        s.to_string()
    } else {
        let mut end = MAX_LOG_BODY_BYTES;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…[truncated]", &s[..end])
    }
}
