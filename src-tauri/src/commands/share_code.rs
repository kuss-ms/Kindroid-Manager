use chrono::Utc;
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::image_share::{
    decode_image, encode_image, strip_kindroid_metadata, ImageShareError,
};
use crate::domain::journal_entry::JournalEntry;
use crate::domain::share_code::PartialCharacter;
use crate::error::AppError;
use crate::storage::Repository;

/// In-app stash of the most recently exported share image.
///
/// When the user clicks "Share" (or "Export share image" on Android),
/// the encoded PNG is written to the OS clipboard via the WebView's
/// `navigator.clipboard.write([new ClipboardItem({'image/png': blob})])`.
/// On Windows WebView2 the OS clipboard often transcodes the PNG to
/// `CF_DIB` (a bitmap), stripping the `kindroid` `tEXt` chunk — so a
/// subsequent paste back into the same app surfaces
/// `"no kindroid metadata in image"`. The same can happen on Linux
/// clipboard managers and OEM-modified Android WebViews.
///
/// To make the in-app copy→paste round-trip work regardless of OS
/// clipboard behaviour, the export command also `put`s the bytes here.
/// The paste handler in the WebView `take`s them (clearing the slot)
/// before falling back to the OS clipboard. The slot is a single
/// `Option<Vec<u8>>` because the export flow is single-shot and the
/// last-stashed bytes are always the right ones for the most recent
/// paste.
pub struct ShareImageStash {
    inner: std::sync::Mutex<Option<Vec<u8>>>,
}

impl ShareImageStash {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(None),
        }
    }

    pub fn put(&self, bytes: Vec<u8>) {
        if let Ok(mut g) = self.inner.lock() {
            *g = Some(bytes);
        }
    }

    pub fn take(&self) -> Option<Vec<u8>> {
        self.inner.lock().ok().and_then(|mut g| g.take())
    }
}

