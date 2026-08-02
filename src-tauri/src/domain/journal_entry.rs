use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_ENTRY_CHARS: usize = 500;
pub const MAX_KEYPHRASES: usize = 8;
/// Hard cap on a single keyphrase. Kindroid's `/journal-create` endpoint
/// rejects any keyphrase longer than 50 Unicode characters with a 400
/// ("keyphrases[i] length must be less than or equal to 50 characters"),
/// so the client-side validator must mirror that limit or the server will
/// reject entries that pass local validation.
pub const MAX_KEYPHRASE_CHARS: usize = 50;

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
                "entry must be {MAX_ENTRY_CHARS} characters or fewer (got {})",
                trimmed.chars().count()
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
                    "keyphrase #{i} must be {MAX_KEYPHRASE_CHARS} characters or fewer (got {})",
                    t.chars().count()
                ));
            }
        }
        Ok(())
    }

    /// Same rules as [`validate`], but wraps the messages with an entry
    /// index hint so per-target automation errors can identify the
    /// offending entry.
    pub fn validate_indexed(
        index: usize,
        entry: &str,
        keyphrases: &[String],
    ) -> Result<(), String> {
        match Self::validate(entry, keyphrases) {
            Ok(()) => Ok(()),
            Err(message) => Err(format!("entry #{index}: {message}")),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_actual_length_when_over_entry_limit() {
        let entry = "a".repeat(MAX_ENTRY_CHARS + 7);
        let err = JournalEntry::validate(&entry, &[]).unwrap_err();
        assert!(
            err.contains(&format!(
                "must be {MAX_ENTRY_CHARS} characters or fewer (got {})",
                entry.chars().count()
            )),
            "{err}"
        );
    }

    #[test]
    fn reports_actual_length_when_over_keyphrase_limit() {
        let kp = "b".repeat(MAX_KEYPHRASE_CHARS + 2);
        let err = JournalEntry::validate("ok", std::slice::from_ref(&kp)).unwrap_err();
        assert!(
            err.contains(&format!(
                "must be {MAX_KEYPHRASE_CHARS} characters or fewer (got {})",
                kp.chars().count()
            )),
            "{err}"
        );
    }

    #[test]
    fn validate_indexed_wraps_with_position() {
        let entry = "a".repeat(MAX_ENTRY_CHARS + 1);
        let err = JournalEntry::validate_indexed(2, &entry, &[]).unwrap_err();
        assert!(err.starts_with("entry #2: "), "{err}");
    }
}
