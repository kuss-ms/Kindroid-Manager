use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::domain::journal_entry::{JournalEntry, JournalEntryInput};
use crate::error::AppError;
use crate::storage::{Repository, StorageError};

pub async fn list_journal_entries(
    repo: Arc<dyn Repository>,
    character_id: Uuid,
) -> Result<Vec<JournalEntry>, AppError> {
    repo.list_journal_entries(character_id)
        .await
        .map_err(|e| AppError::database(e.to_string()))
}

pub async fn save_journal_entry(
    repo: Arc<dyn Repository>,
    character_id: Uuid,
    input: JournalEntryInput,
) -> Result<JournalEntry, AppError> {
    JournalEntry::validate(&input.entry, &input.keyphrases).map_err(AppError::invalid)?;
    let keyphrases = JournalEntry::normalize_keyphrases(&input.keyphrases);
    crate::commands::revisions::snapshot_before(&repo, character_id).await;
    let entry = match input.id.clone() {
        Some(id) => {
            let existing = repo
                .list_journal_entries(character_id)
                .await
                .map_err(|e| AppError::database(e.to_string()))?
                .into_iter()
                .find(|e| e.id == id)
                .ok_or_else(|| AppError::invalid("entry does not belong to character"))?;
            let now = Utc::now();
            JournalEntry {
                id,
                character_id,
                entry: input.entry.trim().to_string(),
                keyphrases,
                created_at: existing.created_at,
                updated_at: now,
            }
        }
        None => {
            let now = Utc::now();
            JournalEntry {
                id: Uuid::new_v4().to_string(),
                character_id,
                entry: input.entry.trim().to_string(),
                keyphrases,
                created_at: now,
                updated_at: now,
            }
        }
    };
    repo.upsert_journal_entry(&entry)
        .await
        .map_err(|e| AppError::database(e.to_string()))?;
    Ok(entry)
}