impl Default for ShareImageStash {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode a Kindroid share image, save the image as the cover, create
/// the new character, and recreate each embedded journal entry (with
/// fresh ids and timestamps — those are local-only).
pub async fn import_share_image(
    repo: std::sync::Arc<dyn Repository>,
    bytes: Vec<u8>,
) -> Result<Character, AppError> {
    let partial = decode_image(&bytes).map_err(image_share_err_to_app)?;
    let mut draft = partial_to_character(&partial);
    let stored = repo
        .save_character_image_bytes(draft.id, &bytes)
        .await
        .map_err(|e| AppError::database(e.to_string()))?;
    // `save_character_image_bytes` already wrote the value to the row,
    // but the in-memory `draft` doesn't see that change. Set the field
    // here so the returned character matches the DB.
    draft.cover_image = Some(stored);
    let character = repo.upsert_character(draft).await?;

    // Recreate each embedded journal entry. We re-run validation +
    // normalisation so an entry that survived the encode round-trip
    // but is still over the length cap (e.g. via a hand-crafted share
    // code) is rejected instead of silently inserted.
    for shared in &partial.journal_entries {
        let kps = JournalEntry::normalize_keyphrases(&shared.keyphrases);
        if let Err(msg) = JournalEntry::validate(&shared.entry, &kps) {
            return Err(AppError::invalid(format!(
                "share image contains an invalid journal entry: {msg}"
            )));
        }
        let now = Utc::now();
        let entry = JournalEntry {
            id: Uuid::new_v4().to_string(),
            character_id: character.id,
            entry: shared.entry.trim().to_string(),
            keyphrases: kps,
            created_at: now,
            updated_at: now,
        };
        repo.upsert_journal_entry(&entry)
            .await
            .map_err(|e| AppError::database(e.to_string()))?;
    }
    Ok(character)
}

/// Encode a Character into a PNG with the persona + journal payload
/// embedded, and stash the bytes in the in-app slot so the paste
/// handler can read them back verbatim even if the OS clipboard
/// transcodes the PNG.
pub async fn export_share_image(
    repo: std::sync::Arc<dyn Repository>,
    stash: std::sync::Arc<ShareImageStash>,
    id: Uuid,
) -> Result<Vec<u8>, AppError> {
    let character = repo.get_character(id).await?;
    let journals = repo
        .list_journal_entries(id)
        .await
        .map_err(|e| AppError::database(e.to_string()))?;
    let image_bytes = repo
        .read_character_image_bytes(id)
        .await
        .map_err(|e| AppError::database(e.to_string()))?
        .ok_or_else(|| AppError::invalid("character has no cover image"))?;
    let encoded =
        encode_image(&image_bytes, &character, &journals).map_err(image_share_err_to_app)?;
    stash.put(encoded.clone());
    Ok(encoded)
}

/// Read and clear the in-app share-image stash. Returns `None` if the
/// slot is empty (the user pasted a different image, or the app was
/// restarted since the last export).
pub fn take_stashed_share_image(stash: std::sync::Arc<ShareImageStash>) -> Option<Vec<u8>> {
    stash.take()
}

/// Upload an image to an existing character. Returns the updated character.
/// Any `kindroid` metadata in the image is stripped before saving — the
/// editor's upload path is for cover images, not for re-importing a
/// share code (use the global drag-drop / paste for that).
pub async fn set_character_image(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
    bytes: Vec<u8>,
) -> Result<Character, AppError> {
    let cleaned = strip_kindroid_metadata(&bytes);
    let _stored = repo
        .save_character_image_bytes(id, &cleaned)
        .await
        .map_err(|e| AppError::database(e.to_string()))?;
    repo.get_character(id)
        .await
        .map_err(|e| AppError::database(e.to_string()))
}

/// Load the cover image bytes for a character. Returns `None` if no image.
pub async fn get_character_image(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Option<Vec<u8>>, AppError> {
    repo.read_character_image_bytes(id)
        .await
        .map_err(|e| AppError::database(e.to_string()))
}

fn image_share_err_to_app(e: ImageShareError) -> AppError {
    AppError::share(format!("{e}"))
}

pub fn partial_to_character(p: &PartialCharacter) -> Character {
    let now = Utc::now();
    Character {
        id: Uuid::new_v4(),
        name: default_name_from_partial(p),
        ai_name: p.ai_name.clone(),
        ai_gender: p.ai_gender.clone(),
        ai_backstory: p.ai_backstory.clone(),
        ai_memory: p.ai_memory.clone(),
        ai_directive: p.ai_directive.clone(),
        ai_example_message: p.ai_example_message.clone(),
        ai_additional_context: p.ai_additional_context.clone(),
        current_scene: p.current_scene.clone(),
        user_name: None,
        user_gender: None,
        greeting: p.greeting.clone(),
        notes: None,
        ai_avatar_description: p.ai_avatar_description.clone(),
        cover_image: None,
        created_at: now,
        updated_at: now,
    }
}

fn default_name_from_partial(p: &PartialCharacter) -> String {
    p.ai_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Imported character".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::image_share::encode_image;
    use crate::storage::sqlite::SqliteRepository;
    use chrono::Utc;
    use std::sync::Arc;

    fn make_png() -> Vec<u8> {
        let mut data = Vec::with_capacity(4);
        for _ in 0..1 {
            data.extend_from_slice(&[255, 0, 0, 255]);
        }
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&data).unwrap();
        }
        out
    }

