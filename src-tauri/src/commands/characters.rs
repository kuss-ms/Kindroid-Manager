use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::character_revision::{CharacterRevision, CharacterRevisionSummary};
use crate::error::AppError;
use crate::storage::{Repository, StorageError};

#[derive(Debug, Serialize, Deserialize)]
pub struct CharacterInput {
    pub id: Option<Uuid>,
    pub name: String,
    #[serde(flatten)]
    pub fields: CharacterFields,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CharacterFields {
    pub ai_name: Option<String>,
    pub ai_gender: Option<String>,
    pub ai_backstory: Option<String>,
    pub ai_memory: Option<String>,
    pub ai_directive: Option<String>,
    pub ai_example_message: Option<String>,
    pub ai_additional_context: Option<String>,
    pub current_scene: Option<String>,
    pub greeting: Option<String>,
    pub notes: Option<String>,
    pub ai_avatar_description: Option<String>,
}

fn empty_to_none(s: Option<String>) -> Option<String> {
    s.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    })
}

fn normalize(fields: CharacterFields) -> CharacterFields {
    CharacterFields {
        ai_name: empty_to_none(fields.ai_name),
        ai_gender: empty_to_none(fields.ai_gender),
        ai_backstory: empty_to_none(fields.ai_backstory),
        ai_memory: empty_to_none(fields.ai_memory),
        ai_directive: empty_to_none(fields.ai_directive),
        ai_example_message: empty_to_none(fields.ai_example_message),
        ai_additional_context: empty_to_none(fields.ai_additional_context),
        current_scene: empty_to_none(fields.current_scene),
        greeting: empty_to_none(fields.greeting),
        notes: fields.notes,
        ai_avatar_description: empty_to_none(fields.ai_avatar_description),
    }
}

pub async fn list_characters(
    repo: std::sync::Arc<dyn Repository>,
) -> Result<Vec<Character>, AppError> {
    Ok(repo.list_characters().await?)
}

pub async fn get_character(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Character, AppError> {
    Ok(repo.get_character(id).await?)
}

pub async fn save_character(
    repo: std::sync::Arc<dyn Repository>,
    input: CharacterInput,
) -> Result<Character, AppError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("name is required"));
    }
    let fields = normalize(input.fields);
    let new_id = input.id.unwrap_or_else(Uuid::new_v4);
    // Capture a pre-save snapshot when this is an update to an
    // existing character. Brand-new saves (no id, or id not in DB) are
    // a no-op beyond the log line.
    crate::commands::revisions::snapshot_before(&repo, new_id).await;
    // Preserve the existing cover image when updating an existing character.
    // The image is managed separately by `set_character_image`; the form
    // never sets it directly.
    let cover_image = if input.id.is_some() {
        repo.get_character(new_id)
            .await
            .ok()
            .and_then(|c| c.cover_image)
    } else {
        None
    };
    let character = Character {
        id: new_id,
        name: name.to_string(),
        ai_name: fields.ai_name,
        ai_gender: fields.ai_gender,
        ai_backstory: fields.ai_backstory,
        ai_memory: fields.ai_memory,
        ai_directive: fields.ai_directive,
        ai_example_message: fields.ai_example_message,
        ai_additional_context: fields.ai_additional_context,
        current_scene: fields.current_scene,
        user_name: None,
        user_gender: None,
        greeting: fields.greeting,
        notes: fields.notes,
        ai_avatar_description: fields.ai_avatar_description,
        cover_image,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    Ok(repo.upsert_character(character).await?)
}