pub async fn delete_journal_entry(
    repo: Arc<dyn Repository>,
    character_id: Uuid,
    entry_id: String,
) -> Result<(), AppError> {
    crate::commands::revisions::snapshot_before(&repo, character_id).await;
    match repo.delete_journal_entry(character_id, &entry_id).await {
        Ok(()) => Ok(()),
        Err(StorageError::NotFound) => Err(AppError::invalid("entry does not belong to character")),
        Err(e) => Err(AppError::database(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::character::Character;
    use crate::domain::target::{Target, TargetKind};
    use crate::storage::sqlite::SqliteRepository;
    use crate::storage::{Repository, StorageError};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;

    fn fixture() -> (Character, Target) {
        (
            Character {
                id: Uuid::new_v4(),
                name: "C".into(),
                ai_name: Some("Aria".into()),
                ai_gender: None,
                ai_backstory: None,
                ai_memory: None,
                ai_directive: None,
                ai_example_message: None,
                ai_additional_context: None,
                current_scene: None,
                user_name: None,
                user_gender: None,
                greeting: None,
                notes: None,
                ai_avatar_description: None,
                cover_image: None,
                default_target_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            Target {
                id: Uuid::new_v4(),
                ai_id: "ai_1".into(),
                kind: TargetKind::Ai,
                label: "T".into(),
                created_at: Utc::now(),
            },
        )
    }

    struct FakeRepo {
        journal: Mutex<Vec<JournalEntry>>,
    }

    #[async_trait]
    impl Repository for FakeRepo {
        // Only the journal methods are actually called.
        async fn list_characters(&self) -> Result<Vec<Character>, StorageError> {
            Ok(Vec::new())
        }
        async fn get_character(&self, _: Uuid) -> Result<Character, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn upsert_character(&self, _: Character) -> Result<Character, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn delete_character(&self, _: Uuid) -> Result<(), StorageError> {
            Ok(())
        }
        async fn list_targets(&self) -> Result<Vec<Target>, StorageError> {
            Ok(Vec::new())
        }
        async fn get_target(&self, _: Uuid) -> Result<Target, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn get_target_by_kind(
            &self,
            _: &str,
            _: TargetKind,
        ) -> Result<Option<Target>, StorageError> {
            Ok(None)
        }
        async fn upsert_target(&self, _: Target) -> Result<Target, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn delete_target(&self, _: Uuid) -> Result<(), StorageError> {
            Ok(())
        }
        async fn append_push_log(
            &self,
            _: crate::domain::push_log::PushLogEntry,
        ) -> Result<crate::domain::push_log::PushLogEntry, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn list_push_history(
            &self,
            _: u32,
            _: u32,
        ) -> Result<Vec<crate::domain::push_log::PushLogEntry>, StorageError> {
            Ok(Vec::new())
        }
        async fn get_push_log(
            &self,
            _: Uuid,
        ) -> Result<crate::domain::push_log::PushLogEntry, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn get_setting(&self, _: &str) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn set_setting(&self, _: &str, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn save_character_image_bytes(
            &self,
            _: Uuid,
            _: &[u8],
        ) -> Result<String, StorageError> {
            Ok(String::new())
        }
        async fn read_character_image_bytes(
            &self,
            _: Uuid,
        ) -> Result<Option<Vec<u8>>, StorageError> {
            Ok(None)
        }
        async fn delete_character_image_bytes(&self, _: Uuid) -> Result<(), StorageError> {
            Ok(())
        }
        async fn upsert_chat_messages(
            &self,
            _: &str,
            _: TargetKind,
            _: &[crate::domain::chat_message::ChatMessage],
        ) -> Result<usize, StorageError> {
            Ok(0)
        }
        async fn list_chat_messages(
            &self,
            _: &str,
            _: TargetKind,
            _: Option<i64>,
            _: u32,
            _: bool,
        ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, StorageError> {
            Ok(Vec::new())
        }
        async fn search_chat(
            &self,
            _: &str,
            _: TargetKind,
            _: &str,
            _: u32,
            _: u32,
            _: bool,
        ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, StorageError> {
            Ok(Vec::new())
        }
        async fn set_chat_message_favourite(
            &self,
            _: &str,
            _: TargetKind,
            _: &str,
            _: bool,
        ) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn chat_message_count(&self, _: &str, _: TargetKind) -> Result<u64, StorageError> {
            Ok(0)
        }
        async fn get_chat_sync_state(
            &self,
            _: &str,
            _: TargetKind,
        ) -> Result<Option<crate::domain::chat_message::ChatSyncState>, StorageError> {
            Ok(None)
        }
        async fn upsert_chat_sync_state(
            &self,
            _: &crate::domain::chat_message::ChatSyncState,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn reset_chat_history(&self, _: &str, _: TargetKind) -> Result<usize, StorageError> {
            Ok(0)
        }
        async fn delete_missing_chat_messages(
            &self,
            _: &str,
            _: TargetKind,
            _: i64,
            _: i64,
            _: &[&str],
        ) -> Result<usize, StorageError> {
            Ok(0)
        }

        async fn list_stable_chat_messages(
            &self,
            _: &str,
            _: TargetKind,
            _: Option<&crate::domain::chat_automation::StableMessageCursor>,
            _: u32,
            _: u32,
        ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, StorageError> {
            Ok(Vec::new())
        }
        async fn latest_stable_cursor(
            &self,
            _: &str,
            _: TargetKind,
            _: u32,
        ) -> Result<Option<crate::domain::chat_automation::StableMessageCursor>, StorageError>
        {
            Ok(None)
        }
        async fn get_chat_automation_state(
            &self,
            _: &str,
        ) -> Result<crate::domain::chat_automation::ChatAutomationState, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn upsert_chat_automation_state(
            &self,
            _: &crate::domain::chat_automation::ChatAutomationState,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn create_auto_journal_run(
            &self,
            _: &crate::domain::chat_automation::AutoJournalRun,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn get_auto_journal_run(
            &self,
            _: &str,
        ) -> Result<crate::domain::chat_automation::AutoJournalRun, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn list_pending_auto_journal_runs(
            &self,
            _: &str,
        ) -> Result<Vec<crate::domain::chat_automation::AutoJournalRun>, StorageError> {
            Ok(Vec::new())
        }
        async fn update_auto_journal_run(
            &self,
            _: &crate::domain::chat_automation::AutoJournalRun,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn delete_auto_journal_run(&self, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn create_auto_journal_entry(
            &self,
            _: &crate::domain::chat_automation::AutoJournalEntry,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn list_auto_journal_entries(
            &self,
            _: &str,
        ) -> Result<Vec<crate::domain::chat_automation::AutoJournalEntry>, StorageError> {
            Ok(Vec::new())
        }
        async fn update_auto_journal_entry(
            &self,
            _: &crate::domain::chat_automation::AutoJournalEntry,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn commit_summary_candidate(
            &self,
            _: &str,
            _: &crate::domain::chat_automation::SummaryCandidate,
            _: Option<&crate::domain::chat_automation::StableMessageCursor>,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn clear_summary_candidate(&self, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn reset_chat_summary(&self, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn list_recent_successful_auto_journal_entries(
            &self,
            _: &str,
            _: u32,
        ) -> Result<Vec<crate::domain::chat_automation::AutoJournalEntry>, StorageError> {
            Ok(Vec::new())
        }
        async fn list_journal_entries(
            &self,
            character_id: Uuid,
        ) -> Result<Vec<JournalEntry>, StorageError> {
            Ok(self
                .journal
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.character_id == character_id)
                .cloned()
                .collect())
        }
        async fn upsert_journal_entry(&self, e: &JournalEntry) -> Result<(), StorageError> {
            let mut g = self.journal.lock().unwrap();
            if let Some(existing) = g.iter_mut().find(|x| x.id == e.id) {
                *existing = e.clone();
            } else {
                g.push(e.clone());
            }
            Ok(())
        }
        async fn delete_journal_entry(
            &self,
            character_id: Uuid,
            entry_id: &str,
        ) -> Result<(), StorageError> {
            let before = self.journal.lock().unwrap().len();
            self.journal
                .lock()
                .unwrap()
                .retain(|e| !(e.id == entry_id && e.character_id == character_id));
            if self.journal.lock().unwrap().len() == before {
                return Err(StorageError::NotFound);
            }
            Ok(())
        }
        async fn snapshot_character(&self, _: Uuid) -> Result<(), StorageError> {
            Ok(())
        }
        async fn list_character_revisions(
            &self,
            _: Uuid,
        ) -> Result<Vec<crate::domain::character_revision::CharacterRevisionSummary>, StorageError>
        {
            Ok(Vec::new())
        }
        async fn get_character_revision(
            &self,
            _: Uuid,
        ) -> Result<crate::domain::character_revision::CharacterRevision, StorageError> {
            Err(StorageError::NotFound)
        }
        async fn restore_character_revision(
            &self,
            _: Uuid,
            _: Uuid,
        ) -> Result<Character, StorageError> {
            Err(StorageError::NotFound)
        }
    }

    #[tokio::test]
    async fn save_validates_length_and_phrase_count() {
        let repo: std::sync::Arc<dyn Repository> = std::sync::Arc::new(FakeRepo {
            journal: Mutex::new(Vec::new()),
        });
        let cid = Uuid::new_v4();
        let too_long = "a".repeat(501);
        let err = save_journal_entry(
            repo.clone(),
            cid,
            JournalEntryInput {
                id: None,
                entry: too_long,
                keyphrases: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));

        let too_many: Vec<String> = (0..9).map(|i| format!("k{i}")).collect();
        let err = save_journal_entry(
            repo.clone(),
            cid,
            JournalEntryInput {
                id: None,
                entry: "fine".into(),
                keyphrases: too_many,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
    }

    #[tokio::test]
    async fn save_generates_id_and_timestamps_on_create() {
        let repo: std::sync::Arc<dyn Repository> = std::sync::Arc::new(FakeRepo {
            journal: Mutex::new(Vec::new()),
        });
        let cid = Uuid::new_v4();
        let saved = save_journal_entry(
            repo.clone(),
            cid,
            JournalEntryInput {
                id: None,
                entry: "hello".into(),
                keyphrases: vec!["a".into(), "b".into()],
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.character_id, cid);
        assert!(!saved.id.is_empty());
        assert_eq!(saved.created_at, saved.updated_at);
        assert_eq!(saved.entry, "hello");
        assert_eq!(saved.keyphrases, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn save_preserves_created_at_on_update() {
        let repo: std::sync::Arc<dyn Repository> = std::sync::Arc::new(FakeRepo {
            journal: Mutex::new(Vec::new()),
        });
        let cid = Uuid::new_v4();
        let created = save_journal_entry(
            repo.clone(),
            cid,
            JournalEntryInput {
                id: None,
                entry: "first".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let updated = save_journal_entry(
            repo.clone(),
            cid,
            JournalEntryInput {
                id: Some(created.id.clone()),
                entry: "second".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.created_at, created.created_at);
        assert!(updated.updated_at > created.updated_at);
        assert_eq!(updated.entry, "second");
    }

    #[tokio::test]
    async fn save_rejects_wrong_character_on_update() {
        let repo: std::sync::Arc<dyn Repository> = std::sync::Arc::new(FakeRepo {
            journal: Mutex::new(Vec::new()),
        });
        let cid_a = Uuid::new_v4();
        let cid_b = Uuid::new_v4();
        let created = save_journal_entry(
            repo.clone(),
            cid_a,
            JournalEntryInput {
                id: None,
                entry: "mine".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        let err = save_journal_entry(
            repo.clone(),
            cid_b,
            JournalEntryInput {
                id: Some(created.id),
                entry: "stolen".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
    }

    #[tokio::test]
    async fn delete_rejects_wrong_character() {
        let repo: std::sync::Arc<dyn Repository> = std::sync::Arc::new(FakeRepo {
            journal: Mutex::new(Vec::new()),
        });
        let cid_a = Uuid::new_v4();
        let cid_b = Uuid::new_v4();
        let created = save_journal_entry(
            repo.clone(),
            cid_a,
            JournalEntryInput {
                id: None,
                entry: "mine".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        let err = delete_journal_entry(repo.clone(), cid_b, created.id)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
    }

    #[tokio::test]
    async fn sqlite_journal_crud_round_trip() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let (c, _) = fixture();
        let cid = c.id;
        repo.upsert_character(c).await.unwrap();

        let now = Utc::now();
        let e1 = JournalEntry {
            id: Uuid::new_v4().to_string(),
            character_id: cid,
            entry: "one".into(),
            keyphrases: vec!["a".into()],
            created_at: now,
            updated_at: now,
        };
        let e2 = JournalEntry {
            id: Uuid::new_v4().to_string(),
            character_id: cid,
            entry: "two".into(),
            keyphrases: vec!["b".into(), "c".into()],
            created_at: now + chrono::Duration::seconds(1),
            updated_at: now,
        };
        repo.upsert_journal_entry(&e1).await.unwrap();
        repo.upsert_journal_entry(&e2).await.unwrap();
        let got = repo.list_journal_entries(cid).await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].entry, "one");
        assert_eq!(got[1].entry, "two");
        assert_eq!(got[1].keyphrases, vec!["b", "c"]);
    }
}
