use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_ENTRY_CHARS: usize = 500;
pub const MAX_KEYPHRASES: usize = 8;
pub const MAX_KEYPHRASE_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub character_id: Uuid,
    pub entry: String,
    pub keyphrases: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntryInput {
    #[serde(default)]
    pub id: Option<String>,
    pub entry: String,
    #[serde(default)]
    pub keyphrases: Vec<String>,
}

impl JournalEntry {
    /// Validate user-provided entry + keyphrases. Returns a message suitable
    /// for `AppError::invalid(...)` or `Ok(())`.
    pub fn validate(entry: &str, keyphrases: &[String]) -> Result<(), String> {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return Err("entry must not be empty".into());
        }
        if trimmed.chars().count() > MAX_ENTRY_CHARS {
            return Err(format!(
                "entry must be {MAX_ENTRY_CHARS} characters or fewer"
            ));
        }
        if keyphrases.len() > MAX_KEYPHRASES {
            return Err(format!("at most {MAX_KEYPHRASES} keyphrases"));
        }
        for (i, kp) in keyphrases.iter().enumerate() {
            let t = kp.trim();
            if t.is_empty() {
                return Err(format!("keyphrase #{i} must not be empty"));
            }
            if t.chars().count() > MAX_KEYPHRASE_CHARS {
                return Err(format!(
                    "keyphrase #{i} must be {MAX_KEYPHRASE_CHARS} characters or fewer"
                ));
            }
        }
        Ok(())
    }

    /// Trim each keyphrase, drop empties, dedupe case-insensitively while
    /// preserving first-occurrence casing, and cap at `MAX_KEYPHRASES`.
    pub fn normalize_keyphrases(input: &[String]) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<String> = Vec::with_capacity(input.len());
        for kp in input {
            let t = kp.trim();
            if t.is_empty() {
                continue;
            }
            let key = t.to_lowercase();
            if seen.insert(key) {
                out.push(t.to_string());
                if out.len() == MAX_KEYPHRASES {
                    break;
                }
            }
        }
        out
    }
}