pub async fn delete_character(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<(), AppError> {
    Ok(repo.delete_character(id).await?)
}

pub async fn duplicate_character(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Character, AppError> {
    let original = repo.get_character(id).await?;
    let now = Utc::now();
    let mut copy = Character {
        id: Uuid::new_v4(),
        name: format!("{} (copy)", original.name),
        ..original.clone()
    };
    copy.created_at = now;
    copy.updated_at = now;
    // Give the duplicate its own copy of the cover image so editing one
    // character doesn't affect the other. Without this the duplicate's
    // `cover_image` would point at the original's file, which the editor
    // can't find (it looks up by the duplicate's own id).
    if let Some(bytes) = repo.read_character_image_bytes(id).await? {
        let new_rel = repo
            .save_character_image_bytes(copy.id, &bytes)
            .await
            .map_err(AppError::from)?;
        copy.cover_image = Some(new_rel);
    } else {
        copy.cover_image = None;
    }
    Ok(repo.upsert_character(copy).await?)
}

pub async fn list_character_revisions(
    repo: std::sync::Arc<dyn Repository>,
    character_id: Uuid,
) -> Result<Vec<CharacterRevisionSummary>, AppError> {
    Ok(repo.list_character_revisions(character_id).await?)
}

pub async fn get_character_revision(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<CharacterRevision, AppError> {
    Ok(repo.get_character_revision(id).await?)
}

pub async fn restore_character_revision(
    repo: std::sync::Arc<dyn Repository>,
    character_id: Uuid,
    revision_id: Uuid,
) -> Result<Character, AppError> {
    match repo
        .restore_character_revision(character_id, revision_id)
        .await
    {
        Ok(c) => Ok(c),
        // The SQL `id = ? AND character_id = ?` filter collapses both
        // "unknown revision id" and "revision belongs to a different
        // character" into a single NotFound, so the caller gets a
        // uniform message.
        Err(StorageError::NotFound) => {
            Err(AppError::invalid("revision not found for this character"))
        }
        Err(e) => Err(AppError::database(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::SqliteRepository;
    use std::sync::Arc;

    fn input(name: &str) -> CharacterInput {
        CharacterInput {
            id: None,
            name: name.to_string(),
            fields: CharacterFields::default(),
        }
    }

    #[tokio::test]
    async fn save_character_preserves_cover_image() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        // Create a character with a cover image.
        let mut c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: "Test".into(),
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                ..make_minimal_character()
            })
            .await
            .unwrap();
        let _ = repo
            .save_character_image_bytes(c.id, b"some-image-bytes")
            .await
            .unwrap();
        c = repo.get_character(c.id).await.unwrap();
        assert!(c.cover_image.is_some(), "cover image should be set");
        let cover_path = c.cover_image.clone().unwrap();

        // Save the character with a new name (no image change).
        let saved = save_character(
            repo.clone(),
            CharacterInput {
                id: Some(c.id),
                name: "Renamed".into(),
                fields: CharacterFields::default(),
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.name, "Renamed");
        assert_eq!(
            saved.cover_image.as_deref(),
            Some(cover_path.as_str()),
            "cover image must survive save"
        );
    }

    #[tokio::test]
    async fn save_character_new_creates_no_cover_image() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let saved = save_character(repo.clone(), input("Fresh")).await.unwrap();
        assert!(saved.cover_image.is_none());
    }

    #[tokio::test]
    async fn duplicate_copies_cover_image_to_a_new_file() {
        // Image storage needs a real data_dir, so use a temp dir instead
        // of the in-memory repo.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open(&db_path).unwrap());

        let c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: "Original".into(),
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                ..make_minimal_character()
            })
            .await
            .unwrap();
        repo.save_character_image_bytes(c.id, b"\x89PNG\r\n\x1a\nfake-png")
            .await
            .unwrap();
        let original = repo.get_character(c.id).await.unwrap();
        let original_path = original.cover_image.clone().unwrap();
        let original_bytes = repo
            .read_character_image_bytes(c.id)
            .await
            .unwrap()
            .unwrap();

        let dup = duplicate_character(repo.clone(), c.id).await.unwrap();

        // Duplicate owns a different image file.
        assert_ne!(dup.cover_image.as_deref(), Some(original_path.as_str()));
        let dup_bytes = repo
            .read_character_image_bytes(dup.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dup_bytes, original_bytes);

        // Deleting the duplicate must not affect the original's file.
        repo.delete_character(dup.id).await.unwrap();
        let still_there = repo.read_character_image_bytes(c.id).await.unwrap();
        assert_eq!(still_there, Some(original_bytes));
    }

    #[tokio::test]
    async fn overwriting_image_with_different_extension_returns_new_bytes() {
        // Regression: previously `read_character_image_bytes` iterated a
        // hardcoded list of extensions and returned whichever file existed
        // first, so overwriting a PNG with a JPG (or vice versa) would
        // leave the stale file on disk and cause subsequent reads to
        // return the wrong bytes.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open(&db_path).unwrap());

        let c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: "Subject".into(),
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                ..make_minimal_character()
            })
            .await
            .unwrap();

        let png_bytes: &[u8] = b"\x89PNG\r\n\x1a\noriginal-png-payload";
        repo.save_character_image_bytes(c.id, png_bytes)
            .await
            .unwrap();
        let jpg_bytes: &[u8] = b"\xff\xd8\xff\xe0replacement-jpg-payload";
        repo.save_character_image_bytes(c.id, jpg_bytes)
            .await
            .unwrap();

        let read_back = repo
            .read_character_image_bytes(c.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            read_back, jpg_bytes,
            "must return the newly uploaded bytes, not the stale PNG"
        );
    }

    fn make_minimal_character() -> Character {
        Character {
            id: Uuid::new_v4(),
            name: "x".into(),
            ai_name: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn in_memory_repo() -> Arc<dyn Repository> {
        Arc::new(SqliteRepository::open_in_memory().unwrap())
    }

    async fn seed_character(repo: &Arc<dyn Repository>, name: &str) -> Character {
        let c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: name.into(),
                ai_name: Some("Original".into()),
                ai_backstory: Some("Original backstory".into()),
                notes: Some("Original notes".into()),
                ..make_minimal_character()
            })
            .await
            .unwrap();
        c
    }

    #[tokio::test]
    async fn save_creates_snapshot_with_prior_state() {
        let repo = in_memory_repo();
        let c = seed_character(&repo, "Original").await;
        let cid = c.id;
        // Update name → triggers snapshot of prior state.
        save_character(
            repo.clone(),
            CharacterInput {
                id: Some(cid),
                name: "Renamed".into(),
                fields: CharacterFields::default(),
            },
        )
        .await
        .unwrap();

        let list = repo.list_character_revisions(cid).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].journal_entry_count, 0);
        let detail = repo.get_character_revision(list[0].id).await.unwrap();
        assert_eq!(detail.character_id, cid);
        assert_eq!(detail.character_payload.name, "Original");
        assert_eq!(
            detail.character_payload.ai_backstory.as_deref(),
            Some("Original backstory")
        );
    }

    #[tokio::test]
    async fn save_does_not_snapshot_new_characters() {
        let repo = in_memory_repo();
        let saved = save_character(repo.clone(), input("Fresh")).await.unwrap();
        let list = repo.list_character_revisions(saved.id).await.unwrap();
        assert!(
            list.is_empty(),
            "brand-new character saves must not create snapshots"
        );
    }

    #[tokio::test]
    async fn save_journal_then_edit_creates_snapshot_with_old_entries() {
        let repo = in_memory_repo();
        let c = seed_character(&repo, "C").await;
        let cid = c.id;
        // Use the command layer (which triggers the snapshot) instead
        // of repo.upsert_journal_entry directly.
        let a = crate::commands::journal::save_journal_entry(
            repo.clone(),
            cid,
            crate::domain::journal_entry::JournalEntryInput {
                id: None,
                entry: "A".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        crate::commands::journal::save_journal_entry(
            repo.clone(),
            cid,
            crate::domain::journal_entry::JournalEntryInput {
                id: Some(a.id.clone()),
                entry: "A v2".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();

        let list = repo.list_character_revisions(cid).await.unwrap();
        // The edit captures the snapshot with entry A in it. The
        // initial insert also captures a snapshot (empty journal), so
        // we look for the one that contains A.
        assert!(!list.is_empty());
        let mut with_a = None;
        for s in &list {
            let r = repo.get_character_revision(s.id).await.unwrap();
            if r.journal_entries.iter().any(|e| e.entry == "A") {
                with_a = Some(r);
                break;
            }
        }
        let with_a = with_a.expect("snapshot capturing entry A must exist");
        assert_eq!(with_a.journal_entries.len(), 1);
        assert_eq!(with_a.journal_entries[0].entry, "A");
    }

    #[tokio::test]
    async fn delete_journal_creates_snapshot() {
        let repo = in_memory_repo();
        let c = seed_character(&repo, "C").await;
        let cid = c.id;
        let entry = crate::commands::journal::save_journal_entry(
            repo.clone(),
            cid,
            crate::domain::journal_entry::JournalEntryInput {
                id: None,
                entry: "before-delete".into(),
                keyphrases: vec!["k".into()],
            },
        )
        .await
        .unwrap();
        crate::commands::journal::delete_journal_entry(repo.clone(), cid, entry.id.clone())
            .await
            .unwrap();

        let list = repo.list_character_revisions(cid).await.unwrap();
        // The delete captures the snapshot containing the entry; the
        // earlier insert also captures an empty-journal snapshot.
        assert!(!list.is_empty());
        let mut with_entry = None;
        for s in &list {
            let r = repo.get_character_revision(s.id).await.unwrap();
            if r.journal_entries.iter().any(|e| e.entry == "before-delete") {
                with_entry = Some(r);
                break;
            }
        }
        let with_entry = with_entry.expect("snapshot capturing the entry must exist");
        assert_eq!(with_entry.journal_entries.len(), 1);
        assert_eq!(with_entry.journal_entries[0].entry, "before-delete");
    }

    #[tokio::test]
    async fn restore_round_trip() {
        let repo = in_memory_repo();
        let c = seed_character(&repo, "Original").await;
        let cid = c.id;
        // Two journal entries — created through the command layer so the
        // snapshot trigger fires.
        let e1 = crate::commands::journal::save_journal_entry(
            repo.clone(),
            cid,
            crate::domain::journal_entry::JournalEntryInput {
                id: None,
                entry: "one".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        let e2 = crate::commands::journal::save_journal_entry(
            repo.clone(),
            cid,
            crate::domain::journal_entry::JournalEntryInput {
                id: None,
                entry: "two".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        let original_created = repo.get_character(cid).await.unwrap().created_at;

        // Diverging save creates a snapshot of the (e1, e2) state.
        save_character(
            repo.clone(),
            CharacterInput {
                id: Some(cid),
                name: "Diverged".into(),
                fields: CharacterFields {
                    ai_backstory: Some("Diverged backstory".into()),
                    ..Default::default()
                },
            },
        )
        .await
        .unwrap();
        // Add a third entry (triggers snapshot of post-Diverged state).
        crate::commands::journal::save_journal_entry(
            repo.clone(),
            cid,
            crate::domain::journal_entry::JournalEntryInput {
                id: None,
                entry: "three".into(),
                keyphrases: vec![],
            },
        )
        .await
        .unwrap();
        // Delete e2 (triggers snapshot of (e1, e3) state).
        crate::commands::journal::delete_journal_entry(repo.clone(), cid, e2.id.clone())
            .await
            .unwrap();

        let list = repo.list_character_revisions(cid).await.unwrap();
        assert!(list.len() >= 2);

        // Find the snapshot captured at "Original" — the one with both
        // entries (e1, e2). The first snapshot taken is the one BEFORE
        // the "Diverged" save, so its payload has the original fields
        // and entries (e1, e2).
        let mut snapshot = None;
        for s in &list {
            let r = repo.get_character_revision(s.id).await.unwrap();
            if r.character_payload.name == "Original" {
                snapshot = Some(r);
                break;
            }
        }
        let snapshot = snapshot.expect("original-state snapshot must exist");

        let restored = restore_character_revision(repo.clone(), cid, snapshot.id)
            .await
            .unwrap();
        assert_eq!(restored.name, "Original");
        assert_eq!(restored.ai_backstory.as_deref(), Some("Original backstory"));
        assert_eq!(restored.created_at, original_created);

        let entries = repo.list_journal_entries(cid).await.unwrap();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(entries.len(), 2);
        assert!(ids.contains(&e1.id.as_str()));
        assert!(ids.contains(&e2.id.as_str()));
        // Original timestamps preserved.
        let restored_e1 = entries.iter().find(|e| e.id == e1.id).unwrap();
        assert_eq!(restored_e1.created_at, e1.created_at);
        assert_eq!(restored_e1.updated_at, e1.updated_at);
    }

    #[tokio::test]
    async fn restore_keeps_cover_image() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open(&db_path).unwrap());
        // Create character, save cover image, then snapshot, then
        // change name + upload a different image, then restore.
        let c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: "Subject".into(),
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                ..make_minimal_character()
            })
            .await
            .unwrap();
        repo.save_character_image_bytes(c.id, b"original-image")
            .await
            .unwrap();
        let with_image = repo.get_character(c.id).await.unwrap();
        let cover_path = with_image.cover_image.clone().unwrap();

        // Diverging save: new name.
        save_character(
            repo.clone(),
            CharacterInput {
                id: Some(c.id),
                name: "Renamed".into(),
                fields: CharacterFields::default(),
            },
        )
        .await
        .unwrap();
        // Take the snapshot of the "with image" state. That's the most
        // recent one (capture-before-edit).
        let list = repo.list_character_revisions(c.id).await.unwrap();
        let snap = repo.get_character_revision(list[0].id).await.unwrap();
        // Sanity: the captured payload's name is the pre-edit "Subject".
        assert_eq!(snap.character_payload.name, "Subject");

        // Overwrite the cover image on disk and in the DB.
        repo.save_character_image_bytes(c.id, b"new-image")
            .await
            .unwrap();

        // Restore the "Subject" snapshot. cover_image must remain the
        // current on-disk value (the image bytes we just wrote) because
        // cover_image is NOT in the snapshot and NOT in the SET list.
        let restored = restore_character_revision(repo.clone(), c.id, snap.id)
            .await
            .unwrap();
        assert_eq!(restored.name, "Subject");
        assert_eq!(
            restored.cover_image.as_deref(),
            Some(cover_path.as_str()),
            "cover image must survive restore"
        );
    }

    #[tokio::test]
    async fn restore_keeps_created_at() {
        let repo = in_memory_repo();
        let c = seed_character(&repo, "C").await;
        let cid = c.id;
        let original_created = repo.get_character(cid).await.unwrap().created_at;
        save_character(
            repo.clone(),
            CharacterInput {
                id: Some(cid),
                name: "Renamed".into(),
                fields: CharacterFields::default(),
            },
        )
        .await
        .unwrap();
        let list = repo.list_character_revisions(cid).await.unwrap();
        let snap = repo.get_character_revision(list[0].id).await.unwrap();
        let restored = restore_character_revision(repo.clone(), cid, snap.id)
            .await
            .unwrap();
        assert_eq!(restored.created_at, original_created);
    }

    #[tokio::test]
    async fn cap_at_50_prunes_oldest() {
        let repo = in_memory_repo();
        let c = seed_character(&repo, "C").await;
        let cid = c.id;
        // Generate 51 snapshots directly. Each upsert_character would
        // also work but triggering 51 via the public surface would be
        // slow; the underlying snapshot path is the same.
        for _ in 0..51 {
            repo.snapshot_character(cid).await.unwrap();
        }
        let list = repo.list_character_revisions(cid).await.unwrap();
        assert_eq!(list.len(), 50);
    }

    #[tokio::test]
    async fn cascade_on_character_delete() {
        let repo = in_memory_repo();
        let c = seed_character(&repo, "C").await;
        let cid = c.id;
        repo.snapshot_character(cid).await.unwrap();
        repo.snapshot_character(cid).await.unwrap();
        assert_eq!(repo.list_character_revisions(cid).await.unwrap().len(), 2);
        repo.delete_character(cid).await.unwrap();
        assert!(repo.list_character_revisions(cid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn cross_character_restore_rejected() {
        let repo = in_memory_repo();
        let a = seed_character(&repo, "A").await;
        let b = seed_character(&repo, "B").await;
        // Snapshot under A.
        repo.snapshot_character(a.id).await.unwrap();
        let list = repo.list_character_revisions(a.id).await.unwrap();
        let snap_id = list[0].id;

        // Try to restore A's revision under B's id — must NotFound.
        let err = restore_character_revision(repo.clone(), b.id, snap_id)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Invalid { .. }),
            "cross-character restore must surface as AppError::Invalid, got {err:?}"
        );
    }

    #[tokio::test]
    async fn snapshot_is_silent_on_missing_character() {
        let repo = in_memory_repo();
        let bogus = Uuid::new_v4();
        // repo returns NotFound; the helper logs and swallows.
        let err = repo.snapshot_character(bogus).await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound));
        // The call site (revisions::snapshot_before) does not propagate.
        crate::commands::revisions::snapshot_before(&repo, bogus).await;
    }
}