    fn make_character() -> Character {
        Character {
            id: Uuid::new_v4(),
            name: "Test".into(),
            ai_name: Some("Aria".into()),
            ai_gender: Some("Female".into()),
            ai_backstory: None,
            ai_memory: None,
            ai_directive: None,
            ai_example_message: None,
            ai_additional_context: None,
            current_scene: None,
            user_name: None,
            user_gender: None,
            greeting: Some("Hello".into()),
            notes: None,
            ai_avatar_description: None,
            cover_image: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn import_share_image_returns_character_with_cover_image() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let character = make_character();
        let png = make_png();
        let encoded = encode_image(&png, &character, &[]).unwrap();
        let returned = import_share_image(repo.clone(), encoded).await.unwrap();
        assert!(
            returned.cover_image.is_some(),
            "returned character must have cover_image"
        );
        // The DB row should also have cover_image set.
        let stored = repo.get_character(returned.id).await.unwrap();
        assert_eq!(stored.cover_image, returned.cover_image);
    }

    #[tokio::test]
    async fn import_share_image_recreates_journal_entries() {
        // The share code carries journal entries (entry text +
        // keyphrases); on import we recreate each one under the new
        // character id with fresh local ids and timestamps.
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let character = make_character();
        let now = Utc::now();
        let journals = vec![
            JournalEntry {
                id: Uuid::new_v4().to_string(),
                character_id: character.id,
                entry: "First entry".into(),
                keyphrases: vec!["alpha".into(), "beta".into()],
                created_at: now,
                updated_at: now,
            },
            JournalEntry {
                id: Uuid::new_v4().to_string(),
                character_id: character.id,
                entry: "Second entry".into(),
                keyphrases: vec![],
                created_at: now,
                updated_at: now,
            },
        ];
        let png = make_png();
        let encoded = encode_image(&png, &character, &journals).unwrap();
        let returned = import_share_image(repo.clone(), encoded).await.unwrap();

        let imported = repo.list_journal_entries(returned.id).await.unwrap();
        assert_eq!(
            imported.len(),
            2,
            "both journal entries must be recreated on import"
        );
        // Order is by created_at ASC; both imported entries share the
        // same `now` so the order is implementation-defined, but the
        // multiset of (entry, keyphrases) must match.
        let pairs: Vec<(String, Vec<String>)> = imported
            .iter()
            .map(|e| (e.entry.clone(), e.keyphrases.clone()))
            .collect();
        assert!(pairs.contains(&(
            "First entry".to_string(),
            vec!["alpha".to_string(), "beta".to_string()]
        )));
        assert!(pairs.contains(&("Second entry".to_string(), vec![])));
        // The local ids must differ from the source ids.
        for imp in &imported {
            assert!(journals.iter().all(|src| src.id != imp.id));
        }
    }

    #[tokio::test]
    async fn import_share_image_rejects_invalid_journal_entry() {
        // Defends against a hand-crafted share code (or a corrupted
        // round-trip) containing a journal entry that violates the
        // length/cap constraints. Validation runs before insert.
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let character = make_character();
        // Build a share image with one over-length entry.
        let bad_journal = JournalEntry {
            id: Uuid::new_v4().to_string(),
            character_id: character.id,
            entry: "x".repeat(501),
            keyphrases: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let png = make_png();
        let encoded = encode_image(&png, &character, &[bad_journal]).unwrap();
        let err = import_share_image(repo.clone(), encoded).await.unwrap_err();
        match err {
            AppError::Invalid { message } => {
                assert!(message.contains("invalid journal entry"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn export_share_image_includes_journal_entries() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let stash = Arc::new(ShareImageStash::new());

        let character = make_character();
        let character_id = character.id;
        repo.upsert_character(character.clone()).await.unwrap();
        repo.save_character_image_bytes(character_id, &make_png())
            .await
            .unwrap();
        let now = Utc::now();
        repo.upsert_journal_entry(&JournalEntry {
            id: Uuid::new_v4().to_string(),
            character_id,
            entry: "Persisted entry".into(),
            keyphrases: vec!["kp".into()],
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

        let encoded = export_share_image(repo.clone(), stash.clone(), character_id)
            .await
            .unwrap();
        let partial = decode_image(&encoded).unwrap();
        assert_eq!(partial.journal_entries.len(), 1);
        assert_eq!(partial.journal_entries[0].entry, "Persisted entry");
        assert_eq!(
            partial.journal_entries[0].keyphrases,
            vec!["kp".to_string()]
        );
    }

    #[tokio::test]
    async fn character_field_does_not_include_user_name_or_gender() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let character = make_character();
        let png = make_png();
        let encoded = encode_image(&png, &character, &[]).unwrap();
        let returned = import_share_image(repo.clone(), encoded).await.unwrap();
        assert!(returned.user_name.is_none());
        assert!(returned.user_gender.is_none());
    }

    #[tokio::test]
    async fn export_share_image_stashes_bytes_for_in_app_paste() {
        // The in-app stash is the fix for "I copy a share image to the
        // clipboard, paste it back into the same app, and the persona is
        // gone" — the OS clipboard transcodes the PNG on Windows WebView2
        // and strips the kindroid tEXt chunk. The export path stashes the
        // bytes verbatim so the paste handler can read them back.
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let stash = Arc::new(ShareImageStash::new());

        let character = make_character();
        let character_id = character.id;
        repo.upsert_character(character).await.unwrap();
        repo.save_character_image_bytes(character_id, &make_png())
            .await
            .unwrap();

        // Stash is empty before export.
        assert!(take_stashed_share_image(stash.clone()).is_none());

        let encoded = export_share_image(repo.clone(), stash.clone(), character_id)
            .await
            .unwrap();

        // After export, the stash contains the same bytes.
        let stashed = take_stashed_share_image(stash.clone()).expect("stashed after export");
        assert_eq!(stashed, encoded);

        // The stashed bytes still decode to the persona (i.e. the kindroid
        // tEXt chunk survived the round-trip through the stash).
        let partial = decode_image(&stashed).unwrap();
        assert_eq!(partial.ai_name.as_deref(), Some("Aria"));

        // The stash is one-shot: a second take returns None.
        assert!(take_stashed_share_image(stash).is_none());
    }
}
